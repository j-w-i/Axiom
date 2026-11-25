//! Adapters through which Axiom drives agent programs a person has installed (decision 0006). The
//! program is the vendor's, unmodified, and signs in by itself: no adapter reads, copies or passes a
//! credential, or says it is another client than Axiom (decision 0007).
//!
//! A harness needs its own network and its own sign-in, so it runs outside Axiom's sandbox, with a
//! person's token for that. Who contains it is then the harness itself, if its sandbox is on, or
//! nobody (SI-26): [`Harness::launch`] says which, and the caller puts it on record before it starts.
//! What a harness reports is its word; that it asks before it acts is nobody's promise (T36).

use axiom_policy::{Access, Place, Reach, Unsandboxed, audit::Containment};
use axiom_spawn::{Contain, Env, Job, Running, spawn};
use tokio::{io::AsyncReadExt, sync::oneshot, time::timeout};

mod acp;
mod claude;
mod codex;
mod drive;
mod event;
pub mod find;
pub mod frames;
mod ledger;
mod rpc;
mod talk;
#[cfg(test)]
mod test;

pub use acp::{Acp, KNOWN, Known};
pub use claude::Prompts;
pub use drive::{Events, Handle, Limits, MOST_LINE, MOST_PROMPT, MOST_SUBJECT};
pub use event::*;
pub use find::{NotFound, Program, find};
pub use frames::{Frame, Frames, Lost, Way};

use talk::{Talk, loose, plain};

/// Which harness, and how it is spoken to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Harness {
    /// Any that speaks the Agent Client Protocol.
    Acp(Acp),
    /// Codex, by its app-server.
    Codex,
    /// Claude Code, by its streamed JSON.
    Claude(Prompts),
}

/// Where and as whom it runs.
#[derive(Debug, Clone)]
pub struct Site {
    pub program: Program,
    /// Where it finds other programs. Trusted: nothing an agent writes to.
    pub path: Vec<Place>,
    pub home: Place,
    /// The person's name on this computer.
    pub user: String,
    pub tmp: Place,
    /// Workspaces, and everything else an agent may write to.
    pub written: Reach,
    /// Where the program keeps what is its own, if that is another place than usual: an account
    /// beside the person's first ([`Harness::accounts`]).
    pub config: Option<Place>,
    pub limits: Limits,
}

/// What a person, or their own rule, decided about the workspace's own configuration: files in it
/// that name programs, servers and modes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trust {
    /// It may take effect: a person consented to what is there (SI-5, SI-6).
    Given,
    /// It must not. A harness that cannot be told to ignore it is not started.
    Withheld,
}

/// An attempt to be started.
#[derive(Debug, Clone)]
pub struct Start {
    pub workspace: Place,
    pub model: Option<String>,
    pub effort: Option<String>,
    pub access: Access,
    /// The session to go on with.
    pub resume: Option<String>,
    pub trust: Trust,
    pub by: By,
    /// A server of Axiom's own that it is given, if it can be and may act.
    pub tools: Option<Server>,
    /// That server's tools in place of its own, all of which are off: what it does, it asks that
    /// server for. Whoever is behind it judges and contains.
    pub hands: bool,
    /// Words for the model, in a file of the host's: what it works with and where.
    pub system: Option<Place>,
}

/// A Model Context Protocol server on standard streams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Server {
    pub name: String,
    /// The one tool it has, if it stands beside the program's own.
    pub tool: String,
    pub command: Place,
    /// No secret: they are seen by other users of the computer.
    pub args: Vec<String>,
}

/// A started attempt.
#[derive(Debug)]
pub struct Live {
    pub handle: Handle,
    pub events: Events,
    pub containment: Containment,
}

/// Says what an attempt says to nobody.
struct Nowhere;
impl Frames for Nowhere {
    async fn keep(&self, _: Frame) -> Result<(), Lost> {
        Ok(())
    }
}

impl Harness {
    /// How it would be started, and who would contain it. Nothing is started.
    pub fn launch(&self, s: &Start) -> Result<Launch, Error> {
        for (what, v) in [("a model", &s.model), ("an effort", &s.effort), ("a session", &s.resume)]
        {
            if let Some(v) = v.as_ref().filter(|v| !plain(v)) {
                return Err(Error::BadValue(what, v.clone()));
            }
        }
        if s.hands && (!self.takes_hands() || s.tools.is_none()) {
            return Err(Error::NoHands);
        }
        if self.person_only() && s.by != By::Person {
            return Err(Error::PersonOnly);
        }
        let l = match self {
            Harness::Acp(a) => a.launch(s)?,
            Harness::Codex => codex::launch(s)?,
            Harness::Claude(p) => claude::launch(*p, s)?,
        };
        // Whatever came to be in it (T34).
        match l.args.iter().find(|a| loose(a)) {
            Some(a) => Err(Error::Unsafe(a.clone())),
            None => Ok(l),
        }
    }

    /// What the program itself documents for keeping several accounts apart: a variable that names
    /// the folder of one. Only where its documentation says that logins are kept apart by it.
    pub fn accounts(&self) -> Option<&'static str> {
        match self {
            // code.claude.com/docs/en/authentication, read 2026-11-25.
            Harness::Claude(_) => Some("CLAUDE_CONFIG_DIR"),
            // CODEX_HOME moves its files; whether it parts what is in the system's store is not said.
            Harness::Codex | Harness::Acp(_) => None,
        }
    }

    /// Whether it can be given a server of Axiom's own, by what it documents.
    pub fn takes_tools(&self) -> bool {
        // Codex: the form of what it takes at a thread's start is nowhere documented.
        !matches!(self, Harness::Codex)
    }

    /// Whether all of its own tools can be switched off, so that it has only those it is given. Where
    /// its documentation says so.
    pub fn takes_hands(&self) -> bool {
        // `--tools ""`: code.claude.com/docs/en/cli-reference, read 2026-11-25.
        matches!(self, Harness::Claude(_))
    }

    /// Whether it can be told how hard to try: low, medium or high, before it is started.
    pub fn takes_effort(&self) -> bool {
        // `--effort` and `model_reasoning_effort`: code.claude.com/docs/en/cli-reference and the
        // app-server's `turn/start`, read 2026-11-25. Of an ACP agent it is known once it runs.
        matches!(self, Harness::Claude(_) | Harness::Codex)
    }

    /// Whether its terms allow only what a person themselves says.
    pub fn person_only(&self) -> bool {
        matches!(self, Harness::Acp(a) if a.person_only)
    }

    /// What it can do, without giving it work. Runs the program, in `site.tmp` and not in any
    /// workspace: the token is for that.
    pub async fn probe(&self, site: &Site, token: Unsandboxed) -> Result<Probe, Error> {
        let talk: Box<dyn Talk> = match self {
            Harness::Claude(p) => return version(self, site, token, *p).await,
            Harness::Acp(a) => Box::new(a.talk(site, None)),
            Harness::Codex => Box::new(codex::Codex::new(site, None)),
        };
        let args = match self {
            Harness::Acp(a) => a.checked()?,
            _ => codex::ARGS.map(String::from).into(),
        };
        let process = run(self, site, args, &site.tmp, false, true, token)?;
        let (tx, rx) = oneshot::channel();
        let given = drive::Given {
            process,
            talk,
            frames: Nowhere,
            limits: site.limits.clone(),
            read_only: true,
            person_only: true,
            probed: Some(tx),
        };
        let (_handle, mut events) = drive::drive(given).ok_or(Error::NoPipe)?;
        let mut words = None;
        while let Some(e) = events.next().await {
            if let Event::Closed(g) = e {
                words = g.words;
            }
        }
        rx.await.map_err(|_| Error::Probe(words.unwrap_or_else(|| "it said nothing".into())))
    }

    /// Starts an attempt. The caller has put [`Self::launch`] on record.
    pub fn start<F: Frames>(
        &self,
        site: &Site,
        s: Start,
        token: Unsandboxed,
        frames: F,
    ) -> Result<Live, Error> {
        let l = self.launch(&s)?;
        // Now and not only when it was found: the workspace may be another.
        let written = Reach::of(site.written.roots().iter().chain([&s.workspace]).cloned());
        for p in site.path.iter().chain([site.program.place()]) {
            if find::inside(p, &written) {
                return Err(Error::Inside(p.clone()));
            }
        }
        let read_only = s.access == Access::Read;
        let (cwd, hands) = (s.workspace.clone(), s.hands);
        let talk: Box<dyn Talk> = match self {
            Harness::Acp(a) => Box::new(a.talk(site, Some(s))),
            Harness::Codex => Box::new(codex::Codex::new(site, Some(s))),
            Harness::Claude(p) => Box::new(claude::Claude::new(*p, s, &l)),
        };
        let given = drive::Given {
            process: run(self, site, l.args, &cwd, hands, true, token)?,
            talk,
            frames,
            limits: site.limits.clone(),
            read_only,
            person_only: self.person_only(),
            probed: None,
        };
        let (handle, events) = drive::drive(given).ok_or(Error::NoPipe)?;
        Ok(Live { handle, events, containment: l.containment })
    }
}

fn run(
    harness: &Harness,
    site: &Site,
    args: Vec<String>,
    cwd: &Place,
    hands: bool,
    stdin: bool,
    token: Unsandboxed,
) -> Result<Running, Error> {
    let mut env = Env::harness(&site.path, &site.home, &site.user, &site.tmp)?;
    if let Some(c) = &site.config {
        // Nothing would keep it apart: it is not started as if something did.
        let name = harness.accounts().ok_or(Error::OneAccount)?;
        env.set(name, c.as_str())?;
    }
    if hands {
        // Otherwise the tools it is given are put off until a tool of its own finds them.
        env.set("ENABLE_TOOL_SEARCH", "false")?;
    }
    Ok(spawn(Job {
        program: site.program.place().clone(),
        args,
        cwd: cwd.clone(),
        env,
        stdin,
        contain: Contain::None(token),
    })?)
}

/// What `--version` says: the only thing Claude Code is asked without work.
async fn version(
    harness: &Harness,
    site: &Site,
    token: Unsandboxed,
    p: Prompts,
) -> Result<Probe, Error> {
    let mut r = run(harness, site, vec!["--version".into()], &site.tmp, false, false, token)?;
    let out = r.stdout.take().ok_or(Error::NoPipe)?;
    let mut said = Vec::new();
    let mut first = out.take(4096);
    let read = timeout(site.limits.opening, first.read_to_end(&mut said)).await;
    drop(first);
    let _ = r.stop(site.limits.stop).await;
    let bad = |w: &str| Err(Error::Probe(w.to_owned()));
    if read.is_err() {
        return bad("it did not answer in time");
    }
    let said = String::from_utf8_lossy(&said).into_owned();
    let Some(version) = claude::version(&said) else {
        return bad("it does not say which version it is");
    };
    Ok(Probe {
        program: site.program.place().clone(),
        name: Some("Claude Code".into()),
        version: Some(version),
        protocol: claude::PROTOCOL.into(),
        caps: claude::caps(p),
    })
}

/// Axiom's own name and version, as every harness is told them.
pub(crate) const CLIENT: (&str, &str, &str) = ("axiom", "Axiom", env!("CARGO_PKG_VERSION"));
