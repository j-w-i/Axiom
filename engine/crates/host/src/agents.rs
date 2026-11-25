//! Agents of this host: how they are set up, what they run on, where they work, and who is asked.

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
};

use axiom_engine::{Agent, Context, Deed, Desk, Limits, Refused, Retries, Say, Stage, Turn, Word};
use axiom_model::{Cache, Codec, Settings};
use axiom_policy::{
    Access, Budget, Caps, Keep, Net, NotAnswered, Place, Policy, Profile, Reach, Tools,
    audit::{Containment, Entry},
    rules::Source,
};
use axiom_proto::{
    AgentId, AgentState, Body, ErrorCode, Execution, RunId, SessionId, WireError, limits, now_ms,
};
use axiom_tools::{Seen, Site};
use serde::{Deserialize, Serialize};
use tokio::sync::{mpsc, watch};

use crate::{
    connections::Connections,
    paths::{Paths, private_dir},
    session::{self, Session},
    store::Store,
};

/// How an agent is set up, as it is kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Setup {
    pub name: String,
    pub execution: Execution,
    pub access: axiom_proto::Access,
    /// The folder it works in: real, without links.
    pub workspace: String,
}

/// Agents of one session.
const MOST: usize = 64;
/// What may wait for an agent that is at work.
const INBOX: usize = 32;
/// Sessions whose agents are remembered beyond what the journal has.
const PARTIES: usize = 1000;

#[derive(Default)]
struct Party {
    desk: Desk,
    agents: HashMap<AgentId, Arc<Slot>>,
}

/// An agent while the host runs.
#[derive(Default)]
pub struct Slot {
    seen: Seen,
    /// The run it is in, and how to speak to it.
    live: Arc<tokio::sync::Mutex<Live>>,
}

type Live = Option<(RunId, mpsc::Sender<Word>)>;

pub struct Agents {
    pub connections: Connections,
    parties: Mutex<HashMap<SessionId, Party>>,
    audit: tokio::sync::Mutex<Option<Entry>>,
    home: Option<PathBuf>,
    own: Vec<PathBuf>,
    scratch: PathBuf,
    rules: PathBuf,
}

pub fn bad(words: impl Into<String>) -> WireError {
    WireError::new(ErrorCode::BadRequest, words)
}
fn internal(words: impl std::fmt::Display) -> WireError {
    WireError::new(ErrorCode::Internal, words.to_string())
}

impl Agents {
    pub fn new(
        connections: Connections,
        paths: &Paths,
        home: Option<PathBuf>,
        scratch: PathBuf,
        endpoint: &str,
    ) -> Self {
        let mut own = vec![paths.data.clone()];
        // Its socket may be elsewhere, where names are shorter.
        own.extend(Path::new(endpoint).parent().map(Path::to_path_buf));
        Self {
            connections,
            parties: Mutex::default(),
            audit: tokio::sync::Mutex::default(),
            home,
            own,
            scratch,
            rules: paths.data.join("policy.toml"),
        }
    }

    fn parties(&self) -> MutexGuard<'_, HashMap<SessionId, Party>> {
        self.parties.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn party<T>(&self, s: &SessionId, f: impl FnOnce(&mut Party) -> T) -> T {
        let mut all = self.parties();
        if all.len() >= PARTIES && !all.contains_key(s) {
            // Forgetting costs no more than asking, and reading, once more.
            all.retain(|_, p| {
                p.agents.values().any(|a| a.live.try_lock().map_or(true, |l| l.is_some()))
            });
        }
        f(all.entry(s.clone()).or_default())
    }

    pub fn desk(&self, s: &SessionId) -> Desk {
        self.party(s, |p| p.desk.clone())
    }

    pub fn slot(&self, s: &SessionId, a: &AgentId) -> Arc<Slot> {
        self.party(s, |p| p.agents.entry(a.clone()).or_default().clone())
    }

    pub fn answer(
        &self,
        s: &SessionId,
        r: &axiom_proto::RequestId,
        a: axiom_proto::Answer,
        digest: &str,
    ) -> Result<(), WireError> {
        let a = match a {
            axiom_proto::Answer::Once => axiom_policy::Answer::Once,
            axiom_proto::Answer::Session => axiom_policy::Answer::Session,
            axiom_proto::Answer::Deny => axiom_policy::Answer::Deny,
        };
        self.desk(s).answer(r, a, digest).map_err(|e| {
            let code =
                if e == NotAnswered::Gone { ErrorCode::NotFound } else { ErrorCode::BadRequest };
            WireError::new(code, e.to_string())
        })
    }

    /// Checks what an agent is to be, and says it as it will be kept.
    pub fn setup(
        &self,
        name: String,
        execution: Execution,
        access: axiom_proto::Access,
        workspace: &str,
    ) -> Result<Setup, WireError> {
        if name.trim().is_empty() || limits::text_len(&name) > 200 {
            return Err(bad("an agent's name has between 1 and 200 bytes"));
        }
        if execution.harness != "axiom" {
            return Err(bad("this host runs the harness axiom only"));
        }
        let (Some(c), Some(m)) = (&execution.connection, &execution.model) else {
            return Err(bad("an agent needs a connection and a model"));
        };
        if m.trim().is_empty() || limits::text_len(m) > 500 {
            return Err(bad("a model's name has between 1 and 500 bytes"));
        }
        self.connections.client(c)?;
        let place = real(Path::new(workspace))?;
        let setup = Setup { name, execution, access, workspace: place.to_string() };
        // What could not be contained later is refused now, in words.
        let site = self.site(&setup, &AgentId("check".into()), false)?;
        Profile::of(
            &Caps { run: true, ..site.caps.clone() },
            &site.keep,
            &site.system,
            &site.scratch,
        )
        .map_err(|e| bad(e.to_string()))?;
        Ok(setup)
    }

    /// Where an agent works and what it may. `make`: with its scratch folder.
    fn site(&self, s: &Setup, agent: &AgentId, make: bool) -> Result<Site, WireError> {
        let access = match s.access {
            axiom_proto::Access::Read => Access::Read,
            axiom_proto::Access::Ask => Access::Ask,
            axiom_proto::Access::Work => Access::Work,
            axiom_proto::Access::Unknown => return Err(bad("the access is not known")),
        };
        let home = self.home.as_deref().ok_or_else(|| bad("there is no home folder"))?;
        let home = real(home)?;
        let workspace = real(Path::new(&s.workspace))?;
        if workspace.as_str() != s.workspace {
            return Err(bad("the workspace leads elsewhere now"));
        }
        let mut own = Vec::new();
        for o in &self.own {
            if let Ok(p) = real(o) {
                own.push(p);
            }
        }
        let keep = Keep::new(home.clone(), own);
        let dir = self.scratch.join(plain(&agent.0));
        if make {
            private_dir(&self.scratch).map_err(internal)?;
            private_dir(&dir).map_err(internal)?;
        }
        let scratch =
            if make { real(&dir)? } else { Place::new(&dir).map_err(|e| bad(e.to_string()))? };
        let (path, system) = axiom_tools::system(&home);
        let write =
            if access == Access::Read { Reach::none() } else { Reach::of([workspace.clone()]) };
        let policy = self.policy(&workspace)?;
        Ok(Site {
            caps: Caps {
                read: Reach::of([workspace.clone(), scratch.clone()]),
                write,
                net: Net::none(),
                run: true,
                tools: Tools::All,
                budget: Budget::default(),
            },
            workspace,
            keep,
            access,
            policy,
            system,
            scratch,
            path,
            chain: vec![agent.clone()],
        })
    }

    /// The person's rules and the workspace's, which only narrow them (SI-5). Rules that cannot be
    /// understood stop everything: one of them might have been the one that denies.
    fn policy(&self, workspace: &Place) -> Result<Policy, WireError> {
        let read = |p: &Path| match std::fs::read_to_string(p) {
            Ok(t) => Ok(Some(t)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(bad(format!("{}: {e}", p.display()))),
        };
        let mut policy = Policy::default();
        if let Some(t) = read(&self.rules)? {
            policy = policy.and(
                Policy::parse(&t, Source::Person).map_err(|e| bad(format!("policy.toml: {e}")))?,
            );
        }
        let place = workspace.join(".axiom/policy.toml").map_err(|e| bad(e.to_string()))?;
        match axiom_tools::fs::find(std::slice::from_ref(workspace), &place) {
            Ok(spot) if spot.kind == axiom_tools::fs::Kind::Missing => {}
            Ok(spot) => {
                let c = spot.read(256 * 1024).map_err(|e| bad(e.to_string()))?;
                let t = String::from_utf8(c.bytes)
                    .map_err(|_| bad("the workspace's rules are not text"))?;
                let p = Policy::parse(&t, Source::Workspace)
                    .map_err(|e| bad(format!("the workspace's .axiom/policy.toml: {e}")))?;
                policy = policy.and(p);
            }
            Err(axiom_tools::fs::FsError::Missing(_)) => {}
            Err(e) => return Err(bad(e.to_string())),
        }
        Ok(policy)
    }

    /// Adds to the audit record (SI-23).
    async fn record(
        &self,
        store: &Store,
        session: &SessionId,
        d: Deed,
    ) -> axiom_journal::Result<()> {
        let mut last = self.audit.lock().await;
        if last.is_none()
            && let Some(t) = store.last_audit().await?
        {
            *last = Some(serde_json::from_str(&t)?);
        }
        let mut subject = d.subject;
        limits::clip(&mut subject, limits::VALUE);
        let e = Entry {
            seq: 0,
            at: now_ms(),
            session: session.clone(),
            workspace: d.workspace,
            chain: d.chain,
            tool: d.tool,
            subject,
            decision: d.decision,
            rules: d.rules,
            answered: d.answered,
            containment: Containment::Axiom,
            before: String::new(),
            digest: String::new(),
        }
        .after(last.as_ref());
        let text = serde_json::to_string(&e)?;
        match store.put_audit(e.seq, text).await {
            Ok(()) => {
                *last = Some(e);
                Ok(())
            }
            Err(err) => {
                // Whether it was written is not known: looked up before the next.
                *last = None;
                Err(err)
            }
        }
    }
}

/// A name for a folder, from an id that may be anybody's.
fn plain(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(64).collect()
}

fn real(p: &Path) -> Result<Place, WireError> {
    if !p.is_absolute() {
        return Err(bad(format!("{} is not an absolute path", p.display())));
    }
    let c = std::fs::canonicalize(p).map_err(|e| bad(format!("{}: {e}", p.display())))?;
    let c = plain_verbatim(c);
    if !c.is_dir() {
        return Err(bad(format!("{} is not a folder", p.display())));
    }
    Place::new(&c).map_err(|e| bad(format!("{}: {e}", p.display())))
}

/// Windows says what it found in a form that is not used here.
fn plain_verbatim(p: PathBuf) -> PathBuf {
    match p.to_str().and_then(|s| s.strip_prefix(r"\\?\")) {
        Some(s) if s.as_bytes().get(1) == Some(&b':') => PathBuf::from(s),
        _ => p,
    }
}

/// What an agent says goes to its session.
#[derive(Clone)]
pub struct Floor {
    pub agents: Arc<Agents>,
    pub store: Store,
    pub session: Session,
    pub agent: AgentId,
}

fn refused(r: session::Refused) -> Refused {
    match r {
        session::Refused::Closed(_) => Refused::Closed,
        session::Refused::TooLong(e) => Refused::This(e.to_string()),
    }
}

impl Stage for Floor {
    async fn say(&self, all: Vec<Say>) -> Result<(), Refused> {
        self.session.say(all).await.map_err(refused)
    }
    async fn sync(&self) -> Result<(), Refused> {
        self.session.sync().await.map(drop).map_err(|_| Refused::Closed)
    }
    async fn remember(&self, n: u64, turn: String) -> Result<(), Refused> {
        let (s, a) = (self.session.id.clone(), self.agent.clone());
        self.store.put_message(s, a, n, turn).await.map_err(|e| Refused::This(e.to_string()))
    }
    async fn audit(&self, deed: Deed) -> Result<(), Refused> {
        self.agents
            .record(&self.store, &self.session.id, deed)
            .await
            .map_err(|e| Refused::This(format!("it cannot be put on record: {e}")))
    }
}

/// An agent ready to run, from the journal.
pub async fn load(
    agents: &Arc<Agents>,
    store: &Store,
    session: Session,
    agent: &AgentId,
    setup: &Setup,
    inbox: mpsc::Receiver<Word>,
) -> Result<Agent<Floor>, WireError> {
    let id = session.id.clone();
    let connection = setup.execution.connection.clone().unwrap_or_default();
    let model = setup.execution.model.clone().unwrap_or_default();
    let (a, s, ag) = (agents.clone(), setup.clone(), agent.clone());
    // Links, folders, the system's store: off the runtime's own threads.
    let (client, site) = tokio::task::spawn_blocking(move || {
        Ok::<_, WireError>((a.connections.client(&connection)?, a.site(&s, &ag, true)?))
    })
    .await
    .map_err(internal)??;
    let kept = store.messages(id.clone(), agent.clone()).await.map_err(internal)?;
    let (context, new) = Context::load(&kept).map_err(internal)?;
    for (n, t) in (kept.len() as u64..).zip(&new) {
        let t: &Turn = t;
        let text = serde_json::to_string(t).map_err(internal)?;
        store.put_message(id.clone(), agent.clone(), n, text).await.map_err(internal)?;
    }
    let quirks = client.profile().quirks(&model);
    let messages = client.profile().codec == Codec::Messages;
    let settings = Settings {
        max_output: quirks
            .max_output
            .map(|m| u32::try_from(m.min(32_000)).unwrap_or(32_000))
            .or(messages.then_some(8192)),
        effort: setup.execution.effort.clone(),
        stop: Vec::new(),
        cache: if messages { Cache::Short } else { Cache::Off },
    };
    Ok(Agent {
        id: agent.clone(),
        stage: Floor {
            agents: agents.clone(),
            store: store.clone(),
            session,
            agent: agent.clone(),
        },
        desk: agents.desk(&id),
        client,
        model,
        settings,
        window: quirks.context,
        site,
        seen: agents.slot(&id, agent).seen.clone(),
        context,
        specs: axiom_tools::specs(),
        instructions: String::new(),
        limits: Limits::default(),
        fill: None,
        told: None,
        retries: Retries::default(),
        inbox,
    })
}

/// What came of saying something to an agent.
pub enum Spoken {
    /// To be heard by the run that is on.
    Heard(Box<Ear>),
    /// A run is to be started, and speaks for itself through this.
    Start(Box<Turnstile>),
}

/// Room for one more thing to be said to a run. The run does not end while this is held.
pub struct Ear {
    pub run: RunId,
    permit: mpsc::OwnedPermit<Word>,
    _slot: tokio::sync::OwnedMutexGuard<Live>,
}

impl Ear {
    pub fn tell(self, text: String) -> RunId {
        self.permit.send(Word { text, shown: true });
        self.run
    }
}

/// Holds an agent's place until its run begins or cannot.
pub struct Turnstile {
    slot: tokio::sync::OwnedMutexGuard<Live>,
}

impl Slot {
    /// A place in the run that is on, or the place for a new one.
    pub async fn speak(&self) -> Result<Spoken, WireError> {
        let live = self.live.clone().lock_owned().await;
        if let Some((run, tx)) = live.as_ref()
            && !tx.is_closed()
        {
            let busy = |words: &str| WireError::new(ErrorCode::Busy, words);
            return match tx.clone().try_reserve_owned() {
                Ok(permit) => {
                    Ok(Spoken::Heard(Box::new(Ear { run: run.clone(), permit, _slot: live })))
                }
                Err(mpsc::error::TrySendError::Full(_)) => {
                    Err(busy("the agent has as much to hear as it may"))
                }
                Err(mpsc::error::TrySendError::Closed(_)) => {
                    Err(busy("the agent is ending a run; say it again"))
                }
            };
        }
        Ok(Spoken::Start(Box::new(Turnstile { slot: live })))
    }

    /// What it has running began as what it was.
    pub fn changed(&self) {
        self.seen.1.end();
    }

    pub async fn busy(&self) -> bool {
        self.live.lock().await.as_ref().is_some_and(|(_, tx)| !tx.is_closed())
    }

    /// After a run: what was said too late for it. The place is free if there is nothing.
    async fn after(&self, rx: &mut mpsc::Receiver<Word>) -> Vec<Word> {
        let mut live = self.live.lock().await;
        let mut left = Vec::new();
        while let Ok(m) = rx.try_recv() {
            left.push(m);
        }
        if left.is_empty() {
            rx.close();
            *live = None;
        }
        left
    }

    async fn renamed(&self, run: &RunId) {
        if let Some((r, _)) = self.live.lock().await.as_mut() {
            *r = run.clone();
        }
    }

    async fn leave(&self) {
        *self.live.lock().await = None;
    }
}

impl Turnstile {
    /// A way to speak to the run that now takes the place.
    pub fn enter(mut self, run: &RunId) -> mpsc::Receiver<Word> {
        let (tx, rx) = mpsc::channel(INBOX);
        *self.slot = Some((run.clone(), tx));
        rx
    }
}

/// Runs an agent until nothing is left that was said to it. `next` makes a run that can be cancelled.
pub async fn work(
    agent: &mut Agent<Floor>,
    slot: &Slot,
    first: (RunId, watch::Receiver<bool>),
    mut next: impl FnMut() -> Result<(RunId, watch::Receiver<bool>), WireError>,
) {
    let (mut run, mut cancel) = first;
    loop {
        let cancelled = cancel.clone();
        agent.run(run, cancel).await;
        let stopped = *cancelled.borrow();
        let left = slot.after(&mut agent.inbox).await;
        if left.is_empty() {
            return;
        }
        // Stopped by a person: what they said before that starts nothing, but is not forgotten.
        let next = if stopped { None } else { next().ok() };
        let Some(n) = next else {
            let _ = agent.note(left).await;
            slot.leave().await;
            return;
        };
        (run, cancel) = n;
        slot.renamed(&run).await;
        if agent.go_on(&run, left).await.is_err() {
            slot.leave().await;
            return;
        }
    }
}

/// The events that make an agent known.
pub fn added(agent: &AgentId, s: &Setup) -> Vec<Say> {
    vec![
        Say::Event(Body::AgentAdded {
            agent: agent.clone(),
            name: s.name.clone(),
            execution: s.execution.clone(),
            parent: None,
            access: Some(s.access),
        }),
        Say::Event(Body::AgentState { agent: agent.clone(), state: AgentState::Waiting }),
    ]
}
pub fn changed(agent: &AgentId, s: &Setup) -> Vec<Say> {
    vec![Say::Event(Body::AgentChanged {
        agent: agent.clone(),
        execution: s.execution.clone(),
        access: Some(s.access),
    })]
}

pub fn full(n: usize) -> bool {
    n >= MOST
}
