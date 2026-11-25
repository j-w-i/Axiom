// A test's helpers fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::{
    collections::BTreeMap,
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

use axiom_harness::{
    Acp, Answer, By, Error, Event, Events, Frame, Frames, Gone, Handle, Harness, Kind, Limits,
    Live, Lost, Prompts, Site, Start, Took, Trust, find,
};
use axiom_policy::{Access, Answer as Yes, Ask, Place, Reach, Requests, Unsandboxed};
use serde_json::{Value, json};

/// As a person would give it.
pub fn token() -> Unsandboxed {
    let ask = Ask {
        chain: vec![],
        workspace: "w".into(),
        tool: "harness".into(),
        schema: String::new(),
        subject: "agent".into(),
        scope: "everything".into(),
        unsandboxed: true,
    };
    let mut r = Requests::default();
    let id = r.open(&ask).unwrap();
    r.answer(&id, Yes::Once, &ask.digest()).unwrap().unwrap().unsandboxed(&ask).unwrap()
}

pub struct World {
    pub dir: tempfile::TempDir,
    pub limits: Limits,
}

const FAKE: &str = env!("CARGO_BIN_EXE_fake-agent");

/// A folder with an agent that plays `script`, a home, a workspace.
pub fn world(script: &str) -> World {
    let dir = tempfile::tempdir().unwrap();
    for d in ["bin", "home", "w", "tmp"] {
        std::fs::create_dir(dir.path().join(d)).unwrap();
    }
    let agent = dir.path().join("bin/agent");
    if std::fs::hard_link(FAKE, &agent).is_err() {
        std::fs::copy(FAKE, &agent).unwrap();
    }
    std::fs::write(dir.path().join("bin/script.jsonl"), script).unwrap();
    let limits = Limits {
        opening: Duration::from_secs(30),
        cancel: Duration::from_secs(20),
        stop: Duration::from_secs(5),
        write: Duration::from_secs(20),
        ask: None,
        silence: None,
    };
    World { dir, limits }
}

impl World {
    pub fn at(&self, s: &str) -> Place {
        Place::new(std::fs::canonicalize(self.dir.path()).unwrap().join(s)).unwrap()
    }
    pub fn read(&self, s: &str) -> String {
        std::fs::read_to_string(self.dir.path().join(s)).unwrap_or_default()
    }
    pub fn seen(&self) -> Value {
        serde_json::from_str(&self.read("bin/seen.json")).unwrap()
    }
    pub fn heard(&self) -> Vec<Value> {
        self.read("bin/heard.jsonl").lines().map(|l| serde_json::from_str(l).unwrap()).collect()
    }
    pub fn site(&self) -> Site {
        let written = Reach::of([self.at("w")]);
        Site {
            program: find("agent", &[self.at("bin")], &written).unwrap(),
            path: ["/usr/bin", "/bin"].map(|p| Place::new(p).unwrap()).into(),
            home: self.at("home"),
            user: "me".into(),
            tmp: self.at("tmp"),
            written,
            config: None,
            limits: self.limits.clone(),
        }
    }
    pub fn start(&self, access: Access) -> Start {
        Start {
            workspace: self.at("w"),
            model: None,
            effort: None,
            access,
            resume: None,
            trust: Trust::Given,
            by: By::Person,
            tools: None,
            hands: false,
            system: None,
        }
    }
    pub fn run(&self, h: &Harness, access: Access) -> Run {
        self.run_with(h, self.start(access))
    }
    pub fn run_with(&self, h: &Harness, s: Start) -> Run {
        self.try_run(h, s, Kept::default()).unwrap()
    }
    pub fn try_run(&self, h: &Harness, s: Start, kept: Kept) -> Result<Run, Error> {
        let Live { handle, events, containment: _ } =
            h.start(&self.site(), s, token(), kept.clone())?;
        Ok(Run { handle, events, kept, all: vec![] })
    }
}

/// The frames of an attempt.
#[derive(Clone, Default)]
pub struct Kept {
    pub frames: Arc<Mutex<Vec<Frame>>>,
    pub broken: Arc<AtomicBool>,
    pub stuck: Arc<AtomicBool>,
}

impl Frames for Kept {
    async fn keep(&self, f: Frame) -> Result<(), Lost> {
        if self.stuck.load(Ordering::SeqCst) {
            std::future::pending::<()>().await;
        }
        if self.broken.load(Ordering::SeqCst) {
            return Err(Lost);
        }
        self.frames.lock().unwrap().push(f);
        Ok(())
    }
}

pub struct Run {
    pub handle: Handle,
    pub events: Events,
    pub kept: Kept,
    pub all: Vec<Event>,
}

impl Run {
    pub async fn next(&mut self) -> Event {
        let e = tokio::time::timeout(Duration::from_secs(60), self.events.next()).await;
        let e = e.unwrap_or_else(|_| panic!("nothing more after {:#?}", self.all));
        let e = e.unwrap_or_else(|| panic!("it is over after {:#?}", self.all));
        self.all.push(e.clone());
        e
    }
    pub async fn until(&mut self, f: impl Fn(&Event) -> bool) -> Event {
        loop {
            let e = self.next().await;
            if f(&e) {
                return e;
            }
            if let Event::Closed(g) = e {
                panic!("closed: {g:#?}\n{:#?}", self.all);
            }
        }
    }
    pub async fn ready(&mut self) {
        self.until(|e| matches!(e, Event::Ready(_))).await;
    }
    pub async fn prompt(&mut self, s: &str) -> Took {
        self.handle.prompt(s, By::Person).await.unwrap()
    }
    pub async fn asked(&mut self) -> String {
        let Event::Asked(q) = self.until(|e| matches!(e, Event::Asked(_))).await else { panic!() };
        q.id
    }
    pub async fn choose(&self, q: &str, c: &str) {
        self.handle.answer(q, Answer::Choose(c.into())).await.unwrap();
    }
    pub async fn done(&mut self) -> axiom_harness::Done {
        let Event::Done(d) = self.until(|e| matches!(e, Event::Done(_))).await else { panic!() };
        d
    }
    /// The end. Nothing comes after it.
    pub async fn closed(&mut self) -> Gone {
        loop {
            if let Event::Closed(g) = self.next().await {
                assert!(self.events.next().await.is_none());
                self.check();
                return *g;
            }
        }
    }

    /// What is true of every attempt, whatever its harness does.
    pub fn check(&self) {
        let (mut tools, mut asks, mut turn, mut ready) = (BTreeMap::new(), vec![], false, false);
        for e in &self.all {
            match e {
                Event::Ready(_) => {
                    assert!(!ready, "ready twice");
                    ready = true;
                }
                Event::Began { .. } => {
                    assert!(ready && !turn);
                    turn = true;
                }
                Event::Done(_) => {
                    assert!(turn);
                    turn = false;
                }
                Event::ToolStarted(t) => assert!(tools.insert(t.id.clone(), true).is_none()),
                Event::ToolUpdated(t) => assert_eq!(tools.get(&t.id), Some(&true)),
                Event::ToolOutput { id, .. } => assert_eq!(tools.get(id), Some(&true)),
                Event::ToolEnded(t) => assert_eq!(tools.insert(t.id.clone(), false), Some(true)),
                Event::Asked(q) => asks.push(q.id.clone()),
                Event::AskClosed { id, .. } => {
                    let at = asks.iter().position(|a| a == id).expect("it was asked");
                    asks.remove(at);
                }
                _ => {}
            }
        }
        assert!(!turn && asks.is_empty() && tools.values().all(|open| !open), "{:#?}", self.all);
        assert!(matches!(self.all.last(), Some(Event::Closed(_))));
    }

    /// The texts, put together as who listens would.
    pub fn texts(&self) -> Vec<(Kind, String)> {
        let mut all: Vec<(String, Kind, String)> = vec![];
        for e in &self.all {
            let Event::Say(s) = e else { continue };
            match all.iter_mut().find(|t| t.0 == s.id) {
                Some(t) if s.whole => t.2.clone_from(&s.text),
                Some(t) => t.2 += &s.text,
                None => all.push((s.id.clone(), s.kind, s.text.clone())),
            }
        }
        all.into_iter().map(|t| (t.1, t.2)).collect()
    }
    pub fn count(&self, f: impl Fn(&Event) -> bool) -> usize {
        self.all.iter().filter(|e| f(e)).count()
    }
}

pub fn acp() -> Harness {
    Harness::Acp(Acp { args: vec!["acp".into()], ..Acp::default() })
}
pub fn claude() -> Harness {
    Harness::Claude(Prompts::Stdio)
}

pub fn lines(all: &[Value]) -> String {
    all.iter().map(|v| format!("{v}\n")).collect()
}
pub fn fixture(name: &str) -> String {
    let at = Path::new(env!("CARGO_MANIFEST_DIR")).join("fixtures").join(name);
    std::fs::read_to_string(at).unwrap()
}

/// The first words of an ACP agent of version 1, and then `more` (the prompt has the id 2).
pub fn acp_1(more: &[Value]) -> String {
    let mut all = vec![
        json!({"hear": {"method": "initialize"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 0, "result": {"protocolVersion": 1, "agentCapabilities": {}}}}),
        json!({"hear": {"method": "session/new"}}),
        json!({"say": {"jsonrpc": "2.0", "id": 1, "result": {"sessionId": "s1"}}}),
    ];
    all.extend_from_slice(more);
    lines(&all)
}
pub fn hear_prompt() -> Value {
    json!({"hear": {"method": "session/prompt"}})
}
pub fn say(u: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": "s1", "update": u}})
}
pub fn message(t: &str) -> Value {
    say(
        json!({"sessionUpdate": "agent_message_chunk", "messageId": "m", "content": {"type": "text", "text": t}}),
    )
}
pub fn ask(id: u64, subject: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "session/request_permission", "params": {"sessionId": "s1",
        "toolCall": {"toolCallId": "c", "title": "Run", "rawInput": {"command": subject}},
        "options": [{"optionId": "yes", "name": "Yes", "kind": "allow_once"}, {"optionId": "no", "name": "No", "kind": "reject_once"}]}})
}
pub fn end_turn(stop: &str) -> Value {
    json!({"say": {"jsonrpc": "2.0", "id": 2, "result": {"stopReason": stop}}})
}

pub fn alive(mark: &str) -> usize {
    let o = std::process::Command::new("/bin/ps").args(["-axo", "stat=,args="]).output().unwrap();
    String::from_utf8_lossy(&o.stdout)
        .lines()
        .filter(|l| l.contains(mark) && l.contains("sleep") && !l.trim_start().starts_with('Z'))
        .count()
}

pub async fn within(what: &str, f: impl Fn() -> bool) {
    let t = std::time::Instant::now();
    while !f() {
        assert!(t.elapsed() < Duration::from_secs(20), "{what}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

pub fn mark(n: u32) -> String {
    format!("7{:05}{n}", std::process::id() % 100_000)
}
