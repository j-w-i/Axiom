#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, dead_code)]

use std::{
    collections::VecDeque,
    path::PathBuf,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use axiom_engine::{Agent, Context, Deed, Desk, Ended, Limits, Refused, Retries, Say, Stage, Turn};
use axiom_model::{Client, Endpoint, Options, Profile, Settings};
use axiom_policy::{Access, Budget, Caps, Keep, Net, Place, Policy, Reach, Tools};
use axiom_proto::{AgentId, Body, ItemId, ItemKind, Outcome, RequestId, RunId};
use axiom_tools::{Seen, Site, specs, system};
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::{mpsc, watch},
};

/// One answer of the server.
#[derive(Clone)]
pub struct Reply {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<String>,
    /// The connection ends before the answer does.
    pub cut: bool,
    hang: bool,
}
impl Reply {
    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
    /// After what it says, nothing more comes.
    pub fn hangs(mut self) -> Self {
        self.hang = true;
        self
    }
}

fn data(v: Value) -> String {
    format!("data: {v}\n\n")
}
fn chunk(delta: Value, finish: Value) -> String {
    data(
        json!({"id": "x", "model": "m", "choices": [{"index": 0, "delta": delta, "finish_reason": finish}]}),
    )
}
fn ends(finish: &str) -> Vec<String> {
    vec![
        chunk(json!({}), json!(finish)),
        data(
            json!({"id": "x", "choices": [], "usage": {"prompt_tokens": 50, "completion_tokens": 5, "total_tokens": 55}}),
        ),
        "data: [DONE]\n\n".into(),
    ]
}
fn sse(body: Vec<String>) -> Reply {
    Reply {
        status: 200,
        headers: vec![("content-type".into(), "text/event-stream".into())],
        body,
        cut: false,
        hang: false,
    }
}
pub fn text(parts: &[&str]) -> Reply {
    let mut b: Vec<String> =
        parts.iter().map(|p| chunk(json!({"content": p}), Value::Null)).collect();
    b.extend(ends("stop"));
    sse(b)
}
pub fn cut(part: &str) -> Reply {
    Reply { cut: true, ..sse(vec![chunk(json!({"content": part}), Value::Null)]) }
}
pub fn calls(all: &[(&str, &str, &str)]) -> Reply {
    let mut b: Vec<String> = all
        .iter()
        .enumerate()
        .map(|(i, (id, name, args))| {
            chunk(
                json!({"tool_calls": [{"index": i, "id": id, "type": "function", "function": {"name": name, "arguments": args}}]}),
                Value::Null,
            )
        })
        .collect();
    b.extend(ends("tool_calls"));
    sse(b)
}
pub fn status(code: u16, body: &str) -> Reply {
    Reply {
        status: code,
        headers: vec![("content-type".into(), "application/json".into())],
        body: vec![body.into()],
        cut: false,
        hang: false,
    }
}

#[derive(Clone)]
pub struct Server {
    pub url: String,
    script: Arc<Mutex<VecDeque<Reply>>>,
    requests: Arc<Mutex<Vec<Value>>>,
    closed: Arc<Mutex<usize>>,
}

impl Server {
    pub async fn start(script: Vec<Reply>) -> Self {
        let l = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let s = Self {
            url: format!("http://127.0.0.1:{}/v1", l.local_addr().unwrap().port()),
            script: Arc::new(Mutex::new(script.into())),
            requests: Arc::default(),
            closed: Arc::default(),
        };
        let me = s.clone();
        tokio::spawn(async move {
            while let Ok((mut c, _)) = l.accept().await {
                let me = me.clone();
                tokio::spawn(async move { me.serve(&mut c).await });
            }
        });
        s
    }
    async fn serve(&self, c: &mut tokio::net::TcpStream) -> Option<()> {
        let mut buf = Vec::new();
        let head = loop {
            let mut b = [0; 8192];
            let n = c.read(&mut b).await.ok().filter(|n| *n > 0)?;
            buf.extend_from_slice(&b[..n]);
            if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                break i + 4;
            }
        };
        let text = String::from_utf8_lossy(&buf[..head]).to_lowercase();
        let len: usize =
            text.lines().find_map(|l| l.strip_prefix("content-length: "))?.trim().parse().ok()?;
        while buf.len() < head + len {
            let mut b = [0; 8192];
            let n = c.read(&mut b).await.ok().filter(|n| *n > 0)?;
            buf.extend_from_slice(&b[..n]);
        }
        self.requests.lock().unwrap().push(serde_json::from_slice(&buf[head..head + len]).ok()?);
        let r = self
            .script
            .lock()
            .unwrap()
            .pop_front()
            .unwrap_or_else(|| status(418, "no more answers"));
        let mut h = format!(
            "HTTP/1.1 {} X\r\ntransfer-encoding: chunked\r\nconnection: close\r\n",
            r.status
        );
        for (k, v) in &r.headers {
            h += &format!("{k}: {v}\r\n");
        }
        c.write_all(format!("{h}\r\n").as_bytes()).await.ok()?;
        for b in &r.body {
            if (r.cut || r.hang) && (b.contains("finish_reason\":\"") || b.contains("[DONE]")) {
                break;
            }
            c.write_all(format!("{:x}\r\n{b}\r\n", b.len()).as_bytes()).await.ok()?;
            c.flush().await.ok()?;
        }
        if r.hang {
            let mut b = [0; 16];
            let _ = c.read(&mut b).await;
            *self.closed.lock().unwrap() += 1;
            return None;
        }
        if !r.cut {
            c.write_all(b"0\r\n\r\n").await.ok()?;
        }
        Some(())
    }
    pub fn add(&self, r: Reply) {
        self.script.lock().unwrap().push_back(r);
    }
    pub fn requests(&self) -> Vec<Value> {
        self.requests.lock().unwrap().clone()
    }
    pub async fn closed(&self) {
        let now = Instant::now();
        while *self.closed.lock().unwrap() == 0 {
            assert!(now.elapsed() < Duration::from_secs(20));
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
}

#[derive(Default)]
struct Kept {
    said: Vec<Say>,
    turns: Vec<String>,
    deeds: Vec<Deed>,
    log: Vec<String>,
    ops: usize,
    dies_at: Option<usize>,
    tool: String,
}

#[derive(Clone, Default)]
pub struct Memory(Arc<Mutex<Kept>>);

impl Memory {
    fn op(&self) -> Result<std::sync::MutexGuard<'_, Kept>, Refused> {
        let mut k = self.0.lock().unwrap();
        if k.dies_at.is_some_and(|d| k.ops >= d) {
            return Err(Refused::Closed);
        }
        k.ops += 1;
        Ok(k)
    }
    pub fn ops(&self) -> usize {
        self.0.lock().unwrap().ops
    }
    pub fn dies_at(&self, k: usize) {
        self.0.lock().unwrap().dies_at = Some(k);
    }
    pub fn lives(&self) {
        self.0.lock().unwrap().dies_at = None;
    }
    pub fn all(&self) -> Vec<Say> {
        self.0.lock().unwrap().said.clone()
    }
    pub fn turns(&self) -> Vec<String> {
        self.0.lock().unwrap().turns.clone()
    }
    pub fn keep(&self, t: &Turn) {
        self.0.lock().unwrap().turns.push(serde_json::to_string(t).unwrap());
    }
    pub fn deeds(&self) -> Vec<Deed> {
        self.0.lock().unwrap().deeds.clone()
    }
    pub fn log(&self) -> Vec<String> {
        self.0.lock().unwrap().log.clone()
    }
    pub fn before(&self, a: &str, b: &str) -> bool {
        let l = self.log();
        let at = |x: &str| l.iter().position(|y| y == x);
        matches!((at(a), at(b)), (Some(x), Some(y)) if x < y)
    }
    fn items(&self, kind: ItemKind) -> Vec<(ItemId, String)> {
        self.all()
            .into_iter()
            .filter_map(|s| match s {
                Say::Event(Body::ItemStarted { item, kind: k, name, .. }) if k == kind => {
                    Some((item, name))
                }
                _ => None,
            })
            .collect()
    }
    pub fn texts(&self, kind: ItemKind) -> Vec<String> {
        let all = self.all();
        self.items(kind)
            .into_iter()
            .map(|(i, _)| {
                all.iter()
                    .filter_map(|s| match s {
                        Say::Text(j, t) if *j == i => Some(t.as_str()),
                        _ => None,
                    })
                    .collect()
            })
            .collect()
    }
    fn end(&self, i: &ItemId) -> Option<Outcome> {
        self.all().into_iter().find_map(|s| match s {
            Say::End(j, o, _) if j == *i => Some(o),
            _ => None,
        })
    }
    pub fn outcomes(&self, kind: ItemKind) -> Vec<Outcome> {
        self.items(kind).iter().filter_map(|(i, _)| self.end(i)).collect()
    }
    pub fn tools(&self) -> Vec<(String, Outcome)> {
        self.items(ItemKind::Tool)
            .into_iter()
            .filter_map(|(i, n)| Some((n, self.end(&i)?)))
            .collect()
    }
    pub fn ended(&self) -> Vec<Outcome> {
        self.all()
            .into_iter()
            .filter_map(|s| match s {
                Say::Event(Body::RunEnded { outcome, .. }) => Some(outcome),
                _ => None,
            })
            .collect()
    }
    pub fn usage(&self) -> usize {
        self.all().iter().filter(|s| matches!(s, Say::Event(Body::Usage { .. }))).count()
    }
    pub fn requests(&self) -> usize {
        self.all().iter().filter(|s| matches!(s, Say::Event(Body::RequestOpened { .. }))).count()
    }
    pub fn answers(&self) -> Vec<String> {
        self.all()
            .into_iter()
            .filter_map(|s| match s {
                Say::Event(Body::RequestClosed { answer, .. }) => Some(answer),
                _ => None,
            })
            .collect()
    }
    pub async fn until(&self, f: impl Fn(&Self) -> bool) {
        let now = Instant::now();
        while !f(self) {
            assert!(now.elapsed() < Duration::from_secs(20), "{:?}", self.log());
            tokio::time::sleep(Duration::from_millis(2)).await;
        }
    }
    /// The question, once it is asked.
    pub async fn request(&self) -> (RequestId, String, String) {
        self.until(|s| s.requests() > 0).await;
        self.all()
            .into_iter()
            .find_map(|s| match s {
                Say::Event(Body::RequestOpened { request, digest, subject, .. }) => {
                    Some((request, digest, subject))
                }
                _ => None,
            })
            .unwrap()
    }
}

impl Stage for Memory {
    async fn say(&self, all: Vec<Say>) -> Result<(), Refused> {
        let mut k = self.op()?;
        for s in &all {
            match s {
                Say::Event(b) => {
                    b.check().map_err(|e| Refused::This(e.to_string()))?;
                    if let Body::ItemStarted { kind: ItemKind::Tool, name, .. } = b {
                        k.tool = name.clone();
                        k.log.push(format!("start {name}"));
                    }
                }
                Say::End(_, _, Some(_)) => {
                    let l = format!("end {}", k.tool);
                    k.log.push(l);
                }
                _ => {}
            }
        }
        k.said.extend(all);
        Ok(())
    }
    async fn sync(&self) -> Result<(), Refused> {
        let mut k = self.op()?;
        let l = format!("sync {}", k.tool);
        k.log.push(l);
        Ok(())
    }
    async fn remember(&self, n: u64, turn: String) -> Result<(), Refused> {
        let mut k = self.op()?;
        assert_eq!(n, k.turns.len() as u64, "a gap or a repeat");
        k.turns.push(turn);
        Ok(())
    }
    async fn audit(&self, deed: Deed) -> Result<(), Refused> {
        let mut k = self.op()?;
        k.log.push(format!("audit {}", deed.tool));
        k.deeds.push(deed);
        Ok(())
    }
}

pub struct T {
    _d: Arc<tempfile::TempDir>,
    pub root: PathBuf,
    pub server: Server,
    pub stage: Memory,
    pub desk: Desk,
    pub inbox: mpsc::Sender<String>,
    rx: Mutex<Option<mpsc::Receiver<String>>>,
    access: Access,
    profile: Profile,
}

impl T {
    fn make(
        d: Arc<tempfile::TempDir>,
        access: Access,
        server: Server,
        stage: Memory,
        profile: Profile,
    ) -> Self {
        let root = std::fs::canonicalize(d.path()).unwrap();
        let (inbox, rx) = mpsc::channel(8);
        Self {
            _d: d,
            root,
            server,
            stage,
            desk: Desk::default(),
            inbox,
            rx: Mutex::new(Some(rx)),
            access,
            profile,
        }
    }
    pub async fn new(access: Access, script: Vec<Reply>) -> Self {
        let d = tempfile::tempdir().unwrap();
        for x in ["w", "tmp", "home", "own"] {
            std::fs::create_dir(d.path().join(x)).unwrap();
        }
        std::fs::write(d.path().join("w/a.txt"), "one\ntwo\n").unwrap();
        let server = Server::start(script).await;
        let profile = Profile::openai_compatible("fake", server.url.clone());
        Self::make(Arc::new(d), access, server, Memory::default(), profile)
    }
    /// The same folder and journal, with another host.
    pub async fn again(t: &T, script: Vec<Reply>) -> Self {
        let server = Server::start(script).await;
        let profile = Profile::openai_compatible("fake", server.url.clone());
        Self::make(t._d.clone(), t.access, server, t.stage.clone(), profile)
    }
    pub async fn live() -> Self {
        let mut t = Self::new(Access::Work, vec![]).await;
        t.profile = Profile::lm_studio();
        t.server.url = "http://localhost:1234/v1".into();
        t
    }
    pub fn agent(&self) -> Agent<Memory> {
        let p = |s: &str| Place::new(self.root.join(s)).unwrap();
        let (path, system) = system(&p("home"));
        let endpoint = Endpoint::parse(&self.server.url).unwrap();
        let id = AgentId("a".into());
        Agent {
            id: id.clone(),
            stage: self.stage.clone(),
            desk: self.desk.clone(),
            client: Client::new(self.profile.clone(), endpoint, None, Options::default()).unwrap(),
            model: "m".into(),
            settings: Settings::default(),
            window: Some(1000),
            site: Site {
                workspace: p("w"),
                caps: Caps {
                    read: Reach::of([p("w"), p("tmp")]),
                    write: Reach::of([p("w")]),
                    net: Net::none(),
                    run: true,
                    tools: Tools::All,
                    budget: Budget::default(),
                },
                keep: Keep::new(p("home"), vec![p("own")]),
                access: self.access,
                policy: Policy::default(),
                system,
                scratch: p("tmp"),
                path,
                chain: vec![id],
            },
            seen: Seen::default(),
            context: Context::default(),
            specs: specs(),
            instructions: String::new(),
            limits: Limits::default(),
            retries: Retries {
                most: 3,
                first: Duration::from_millis(1),
                longest: Duration::from_secs(5),
            },
            inbox: self.rx.lock().unwrap().take().unwrap_or_else(|| mpsc::channel(1).1),
        }
    }
    pub async fn run(&self, a: &mut Agent<Memory>, prompt: &str) -> Ended {
        let (_tx, rx) = watch::channel(false);
        self.run_with(a, prompt, rx).await
    }
    pub async fn run_with(
        &self,
        a: &mut Agent<Memory>,
        prompt: &str,
        cancel: watch::Receiver<bool>,
    ) -> Ended {
        let run = a.run(RunId::new(), prompt.into(), cancel);
        tokio::time::timeout(Duration::from_secs(60), run).await.expect("it ends")
    }
    pub fn file(&self, s: &str) -> Option<String> {
        std::fs::read_to_string(self.root.join("w").join(s)).ok()
    }
}
