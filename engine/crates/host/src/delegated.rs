//! Agents that are programs of the person's own (decision 0006). A program runs outside Axiom's
//! sandbox, so each start of one is asked about by itself, and put on record with who contains it
//! (SI-23, SI-26). Once started it stays, between runs, until its agent is changed, the session ends
//! or the host does. What it tells is its word and is shown as that.

use std::{
    collections::{HashMap, HashSet},
    sync::{Arc, PoisonError},
    time::Duration,
};

use axiom_engine::{Deed, Ended, FROM_AGENT, Refused, Say, Stage, Word, said};
use axiom_harness::{
    Answer, By, ChoiceKind, Diff, Done, Error, Event, Events, Frame, Frames, Gone, Handle, Harness,
    KNOWN, Kind, Level, Lost, Program, Prompts, Question, Ready, Server, Settled, Site, Start,
    Trust, Why, find,
};
use axiom_policy::{
    Access, Approval, Ask, Place, Reach, Unsandboxed, audit::Containment, rules::Decision,
};
use axiom_proto::{
    AgentState, Answerable, Body, HarnessInfo, ItemId, ItemKind, Outcome, Provenance, RequestId,
    RunId, Usage, limits,
};
use serde_json::{Value, json};
use tokio::{
    sync::{mpsc, oneshot, watch},
    task::{AbortHandle, JoinSet},
};

use crate::agents::{Floor, Grant, Setup, Slot};

/// A program that has nothing to do for this long is ended; it goes on where it was when it is
/// spoken to.
pub const IDLE: Duration = Duration::from_secs(60 * 60);

/// A harness this host can drive.
pub struct Known {
    pub id: &'static str,
    pub name: &'static str,
    pub program: &'static str,
    pub harness: Harness,
}

pub fn known() -> Vec<Known> {
    let mut all = vec![
        Known {
            id: "claude",
            name: "Claude Code",
            program: "claude",
            harness: Harness::Claude(Prompts::Refused),
        },
        Known { id: "codex", name: "Codex", program: "codex", harness: Harness::Codex },
    ];
    all.extend(KNOWN.iter().map(|k| Known {
        id: k.program,
        name: k.name,
        program: k.program,
        harness: Harness::Acp(k.acp()),
    }));
    all
}

pub fn known_as(id: &str) -> Option<Known> {
    known().into_iter().find(|k| k.id == id)
}

/// Axiom's own, as it is named to a program.
fn server(command: Place, args: Vec<String>) -> Server {
    Server { name: SERVER.into(), tool: crate::peers::TOOL.into(), command, args }
}
const SERVER: &str = "axiom";

impl Known {
    fn start(&self, workspace: Place, access: Access, trust: bool) -> Start {
        Start {
            workspace,
            model: None,
            effort: None,
            access,
            resume: None,
            trust: if trust { Trust::Given } else { Trust::Withheld },
            by: By::Person,
            tools: None,
            hands: false,
            system: None,
        }
    }

    /// What it is and may be, found without running it.
    pub fn info(&self, path: &[Place], written: &Reach) -> HarnessInfo {
        let program = find(self.program, path, written);
        let here = path.first().cloned();
        let access = |trust| {
            [
                (Access::Read, axiom_proto::Access::Read),
                (Access::Ask, axiom_proto::Access::Ask),
                (Access::Work, axiom_proto::Access::Work),
            ]
            .into_iter()
            .filter(|(a, _)| {
                here.clone().is_some_and(|w| self.harness.launch(&self.start(w, *a, trust)).is_ok())
            })
            .map(|(_, a)| a)
            .collect::<Vec<_>>()
        };
        let (trusted, not) = (access(true), access(false));
        HarnessInfo {
            id: self.id.into(),
            name: self.name.into(),
            program: program.as_ref().ok().map(|p| p.place().to_string()),
            problem: program.err().map(|e| e.to_string()),
            needs_trust: not.is_empty(),
            hands: self.harness.takes_hands(),
            effort: self.harness.takes_effort(),
            accounts: crate::connections::provider_of(self.id).map(str::to_owned),
            access: trusted,
        }
    }

    /// Whether it can be started so, and how, in words.
    pub fn check(
        &self,
        s: &Setup,
        access: Access,
        workspace: Place,
        path: &[Place],
        written: &Reach,
    ) -> Result<Program, String> {
        let program = find(self.program, path, written).map_err(|e| e.to_string())?;
        let start = Start {
            model: s.execution.model.clone(),
            effort: s.execution.effort.clone(),
            hands: s.hands(),
            // Only whether it could be given one.
            tools: s.hands().then(|| server(program.place().clone(), Vec::new())),
            ..self.start(workspace, access, s.trust)
        };
        match self.harness.launch(&start) {
            Ok(_) => Ok(program),
            Err(Error::Untrusted) => Err(format!(
                "{} cannot be told to ignore the folder's own settings for it; say that they may take effect",
                self.name
            )),
            Err(Error::NoMode(_)) => Err(format!("{} has no mode for this access", self.name)),
            Err(e) => Err(e.to_string()),
        }
    }
}

/// What is needed to run one agent's program.
pub struct Worker {
    pub floor: Floor,
    pub desk: axiom_engine::Desk,
    pub slot: Arc<Slot>,
    pub setup: Setup,
    pub known: Known,
    pub site: Site,
    pub access: Access,
    pub workspace: Place,
    /// This program, for the tools it is given.
    pub bridge: Option<Place>,
    /// What its model is told, if its hands are Axiom's.
    pub words: Option<String>,
    pub inbox: mpsc::Receiver<Word>,
    /// Said and not yet told it.
    pub waits: Vec<(ItemId, String)>,
}

/// Why a run stops.
enum Halt {
    Cancelled,
    Failed(String),
    Gone,
}
impl From<Refused> for Halt {
    fn from(r: Refused) -> Self {
        match r {
            Refused::Closed => Halt::Gone,
            Refused::This(why) => Halt::Failed(why),
        }
    }
}

impl Worker {
    /// As [`axiom_engine::Agent::begin`].
    pub async fn begin(
        &mut self,
        run: &RunId,
        before: Vec<Say>,
        prompt: Option<Word>,
    ) -> Result<(), Refused> {
        self.floor.say(begun(&self.floor, run)).await?;
        let Err(r) = self.opening(run, before, prompt).await else { return Ok(()) };
        let error = Some(r.to_string());
        let _ = self.floor.say(ended(&self.floor, run, Outcome::Failed, error)).await;
        Err(r)
    }

    async fn opening(
        &mut self,
        run: &RunId,
        before: Vec<Say>,
        prompt: Option<Word>,
    ) -> Result<(), Refused> {
        if !before.is_empty() {
            self.floor.say(before).await?;
        }
        if let Some(Word { text, shown, .. }) = prompt {
            let item = ItemId::new();
            let (by, show) = match &shown {
                Some((by, show)) => (by.as_str(), show),
                None => ("", &text),
            };
            self.floor.say(said(&item, run, &self.floor.agent, show, by)).await?;
            self.floor.sync().await?;
            self.floor.wait(&item, &text).await?;
            self.waits.push((item, text));
        }
        self.floor.sync().await
    }

    /// Runs until the program has no more to do, or cannot go on.
    pub async fn run(&mut self, run: RunId, mut cancel: watch::Receiver<bool>) -> Ended {
        let (outcome, error) = match self.go(&run, &mut cancel).await {
            Ok(e) => return e,
            Err(Halt::Cancelled) => (Outcome::Cancelled, None),
            Err(Halt::Failed(why)) => (Outcome::Failed, Some(why)),
            Err(Halt::Gone) => return Ended { outcome: Outcome::Interrupted, error: None },
        };
        if self.floor.say(ended(&self.floor, &run, outcome, error.clone())).await.is_ok() {
            let _ = self.floor.sync().await;
        }
        Ended { outcome, error }
    }

    async fn go(&mut self, run: &RunId, cancel: &mut watch::Receiver<bool>) -> Result<Ended, Halt> {
        loop {
            let there = self.slot.attempt.lock().await.clone().filter(|a| !a.is_closed());
            let tx = match there {
                Some(tx) => tx,
                None => self.start(run, cancel).await?,
            };
            let (done, ended) = oneshot::channel();
            let closed = mpsc::channel(1).1;
            let turn = Turn {
                run: run.clone(),
                inbox: std::mem::replace(&mut self.inbox, closed),
                words: std::mem::take(&mut self.waits),
                cancel: cancel.clone(),
                done,
            };
            match tx.send(turn).await {
                Ok(()) => {
                    let (e, inbox) = ended.await.map_err(|_| Halt::Gone)?;
                    self.inbox = inbox;
                    return Ok(e);
                }
                // It ended this moment.
                Err(mpsc::error::SendError(t)) => {
                    (self.inbox, self.waits) = (t.inbox, t.words);
                }
            }
        }
    }

    /// Asks a person, puts it on record, starts it.
    async fn start(
        &mut self,
        run: &RunId,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<mpsc::Sender<Turn>, Halt> {
        let mut start = Start {
            workspace: self.workspace.clone(),
            model: self.setup.execution.model.clone(),
            effort: self.setup.execution.effort.clone(),
            access: self.access,
            resume: self.setup.resume.clone(),
            trust: if self.setup.trust { Trust::Given } else { Trust::Withheld },
            by: By::Person,
            tools: None,
            hands: self.setup.hands(),
            system: None,
        };
        // A key for it alone, gone with its attempt. Without one it works all the same, unless it has
        // nothing else to work with.
        let grant = match &self.bridge {
            Some(_) if self.known.harness.takes_tools() => {
                let f = &self.floor;
                let words = self.words.as_deref();
                f.agents.grant(&f.session.id, &f.agent, &self.site.tmp, start.hands, words).ok()
            }
            _ => None,
        };
        if let (Some(g), Some(command)) = (&grant, &self.bridge) {
            let args = vec![crate::mcp::ARG.into(), g.file.display().to_string()];
            start.tools = Some(server(command.clone(), args));
            start.system = g.words.as_ref().and_then(|w| Place::new(w).ok());
        }
        let failed = |e: Error| Halt::Failed(e.to_string());
        let launch = self.known.harness.launch(&start).map_err(failed)?;
        let subject = command(self.site.program.place(), &launch.args);
        let ask = Ask {
            chain: vec![self.floor.agent.clone()],
            workspace: self.workspace.to_string(),
            tool: format!("harness/{}", self.known.id),
            schema: String::new(),
            subject,
            scope: scope(
                &self.known,
                launch.containment,
                self.setup.trust,
                self.site.config.as_ref(),
                start.hands,
            ),
            unsandboxed: true,
        };
        let what = format!("asks to start {}", self.known.name);
        let (id, token) = self.ask(&ask, &what, cancel).await?;
        self.floor
            .record(
                Deed {
                    chain: ask.chain.clone(),
                    workspace: ask.workspace.clone(),
                    tool: ask.tool.clone(),
                    subject: ask.subject.clone(),
                    decision: Decision::Ask,
                    rules: vec![],
                    answered: Some((id, ask.digest())),
                },
                launch.containment,
            )
            .await?;
        self.floor.sync().await?;
        let frames = Record(self.floor.clone());
        let start_hands = start.hands;
        let live = self.known.harness.start(&self.site, start, token, frames).map_err(failed)?;
        let (tx, rx) = mpsc::channel(1);
        let attempt = Attempt {
            floor: self.floor.clone(),
            desk: self.desk.clone(),
            name: self.known.name,
            id: self.known.id,
            workspace: self.workspace.to_string(),
            handle: live.handle,
            events: live.events,
            containment: launch.containment,
            hands: start_hands,
            _grant: grant,
            ready: false,
            resumes: false,
            run: None,
            items: HashMap::new(),
            asked: JoinSet::new(),
            questions: HashMap::new(),
            slot: self.slot.clone(),
            model: self.setup.execution.model.clone(),
            plan: None,
            limit: None,
            early: None,
        };
        if self.setup.began && self.setup.resume.is_none() {
            let words = format!(
                "{} cannot go on with what was said before: it begins anew.",
                self.known.name
            );
            self.floor.notice(run, &words).await?;
        }
        self.setup.began = true;
        self.floor.began().await;
        let slot = self.slot.clone();
        slot.stays.send_modify(|n| *n += 1);
        tokio::spawn(async move {
            attempt.stay(rx).await;
            slot.stays.send_modify(|n| *n -= 1);
        });
        *self.slot.attempt.lock().await = Some(tx.clone());
        Ok(tx)
    }

    async fn ask(
        &self,
        ask: &Ask,
        what: &str,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<(RequestId, Unsandboxed), Halt> {
        let mut open = self.desk.open(ask).map_err(|e| Halt::Failed(e.to_string()))?;
        let id = open.id.clone();
        self.floor.say(opened(&self.floor, &id, ask, what)).await?;
        self.floor.sync().await?;
        let answer = tokio::select! {
            biased;
            () = stopped(cancel) => None,
            () = self.floor.closed() => return Err(Halt::Gone),
            a = open.wait() => Some(a),
        };
        let token = answer.flatten().and_then(|a| a.unsandboxed(ask));
        let words = match &token {
            Some(_) => "allowed",
            None if *cancel.borrow() => "expired",
            None => "denied",
        };
        self.floor
            .say(vec![
                Say::Event(Body::RequestClosed { request: id.clone(), answer: words.into() }),
                state(&self.floor, AgentState::Running),
            ])
            .await?;
        match token {
            Some(t) => Ok((id, t)),
            None if *cancel.borrow() => Err(Halt::Cancelled),
            None => {
                Err(Halt::Failed(format!("a person did not allow starting {}", self.known.name)))
            }
        }
    }
}

/// Frames go to the journal before anything is made of them.
struct Record(Floor);
impl Frames for Record {
    async fn keep(&self, f: Frame) -> Result<(), Lost> {
        let way = if f.way == axiom_harness::Way::In { "in" } else { "out" };
        let body = json!({"way": way, "bytes": f.bytes, "text": f.text}).to_string();
        let (s, a) = (self.0.session.id.clone(), self.0.agent.clone());
        self.0.store.put_frames(s, a, vec![(f.at, body)]).await.map_err(|_| Lost)
    }
}

/// What a run hands to its program.
pub struct Turn {
    run: RunId,
    inbox: mpsc::Receiver<Word>,
    words: Vec<(ItemId, String)>,
    cancel: watch::Receiver<bool>,
    done: oneshot::Sender<(Ended, mpsc::Receiver<Word>)>,
}

/// A program that runs, and what it says.
struct Attempt {
    floor: Floor,
    desk: axiom_engine::Desk,
    name: &'static str,
    id: &'static str,
    workspace: String,
    handle: Handle,
    events: Events,
    containment: Containment,
    /// Its tool calls are shown where they are done: by the host itself.
    hands: bool,
    _grant: Option<Grant>,
    ready: bool,
    resumes: bool,
    run: Option<RunId>,
    /// By the program's own names.
    items: HashMap<String, Open>,
    asked: JoinSet<(String, RequestId, Ask, Option<Approval>)>,
    questions: HashMap<String, (AbortHandle, RequestId, Option<String>)>,
    slot: Arc<Slot>,
    model: Option<String>,
    plan: Option<String>,
    limit: Option<String>,
    /// A turn that ended before it was followed.
    early: Option<Done>,
}

/// An item that is open on the transcript.
struct Open {
    item: ItemId,
    text: String,
    /// Text of it is not kept twice beyond this.
    long: bool,
    prose: bool,
    diffs: Vec<Diff>,
}

/// Of one item's text, held to tell what is new when all of it is said again.
const HELD: usize = 256 * 1024;

impl Attempt {
    /// Until nothing more is wanted of it.
    async fn stay(mut self, mut turns: mpsc::Receiver<Turn>) {
        let mut gone = false;
        while !gone {
            tokio::select! {
                t = turns.recv() => match t {
                    Some(t) => gone = self.turn(t).await,
                    None => break,
                },
                e = self.events.next() => match e {
                    Some(Event::Closed(_)) | None => gone = true,
                    // It works by itself, outside any run a person could stop: it does not.
                    Some(Event::Began { by_agent: true }) => {
                        let _ = self.handle.interrupt().await;
                    }
                    Some(e) => gone = self.tell(e).await.is_err(),
                },
                () = tokio::time::sleep(IDLE) => break,
                () = self.floor.closed() => {
                    let _ = self.handle.cancel().await;
                    break;
                }
            }
        }
        // Whoever comes too late finds nobody, and starts it anew.
        turns.close();
        let _ = self.handle.shut().await;
        // It ends, and all it runs; if it does not, the driver ends it.
        while let Some(e) = self.events.next().await {
            if matches!(e, Event::Closed(_)) {
                break;
            }
        }
    }

    /// One run. Whether the program is gone after it.
    async fn turn(&mut self, t: Turn) -> bool {
        let Turn { run, mut inbox, words, mut cancel, done } = t;
        self.run = Some(run.clone());
        self.slot.said.lock().unwrap_or_else(PoisonError::into_inner).clear();
        let r = self.work(&mut inbox, words, &mut cancel).await;
        self.asked.abort_all();
        while self.asked.join_next().await.is_some() {}
        let (outcome, error, gone) = match r {
            Ok(d) => (d.outcome, d.error, false),
            // Whether it still works on it is not known: it is ended.
            Err(Stop::Failed(why)) => (Outcome::Failed, Some(why), true),
            Err(Stop::Closed) => {
                let _ = done.send((Ended { outcome: Outcome::Interrupted, error: None }, inbox));
                let _ = self.handle.cancel().await;
                return true;
            }
            Err(Stop::Gone(g)) => {
                let stopped = *cancel.borrow();
                let outcome = match g.why {
                    _ if stopped => Outcome::Cancelled,
                    Why::Asked => Outcome::Interrupted,
                    _ => Outcome::Failed,
                };
                (outcome, if stopped { None } else { g.words.clone() }, true)
            }
        };
        // Stopped by a person is stopped, whatever it calls it.
        let outcome = match outcome {
            Outcome::Completed | Outcome::Failed if *cancel.borrow() => Outcome::Cancelled,
            Outcome::Unknown => Outcome::Failed,
            o => o,
        };
        let error = error.filter(|_| outcome == Outcome::Failed).or_else(|| {
            (outcome == Outcome::Failed)
                .then(|| format!("{} stopped and did not say why", self.name))
        });
        let mut say = self.close(outcome);
        say.extend(ended(&self.floor, &run, outcome, error.clone()));
        if self.floor.say(say).await.is_ok() {
            let _ = self.floor.sync().await;
        }
        self.run = None;
        let _ = done.send((Ended { outcome, error }, inbox));
        gone
    }

    async fn work(
        &mut self,
        inbox: &mut mpsc::Receiver<Word>,
        mut words: Vec<(ItemId, String)>,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<Done, Stop> {
        let mut noted: HashSet<ItemId> = words.iter().map(|w| w.0.clone()).collect();
        self.opened(cancel).await?;
        let mut done = Done { outcome: Outcome::Completed, stop: String::new(), error: None };
        while !words.is_empty() && done.outcome == Outcome::Completed && !*cancel.borrow() {
            while let Ok(w) = inbox.try_recv() {
                if let Some(item) = w.item
                    && noted.insert(item.clone())
                {
                    words.push((item, w.text));
                }
            }
            match self.give(&mut words).await? {
                Given::Taken => {}
                // All of it was taken back: there is nothing to wait for.
                Given::Nothing => continue,
                Given::NotNow => {
                    return Err(Stop::Failed(format!("{} takes nothing now", self.name)));
                }
            }
            done = self.until_done(inbox, &mut words, &mut noted, cancel).await?;
        }
        Ok(done)
    }

    /// Until its session is there.
    async fn opened(&mut self, cancel: &mut watch::Receiver<bool>) -> Result<(), Stop> {
        while !self.ready {
            let e = tokio::select! {
                biased;
                () = stopped(cancel) => {
                    let _ = self.handle.cancel().await;
                    return Err(Stop::Failed("it was stopped before it was ready".into()));
                }
                () = self.floor.closed() => return Err(Stop::Closed),
                e = self.events.next() => e,
            };
            match e {
                None => return Err(Stop::Failed(format!("{} ended", self.name))),
                Some(Event::Closed(g)) => return Err(Stop::Gone(g)),
                Some(e) => self.tell(e).await?,
            }
        }
        Ok(())
    }

    /// Tells it what waits. What it is told waits no longer before it is told: a person sees that it
    /// was not answered, and nothing is done twice.
    async fn give(&mut self, words: &mut Vec<(ItemId, String)>) -> Result<Given, Stop> {
        let items: Vec<ItemId> = words.iter().map(|w| w.0.clone()).collect();
        // What has been taken back meanwhile is not told.
        let waited = self.floor.heard(items).await?;
        words.retain(|w| waited.contains(&w.0));
        if words.is_empty() {
            return Ok(Given::Nothing);
        }
        let text: Vec<&str> = words.iter().map(|w| w.1.as_str()).collect();
        let text = text.join("\n\n");
        let handle = self.handle.clone();
        // Whatever another agent has a part in is not a person's own words.
        let by =
            if words.iter().any(|w| w.1.starts_with(FROM_AGENT)) { By::Agent } else { By::Person };
        let prompt = handle.prompt(&text, by);
        tokio::pin!(prompt);
        // What it says while it takes it is told meanwhile, or it would wait for us and we for it.
        let r = loop {
            tokio::select! {
                biased;
                r = &mut prompt => break r,
                e = self.events.next() => match e {
                    None => return Err(Stop::Failed(format!("{} ended", self.name))),
                    Some(Event::Closed(g)) => return Err(Stop::Gone(g)),
                    Some(Event::Done(d)) => self.early = Some(d),
                    Some(e) => self.tell(e).await?,
                },
            }
        };
        match r {
            Ok(_) => {
                words.clear();
                Ok(Given::Taken)
            }
            Err(e) => {
                for (item, text) in words.iter() {
                    self.floor.wait(item, text).await?;
                }
                match e {
                    axiom_harness::Refused::Busy | axiom_harness::Refused::NotReady => {
                        Ok(Given::NotNow)
                    }
                    e => Err(Stop::Failed(format!("{} did not take it: {e}", self.name))),
                }
            }
        }
    }

    /// Follows a turn to its end.
    async fn until_done(
        &mut self,
        inbox: &mut mpsc::Receiver<Word>,
        words: &mut Vec<(ItemId, String)>,
        noted: &mut HashSet<ItemId>,
        cancel: &mut watch::Receiver<bool>,
    ) -> Result<Done, Stop> {
        let mut stopping = false;
        loop {
            if let Some(d) = self.early.take() {
                return Ok(d);
            }
            tokio::select! {
                biased;
                () = stopped(cancel), if !stopping => {
                    stopping = true;
                    // If it does not end its turn in time, it is ended whole.
                    let _ = self.handle.interrupt().await;
                }
                () = self.floor.closed() => return Err(Stop::Closed),
                e = self.events.next() => match e {
                    None => return Err(Stop::Failed(format!("{} ended", self.name))),
                    Some(Event::Closed(g)) => return Err(Stop::Gone(g)),
                    Some(Event::Done(d)) => return Ok(d),
                    Some(e) => self.tell(e).await?,
                },
                Some(w) = inbox.recv(), if !stopping && words.is_empty() => {
                    let Some(item) = w.item else { continue };
                    if !noted.insert(item.clone()) {
                        continue;
                    }
                    words.push((item, w.text));
                    // What it cannot hear now it is told after this turn.
                    self.give(words).await?;
                }
                Some(a) = self.asked.join_next(), if !self.asked.is_empty() => {
                    if let Ok((q, id, ask, approval)) = a {
                        self.answer(q, id, &ask, approval).await?;
                    }
                }
            }
        }
    }

    /// A person's answer goes to who asked.
    async fn answer(
        &mut self,
        q: String,
        id: RequestId,
        ask: &Ask,
        approval: Option<Approval>,
    ) -> Result<(), Stop> {
        let Some((_, _, yes)) = self.questions.remove(&q) else { return Ok(()) };
        let allowed = approval.and_then(|a| a.unsandboxed(ask)).is_some();
        let (a, words) = match yes.filter(|_| allowed) {
            Some(option) => (Answer::Choose(option), "allowed"),
            None => (Answer::Refuse, "denied"),
        };
        if allowed {
            let deed = Deed {
                chain: ask.chain.clone(),
                workspace: ask.workspace.clone(),
                tool: ask.tool.clone(),
                subject: ask.subject.clone(),
                decision: Decision::Ask,
                rules: vec![],
                answered: Some((id.clone(), ask.digest())),
            };
            self.floor.record(deed, self.containment).await?;
        }
        // It may have stopped waiting this moment.
        let heard = self.handle.answer(&q, a).await.is_ok();
        let words = if heard { words } else { "expired" };
        let mut say = vec![Say::Event(Body::RequestClosed { request: id, answer: words.into() })];
        if self.questions.is_empty() {
            say.push(state(&self.floor, AgentState::Running));
        }
        Ok(self.floor.say(say).await?)
    }

    /// Tells the session what the program said.
    async fn tell(&mut self, e: Event) -> Result<(), Stop> {
        match e {
            Event::Ready(r) => self.is_ready(*r).await?,
            Event::Session(s) if self.resumes => self.floor.resume(Some(s)).await,
            Event::Say(s) => {
                let kind = match s.kind {
                    Kind::Prose => ItemKind::Prose,
                    Kind::Reasoning => ItemKind::Reasoning,
                };
                self.text(&s.id, kind, &s.text, s.whole).await?;
            }
            Event::Said { id } => {
                if let Some(o) = self.items.remove(&id) {
                    if o.prose && !o.text.is_empty() {
                        let mut said =
                            self.slot.said.lock().unwrap_or_else(PoisonError::into_inner);
                        said.clone_from(&o.text);
                    }
                    self.floor.say(vec![Say::End(o.item, Outcome::Completed, None)]).await?;
                }
            }
            Event::ToolStarted(t)
                if self.hands && t.name.starts_with(&format!("mcp__{SERVER}__")) => {}
            Event::ToolStarted(t) => {
                let t = *t;
                let name = [t.title.as_deref().unwrap_or_default(), &t.name];
                let name = name.into_iter().find(|n| !n.is_empty()).unwrap_or("tool");
                let mut name = if t.name.is_empty() { name.to_owned() } else { t.name.clone() };
                limits::clip(&mut name, 200);
                let args = t.args.or_else(|| t.title.as_ref().map(|t| json!({ "title": t })));
                let item = ItemId::new();
                self.begin_item(&item, ItemKind::Tool, name, args, t.args_cut).await?;
                let o =
                    Open { item, text: String::new(), long: false, prose: false, diffs: t.diffs };
                if let Some(old) = self.items.insert(t.id, o) {
                    let end = Say::End(old.item, Outcome::Interrupted, None);
                    self.floor.say(vec![end]).await?;
                }
            }
            Event::ToolUpdated(t) => {
                if let Some(o) = self.items.get_mut(&t.id)
                    && !t.diffs.is_empty()
                {
                    o.diffs = t.diffs;
                }
            }
            Event::ToolOutput { id, text, whole, cut } => {
                let text = if cut > 0 { format!("{text}\n[{cut} bytes more]") } else { text };
                if self.items.contains_key(&id) {
                    self.text(&id, ItemKind::Tool, &text, whole).await?;
                }
            }
            Event::ToolEnded(t) => {
                if let Some(text) = t.output.as_deref().filter(|_| self.items.contains_key(&t.id)) {
                    self.text(&t.id, ItemKind::Tool, text, true).await?;
                }
                if let Some(o) = self.items.remove(&t.id) {
                    let diffs = if t.diffs.is_empty() { o.diffs } else { t.diffs };
                    let result = result(&diffs, t.error.as_deref());
                    self.floor.say(vec![Say::End(o.item, t.outcome, result)]).await?;
                }
            }
            Event::Asked(q) => self.question(*q).await?,
            Event::AskClosed { id, how } => {
                if how != Settled::Answered
                    && let Some((task, request, _)) = self.questions.remove(&id)
                {
                    task.abort();
                    let answer = if how == Settled::TimedOut { "denied" } else { "expired" };
                    let mut say =
                        vec![Say::Event(Body::RequestClosed { request, answer: answer.into() })];
                    if self.questions.is_empty() && self.run.is_some() {
                        say.push(state(&self.floor, AgentState::Running));
                    }
                    self.floor.say(say).await?;
                }
            }
            Event::Usage(s) => {
                let share = s
                    .context
                    .and_then(|(n, of)| (of > 0).then(|| (n as f64 / of as f64).clamp(0.0, 1.0)));
                let usage = Usage { context: share.or(s.usage.context), ..s.usage };
                let provenance = s.provenance.unwrap_or(Provenance::Reported);
                let agent = self.floor.agent.clone();
                let body = Body::Usage { agent, usage, provenance };
                self.floor.say(vec![Say::Event(body)]).await?;
            }
            Event::Plan(p) => {
                let mark = |p| match p {
                    axiom_harness::Progress::Done => "[x]",
                    axiom_harness::Progress::Doing => "[~]",
                    _ => "[ ]",
                };
                let steps: Vec<String> =
                    p.steps.iter().map(|(t, s)| format!("{} {t}", mark(*s))).collect();
                let text = p.text.unwrap_or_default() + &steps.join("\n");
                if !text.is_empty() && Some(&text) != self.plan.as_ref() {
                    self.notice(&format!("Its plan:\n{text}")).await?;
                    self.plan = Some(text);
                }
            }
            Event::Notice { level, text } if level != Level::Info => self.notice(&text).await?,
            Event::NotHeard { why } => {
                self.notice(&format!("What was said was not taken: {why}")).await?;
            }
            Event::Limit(l) => {
                let Some(used) = l.used.filter(|u| *u >= 0.9) else { return Ok(()) };
                let words = format!("{}: {:.0} % of {} is used.", self.name, used * 100.0, l.name);
                if self.limit.as_ref() != Some(&words) {
                    self.notice(&words).await?;
                    self.limit = Some(words);
                }
            }
            Event::Began { .. }
            | Event::Session(_)
            | Event::Notice { .. }
            | Event::Unknown { .. }
            | Event::Done(_)
            | Event::Closed(_) => {}
        }
        Ok(())
    }

    async fn is_ready(&mut self, r: Ready) -> Result<(), Stop> {
        self.ready = true;
        self.resumes = r.caps.resume;
        // Otherwise it begins anew when its program has ended, and says so.
        self.floor.resume(r.caps.resume.then_some(r.session)).await;
        if let (Some(is), Some(asked)) = (&r.model, &self.model)
            && is != asked
        {
            self.notice(&format!("{} says it runs on {is}, not {asked}.", self.name)).await?;
        }
        Ok(())
    }

    async fn begin_item(
        &mut self,
        item: &ItemId,
        kind: ItemKind,
        name: String,
        args: Option<Value>,
        args_cut: u64,
    ) -> Result<(), Stop> {
        // Outside a run nothing is shown: there is nothing it would be part of.
        let Some(run) = self.run.clone() else { return Ok(()) };
        let (item, agent) = (item.clone(), self.floor.agent.clone());
        let body = Body::ItemStarted { item, run, agent, kind, name, args, args_cut };
        Ok(self.floor.say(vec![Say::Event(body)]).await?)
    }

    /// More of an item's text, or all of it anew: then only what is new is said, and if it is another
    /// text altogether, in another item.
    async fn text(
        &mut self,
        id: &str,
        kind: ItemKind,
        text: &str,
        whole: bool,
    ) -> Result<(), Stop> {
        if self.run.is_none() {
            return Ok(());
        }
        let mut new = text;
        if let Some(o) = self.items.get(id).filter(|_| whole) {
            match text.strip_prefix(o.text.as_str()).filter(|_| !o.long) {
                Some(rest) => new = rest,
                // A tool's item cannot begin again; what it said stays, and what it says now is added.
                None if kind == ItemKind::Tool => {
                    if o.long {
                        return Ok(());
                    }
                    let item = o.item.clone();
                    let line = Say::Text(item, "\n---\n".into());
                    self.floor.say(vec![line]).await?;
                }
                None => {
                    let end = Say::End(o.item.clone(), Outcome::Completed, None);
                    self.items.remove(id);
                    self.floor.say(vec![end]).await?;
                }
            }
        }
        if !self.items.contains_key(id) {
            let item = ItemId::new();
            self.begin_item(&item, kind, String::new(), None, 0).await?;
            let prose = kind == ItemKind::Prose;
            let o = Open { item, text: String::new(), long: false, prose, diffs: vec![] };
            self.items.insert(id.to_owned(), o);
        }
        let Some(o) = self.items.get_mut(id) else { return Ok(()) };
        if whole {
            o.text.clear();
            o.text.push_str(text);
        } else {
            o.text.push_str(text);
        }
        if o.text.len() > HELD {
            o.long = true;
            o.text = String::new();
        }
        if !new.is_empty() {
            let say = Say::Text(o.item.clone(), new.to_owned());
            self.floor.say(vec![say]).await?;
        }
        Ok(())
    }

    async fn notice(&mut self, text: &str) -> Result<(), Stop> {
        let item = ItemId::new();
        if self.run.is_none() {
            return Ok(());
        }
        self.begin_item(&item, ItemKind::Notice, String::new(), None, 0).await?;
        Ok(self.floor.say(noted(item, text)).await?)
    }

    /// What it asks goes to a person, whole, and once: what "always" would be is the program's own.
    async fn question(&mut self, q: Question) -> Result<(), Stop> {
        let yes = q.options.iter().find(|o| o.kind == ChoiceKind::AllowOnce).map(|o| o.id.clone());
        let (Some(yes), true) = (yes, self.run.is_some()) else {
            let _ = self.handle.answer(&q.id, Answer::Refuse).await;
            let words = format!("Not allowed, it cannot be allowed once: {}", q.title);
            return self.notice(&words).await;
        };
        let mut subject = q.title.clone();
        for part in [q.detail.as_deref().unwrap_or_default(), &pretty(&q.subject)] {
            if !part.is_empty() {
                subject.push('\n');
                subject.push_str(part);
            }
        }
        let ask = Ask {
            chain: vec![self.floor.agent.clone()],
            workspace: self.workspace.clone(),
            tool: format!("{}/{}", self.id, q.tool.as_deref().unwrap_or("tool")),
            schema: String::new(),
            subject,
            scope: format!(
                "done by {}, outside Axiom's sandbox{}",
                self.name,
                q.by.as_ref().map(|b| format!("; asked by its agent {b}")).unwrap_or_default()
            ),
            unsandboxed: true,
        };
        let refuse = |why: String| format!("Not allowed, {why}: {}", q.title);
        let mut open = match self.desk.open(&ask) {
            Ok(o) => o,
            Err(e) => {
                let _ = self.handle.answer(&q.id, Answer::Refuse).await;
                return self.notice(&refuse(e.to_string())).await;
            }
        };
        let id = open.id.clone();
        let what = match q.tool.as_deref() {
            Some(t) if t.len() <= 80 => format!("({}) asks to use {t}", self.name),
            _ => format!("({}) asks", self.name),
        };
        match self.floor.say(opened(&self.floor, &id, &ask, &what)).await {
            Ok(()) => {}
            Err(Refused::Closed) => return Err(Stop::Closed),
            Err(Refused::This(why)) => {
                let _ = self.handle.answer(&q.id, Answer::Refuse).await;
                return self.notice(&refuse(why)).await;
            }
        }
        let (name, request) = (q.id.clone(), id.clone());
        let task = self.asked.spawn(async move {
            let a = open.wait().await;
            (name, request, ask, a)
        });
        self.questions.insert(q.id, (task, id, Some(yes)));
        Ok(())
    }

    /// Ends what is open on the transcript.
    fn close(&mut self, outcome: Outcome) -> Vec<Say> {
        let outcome = if outcome == Outcome::Completed { Outcome::Interrupted } else { outcome };
        let mut say: Vec<Say> =
            self.items.drain().map(|(_, o)| Say::End(o.item, outcome, None)).collect();
        for (_, (task, request, _)) in self.questions.drain() {
            task.abort();
            say.push(Say::Event(Body::RequestClosed { request, answer: "expired".into() }));
        }
        say
    }
}

/// What came of telling a program what waits.
enum Given {
    Taken,
    NotNow,
    /// Nothing waited any more.
    Nothing,
}

/// Why a turn is not followed to its end.
enum Stop {
    Failed(String),
    /// Its session is closed.
    Closed,
    Gone(Box<Gone>),
}
impl From<Refused> for Stop {
    fn from(r: Refused) -> Self {
        match r {
            Refused::Closed => Stop::Closed,
            Refused::This(why) => Stop::Failed(why),
        }
    }
}

fn noted(item: ItemId, text: &str) -> Vec<Say> {
    vec![Say::Text(item.clone(), text.to_owned()), Say::End(item, Outcome::Completed, None)]
}

impl Floor {
    async fn notice(&self, run: &RunId, text: &str) -> Result<(), Refused> {
        let item = ItemId::new();
        let body = Body::ItemStarted {
            item: item.clone(),
            run: run.clone(),
            agent: self.agent.clone(),
            kind: ItemKind::Notice,
            name: String::new(),
            args: None,
            args_cut: 0,
        };
        let mut say = vec![Say::Event(body)];
        say.extend(noted(item, text));
        self.say(say).await
    }
}

async fn stopped(c: &mut watch::Receiver<bool>) {
    if c.wait_for(|c| *c).await.is_err() {
        // Nobody can stop it any more.
        std::future::pending::<()>().await;
    }
}

fn state(f: &Floor, state: AgentState) -> Say {
    Say::Event(Body::AgentState { agent: f.agent.clone(), state })
}

fn begun(f: &Floor, run: &RunId) -> Vec<Say> {
    vec![
        Say::Event(Body::RunStarted { run: run.clone(), agent: f.agent.clone() }),
        state(f, AgentState::Running),
    ]
}

fn ended(f: &Floor, run: &RunId, outcome: Outcome, error: Option<String>) -> Vec<Say> {
    let now = match outcome {
        Outcome::Completed => AgentState::Done,
        Outcome::Failed => AgentState::Failed,
        _ => AgentState::Paused,
    };
    vec![Say::Event(Body::RunEnded { run: run.clone(), outcome, error }), state(f, now)]
}

fn opened(f: &Floor, id: &RequestId, ask: &Ask, what: &str) -> Vec<Say> {
    vec![
        Say::Event(Body::RequestOpened {
            request: id.clone(),
            agent: f.agent.clone(),
            what: what.into(),
            subject: ask.subject.clone(),
            scope: ask.scope.clone(),
            answerable: Answerable::Live,
            digest: ask.digest(),
            item: None,
            session: false,
        }),
        state(f, AgentState::Asking),
    ]
}

/// As a person would write it in a shell.
fn command(program: &Place, args: &[String]) -> String {
    let quoted = |a: &str| {
        let plain = |c: char| c.is_ascii_alphanumeric() || "-_=./,:@%+".contains(c);
        if !a.is_empty() && a.chars().all(plain) {
            a.to_owned()
        } else {
            format!("'{}'", a.replace('\'', "'\\''"))
        }
    };
    std::iter::once(program.as_str())
        .chain(args.iter().map(String::as_str))
        .map(quoted)
        .collect::<Vec<_>>()
        .join(" ")
}

fn scope(k: &Known, c: Containment, trust: bool, account: Option<&Place>, hands: bool) -> String {
    let held = match c {
        _ if hands => {
            "it has no tools of its own: what it does, it does with Axiom's, judged, asked \
             about and contained as Axiom's own agents are"
                .into()
        }
        Containment::Harness => format!("{} says its own sandbox holds it back", k.name),
        _ => "nothing holds it back: it can do whatever you can".into(),
    };
    let folder = if trust && !hands {
        "The folder's own settings for it take effect."
    } else {
        "The folder's own settings for it are ignored."
    };
    let account = account.map(|a| format!(" Its account is kept in {a}.")).unwrap_or_default();
    format!(
        "Outside Axiom's sandbox; {held}. It signs in by itself, and reaches the network. {folder}{account}"
    )
}

fn pretty(v: &Value) -> String {
    match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        v => serde_json::to_string_pretty(v).unwrap_or_default(),
    }
}

/// What a tool did, for a person: reported, not proven.
fn result(diffs: &[Diff], error: Option<&str>) -> Option<Value> {
    let mut out = serde_json::Map::new();
    if let Some(e) = error {
        out.insert("error".into(), json!(e));
    }
    let mut text = String::new();
    for d in diffs {
        match (&d.patch, &d.old, &d.new) {
            (Some(p), ..) => {
                text.push_str(p);
                if !text.ends_with('\n') {
                    text.push('\n');
                }
            }
            (None, None, None) => text.push_str(&format!("changed {}\n", d.path)),
            (None, old, new) => {
                text.push_str(&axiom_tools::text::diff(&d.path, old.as_deref(), new.as_deref()));
            }
        }
        if d.cut > 0 {
            text.push_str(&format!("[{} bytes more]\n", d.cut));
        }
    }
    if !text.is_empty() {
        out.insert("diff".into(), json!(text));
    }
    (!out.is_empty()).then_some(Value::Object(out))
}
