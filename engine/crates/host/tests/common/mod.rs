// Each test file uses some of it; helpers fail the test.
#![allow(dead_code, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    process::{Child, ChildStderr, Command, Stdio},
    time::Duration,
};

use axiom_host::{Config, Host, HostError, client::Client, paths::Paths};
use axiom_proto::*;

pub const LONG: Duration = Duration::from_secs(10);

pub struct Fixture {
    pub dir: tempfile::TempDir,
    pub paths: Paths,
}
pub fn fixture() -> Fixture {
    // On Linux not in /tmp, which is another one inside the sandbox.
    let dir = if cfg!(target_os = "linux") {
        tempfile::tempdir_in(env!("CARGO_TARGET_TMPDIR"))
    } else {
        tempfile::tempdir()
    }
    .unwrap();
    let paths = Paths::new(dir.path().join("data"));
    Fixture { dir, paths }
}
impl Fixture {
    pub fn serve(&self) -> tokio::task::JoinHandle<Result<(), HostError>> {
        self.serve_with(Config::new(self.paths.clone()))
    }
    pub fn serve_with(&self, c: Config) -> tokio::task::JoinHandle<Result<(), HostError>> {
        // A process another test is just starting may hold, for a moment, a copy of a lock that was
        // let go here.
        let until = std::time::Instant::now() + LONG;
        loop {
            match Host::start(c.clone()) {
                Err(HostError::AlreadyRunning) if std::time::Instant::now() < until => {
                    std::thread::sleep(Duration::from_millis(5));
                }
                h => return tokio::spawn(h.unwrap().serve()),
            }
        }
    }
    pub fn script(&self, steps: &str) -> String {
        let p = self.dir.path().join(format!("{}.jsonl", ItemId::new()));
        std::fs::write(&p, steps).unwrap();
        p.to_str().unwrap().to_owned()
    }
    /// `n` pieces of text, `after` ms apart, in one item.
    pub fn talker(&self, n: usize, after: u64) -> String {
        let mut s = String::from(
            "{\"do\":\"agent\",\"name\":\"T\"}\n{\"do\":\"start\",\"item\":\"p\",\"kind\":\"prose\"}\n",
        );
        for i in 0..n {
            s += &format!(
                "{{\"after\":{after},\"do\":\"text\",\"item\":\"p\",\"text\":\"w{i} \"}}\n"
            );
        }
        s += "{\"do\":\"end\",\"item\":\"p\",\"outcome\":\"completed\"}\n{\"do\":\"finish\",\"outcome\":\"completed\"}\n";
        self.script(&s)
    }
    pub async fn client(&self) -> Client {
        let c = Client::connect(&self.paths, "test");
        tokio::time::timeout(LONG, c).await.expect("a host that answers").unwrap()
    }
}
pub fn words(n: usize) -> String {
    (0..n).map(|i| format!("w{i} ")).collect()
}
pub async fn session(c: &mut Client) -> SessionId {
    let v = c.call(Call::SessionCreate { title: "S".into(), workspace: None }).await.unwrap();
    SessionId(v["session"].as_str().unwrap().into())
}
pub async fn replay(c: &mut Client, s: &SessionId, path: &str, speed: f64) -> RunId {
    let v =
        c.call(Call::ReplayStart { session: s.clone(), path: path.into(), speed }).await.unwrap();
    RunId(v["run"].as_str().unwrap().into())
}

/// What a client would show, rebuilt from events, checking order on the way.
#[derive(Default, Debug, PartialEq, Clone)]
pub struct View {
    pub seq: u64,
    pub text: BTreeMap<String, String>,
    pub ended: Option<Outcome>,
    pub state: Option<AgentState>,
    pub resync: bool,
}
impl View {
    pub fn take(&mut self, m: Message) {
        match m {
            Message::Events { events, .. } => {
                for e in events {
                    assert!(e.seq > self.seq, "event {} after {}", e.seq, self.seq);
                    self.seq = e.seq;
                    match e.body {
                        Body::ItemStarted { item, .. } => {
                            self.text.insert(item.0, String::new());
                        }
                        Body::ItemDelta { item, text } => {
                            self.text.get_mut(&item.0).unwrap().push_str(&text)
                        }
                        Body::ItemEnded { item, text, .. } => {
                            self.text.insert(item.0, text);
                        }
                        Body::RunEnded { outcome, .. } => self.ended = Some(outcome),
                        Body::AgentState { state, .. } => self.state = Some(state),
                        b => assert!(!b.malformed()),
                    }
                }
            }
            Message::Resync { .. } => self.resync = true,
            Message::Fault { fault, .. } => panic!("{fault}"),
            _ => {}
        }
    }
    pub async fn until_end(&mut self, c: &mut Client) {
        while self.ended.is_none() {
            let m = c.next_within(LONG).await.unwrap().expect("the run ends");
            self.take(m);
        }
    }
}

/// Never outlives its test, whatever happens to the test.
pub struct Process(pub Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn spawn(f: &Fixture) -> Process {
    spawn_as(f, &mut Command::new(env!("CARGO_BIN_EXE_axiom-host")))
}
pub fn spawn_as(f: &Fixture, command: &mut Command) -> Process {
    start(f, command.stderr(Stdio::null()))
}
/// With the other end of its stderr.
pub fn spawn_heard(f: &Fixture, command: &mut Command) -> (Process, ChildStderr) {
    let mut p = start(f, command.stderr(Stdio::piped()));
    let said = p.0.stderr.take().unwrap();
    (p, said)
}
fn start(f: &Fixture, command: &mut Command) -> Process {
    let mut c = command.env("AXIOM_DATA", &f.paths.data).stdout(Stdio::piped()).spawn().unwrap();
    let mut line = String::new();
    BufReader::new(c.stdout.take().unwrap()).read_line(&mut line).unwrap();
    let p = Process(c);
    assert_eq!(line.trim(), "ready");
    p
}

/// Nothing runs any more.
pub async fn quiet(c: &mut Client) {
    while c.call(Call::Status {}).await.unwrap()["runs"] != 0 {
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

/// The journal as it lies on disk, without bringing it into order.
pub fn journal(f: &Fixture) -> axiom_journal::Reader {
    axiom_journal::Reader::open(&f.paths.journal()).unwrap()
}
pub fn history(f: &Fixture, s: &SessionId) -> Vec<Body> {
    // A host that has ended may still be putting its file in order, for as long as the disk takes.
    let until = std::time::Instant::now() + LONG;
    let all = loop {
        match journal(f).read(s, 0, usize::MAX) {
            Err(e) if e.busy() && std::time::Instant::now() < until => {
                std::thread::sleep(Duration::from_millis(20));
            }
            r => break r.unwrap(),
        }
    };
    all.into_iter().map(|e| e.body).collect()
}
pub fn refused(r: Result<serde_json::Value, axiom_host::client::ClientError>) -> WireError {
    match r {
        Err(axiom_host::client::ClientError::Refused(e)) => e,
        r => panic!("{r:?}"),
    }
}
