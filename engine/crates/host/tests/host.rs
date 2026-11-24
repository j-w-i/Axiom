//! Through the real socket; the crash tests through the real process.

// A test's helpers fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    collections::BTreeMap,
    io::{BufRead, BufReader},
    path::Path,
    process::{Child, Command, Stdio},
    time::Duration,
};

use axiom_host::{
    Config, Host, HostError,
    client::{Client, ClientError},
    paths::Paths,
};
use axiom_proto::*;

const LONG: Duration = Duration::from_secs(10);

struct Fixture {
    dir: tempfile::TempDir,
    paths: Paths,
}
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("data"));
    Fixture { dir, paths }
}
impl Fixture {
    fn serve(&self) -> tokio::task::JoinHandle<Result<(), HostError>> {
        self.serve_with(Config::new(self.paths.clone()))
    }
    fn serve_with(&self, c: Config) -> tokio::task::JoinHandle<Result<(), HostError>> {
        let h = Host::start(c).unwrap();
        tokio::spawn(h.serve())
    }
    fn script(&self, steps: &str) -> String {
        let p = self.dir.path().join(format!("{}.jsonl", ItemId::new()));
        std::fs::write(&p, steps).unwrap();
        p.to_str().unwrap().to_owned()
    }
    /// `n` pieces of text, `after` ms apart, in one item.
    fn talker(&self, n: usize, after: u64) -> String {
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
    async fn client(&self) -> Client {
        Client::connect(&self.paths, "test").await.unwrap()
    }
}
fn words(n: usize) -> String {
    (0..n).map(|i| format!("w{i} ")).collect()
}
async fn session(c: &mut Client) -> SessionId {
    let v = c
        .call(Call::SessionCreate {
            title: "S".into(),
            workspace: None,
        })
        .await
        .unwrap();
    SessionId(v["session"].as_str().unwrap().into())
}
async fn replay(c: &mut Client, s: &SessionId, path: &str, speed: f64) -> RunId {
    let v = c
        .call(Call::ReplayStart {
            session: s.clone(),
            path: path.into(),
            speed,
        })
        .await
        .unwrap();
    RunId(v["run"].as_str().unwrap().into())
}

/// What a client would show, rebuilt from events, checking order on the way.
#[derive(Default, Debug, PartialEq, Clone)]
struct View {
    seq: u64,
    text: BTreeMap<String, String>,
    ended: Option<Outcome>,
    resync: bool,
}
impl View {
    fn take(&mut self, m: Message) {
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
                        b => assert!(!b.malformed()),
                    }
                }
            }
            Message::Resync { .. } => self.resync = true,
            _ => {}
        }
    }
    async fn until_end(&mut self, c: &mut Client) {
        while self.ended.is_none() {
            let m = c.next_within(LONG).await.unwrap().expect("the run ends");
            self.take(m);
        }
    }
}

#[tokio::test]
async fn a_run_is_seen_whole_by_every_follower() {
    let f = fixture();
    f.serve();
    let (mut a, mut b) = (f.client().await, f.client().await);
    let s = session(&mut a).await;
    for c in [&mut a, &mut b] {
        c.call(Call::Follow {
            session: s.clone(),
            after: 0,
        })
        .await
        .unwrap();
    }
    replay(&mut a, &s, &f.talker(400, 1), 4.0).await;
    let (mut va, mut vb) = (View::default(), View::default());
    va.until_end(&mut a).await;
    vb.until_end(&mut b).await;
    assert_eq!(va, vb);
    assert_eq!(va.text.values().next().unwrap(), &words(400));
    assert_eq!(va.ended, Some(Outcome::Completed));
    // A late follower reads the folded history and arrives at the same view.
    let mut late = f.client().await;
    late.call(Call::Follow {
        session: s.clone(),
        after: 0,
    })
    .await
    .unwrap();
    let mut vl = View::default();
    vl.until_end(&mut late).await;
    assert_eq!(vl.text, va.text);
    let list = a.call(Call::SessionList {}).await.unwrap();
    assert_eq!(list[0]["running"], false);
}

#[tokio::test]
async fn reconnecting_in_the_middle_loses_and_repeats_nothing() {
    let f = fixture();
    f.serve();
    let mut a = f.client().await;
    let s = session(&mut a).await;
    replay(&mut a, &s, &f.talker(1500, 1), 4.0).await;
    let mut v = View::default();
    let mut rounds = 0;
    while v.ended.is_none() {
        let mut c = f.client().await;
        c.call(Call::Follow {
            session: s.clone(),
            after: v.seq,
        })
        .await
        .unwrap();
        // Read a little, then drop the connection without a word.
        for _ in 0..3 {
            match c.next_within(Duration::from_millis(60)).await.unwrap() {
                Some(m) => v.take(m),
                None => break,
            }
        }
        rounds += 1;
        assert!(rounds < 500);
    }
    assert!(
        rounds > 3,
        "the point is to reconnect while it streams ({rounds})"
    );
    assert_eq!(v.text.values().next().unwrap(), &words(1500));
    assert!(!v.resync);
}

#[tokio::test]
async fn a_client_that_stops_reading_is_told_to_resync_and_harms_nobody() {
    let f = fixture();
    let mut cfg = Config::new(f.paths.clone());
    cfg.outbox = 4 * 1024;
    f.serve_with(cfg);
    let (mut slow, mut quick) = (f.client().await, f.client().await);
    let s = session(&mut quick).await;
    slow.call(Call::Follow {
        session: s.clone(),
        after: 0,
    })
    .await
    .unwrap();
    quick
        .call(Call::Follow {
            session: s.clone(),
            after: 0,
        })
        .await
        .unwrap();
    // Far more than the socket buffer and the outbox hold.
    let mut script = String::from("{\"do\":\"agent\",\"name\":\"T\"}\n");
    let big = "x".repeat(2000);
    for i in 0..600 {
        script += &format!(
            "{{\"do\":\"start\",\"item\":\"{i}\",\"kind\":\"prose\"}}\n{{\"do\":\"text\",\"item\":\"{i}\",\"text\":\"{big}\"}}\n{{\"after\":1,\"do\":\"end\",\"item\":\"{i}\",\"outcome\":\"completed\"}}\n"
        );
    }
    replay(&mut quick, &s, &f.script(&script), 1.0).await;
    let mut q = View::default();
    q.until_end(&mut quick).await;
    assert_eq!(q.text.len(), 600);
    let mut v = View::default();
    while !v.resync {
        let m = slow
            .next_within(LONG)
            .await
            .unwrap()
            .expect("a resync notice");
        v.take(m);
    }
    assert!(v.text.len() < 600);
    // It follows again from where it was, at its own pace, and ends up equal.
    v.resync = false;
    slow.call(Call::Follow {
        session: s.clone(),
        after: v.seq,
    })
    .await
    .unwrap();
    v.until_end(&mut slow).await;
    assert_eq!(v.text, q.text);
}

#[tokio::test]
async fn text_arrives_before_the_event_that_followed_it() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    c.call(Call::Follow {
        session: s.clone(),
        after: 0,
    })
    .await
    .unwrap();
    let script = f.script(
        "{\"do\":\"start\",\"item\":\"p\",\"kind\":\"prose\"}
{\"do\":\"text\",\"item\":\"p\",\"text\":\"Reading it now.\"}
{\"do\":\"start\",\"item\":\"t\",\"kind\":\"tool\",\"name\":\"read\"}
{\"after\":300,\"do\":\"finish\",\"outcome\":\"completed\"}",
    );
    replay(&mut c, &s, &script, 1.0).await;
    let mut seen = Vec::new();
    while !seen.contains(&"end") {
        if let Message::Events { events, .. } = c.next_within(LONG).await.unwrap().unwrap() {
            seen.extend(events.iter().filter_map(|e| match &e.body {
                Body::ItemDelta { .. } => Some("text"),
                Body::ItemStarted {
                    kind: ItemKind::Tool,
                    ..
                } => Some("tool"),
                Body::RunEnded { .. } => Some("end"),
                _ => None,
            }));
        }
    }
    assert_eq!(seen, ["text", "tool", "end"]);
}

#[tokio::test]
async fn cancelling_ends_everything_that_was_open() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    c.call(Call::Follow {
        session: s.clone(),
        after: 0,
    })
    .await
    .unwrap();
    let run = replay(&mut c, &s, &f.talker(10_000, 5), 1.0).await;
    let mut v = View::default();
    while v.text.values().next().is_none_or(String::is_empty) {
        let m = c.next_within(LONG).await.unwrap().unwrap();
        v.take(m);
    }
    let t = std::time::Instant::now();
    c.call(Call::Cancel {
        session: s.clone(),
        run: run.clone(),
    })
    .await
    .unwrap();
    v.until_end(&mut c).await;
    assert!(
        t.elapsed() < Duration::from_millis(500),
        "{:?}",
        t.elapsed()
    );
    assert_eq!(v.ended, Some(Outcome::Cancelled));
    assert!(words(10_000).starts_with(v.text.values().next().unwrap().as_str()));
    let again = c.call(Call::Cancel { session: s, run }).await;
    assert!(matches!(again, Err(ClientError::Refused(e)) if e.code == ErrorCode::NotFound));
}

#[tokio::test]
async fn following_from_the_future_is_answered_with_a_resync() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    c.call(Call::Follow {
        session: s,
        after: 999,
    })
    .await
    .unwrap();
    let m = c.next_within(LONG).await.unwrap().unwrap();
    assert!(matches!(m, Message::Resync { resync: 0, .. }), "{m:?}");
}

#[tokio::test]
async fn an_unfollowed_session_is_quiet() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    c.call(Call::Follow {
        session: s.clone(),
        after: 0,
    })
    .await
    .unwrap();
    c.call(Call::Unfollow { session: s.clone() }).await.unwrap();
    c.queue.clear();
    replay(&mut c, &s, &f.talker(50, 0), 0.0).await;
    let m = c.next_within(Duration::from_millis(200)).await.unwrap();
    assert_eq!(m, None);
}

#[tokio::test]
async fn strangers_learn_nothing() {
    let f = fixture();
    f.serve();
    f.client().await;
    let hello = |t: &str, p| Call::Hello {
        token: t.into(),
        client: "x".into(),
        protocol: p,
    };
    let token = std::fs::read_to_string(f.paths.token()).unwrap();
    for (call, code) in [
        (hello("wrong", PROTOCOL), ErrorCode::Unauthorised),
        (hello(&token[1..], PROTOCOL), ErrorCode::Unauthorised),
        (Call::SessionList {}, ErrorCode::Unauthorised),
        (hello(&token, PROTOCOL + 1), ErrorCode::Protocol),
    ] {
        let mut c = Client::raw(&f.paths).await.unwrap();
        match c.call(call).await {
            Err(ClientError::Refused(e)) => assert_eq!(e.code, code),
            r => panic!("{r:?}"),
        }
        assert!(matches!(
            c.next().await,
            Err(ClientError::Closed | ClientError::Io(_))
        ));
    }
    for garbage in [&b"\x00\xff\xfe\n"[..], b"{}\n", b"GET / HTTP/1.1\r\n\r\n"] {
        let mut c = Client::raw(&f.paths).await.unwrap();
        c.send_raw(garbage).await.unwrap();
        while let Ok(m) = c.next().await {
            assert!(matches!(m, Message::Err { .. }), "{m:?}");
        }
    }
    // The host is still well.
    f.client().await.call(Call::Status {}).await.unwrap();
}

#[tokio::test]
async fn bad_requests_do_not_end_a_known_connection() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    c.send_raw(b"nonsense\n{\"id\":77,\"method\":\"no.such\",\"params\":{}}\n")
        .await
        .unwrap();
    for want in [0, 77] {
        match c.next().await.unwrap() {
            Message::Err { id, error } => {
                assert_eq!((id, error.code), (want, ErrorCode::BadRequest));
            }
            m => panic!("{m:?}"),
        }
    }
    for title in [String::new(), " ".into(), "x".repeat(501)] {
        assert!(
            c.call(Call::SessionCreate {
                title,
                workspace: None
            })
            .await
            .is_err()
        );
    }
    let s = session(&mut c).await;
    let gone = SessionId("s_none".into());
    for call in [
        Call::Follow {
            session: gone.clone(),
            after: 0,
        },
        Call::ReplayStart {
            session: gone,
            path: "/x".into(),
            speed: 1.0,
        },
        Call::ReplayStart {
            session: s.clone(),
            path: "/no/such/file".into(),
            speed: 1.0,
        },
        Call::ReplayStart {
            session: s.clone(),
            path: "/".into(),
            speed: 1.0,
        },
        Call::ReplayStart {
            session: s.clone(),
            path: f.script("{\"do\":\"fly\"}\n"),
            speed: 1.0,
        },
        Call::ReplayStart {
            session: s,
            path: f.talker(1, 0),
            speed: -1.0,
        },
    ] {
        assert!(matches!(c.call(call).await, Err(ClientError::Refused(_))));
    }
    c.call(Call::Status {}).await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn the_endpoint_is_private() {
    use std::os::unix::fs::PermissionsExt;
    let f = fixture();
    f.serve();
    let mode = |p: &Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&f.paths.data), 0o700);
    assert_eq!(mode(&f.paths.run()), 0o700);
    assert_eq!(mode(&f.paths.token()), 0o600);
    assert_eq!(mode(Path::new(&f.paths.endpoint().unwrap())), 0o600);
}

#[cfg(unix)]
#[tokio::test]
async fn a_long_data_directory_still_gets_a_private_endpoint() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let f = Fixture {
        paths: Paths::new(d.path().join("n".repeat(120))),
        dir: d,
    };
    f.serve();
    f.client().await.call(Call::Status {}).await.unwrap();
    let e = f.paths.endpoint().unwrap();
    let dir = Path::new(&e).parent().unwrap();
    assert_eq!(
        std::fs::metadata(dir).unwrap().permissions().mode() & 0o777,
        0o700
    );
}

#[tokio::test]
async fn one_host_per_data_directory() {
    let f = fixture();
    f.serve();
    f.client().await;
    let second = Host::start(Config::new(f.paths.clone()));
    assert!(matches!(second, Err(HostError::AlreadyRunning)));
    f.client().await.call(Call::Status {}).await.unwrap();
}

#[tokio::test]
async fn shutdown_waits_for_consent_while_agents_run() {
    let f = fixture();
    let host = f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    replay(&mut c, &s, &f.talker(10_000, 5), 1.0).await;
    let busy = c.call(Call::Shutdown { force: false }).await;
    assert!(matches!(busy, Err(ClientError::Refused(e)) if e.code == ErrorCode::Busy));
    c.call(Call::Shutdown { force: true }).await.unwrap();
    tokio::time::timeout(LONG, host)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(!f.paths.token().exists());
}

#[tokio::test]
async fn an_idle_host_leaves_and_a_busy_one_stays() {
    let f = fixture();
    let mut cfg = Config::new(f.paths.clone());
    cfg.idle_exit = Some(Duration::from_millis(150));
    let host = f.serve_with(cfg);
    let mut c = f.client().await;
    let s = session(&mut c).await;
    replay(&mut c, &s, &f.talker(100, 5), 1.0).await;
    drop(c); // the window is closed; the work goes on (PR-14)
    tokio::time::sleep(Duration::from_millis(250)).await;
    assert!(!host.is_finished());
    tokio::time::timeout(LONG, host)
        .await
        .unwrap()
        .unwrap()
        .unwrap();
    let j = axiom_journal::Journal::open(&f.paths.journal()).unwrap();
    assert!(
        !j.session(&s).unwrap().unwrap().running,
        "the run finished without a client"
    );
}

// ---- the real process ----

/// Never outlives its test, whatever happens to the test.
struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn spawn(f: &Fixture) -> Process {
    let mut c = Command::new(env!("CARGO_BIN_EXE_axiom-host"))
        .env("AXIOM_DATA", &f.paths.data)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let mut line = String::new();
    BufReader::new(c.stdout.take().unwrap())
        .read_line(&mut line)
        .unwrap();
    assert_eq!(line.trim(), "ready");
    Process(c)
}

#[tokio::test]
async fn killed_in_the_middle_nothing_seen_is_lost() {
    let f = fixture();
    let mut p = spawn(&f);
    let mut c = f.client().await;
    let s = session(&mut c).await;
    c.call(Call::Follow {
        session: s.clone(),
        after: 0,
    })
    .await
    .unwrap();
    replay(&mut c, &s, &f.talker(10_000, 1), 4.0).await;
    let mut before = View::default();
    while before.text.values().next().is_none_or(|t| t.len() < 400) {
        let m = c.next_within(LONG).await.unwrap().unwrap();
        before.take(m);
    }
    p.0.kill().unwrap();
    p.0.wait().unwrap();
    while let Ok(Some(m)) = c.next_within(Duration::from_millis(200)).await {
        before.take(m);
    }
    assert!(
        f.paths.run().exists(),
        "a killed host cleans nothing; the next one must cope"
    );

    let mut p = spawn(&f);
    let mut c = f.client().await;
    c.call(Call::Follow {
        session: s.clone(),
        after: before.seq,
    })
    .await
    .unwrap();
    let mut after = before.clone();
    after.until_end(&mut c).await;
    assert_eq!(after.ended, Some(Outcome::Interrupted));
    let (b, a) = (
        before.text.values().next().unwrap(),
        after.text.values().next().unwrap(),
    );
    assert!(a.starts_with(b.as_str()), "shown text must survive");
    assert!(words(10_000).starts_with(a.as_str()));
    let list = c.call(Call::SessionList {}).await.unwrap();
    assert_eq!(list[0]["running"], false);
    c.call(Call::Shutdown { force: false }).await.unwrap();
    assert!(p.0.wait().unwrap().success());
}

#[tokio::test]
async fn a_second_process_is_turned_away() {
    let f = fixture();
    let mut p = spawn(&f);
    let out = Command::new(env!("CARGO_BIN_EXE_axiom-host"))
        .env("AXIOM_DATA", &f.paths.data)
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(3));
    f.client()
        .await
        .call(Call::Shutdown { force: false })
        .await
        .unwrap();
    p.0.wait().unwrap();
}
