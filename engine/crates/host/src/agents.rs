//! Agents of this host: how they are set up, what they run on, where they work, and who is asked.

use std::{
    collections::{HashMap, VecDeque},
    path::{Path, PathBuf},
    pin::Pin,
    sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak, atomic::AtomicU32},
    time::Duration,
};

use axiom_engine::{Agent, Context, Deed, Desk, Ended, Other, Refused, Retries, Say, Stage, Word};
use axiom_model::{Cache, Codec, Settings};
use axiom_policy::{
    Access, Budget, Caps, Keep, Net, NotAnswered, Place, Policy, Profile, Reach, Tools,
    audit::{Containment, Entry},
    rules::Source,
};
use axiom_proto::{
    AgentId, AgentState, Body, Branched, ErrorCode, Execution, HarnessInfo, Instructions, ItemId,
    Outcome, RunId, SessionId, UsedOn, WireError, limits, now_ms,
};
use axiom_tools::{Output, Seen, Site, Spec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::{mpsc, watch};

use crate::{
    catalogue::Catalogue,
    connections::{self, Connections},
    copies::{self, Copies},
    delegated, mcp,
    paths::{Paths, private_dir, private_file},
    peers, planning,
    programs::{How, Programs, Said},
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
    /// Of the workspace, what a person has read and wants followed (T6): other content is not.
    #[serde(default)]
    pub instructions: Vec<Instructions>,
    /// Of a delegated agent: the folder's own settings for its program may take effect.
    #[serde(default)]
    pub trust: bool,
    /// Of a delegated agent: what to go on with.
    #[serde(default)]
    pub resume: Option<String>,
    /// Of a delegated agent: its program has run.
    #[serde(default)]
    pub began: bool,
    /// It works in a copy of the folder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub apart: Option<Apart>,
    /// Its conversation began as another's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Origin>,
    /// It only reads, and hands in a plan.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub plan: bool,
    /// The last it has handed in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planned: Option<ItemId>,
}

/// Whose conversation, and how many turns of it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Origin {
    pub agent: AgentId,
    pub turns: u64,
}

/// Of which folder, and whether there is one yet: then that is its workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Apart {
    pub folder: String,
    pub made: bool,
}

/// Files whose words are for agents, as they are named in the top folder of a workspace.
const FOR_AGENTS: &[&str] = &["AGENTS.md", "CLAUDE.md"];
/// Longer ones are not followed.
pub const INSTRUCTIONS: u64 = 64 * 1024;

/// Of why a person said no.
const WORDS: usize = 4096;
/// Agents of one session.
pub const MOST: usize = 64;
/// What may wait for an agent that is at work.
pub const INBOX: usize = 32;
/// How long what an agent began may take to end.
const STILL: Duration = Duration::from_secs(15);
/// Sessions whose agents are remembered beyond what the journal has.
const PARTIES: usize = 1000;
/// Sessions whose folders programs are not taken from.
const WORKSPACES: u32 = 10_000;

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
    keys: Mutex<VecDeque<(String, Value)>>,
    /// What its last runs came to.
    pub last: watch::Sender<Option<Arc<Finished>>>,
    /// Who began its run, from the session's own agent down, and itself.
    pub chain: Mutex<Vec<AgentId>>,
    /// Runs it began by telling others, whose end it has not been told. As the journal has them, once
    /// it has been in a run.
    pub owed: Mutex<HashMap<RunId, Debt>>,
    /// The journal has been asked.
    owes: tokio::sync::OnceCell<()>,
    /// Runs of it that others' ends began, since a person spoke to it.
    pub wakes: AtomicU32,
    /// Who it waits for now.
    pub waits: Mutex<Vec<AgentId>>,
    /// Of a delegated agent: what it said last in this run.
    pub said: Mutex<String>,
    /// Of a delegated agent: its program, if it runs.
    pub attempt: tokio::sync::Mutex<Option<mpsc::Sender<delegated::Turn>>>,
    /// Programs of its that have not ended, also those nobody speaks to any more.
    pub stays: watch::Sender<usize>,
}

/// Something an agent is owed.
#[derive(Debug, Clone)]
pub struct Debt {
    pub child: AgentId,
    /// What it came to, if that is known from the journal.
    pub ended: Option<Arc<Finished>>,
}

/// Runs that followed one another, and what they came to.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finished {
    pub runs: Vec<RunId>,
    pub outcome: Outcome,
    pub error: Option<String>,
    /// What it said last.
    pub text: String,
}

/// Who holds a key, and what it opens: the tool `agent`, or all an agent of Axiom's has.
#[derive(Debug, Clone)]
pub struct Holder {
    pub session: SessionId,
    pub agent: AgentId,
    pub all: bool,
    /// And the one that hands in a plan.
    pub plan: bool,
}

/// A key that is good until this is dropped.
pub struct Grant {
    agents: Arc<Agents>,
    key: String,
    pub file: PathBuf,
    /// What the program's model is told.
    pub words: Option<PathBuf>,
}
impl Drop for Grant {
    fn drop(&mut self) {
        let mut all = self.agents.grants.lock().unwrap_or_else(PoisonError::into_inner);
        all.remove(&self.key);
        for f in [Some(&self.file), self.words.as_ref()].into_iter().flatten() {
            let _ = std::fs::remove_file(f);
        }
    }
}

/// How an agent reaches the others of its session. Given by the host once it runs.
pub trait Switch: Send + Sync {
    fn tell(
        self: Arc<Self>,
        session: SessionId,
        agent: AgentId,
        told: Told,
    ) -> Pin<Box<dyn Future<Output = Result<RunId, WireError>> + Send>>;
    fn stop(&self, run: &RunId);
}

/// What one agent says to another.
#[derive(Debug, Clone)]
pub struct Told {
    pub by: String,
    pub text: String,
    /// The agents from the session's own to the one that says it.
    pub chain: Vec<AgentId>,
}

impl Setup {
    /// The folder its work is for, wherever it works.
    pub fn folder(&self) -> &str {
        self.apart.as_ref().map_or(&self.workspace, |a| &a.folder)
    }

    /// A program of the person's own that works with Axiom's tools in place of its own.
    pub fn hands(&self) -> bool {
        self.execution.environment.as_deref() == Some(AXIOM)
    }
}

/// The environment in which what an agent does is done by Axiom.
pub const AXIOM: &str = "axiom";

impl Slot {
    /// Nobody uses it, or is about to, and nothing runs in it.
    fn idle(self: &Arc<Self>) -> bool {
        Arc::strong_count(self) == 1
            && self.live.try_lock().is_ok_and(|l| l.is_none())
            && self.seen.1.running().is_empty()
            && self.owed.lock().is_ok_and(|o| o.is_empty())
            && self.attempt.try_lock().is_ok_and(|a| a.as_ref().is_none_or(|a| a.is_closed()))
    }
}

/// Forgetting costs no more than asking, and reading, once more.
fn forget_idle(all: &mut HashMap<SessionId, Party>) {
    all.retain(|_, p| !p.agents.values().all(Slot::idle));
}

type Live = Option<Now>;

/// The run an agent is in.
struct Now {
    run: RunId,
    tx: mpsc::Sender<Word>,
    cancel: watch::Receiver<bool>,
    /// Something was said after a person stopped it: that begins another.
    late: bool,
}

/// Prompts whose names are remembered.
const KEYS: usize = 64;

pub struct Agents {
    pub connections: Arc<Connections>,
    parties: Mutex<HashMap<SessionId, Party>>,
    audit: tokio::sync::Mutex<Option<Entry>>,
    pub switch: OnceLock<Weak<dyn Switch>>,
    /// What opens the tool `agent` for an installed program, while its attempt lives.
    grants: Mutex<HashMap<String, Holder>>,
    data: PathBuf,
    /// The program that gives installed programs the tool `agent`: the host's own.
    bridge: Option<Place>,
    home: Option<PathBuf>,
    own: Vec<PathBuf>,
    scratch: PathBuf,
    pub programs: Programs,
    pub catalogue: Catalogue,
    rules: PathBuf,
    pub copies: Copies,
}

pub fn bad(words: impl Into<String>) -> WireError {
    WireError::new(ErrorCode::BadRequest, words)
}
fn internal(words: impl std::fmt::Display) -> WireError {
    WireError::new(ErrorCode::Internal, words.to_string())
}

impl Agents {
    pub fn new(
        connections: Arc<Connections>,
        paths: &Paths,
        home: Option<PathBuf>,
        scratch: PathBuf,
        programs: Programs,
        bridge: Option<PathBuf>,
        endpoint: &str,
    ) -> Self {
        let mut own = vec![paths.data.clone()];
        // Its socket may be elsewhere, where names are shorter.
        own.extend(Path::new(endpoint).parent().map(Path::to_path_buf));
        Self {
            connections,
            parties: Mutex::default(),
            audit: tokio::sync::Mutex::default(),
            switch: OnceLock::new(),
            grants: Mutex::default(),
            data: paths.data.clone(),
            bridge: bridge.and_then(|b| real_file(&b)),
            copies: Copies::new(&paths.data, home.as_deref()),
            home,
            own,
            scratch,
            programs,
            catalogue: Catalogue::open(&paths.data.join("models.json")),
            rules: paths.data.join("policy.toml"),
        }
    }

    fn parties(&self) -> MutexGuard<'_, HashMap<SessionId, Party>> {
        self.parties.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn party<T>(&self, s: &SessionId, f: impl FnOnce(&mut Party) -> T) -> T {
        let mut all = self.parties();
        if all.len() >= PARTIES && !all.contains_key(s) {
            forget_idle(&mut all);
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
        words: Option<String>,
    ) -> Result<(), WireError> {
        let gone = |e: NotAnswered| {
            let code =
                if e == NotAnswered::Gone { ErrorCode::NotFound } else { ErrorCode::BadRequest };
            WireError::new(code, e.to_string())
        };
        if let Some(words) = words {
            if a != axiom_proto::Answer::Deny {
                return Err(bad("a reason goes with a no"));
            }
            if limits::text_len(&words) > WORDS {
                return Err(bad("a reason has at most 4096 bytes"));
            }
            return self.desk(s).deny(r, words).map_err(gone);
        }
        let a = match a {
            axiom_proto::Answer::Once => axiom_policy::Answer::Once,
            axiom_proto::Answer::Session => axiom_policy::Answer::Session,
            axiom_proto::Answer::Deny => axiom_policy::Answer::Deny,
        };
        self.desk(s).answer(r, a, digest).map_err(gone)
    }

    /// Checks what an agent is to be, and says it as it will be kept.
    pub fn setup(&self, s: Setup, written: &Reach) -> Result<Setup, WireError> {
        let Setup { name, execution, access, workspace, instructions, trust, .. } = s;
        let (resume, began, apart, from) = (s.resume, s.began, s.apart, s.from);
        let (plan, planned) = (s.plan, s.planned.filter(|_| s.plan));
        if plan && access != axiom_proto::Access::Read {
            return Err(bad("who hands in a plan first only reads until it is accepted"));
        }
        let workspace = workspace.as_str();
        let known =
            |i: &Instructions| FOR_AGENTS.contains(&i.path.as_str()) && i.digest.len() <= 128;
        if instructions.len() > FOR_AGENTS.len() || !instructions.iter().all(known) {
            return Err(bad("these are not instructions for agents"));
        }
        let instructions: Vec<Instructions> =
            instructions.into_iter().map(|i| Instructions { text: None, ..i }).collect();
        if name.trim().is_empty() || limits::text_len(&name) > 200 {
            return Err(bad("an agent's name has between 1 and 200 bytes"));
        }
        if execution.harness != "axiom" {
            let known = self
                .programs
                .known_as(&execution.harness)
                .ok_or_else(|| bad("this host does not know the harness"))?;
            if let Some(c) = &execution.connection {
                if connections::provider_of(&execution.harness).is_none() {
                    return Err(bad("this harness has one account: its own sign-in"));
                }
                self.connections.account(c, &execution.harness)?;
            }
            if execution.thinking.is_some() {
                return Err(bad("this harness cannot be told whether to think"));
            }
            if !instructions.is_empty() {
                return Err(bad("this harness reads the folder's words for agents by itself"));
            }
            match execution.environment.as_deref() {
                None => {}
                Some(AXIOM) if known.harness.takes_hands() && self.bridge.is_some() => {}
                _ => return Err(bad("this harness has no such environment")),
            }
            if plan && execution.environment.as_deref() != Some(AXIOM) {
                return Err(bad("this harness cannot be asked for a plan first"));
            }
            let workspace = real(Path::new(workspace))?.to_string();
            let execution = Execution { provider: None, ..execution };
            let setup = Setup {
                name,
                execution,
                access,
                workspace,
                instructions,
                trust,
                resume,
                began,
                apart,
                from,
                plan,
                planned,
            };
            let site = self.site(&setup, &AgentId("check".into()), false)?;
            let path = self.looked_in(&site.path)?;
            let how = self.how(&setup)?;
            known
                .check(&setup, &how, site.access, site.workspace.clone(), &path, written)
                .map_err(bad)?;
            return Ok(setup);
        }
        let (Some(c), Some(m)) = (&execution.connection, &execution.model) else {
            return Err(bad("an agent needs a connection and a model"));
        };
        if m.trim().is_empty() || limits::text_len(m) > 500 {
            return Err(bad("a model's name has between 1 and 500 bytes"));
        }
        let client = self.connections.client(c)?;
        if execution.effort.is_some() && client.profile().quirks(m).effort.is_none() {
            return Err(bad("the effort cannot be set for this provider and model"));
        }
        let quirks = client.profile().quirks(m);
        if execution.thinking.is_some() && quirks.thinking.is_none() {
            return Err(bad("whether it thinks cannot be set for this provider and model"));
        }
        if let Some(s) = &execution.speed
            && !quirks.speeds.is_some_and(|all| all.contains(s))
        {
            return Err(bad("the speed cannot be set for this provider and model"));
        }
        let place = real(Path::new(workspace))?;
        let setup = Setup {
            name,
            execution,
            access,
            workspace: place.to_string(),
            instructions,
            trust: false,
            resume: None,
            began: false,
            apart,
            from,
            plan,
            planned,
        };
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

    /// For the host's own look at a folder: as an agent that only looks.
    pub fn looking(&self, workspace: &str, who: &AgentId) -> Result<Site, WireError> {
        let s = Setup {
            name: String::new(),
            execution: Execution {
                harness: "axiom".into(),
                provider: None,
                model: None,
                connection: None,
                effort: None,
                environment: None,
                ..Execution::default()
            },
            access: axiom_proto::Access::Read,
            workspace: real(Path::new(workspace))?.to_string(),
            instructions: vec![],
            trust: false,
            resume: None,
            began: false,
            apart: None,
            from: None,
            plan: false,
            planned: None,
        };
        self.site(&s, who, true)
    }

    /// Programs of delegated agents end.
    pub async fn end(&self) {
        let slots: Vec<Arc<Slot>> =
            self.parties().values().flat_map(|p| p.agents.values().cloned()).collect();
        for s in slots {
            *s.attempt.lock().await = None;
        }
    }

    /// Where programs of the person's own are looked for.
    fn looked_in(&self, usual: &[Place]) -> Result<Vec<Place>, WireError> {
        match &self.programs.looked_in {
            Some(p) => p.iter().map(|p| real(p)).collect(),
            None => Ok(usual.to_vec()),
        }
    }

    /// Everything agents may write to: every session's folder, and their own.
    pub async fn written(&self, store: &Store) -> Result<Reach, WireError> {
        let all = store.sessions(WORKSPACES, 0).await.map_err(internal)?;
        let folders =
            all.into_iter().filter_map(|s| s.workspace).filter_map(|w| Place::new(w).ok());
        let own = Place::new(plain_verbatim(self.scratch.clone())).ok();
        let copies = self.copies.root().and_then(|r| real(&r).ok());
        Ok(Reach::of(folders.chain(own).chain(copies)))
    }

    pub fn harnesses(&self, written: &Reach) -> Result<Vec<HarnessInfo>, WireError> {
        let home = self.home.as_deref().ok_or_else(|| bad("there is no home folder"))?;
        let path = self.looked_in(&axiom_tools::system(&real(home)?).0)?;
        let one =
            |k: delegated::Known| Ok(k.info(&self.programs.how(&k.id, None)?, &path, written));
        self.programs.known().into_iter().map(one).collect()
    }

    /// What a person says of how a program is started.
    pub fn program(&self, s: Said, written: &Reach) -> Result<String, WireError> {
        if let (Some(a), Some(h)) = (&s.account, &s.harness) {
            self.connections.account(a, h)?;
        }
        self.programs.set(s, written)
    }

    /// How an agent's program is started.
    fn how(&self, s: &Setup) -> Result<How, WireError> {
        self.programs.how(&s.execution.harness, s.execution.connection.as_deref())
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
        // By its long name, on a system where folders have two.
        private_dir(&self.scratch).map_err(internal)?;
        let scratch =
            real(&self.scratch)?.join(&plain(&agent.0)).map_err(|e| bad(e.to_string()))?;
        if make {
            private_dir(scratch.as_path()).map_err(internal)?;
            // The agent writes there, and could have put a link in its place: what it may write
            // would be what that leads to.
            if real(scratch.as_path())? != scratch {
                return Err(bad("the agent's own folder leads elsewhere"));
            }
        }
        let (path, system) = axiom_tools::system(&home);
        let write =
            if access == Access::Read { Reach::none() } else { Reach::of([workspace.clone()]) };
        // The rules are the folder's, also where one works in a copy of it.
        let policy = self.policy(&real(Path::new(s.folder()))?)?;
        Ok(Site {
            caps: Caps {
                read: Reach::of([workspace.clone(), scratch.clone()]),
                write,
                net: Net::none(),
                web: true,
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

    /// In the agent's own folder, where it is shown the folder's side of a clash.
    pub fn beside(&self, agent: &AgentId) -> Result<Place, WireError> {
        private_dir(&self.scratch).map_err(internal)?;
        let mut at = real(&self.scratch)?;
        for n in [plain(&agent.0).as_str(), "project"] {
            at = at.join(n).map_err(|e| bad(e.to_string()))?;
            private_dir(at.as_path()).map_err(internal)?;
            if real(at.as_path())? != at {
                return Err(bad("the agent's own folder leads elsewhere"));
            }
        }
        // What was there the last time is not this time's.
        std::fs::remove_dir_all(at.as_path()).map_err(internal)?;
        private_dir(at.as_path()).map_err(internal)?;
        Ok(at)
    }

    /// What a folder would have agents told, for a person to read.
    pub fn look(&self, path: &str) -> Result<Vec<Instructions>, WireError> {
        let place = real(Path::new(path))?;
        Ok(FOR_AGENTS
            .iter()
            .filter_map(|name| {
                let (digest, text) = words(&place, name)?;
                Some(Instructions { path: (*name).into(), digest, text: Some(text) })
            })
            .collect())
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

    /// A key for one agent's program, and the file that holds it. Both go with what is returned.
    pub fn grant(
        self: &Arc<Self>,
        s: &SessionId,
        a: &AgentId,
        scratch: &Place,
        (all, plan): (bool, bool),
        words: Option<&str>,
    ) -> Result<Grant, String> {
        let mut b = [0u8; 32];
        axiom_proto::random(&mut b)?;
        let key: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let file = scratch.as_path().join(format!("peer-{}.json", &key[..12]));
        let way = mcp::Way { data: self.data.clone(), grant: key.clone() };
        let text = serde_json::to_string(&way).map_err(|e| e.to_string())?;
        private_file(&file, &text).map_err(|e| e.to_string())?;
        self.grants
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .insert(key.clone(), Holder { session: s.clone(), agent: a.clone(), all, plan });
        // Whatever goes wrong from here on, the key and its file go again.
        let mut grant = Grant { agents: self.clone(), key, file, words: None };
        if let Some(w) = words {
            let f = grant.file.with_extension("md");
            private_file(&f, w).map_err(|e| e.to_string())?;
            grant.words = Some(f);
        }
        Ok(grant)
    }

    /// Whose key it is.
    pub fn granted(&self, key: &str) -> Option<Holder> {
        self.grants.lock().unwrap_or_else(PoisonError::into_inner).get(key).cloned()
    }

    /// Adds to the audit record (SI-23).
    pub async fn record(
        &self,
        store: &Store,
        session: &SessionId,
        d: Deed,
        containment: Containment,
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
            containment,
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

/// A file of the workspace's top folder, if it is text of a size to be followed.
fn words(workspace: &Place, name: &str) -> Option<(String, String)> {
    let place = workspace.join(name).ok()?;
    let spot = axiom_tools::fs::find(std::slice::from_ref(workspace), &place).ok()?;
    let c = spot.read(INSTRUCTIONS).ok()?;
    Some((c.digest, String::from_utf8(c.bytes).ok()?))
}

/// What is followed, and what is not any more because it is not what the person read.
fn followed(workspace: &Place, all: &[Instructions]) -> (String, Vec<String>) {
    let (mut text, mut not) = (String::new(), Vec::new());
    for i in all {
        match words(workspace, &i.path) {
            Some((digest, t)) if digest == i.digest => {
                text += &format!("From {}:\n\n{}\n\n", i.path, t.trim());
            }
            _ => not.push(i.path.clone()),
        }
    }
    (text, not)
}

/// A name for a folder, from an id that may be anybody's.
pub fn plain(id: &str) -> String {
    id.chars().filter(|c| c.is_ascii_alphanumeric() || *c == '_').take(64).collect()
}

pub fn real(p: &Path) -> Result<Place, WireError> {
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

fn real_file(p: &Path) -> Option<Place> {
    let c = plain_verbatim(std::fs::canonicalize(p).ok()?);
    c.is_file().then(|| Place::new(c).ok()).flatten()
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
    fn others(&self) -> Vec<Spec> {
        let mut all = peers::specs();
        all.push(planning::todo());
        all
    }
    async fn judge(&self, tool: &str, args: &Value) -> Option<Other> {
        match tool {
            peers::TOOL => Some(peers::judge(self, args).await),
            planning::TODO => Some(planning::judge(args)),
            planning::PLAN => Some(planning::judge_plan(self, args).await),
            _ => None,
        }
    }
    async fn perform(&self, tool: &str, args: &Value) -> Output {
        match tool {
            planning::TODO => planning::perform(self, args).await,
            planning::PLAN => planning::hand_in(self, args).await,
            _ => peers::perform(self, args).await,
        }
    }
    async fn say(&self, all: Vec<Say>) -> Result<(), Refused> {
        self.session.say(all).await.map_err(refused)
    }
    async fn sync(&self) -> Result<(), Refused> {
        self.session.sync().await.map(drop).map_err(|_| Refused::Closed)
    }
    async fn remember(&self, n: u64, turn: String, heard: Option<ItemId>) -> Result<bool, Refused> {
        let (s, a) = (self.session.id.clone(), self.agent.clone());
        self.store.put_message(s, a, n, turn, heard).await.map_err(|e| Refused::This(e.to_string()))
    }
    async fn mark(&self, item: &ItemId, n: u64, with: bool) {
        let (s, a) = (self.session.id.clone(), self.agent.clone());
        let _ = self.store.put_cut(s, a, item.clone(), n, with).await;
    }
    async fn closed(&self) {
        self.session.closed().await;
    }
    async fn audit(&self, deed: Deed) -> Result<(), Refused> {
        self.agents
            .record(&self.store, &self.session.id, deed, Containment::Axiom)
            .await
            .map_err(|e| Refused::This(format!("it cannot be put on record: {e}")))
    }
}

impl Floor {
    /// Of what Axiom does not contain itself.
    pub async fn record(&self, deed: Deed, c: Containment) -> Result<(), Refused> {
        self.agents
            .record(&self.store, &self.session.id, deed, c)
            .await
            .map_err(|e| Refused::This(format!("it cannot be put on record: {e}")))
    }
    /// The run it is in.
    pub async fn run(&self) -> Option<RunId> {
        self.agents.slot(&self.session.id, &self.agent).now().await.map(|n| n.0)
    }
    /// How this agent is set up.
    pub async fn setup(&self) -> Option<Setup> {
        let all = self.store.agents(self.session.id.clone()).await.ok()?;
        let (_, text) = all.into_iter().find(|(a, _)| *a == self.agent)?;
        serde_json::from_str(&text).ok()
    }
    /// Where this agent works.
    pub async fn workspace(&self) -> Option<String> {
        self.setup().await.map(|s| s.workspace)
    }
    /// What was said waits until it is heard.
    pub async fn wait(&self, item: &ItemId, text: &str) -> Result<(), Refused> {
        let (s, a) = (self.session.id.clone(), self.agent.clone());
        self.store
            .put_pending(s, a, item.clone(), text.to_owned())
            .await
            .map_err(|e| Refused::This(e.to_string()))
    }
    /// Those that still waited.
    pub async fn heard(&self, items: Vec<ItemId>) -> Result<Vec<ItemId>, Refused> {
        let (s, a) = (self.session.id.clone(), self.agent.clone());
        self.store.heard(s, a, items).await.map_err(|e| Refused::This(e.to_string()))
    }
    /// Keeps what a delegated agent is to be gone on with. If that fails it begins anew, some day.
    pub async fn resume(&self, with: Option<String>) {
        self.keep(|s| s.resume = with).await;
    }
    pub async fn began(&self) {
        self.keep(|s| s.began = true).await;
    }
    pub async fn keep(&self, f: impl FnOnce(&mut Setup)) {
        let Ok(all) = self.store.agents(self.session.id.clone()).await else { return };
        let Some((_, text)) = all.into_iter().find(|(a, _)| *a == self.agent) else { return };
        let Ok(mut s) = serde_json::from_str::<Setup>(&text) else { return };
        let was = s.clone();
        f(&mut s);
        if s == was {
            return;
        }
        if let Ok(text) = serde_json::to_string(&s) {
            let _ = self.store.put_agent(self.session.id.clone(), self.agent.clone(), text).await;
        }
    }
}

/// An agent ready to run.
pub enum Worker {
    Native(Box<Agent<Floor>>),
    Delegated(Box<delegated::Worker>),
}

impl Worker {
    pub async fn load(
        agents: &Arc<Agents>,
        store: &Store,
        session: Session,
        agent: &AgentId,
        setup: &Setup,
        inbox: mpsc::Receiver<Word>,
    ) -> Result<Self, WireError> {
        if setup.execution.harness == "axiom" {
            let a = load(agents, store, session, agent, setup, inbox).await?;
            return Ok(Self::Native(Box::new(a)));
        }
        let floor = Floor {
            agents: agents.clone(),
            store: store.clone(),
            session: session.clone(),
            agent: agent.clone(),
        };
        let waits = store.pending(session.id.clone(), agent.clone()).await.map_err(internal)?;
        let written = agents.written(store).await?;
        let (a, s, ag) = (agents.clone(), setup.clone(), agent.clone());
        let (known, how, site, access, workspace, words) = tokio::task::spawn_blocking(move || {
            let known = a.programs.known_as(&s.execution.harness).ok_or_else(|| {
                bad("this host does not know the agent's harness: its program was removed")
            })?;
            let site = a.site(&s, &ag, true)?;
            let mut path = a.looked_in(&site.path)?;
            let how = a.how(&s)?;
            let access = site.access;
            let mut told = HANDS.to_owned();
            for (so, words) in [(s.apart.is_some(), copies::WORDS), (s.plan, planning::WORDS)] {
                if so {
                    told = format!("{told}\n\n{words}");
                }
            }
            let words = s.hands().then(|| axiom_engine::context::system(&site, &told));
            let program = known
                .check(&s, &how, access, site.workspace.clone(), &path, &written)
                .map_err(bad)?;
            if let Some(p) = &how.path {
                // What it starts itself often lies beside it.
                let beside =
                    axiom_harness::find::beside(p, &written).map_err(|e| bad(e.to_string()))?;
                path.retain(|p| *p != beside);
                path.insert(0, beside);
            }
            let vars = a.programs.vars(&how)?;
            let config = match &s.execution.connection {
                Some(c) => Some(real(&a.connections.account(c, &s.execution.harness)?)?),
                None => None,
            };
            let site_of = axiom_harness::Site {
                program,
                config,
                vars,
                path,
                home: site.keep.home().clone(),
                user: user()?,
                tmp: site.scratch.clone(),
                written,
                limits: crate::limits::program(),
            };
            Ok::<_, WireError>((known, how, site_of, access, site.workspace, words))
        })
        .await
        .map_err(internal)??;
        Ok(Self::Delegated(Box::new(delegated::Worker {
            floor,
            desk: agents.desk(&session.id),
            slot: agents.slot(&session.id, agent),
            setup: setup.clone(),
            known,
            how,
            site,
            access,
            workspace,
            bridge: agents.bridge.clone(),
            words,
            inbox,
            waits,
        })))
    }

    /// Its next run only makes room in its conversation.
    pub fn compacts(&mut self) {
        if let Self::Native(a) = self {
            a.compact = true;
        }
    }
    pub async fn begin(
        &mut self,
        run: &RunId,
        before: Vec<Say>,
        prompt: Option<Word>,
    ) -> Result<(), Refused> {
        match self {
            Self::Native(a) => a.begin(run, before, prompt).await,
            Self::Delegated(w) => w.begin(run, before, prompt).await,
        }
    }

    /// Runs, and gives back what is still to be heard, how it ended and what it said last.
    pub async fn run(
        self,
        run: RunId,
        cancel: watch::Receiver<bool>,
    ) -> (mpsc::Receiver<Word>, Ended, String) {
        match self {
            Self::Native(mut a) => {
                let before = a.context.answers();
                let e = a.run(run, cancel).await;
                // Of this run: what it said before is no answer to what it was told now.
                let last = a.context.last_since(before);
                (a.inbox, e, last)
            }
            Self::Delegated(mut w) => {
                let e = w.run(run, cancel).await;
                let last = std::mem::take(
                    &mut *w.slot.said.lock().unwrap_or_else(PoisonError::into_inner),
                );
                (w.inbox, e, last)
            }
        }
    }
}

/// For a model whose program describes other tools than it has.
const HANDS: &str = "Your only tools are those of the server axiom (read, write, edit, patch, list, glob, search, shell, job, fetch, agent, todo). What is said elsewhere of other tools does not apply: you have none of them. A call may wait for a person's answer.";

/// Where an agent works, and under whom.
pub async fn site_of(
    agents: &Arc<Agents>,
    setup: &Setup,
    agent: &AgentId,
    slot: &Slot,
) -> Result<Site, WireError> {
    let (a, s, ag) = (agents.clone(), setup.clone(), agent.clone());
    let mut site =
        tokio::task::spawn_blocking(move || a.site(&s, &ag, true)).await.map_err(internal)??;
    let chain = slot.chain.lock().unwrap_or_else(PoisonError::into_inner);
    if !chain.is_empty() {
        site.chain.clone_from(&chain);
    }
    Ok(site)
}

/// The person's name on this computer.
fn user() -> Result<String, WireError> {
    ["USER", "LOGNAME", "USERNAME"]
        .iter()
        .filter_map(|v| std::env::var(v).ok())
        .find(|v| !v.is_empty())
        .ok_or_else(|| bad("this computer does not say who uses it"))
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
    let asked = connection.clone();
    let model = setup.execution.model.clone().unwrap_or_default();
    let listed = agents.catalogue.find("axiom", Some(&connection), &model);
    let (a, s, ag) = (agents.clone(), setup.clone(), agent.clone());
    // Links, folders, the system's store: off the runtime's own threads.
    let (client, site, (instructions, not)) = tokio::task::spawn_blocking(move || {
        let site = a.site(&s, &ag, true)?;
        // The folder's, as a person read them: not what the agent may have made of its copy.
        let mut told = followed(&real(Path::new(s.folder()))?, &s.instructions);
        if s.apart.is_some() {
            told.0 = format!("{}\n\n{}", copies::WORDS, told.0);
        }
        if s.plan {
            told.0 = format!("{}\n\n{}", planning::WORDS, told.0);
        }
        Ok::<_, WireError>((a.connections.client(&asked)?, site, told))
    })
    .await
    .map_err(internal)??;
    let kept = store.messages(id.clone(), agent.clone()).await.map_err(internal)?;
    let context = Context::load(&kept).map_err(internal)?;
    let waits = store.pending(id.clone(), agent.clone()).await.map_err(internal)?;
    let quirks = client.profile().quirks(&model);
    let messages = client.profile().codec == Codec::Messages;
    let marks = matches!(client.profile().codec, Codec::Messages | Codec::Converse);
    let settings = Settings {
        max_output: quirks
            .max_output
            .map(|m| u32::try_from(m.min(32_000)).unwrap_or(32_000))
            .or(messages.then_some(8192)),
        effort: setup.execution.effort.clone(),
        thinking: setup.execution.thinking,
        speed: setup.execution.speed.clone(),
        stop: Vec::new(),
        cache: if marks { Cache::Short } else { Cache::Off },
    };
    let floor =
        Floor { agents: agents.clone(), store: store.clone(), session, agent: agent.clone() };
    let mut site = site;
    let slot = agents.slot(&id, agent);
    {
        let chain = slot.chain.lock().unwrap_or_else(PoisonError::into_inner);
        if !chain.is_empty() {
            site.chain = chain.clone();
        }
    }
    let mut a = Agent {
        id: agent.clone(),
        desk: agents.desk(&id),
        on: UsedOn {
            harness: setup.execution.harness.clone(),
            lane: agents.connections.lane(&connection),
            provider: Some(client.profile().provider.clone()),
            connection: Some(connection),
            ..UsedOn::default()
        },
        client,
        model,
        settings,
        // As its provider's list has it, where nothing is known for certain.
        window: quirks.context.or_else(|| listed?.context),
        site,
        seen: agents.slot(&id, agent).seen.clone(),
        context,
        specs: {
            let mut all = axiom_tools::specs();
            all.extend(floor.others());
            if setup.plan {
                all.push(planning::plan());
            }
            all
        },
        stage: floor,
        instructions,
        not_followed: not,
        limits: crate::limits::run(),
        fill: None,
        told: None,
        waited: Duration::ZERO,
        last_words: None,
        noted: Default::default(),
        compact: false,
        retries: Retries::default(),
        inbox,
    };
    // What was said and not heard: before a host was gone, or a run was stopped.
    a.note(waits).await.map_err(internal)?;
    Ok(a)
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
    slot: tokio::sync::OwnedMutexGuard<Live>,
}

impl Ear {
    pub fn tell(mut self, text: String, item: ItemId) -> RunId {
        // Only what is said counts as said: whoever merely looks for room says nothing.
        if let Some(now) = self.slot.as_mut() {
            now.late |= *now.cancel.borrow();
        }
        self.permit.send(Word { text, item: Some(item), shown: None });
        self.run
    }
}

/// Holds an agent's place until its run begins or cannot.
pub struct Turnstile {
    slot: tokio::sync::OwnedMutexGuard<Live>,
}

impl Slot {
    /// What was answered to a prompt of this name.
    pub fn answered(&self, key: &str) -> Option<Value> {
        let keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        keys.iter().find(|(k, _)| k == key).map(|(_, v)| v.clone())
    }
    pub fn remember(&self, key: String, answer: Value) {
        let mut keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        keys.push_back((key, answer));
        while keys.len() > KEYS {
            keys.pop_front();
        }
    }

    /// What was said under this name came to nothing.
    pub fn forget(&self, key: &str) {
        let mut keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        keys.retain(|(k, _)| k != key);
    }

    /// Nothing is said, and nothing begins, while this is held.
    pub async fn hold(&self) -> Result<impl Sized + use<>, WireError> {
        let live = self.live.clone().lock_owned().await;
        if live.as_ref().is_some_and(|n| !n.tx.is_closed()) {
            return Err(WireError::new(ErrorCode::Busy, "the agent is at work"));
        }
        Ok(live)
    }

    /// A place in the run that is on, or the place for a new one.
    pub async fn speak(&self) -> Result<Spoken, WireError> {
        let live = self.live.clone().lock_owned().await;
        if let Some(now) = live.as_ref()
            && !now.tx.is_closed()
        {
            let busy = |words: &str| WireError::new(ErrorCode::Busy, words);
            let (run, reserved) = (now.run.clone(), now.tx.clone().try_reserve_owned());
            return match reserved {
                Ok(permit) => Ok(Spoken::Heard(Box::new(Ear { run, permit, slot: live }))),
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

    /// What it began is nobody's any more.
    pub fn forgive(&self) -> Vec<RunId> {
        self.owed().drain().map(|(r, _)| r).collect()
    }

    pub fn owed(&self) -> MutexGuard<'_, HashMap<RunId, Debt>> {
        self.owed.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// What the journal has of what it is owed, once. `live`: whether a run is on in this host. One
    /// that is not and has no end was cut short with the host before.
    pub async fn load_owed(
        &self,
        store: &Store,
        session: &SessionId,
        agent: &AgentId,
        live: impl Fn(&RunId) -> bool,
    ) -> Result<(), WireError> {
        self.owes
            .get_or_try_init(|| async {
                let rows = store.owed(session.clone(), agent.clone()).await.map_err(internal)?;
                for (run, child, ended) in rows {
                    let ended = match ended {
                        Some(t) => Some(serde_json::from_str(&t).map_err(internal)?),
                        None if live(&run) => None,
                        None => Some(Finished {
                            runs: vec![run.clone()],
                            outcome: Outcome::Interrupted,
                            error: Some("the host ended meanwhile".into()),
                            text: String::new(),
                        }),
                    };
                    self.owed().entry(run).or_insert(Debt { child, ended: ended.map(Arc::new) });
                }
                Ok(())
            })
            .await
            .map(drop)
    }

    /// The run it is in, and what stops that.
    pub async fn now(&self) -> Option<(RunId, watch::Receiver<bool>)> {
        self.live
            .lock()
            .await
            .as_ref()
            .filter(|n| !n.tx.is_closed())
            .map(|n| (n.run.clone(), n.cancel.clone()))
    }

    /// What it has running in the background ends.
    pub fn stop_jobs(&self) {
        self.seen.1.end();
    }

    /// Of a delegated agent: its program runs.
    pub async fn has_program(&self) -> bool {
        self.attempt.lock().await.as_ref().is_some_and(|a| !a.is_closed())
    }

    pub fn seen(&self) -> Seen {
        self.seen.clone()
    }

    /// Whether it is in a run.
    pub async fn busy(&self) -> bool {
        self.live.lock().await.as_ref().is_some_and(|n| !n.tx.is_closed())
    }

    /// Its runs have ended: whoever waits for that is told.
    pub fn finish(&self, f: Finished) {
        self.last.send_replace(Some(Arc::new(f)));
    }

    /// What it has running began as what it was.
    pub async fn changed(&self) {
        self.stop_jobs();
        // Its program ends; what it was told is kept by the program, and gone on with.
        *self.attempt.lock().await = None;
    }

    /// The same, and nothing of it runs any more.
    pub async fn still(&self) -> Result<(), WireError> {
        self.changed().await;
        let mut stays = self.stays.subscribe();
        let ended = async {
            self.seen.1.ended().await;
            let _ = stays.wait_for(|n| *n == 0).await;
        };
        tokio::time::timeout(STILL, ended).await.map_err(|_| {
            WireError::new(ErrorCode::Busy, "something the agent began still runs; try again")
        })?;
        Ok(())
    }

    /// After a run: whether something was said too late for it that is to begin another. If not,
    /// the place is free.
    pub async fn after(&self, rx: &mut mpsc::Receiver<Word>, stopped: bool) -> bool {
        let mut live = self.live.lock().await;
        let mut left = 0;
        while rx.try_recv().is_ok() {
            left += 1;
        }
        let late = live.as_ref().is_some_and(|n| n.late);
        // Stopped by a person: what they had said before waits for the next time they speak.
        if left == 0 || (stopped && !late) {
            rx.close();
            *live = None;
            return false;
        }
        true
    }

    pub async fn renamed(&self, run: &RunId, cancel: &watch::Receiver<bool>) {
        if let Some(n) = self.live.lock().await.as_mut() {
            n.run = run.clone();
            n.cancel = cancel.clone();
            n.late = false;
        }
    }

    pub async fn leave(&self) {
        *self.live.lock().await = None;
    }
}

impl Turnstile {
    /// A way to speak to the run that now takes the place.
    pub fn enter(mut self, run: &RunId, cancel: &watch::Receiver<bool>) -> mpsc::Receiver<Word> {
        let (tx, rx) = mpsc::channel(INBOX);
        *self.slot = Some(Now { run: run.clone(), tx, cancel: cancel.clone(), late: false });
        rx
    }
}

/// The events that make an agent known.
pub fn added(agent: &AgentId, s: &Setup, from: Option<Branched>) -> Vec<Say> {
    vec![
        Say::Event(Body::AgentAdded {
            agent: agent.clone(),
            name: s.name.clone(),
            execution: Box::new(s.execution.clone()),
            parent: None,
            access: Some(s.access),
            apart: s.apart.is_some(),
            from: from.map(Box::new),
            plan: s.plan,
        }),
        Say::Event(Body::AgentState { agent: agent.clone(), state: AgentState::Waiting }),
    ]
}
pub fn changed(agent: &AgentId, s: &Setup) -> Vec<Say> {
    vec![Say::Event(Body::AgentChanged {
        agent: agent.clone(),
        execution: Box::new(s.execution.clone()),
        access: Some(s.access),
        plan: s.plan,
    })]
}

pub fn full(n: usize) -> bool {
    n >= MOST
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// What waited when a person stopped it waits until they speak, and not until something looks for
    /// room.
    #[tokio::test]
    async fn looking_for_room_is_not_speaking() {
        for (speaks, goes_on) in [(false, false), (true, true)] {
            let slot = Slot::default();
            let (tx, mut rx) = mpsc::channel(4);
            let (_stop, cancel) = watch::channel(true);
            tx.send(Word::new("before".into())).await.unwrap();
            *slot.live.lock().await = Some(Now { run: RunId::new(), tx, cancel, late: false });
            match slot.speak().await {
                Ok(Spoken::Heard(ear)) if speaks => drop(ear.tell("x".into(), ItemId::new())),
                Ok(Spoken::Heard(ear)) => drop(ear),
                _ => unreachable!(),
            }
            assert_eq!(slot.after(&mut rx, true).await, goes_on);
        }
    }

    /// Two slots of one agent would be two runs of it.
    #[tokio::test]
    async fn who_is_at_work_is_not_forgotten() {
        let mut all: HashMap<SessionId, Party> = HashMap::new();
        let mut add = |n: &str| {
            let slot = Arc::new(Slot::default());
            let mut p = Party::default();
            p.agents.insert(AgentId(n.into()), slot.clone());
            p.agents.insert(AgentId("another".into()), Arc::default());
            all.insert(SessionId(n.into()), p);
            slot
        };
        let (works, _) = (add("works"), add("idle"));
        let wanted = add("wanted");
        let spoken_to = add("spoken to");
        let (_tx, cancel) = watch::channel(false);
        let (tx, _rx) = mpsc::channel(1);
        *works.live.lock().await = Some(Now { run: RunId::new(), tx, cancel, late: false });
        drop(works);
        let live = spoken_to.live.clone();
        drop(spoken_to);
        let _held = live.lock().await;
        forget_idle(&mut all);
        let mut left: Vec<&str> = all.keys().map(|s| s.0.as_str()).collect();
        left.sort();
        assert_eq!(left, ["spoken to", "wanted", "works"]);
        drop(wanted);
        forget_idle(&mut all);
        assert_eq!(all.len(), 2);
    }
}
