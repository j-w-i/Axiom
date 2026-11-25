//! One run: from what a person said until the agent has nothing more to do.

use std::{
    collections::HashSet,
    time::{Duration, Instant},
};

use axiom_model::{
    Accumulator, Args, Block, CallError, Class, Client, Content, Delta, Event, Message, Reply,
    Request, Retry, Role, Settings, Start, Stop, Tool,
};
use axiom_proto::{AgentId, AgentState, Body, ItemId, ItemKind, Outcome, Provenance, RunId, Usage};
use axiom_tools::{Seen, Site, Spec};
use serde_json::Value;
use tokio::sync::{mpsc, watch};

use crate::{
    Desk, Refused, Say, Stage, compact,
    context::{Context, Turn},
    hands::{Done, Hands, begins},
};

/// How far a run may go.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// Answers of the model.
    pub steps: u32,
    pub time: Duration,
    /// Of all answers together, as the provider counts them.
    pub output_tokens: Option<u64>,
}
impl Default for Limits {
    fn default() -> Self {
        Self { steps: 200, time: Duration::from_secs(4 * 3600), output_tokens: None }
    }
}

/// How a failed request is tried again.
#[derive(Debug, Clone, Copy)]
pub struct Retries {
    pub most: u32,
    pub first: Duration,
    /// Longer than this is not waited, also if the provider asks for it.
    pub longest: Duration,
}
impl Default for Retries {
    fn default() -> Self {
        Self { most: 6, first: Duration::from_secs(1), longest: Duration::from_secs(120) }
    }
}

/// What a person said to an agent that is at work.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Word {
    pub text: String,
    /// Where it is in the journal already, if it is: shown to who reads along, and waiting to be
    /// heard.
    pub item: Option<ItemId>,
    /// If another agent says it: its name, and what is shown.
    pub shown: Option<(String, String)>,
}

impl Word {
    /// A person's, not said to the session yet.
    pub fn new(text: String) -> Self {
        Self { text, item: None, shown: None }
    }
}

/// An agent, ready to run.
pub struct Agent<S: Stage> {
    pub id: AgentId,
    pub stage: S,
    pub desk: Desk,
    pub client: Client,
    pub model: String,
    pub settings: Settings,
    /// Tokens the model takes, if they are known.
    pub window: Option<u64>,
    pub site: Site,
    pub seen: Seen,
    pub context: Context,
    pub specs: Vec<Spec>,
    /// Said by the person of this agent's work, beside what Axiom says.
    pub instructions: String,
    /// Files of the workspace that were to be followed and are not what the person read.
    pub not_followed: Vec<String>,
    pub limits: Limits,
    pub retries: Retries,
    /// What a person says meanwhile.
    pub inbox: mpsc::Receiver<Word>,
    /// Taken in from what waited, or taken back by then: it may be in the inbox as well.
    pub noted: HashSet<ItemId>,
    /// The next run only makes room, because a person asks for it.
    pub compact: bool,
    /// Share of the window in use, as the last answer had it.
    pub fill: Option<f64>,
    /// Another model that answered, once a person has been told.
    pub told: Option<String>,
    /// How long this run has waited for a person: that is not its own time.
    pub waited: Duration,
    /// The last words of the answer that is being given or was given last, as they are shown.
    pub last_words: Option<ItemId>,
}

fn fill(r: &Reply, window: Option<u64>) -> Option<f64> {
    let (p, o) = r.usage.prompt.zip(r.usage.output)?;
    let w = window.filter(|w| *w > 0)?;
    Some((p + o) as f64 / w as f64)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ended {
    pub outcome: Outcome,
    pub error: Option<String>,
}

/// For a call that was stopped while a person was asked: nothing of it was begun.
pub(crate) const ASKED: &str = "Not done: the run was stopped while a person was being asked.";

/// The events of something a person said.
/// `by`: the name of the agent that says it, if it is not a person.
pub fn said(item: &ItemId, run: &RunId, agent: &AgentId, text: &str, by: &str) -> Vec<Say> {
    let item = item.clone();
    vec![
        Say::Event(Body::ItemStarted {
            item: item.clone(),
            run: run.clone(),
            agent: agent.clone(),
            kind: ItemKind::Prompt,
            name: by.to_owned(),
            args: None,
            args_cut: 0,
        }),
        Say::Text(item.clone(), text.to_owned()),
        Say::End(item, Outcome::Completed, None),
    ]
}

/// Why a run stops before the agent is done.
pub(crate) enum Halt {
    Cancelled,
    /// Cancelled before anything of the call was begun.
    Unbegun,
    Failed(String),
    /// What is sent is too long for the model.
    Full(String),
    /// Nobody to say anything to.
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

pub(crate) type Cancel = watch::Receiver<bool>;

pub(crate) async fn stopped(c: &mut Cancel) {
    if c.wait_for(|c| *c).await.is_err() {
        // Nobody can cancel it any more.
        std::future::pending::<()>().await;
    }
}

impl<S: Stage> Agent<S> {
    /// Begins a run, with what a person said if it is not waiting in the conversation already. It is
    /// in the journal for good when this returns; if it cannot begin, it has ended.
    /// `before`: what it is told with it and has been taken in: said first.
    pub async fn begin(
        &mut self,
        run: &RunId,
        before: Vec<Say>,
        prompt: Option<Word>,
    ) -> Result<(), Refused> {
        self.stage
            .say(vec![
                Say::Event(Body::RunStarted { run: run.clone(), agent: self.id.clone() }),
                Say::Event(Body::AgentState { agent: self.id.clone(), state: AgentState::Running }),
            ])
            .await?;
        let r = match self.opening(run, before, prompt).await {
            Ok(()) => return Ok(()),
            Err(Halt::Failed(why) | Halt::Full(why)) => Refused::This(why),
            Err(Halt::Cancelled | Halt::Unbegun | Halt::Gone) => Refused::Closed,
        };
        let error = Some(r.to_string());
        let end = vec![
            Say::Event(Body::RunEnded { run: run.clone(), outcome: Outcome::Failed, error }),
            Say::Event(Body::AgentState { agent: self.id.clone(), state: AgentState::Failed }),
        ];
        let _ = self.stage.say(end).await;
        Err(r)
    }

    async fn opening(
        &mut self,
        run: &RunId,
        before: Vec<Say>,
        prompt: Option<Word>,
    ) -> Result<(), Halt> {
        if !before.is_empty() {
            self.stage.say(before).await?;
        }
        if let Some(w) = prompt {
            self.heard(run, w).await?;
        }
        for f in std::mem::take(&mut self.not_followed) {
            let words = format!(
                "{f} is not what you read when you said it should be followed. It is not followed."
            );
            self.notice(run, &words).await?;
        }
        Ok(self.stage.sync().await?)
    }

    /// Runs until the agent is done, cancelled, or cannot go on. How it ended is in the journal when
    /// this returns.
    pub async fn run(&mut self, run: RunId, mut cancel: Cancel) -> Ended {
        let r = self.go(&run, &mut cancel).await;
        // Who stops an agent stops all of it (SI-24). Otherwise what goes on is said.
        match r {
            Err(Halt::Cancelled | Halt::Unbegun) => self.seen.1.end(),
            _ => {
                let jobs = self.seen.1.running();
                if !jobs.is_empty() {
                    let words = format!("Still running in the background: {}", jobs.join("; "));
                    let _ = self.notice(&run, &words).await;
                }
            }
        }
        let (outcome, error) = match r {
            Ok(()) => (Outcome::Completed, None),
            Err(Halt::Cancelled | Halt::Unbegun) => (Outcome::Cancelled, None),
            Err(Halt::Failed(why) | Halt::Full(why)) => (Outcome::Failed, Some(why)),
            Err(Halt::Gone) => return Ended { outcome: Outcome::Interrupted, error: None },
        };
        let state = match outcome {
            Outcome::Completed => AgentState::Done,
            Outcome::Failed => AgentState::Failed,
            _ => AgentState::Paused,
        };
        let say = vec![
            Say::Event(Body::RunEnded { run, outcome, error: error.clone() }),
            Say::Event(Body::AgentState { agent: self.id.clone(), state }),
        ];
        if self.stage.say(say).await.is_ok() {
            let _ = self.stage.sync().await;
        }
        Ended { outcome, error }
    }

    async fn go(&mut self, run: &RunId, cancel: &mut Cancel) -> Result<(), Halt> {
        let began = Instant::now();
        let (mut output, mut unread) = (0, 0);
        self.waited = Duration::ZERO;
        if std::mem::take(&mut self.compact) {
            if self.context.messages().len() < 2 {
                return Err(Halt::Failed("there is nothing to summarise yet".into()));
            }
            return self.make_room(run, cancel, "a person asked for it".into()).await;
        }
        for _ in 0..self.limits.steps {
            if *cancel.borrow() {
                return Err(Halt::Cancelled);
            }
            if self.fill.is_some_and(|f| f >= compact::FULL) {
                self.make_room(run, cancel, "the conversation fills the model's window".into())
                    .await?;
            }
            let mut more = Vec::new();
            while let Ok(m) = self.inbox.try_recv() {
                more.push(m);
            }
            for m in more {
                self.heard(run, m).await?;
            }
            if began.elapsed().saturating_sub(self.waited) > self.limits.time {
                return Err(Halt::Failed("the run has taken as long as it may".into()));
            }
            if self.limits.output_tokens.is_some_and(|most| output >= most) {
                return Err(Halt::Failed("the run has used as many tokens as it may".into()));
            }
            let mut reply = match self.answer(run, cancel).await {
                Err(Halt::Full(words)) => {
                    // Once: what is too long after that stays so.
                    self.make_room(run, cancel, words).await?;
                    self.answer(run, cancel).await.map_err(|e| match e {
                        Halt::Full(words) => Halt::Failed(words),
                        e => e,
                    })?
                }
                r => r?,
            };
            output += reply.usage.output.unwrap_or(0);
            self.compare(run, &reply).await?;
            self.usage(&reply).await?;
            self.fill = fill(&reply, self.window);
            if reply.stop == Some(Stop::ContextFull) {
                // What it had said is not whole; it says it again, from its notes.
                self.make_room(run, cancel, "the model's context is full".into()).await?;
                reply = self.answer(run, cancel).await.map_err(|e| match e {
                    Halt::Full(words) => Halt::Failed(words),
                    e => e,
                })?;
                self.usage(&reply).await?;
                self.fill = fill(&reply, self.window);
            }
            let stop = reply.stop.clone();
            let message = reply.message;
            self.remember(Turn::Message(message.clone())).await?;
            let ready = message.calls().next().is_some();
            let has_calls =
                message.blocks.iter().any(|b| matches!(b.content, Content::ToolCall(_)));
            let ended = matches!(stop, Some(Stop::Refusal(_) | Stop::Other(_)));
            if has_calls && !ended {
                // What cannot be read is told to the model, and not for ever.
                unread = if ready { 0 } else { unread + 1 };
                if unread > 2 {
                    return Err(Halt::Failed("the model's calls cannot be read".into()));
                }
                self.calls(run, &message, cancel).await?;
                continue;
            }
            match stop {
                Some(Stop::EndTurn | Stop::Sequence(_) | Stop::ToolUse) | None => {
                    // What was said while it answered is heard before it is done.
                    // Of what was taken back there is nothing to answer.
                    let mut heard = false;
                    while !heard && let Ok(m) = self.inbox.try_recv() {
                        heard = self.heard(run, m).await?;
                    }
                    if !heard {
                        if let Some(item) = self.last_words.take() {
                            self.stage.mark(&item, self.context.turns(), true).await;
                        }
                        return Ok(());
                    }
                }
                Some(Stop::Pause) => {}
                Some(Stop::MaxOutput) => {
                    self.notice(run, "The answer reached its longest length; the agent goes on.")
                        .await?;
                    self.remember(Turn::Message(Message::user("Go on."))).await?;
                }
                Some(Stop::ContextFull) => {
                    return Err(Halt::Failed("the model's context is full".into()));
                }
                Some(Stop::Refusal(why)) => {
                    let why = why.map(|w| format!(" ({w})")).unwrap_or_default();
                    return Err(Halt::Failed(format!("the provider declined to answer{why}")));
                }
                Some(Stop::Other(why)) => {
                    return Err(Halt::Failed(format!("the model stopped: {why}")));
                }
            }
        }
        Err(Halt::Failed(format!(
            "the run has taken {} steps, as many as it may",
            self.limits.steps
        )))
    }

    /// Takes in what a person said: first for who reads along, then for the model. Not what has been
    /// taken back meanwhile: whether there is something to answer.
    async fn heard(&mut self, run: &RunId, w: Word) -> Result<bool, Halt> {
        // It was taken in before the first answer, or taken back: that has been answered.
        if w.item.as_ref().is_some_and(|i| self.noted.remove(i)) {
            return Ok(false);
        }
        // What begins a run, and is a person's: the conversation before it can be gone on with
        // otherwise.
        let mut begins = None;
        if w.item.is_none() {
            let (by, text) = match &w.shown {
                Some((by, text)) => (by.as_str(), text),
                None => ("", &w.text),
            };
            let item = ItemId::new();
            self.stage.say(said(&item, run, &self.id, text, by)).await?;
            self.stage.sync().await?;
            begins = w.shown.is_none().then_some((item, self.context.turns()));
        }
        let kept = self.keep_turn(Turn::Message(Message::user(w.text)), w.item).await?;
        if let Some((item, before)) = begins {
            self.stage.mark(&item, before, false).await;
        }
        Ok(kept)
    }

    /// Takes in what waits from before: a host that was gone, or a run that was stopped.
    pub async fn note(&mut self, words: Vec<(ItemId, String)>) -> Result<(), Refused> {
        for (item, text) in words {
            self.noted.insert(item.clone());
            match self.keep_turn(Turn::Message(Message::user(text)), Some(item)).await {
                Ok(_) => {}
                Err(Halt::Failed(why) | Halt::Full(why)) => return Err(Refused::This(why)),
                Err(Halt::Cancelled | Halt::Unbegun | Halt::Gone) => return Err(Refused::Closed),
            }
        }
        Ok(())
    }

    async fn notice(&self, run: &RunId, text: &str) -> Result<(), Halt> {
        let item = ItemId::new();
        let say = vec![
            Say::Event(self.item(run, &item, ItemKind::Notice, "", None)),
            Say::Text(item.clone(), text.into()),
            Say::End(item, Outcome::Completed, None),
        ];
        Ok(self.stage.say(say).await?)
    }

    fn item(
        &self,
        run: &RunId,
        item: &ItemId,
        kind: ItemKind,
        name: &str,
        args: Option<Value>,
    ) -> Body {
        begins(&self.id, run, item, kind, name, args)
    }

    /// On the drive before anything follows from it.
    async fn remember(&mut self, t: Turn) -> Result<(), Halt> {
        self.keep_turn(t, None).await.map(drop)
    }

    /// What has been taken back is not kept, and nobody is told of it.
    async fn keep_turn(&mut self, t: Turn, heard: Option<ItemId>) -> Result<bool, Halt> {
        let text = serde_json::to_string(&t).map_err(|e| Halt::Failed(e.to_string()))?;
        let kept = self.stage.remember(self.context.turns(), text, heard).await?;
        if kept {
            self.context.push(&t);
        }
        Ok(kept)
    }

    fn request(&self) -> Result<Request, Halt> {
        let tools = self
            .specs
            .iter()
            .filter(|s| self.site.caps.tools.has(s.name))
            .map(|s| {
                let schema =
                    axiom_model::Json::of(&s.schema).map_err(|e| Halt::Failed(e.to_string()))?;
                Ok(Tool { name: s.name.into(), description: s.description.into(), schema })
            })
            .collect::<Result<_, Halt>>()?;
        Ok(Request {
            model: self.model.clone(),
            system: crate::context::system(&self.site, &self.instructions),
            tools,
            messages: self.context.messages(),
            settings: self.settings.clone(),
        })
    }

    /// Has the model write down what matters, and goes on from that.
    async fn make_room(
        &mut self,
        run: &RunId,
        cancel: &mut Cancel,
        why: String,
    ) -> Result<(), Halt> {
        let messages = self.context.messages();
        if messages.len() < 2 {
            // There is nothing to leave out: what was said is itself too long.
            return Err(Halt::Failed(why));
        }
        let mut most = compact::room(self.window);
        for _ in 0..4 {
            let request = compact::request(&self.model, &self.settings, &messages, most);
            let reply = match self.ask(run, &request, cancel, true).await {
                Err(Halt::Full(_)) => {
                    most /= 2;
                    continue;
                }
                r => r?,
            };
            let summary: String = reply
                .message
                .blocks
                .iter()
                .filter_map(|b| match &b.content {
                    Content::Text { text } => Some(text.as_str()),
                    _ => None,
                })
                .collect();
            if summary.trim().is_empty() {
                return Err(Halt::Failed(format!("{why}, and the model wrote no summary")));
            }
            self.remember(Turn::Compacted { summary: compact::notes(&summary, &messages) }).await?;
            // What it knew of files went with the conversation.
            self.seen.forget();
            self.fill = None;
            // What it goes on with is there to be read.
            return self
                .notice(
                    run,
                    &format!("The conversation was summarised, because {why}. What was kept:\n\n{summary}"),
                )
                .await;
        }
        Err(Halt::Failed(format!("{why}, and it cannot be summarised either")))
    }

    /// Asks the model, again if that may help. What came of a try that failed is marked as such.
    async fn answer(&mut self, run: &RunId, cancel: &mut Cancel) -> Result<Reply, Halt> {
        let request = self.request()?;
        self.ask(run, &request, cancel, false).await
    }

    /// `aside`: not a part of the conversation, and not shown as one.
    async fn ask(
        &mut self,
        run: &RunId,
        request: &Request,
        cancel: &mut Cancel,
        aside: bool,
    ) -> Result<Reply, Halt> {
        let mut tries = 0;
        loop {
            let (retry, class, words, after) = match self.one(run, request, cancel, aside).await? {
                Ok(r) => return Ok(r),
                Err(CallError::Encode(e)) => return Err(Halt::Failed(e.to_string())),
                Err(CallError::Provider(e)) => (e.retry, e.class, e.to_string(), e.retry_after),
            };
            if class == Class::ContextTooLong {
                return Err(Halt::Full(words));
            }
            tries += 1;
            if retry == Retry::Never || tries > self.retries.most {
                return Err(Halt::Failed(words));
            }
            let wait = after.unwrap_or_else(|| self.retries.first * (1 << (tries - 1).min(10)));
            if wait > self.retries.longest {
                return Err(Halt::Failed(format!(
                    "{words} (it asks to be left alone for {} s)",
                    wait.as_secs()
                )));
            }
            let what = match class {
                Class::RateLimit => "The provider limits how often it is asked",
                Class::Overloaded => "The provider has too much to do",
                Class::Network => "The provider was not reached",
                Class::CutShort => "The answer broke off",
                _ => "The provider failed",
            };
            self.notice(
                run,
                &format!(
                    "{what}. Trying again in {:.0} s ({tries} of {}).",
                    wait.as_secs_f64().ceil(),
                    self.retries.most
                ),
            )
            .await?;
            tokio::select! {
                () = tokio::time::sleep(wait) => {}
                () = self.stage.closed() => return Err(Halt::Gone),
                () = stopped(cancel) => return Err(Halt::Cancelled),
            }
        }
    }

    /// One request and its answer, said as it comes.
    async fn one(
        &mut self,
        run: &RunId,
        request: &Request,
        cancel: &mut Cancel,
        aside: bool,
    ) -> Result<Result<Reply, CallError>, Halt> {
        let mut answer = tokio::select! {
            biased;
            () = stopped(cancel) => return Err(Halt::Cancelled),
            () = self.stage.closed() => return Err(Halt::Gone),
            a = self.client.stream(request) => match a {
                Ok(a) => a,
                Err(e) => return Ok(Err(e)),
            },
        };
        let mut fold = Accumulator::new(answer.scope().clone());
        self.last_words = None;
        // By block: the item it is shown as, and whether it has ended.
        let mut items: Vec<Option<ItemId>> = Vec::new();
        let failed = loop {
            let e = tokio::select! {
                biased;
                () = stopped(cancel) => {
                    // The connection closes first.
                    drop(answer);
                    self.end_all(&mut items, Outcome::Cancelled).await?;
                    if aside {
                        return Err(Halt::Cancelled);
                    }
                    return self.keep(fold.finish()).await;
                }
                () = self.stage.closed() => return Err(Halt::Gone),
                e = answer.next() => e,
            };
            let e = match e {
                None => break None,
                Some(Err(e)) => break Some(e),
                Some(Ok(e)) => e,
            };
            if fold.push(&e).is_err() {
                return Err(Halt::Failed("the answer cannot be read".into()));
            }
            if !aside {
                self.show(run, &e, &mut items).await?;
            }
        };
        if let Some(e) = failed {
            self.end_all(&mut items, Outcome::Failed).await?;
            return Ok(Err(CallError::Provider(e)));
        }
        self.end_all(&mut items, Outcome::Completed).await?;
        Ok(Ok(fold.finish()))
    }

    /// What a cancelled answer leaves: the words it had said, so that the agent knows them too.
    async fn keep(&mut self, r: Reply) -> Result<Result<Reply, CallError>, Halt> {
        let text: Vec<Block> = r
            .message
            .blocks
            .into_iter()
            .filter_map(|b| match b.content {
                Content::Text { text } if !text.trim().is_empty() => Some(Block::text(text)),
                _ => None,
            })
            .collect();
        if !text.is_empty() {
            self.remember(Turn::Message(Message { role: Role::Assistant, blocks: text })).await?;
        }
        Err(Halt::Cancelled)
    }

    async fn end_all(&self, items: &mut [Option<ItemId>], outcome: Outcome) -> Result<(), Halt> {
        let say: Vec<Say> =
            items.iter_mut().filter_map(Option::take).map(|i| Say::End(i, outcome, None)).collect();
        if !say.is_empty() {
            self.stage.say(say).await?;
        }
        Ok(())
    }

    async fn show(
        &mut self,
        run: &RunId,
        e: &Event,
        items: &mut Vec<Option<ItemId>>,
    ) -> Result<(), Halt> {
        let say = match e {
            Event::BlockStart { start, .. } => {
                // A call is shown when it is judged, whole.
                let kind = match start {
                    Start::Text => Some(ItemKind::Prose),
                    Start::Reasoning => Some(ItemKind::Reasoning),
                    Start::ToolCall { .. } | Start::Unknown { .. } => None,
                };
                let item = kind.map(|_| ItemId::new());
                items.push(item.clone());
                if kind == Some(ItemKind::Prose) {
                    self.last_words.clone_from(&item);
                }
                match (kind, item) {
                    (Some(k), Some(i)) => vec![Say::Event(self.item(run, &i, k, "", None))],
                    _ => return Ok(()),
                }
            }
            Event::Delta { block, delta: Delta::Text(t) | Delta::Reasoning(t) } => {
                match items.get(*block).and_then(Option::as_ref) {
                    Some(i) if !t.is_empty() => vec![Say::Text(i.clone(), t.clone())],
                    _ => return Ok(()),
                }
            }
            Event::BlockStop { block, .. } => match items.get_mut(*block).and_then(Option::take) {
                Some(i) => vec![Say::End(i, Outcome::Completed, None)],
                None => return Ok(()),
            },
            _ => return Ok(()),
        };
        Ok(self.stage.say(say).await?)
    }

    /// Some servers let a model they have answer for one they have not, and say so nowhere else. A
    /// name that only says which version of the model it is is the same one.
    async fn compare(&mut self, run: &RunId, r: &Reply) -> Result<(), Halt> {
        let Some(is) = r.model.as_deref().filter(|m| !m.is_empty()) else { return Ok(()) };
        let (a, b) = (is.to_lowercase(), self.model.to_lowercase());
        let tail = |s: &str| s.rsplit('/').next().unwrap_or(s).to_owned();
        let (a, b) = (tail(&a), tail(&b));
        if a.starts_with(&b) || b.starts_with(&a) || self.told.as_deref() == Some(is) {
            return Ok(());
        }
        self.told = Some(is.to_owned());
        self.notice(run, &format!("{is} answers, not {}, which was asked for.", self.model)).await
    }

    async fn usage(&self, r: &Reply) -> Result<(), Halt> {
        let u = &r.usage;
        if *u == axiom_model::Usage::default() {
            return Ok(());
        }
        let context = fill(r, self.window).map(|f| f.min(1.0));
        let usage = Usage {
            input: u.uncached.or(u.prompt),
            cache_read: u.cache_read,
            cache_write: u.cache_write,
            output: u.output,
            reasoning: u.reasoning,
            context,
        };
        let b = Body::Usage { agent: self.id.clone(), usage, provenance: Provenance::Reported };
        Ok(self.stage.say(vec![Say::Event(b)]).await?)
    }

    /// Does what the answer asks for, in its order. Every call gets a result, whatever happens.
    async fn calls(&mut self, run: &RunId, m: &Message, cancel: &mut Cancel) -> Result<(), Halt> {
        let mut halt = None;
        let mut unread = Vec::new();
        for b in &m.blocks {
            let Content::ToolCall(c) = &b.content else { continue };
            if halt.is_none() && *cancel.borrow() {
                halt = Some(Halt::Cancelled);
            }
            if halt.is_some() {
                if c.ready().is_some() {
                    let not = "Not done: the run was stopped before it.";
                    self.result(&c.id, not, true).await?;
                }
                continue;
            }
            let shown = match &c.args {
                Args::Ready(a) => Value::Object(a.clone()),
                _ => Value::String(c.raw.clone()),
            };
            let args = match (c.ready(), &c.args) {
                (Some(call), _) => Ok(Value::Object(call.args().clone())),
                (None, Args::Broken(why)) => Err(why.clone()),
                (None, _) => Err("the answer ended inside it".into()),
            };
            let hands = Hands {
                id: &self.id,
                stage: &self.stage,
                desk: &self.desk,
                site: &self.site,
                seen: &self.seen,
                specs: &self.specs,
            };
            let mut waited = Duration::ZERO;
            let done = hands.one(run, &c.name, shown, args, cancel, &mut waited).await;
            self.waited += waited;
            let Done { text, failed, halt: stop } = done?;
            halt = stop;
            if c.ready().is_some() {
                self.result(&c.id, &text, failed).await?;
            } else {
                unread.push(format!("Your call of {}: {text}", c.name));
            }
        }
        if !unread.is_empty() {
            // No provider takes such a call back, nor a result for it.
            self.remember(Turn::Message(Message::user(unread.join("\n")))).await?;
        }
        halt.map_or(Ok(()), Err)
    }

    /// By itself, at once: what was done is known though the host is killed before the next call.
    async fn result(&mut self, id: &str, text: &str, failed: bool) -> Result<(), Halt> {
        let blocks = vec![Block::result(id, text, failed)];
        self.remember(Turn::Message(Message { role: Role::User, blocks })).await
    }
}
