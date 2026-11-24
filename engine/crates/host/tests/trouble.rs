//! What goes wrong: long lines, strangers, damaged and locked journals, stopping.

// A test's helpers fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

#[cfg(unix)]
use axiom_host::paths::Paths;
use axiom_host::{
    Config,
    client::{Client, ClientError},
};
use axiom_proto::*;
#[cfg(unix)]
use std::process::Command;

mod common;
use common::*;

fn item(name: &str, text: &str) -> String {
    format!(
        "{{\"do\":\"start\",\"item\":\"{name}\",\"kind\":\"tool\"}}\n\
         {{\"do\":\"text\",\"item\":\"{name}\",\"text\":\"{text}\"}}\n\
         {{\"do\":\"end\",\"item\":\"{name}\",\"outcome\":\"completed\"}}\n"
    )
}
const AGENT: &str = "{\"do\":\"agent\",\"name\":\"T\"}\n";
const FINISH: &str = "{\"do\":\"finish\",\"outcome\":\"completed\"}\n";

/// `n` events that are flushed to the drive, the first `after` ms in, and then no end to wait for:
/// how long the flushing takes depends on the drive.
fn asks(n: usize, after: u64) -> String {
    let ask = |i| {
        let after = if i == 0 { after } else { 0 };
        format!(
            "{{\"after\":{after},\"do\":\"ask\",\"request\":\"{i}\",\"what\":\"w\",\"subject\":\"s\",\"scope\":\"\"}}\n"
        )
    };
    AGENT.to_owned()
        + &(0..n).map(ask).collect::<String>()
        + "{\"after\":50000,\"do\":\"finish\",\"outcome\":\"completed\"}\n"
}

fn damage(f: &Fixture, sql: &str) {
    let c = rusqlite::Connection::open(f.paths.journal()).unwrap();
    assert!(c.execute(sql, []).unwrap() > 0);
}

// ---- lines ----

#[tokio::test]
async fn whatever_was_said_can_be_read_again() {
    let f = fixture();
    let mut p = spawn(&f);
    let mut c = f.client().await;
    let s = session(&mut c).await;
    let mut script = AGENT.to_owned();
    for i in 0..12 {
        script += &item(&i.to_string(), &"x".repeat(1024 * 1024));
    }
    // One NUL is six bytes on the wire.
    script += &item("nul", &"\\u0000".repeat(1500 * 1024));
    for n in [60, 100, 120, 124] {
        let deep = format!("{}{}", "[".repeat(n), "]".repeat(n));
        script += &format!(
            "{{\"do\":\"start\",\"item\":\"d{n}\",\"kind\":\"tool\",\"args\":{deep}}}\n\
             {{\"do\":\"end\",\"item\":\"d{n}\",\"outcome\":\"completed\",\"result\":[\"{}\"]}}\n",
            "y".repeat(300_000)
        );
    }
    // Open when the run ends: all of them end together.
    for i in 0..12 {
        let start = item(&format!("o{i}"), &"\\u0000".repeat(170_000));
        script += start.split_inclusive('\n').take(2).collect::<String>().as_str();
    }
    script += FINISH;
    let path = f.script(&script);
    let mut live = f.client().await;
    live.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
    replay(&mut c, &s, &path, 0.0).await;
    let mut seen = View::default();
    // Every message is within the limit: the client refuses a longer one.
    while seen.ended.is_none() {
        seen.take(live.next_within(LONG).await.unwrap().unwrap());
        if seen.resync {
            seen.resync = false;
            let after = seen.seq;
            live.call(Call::Follow { session: s.clone(), after }).await.unwrap();
        }
    }
    quiet(&mut c).await;
    for restart in [false, true] {
        if restart {
            c.call(Call::Shutdown { force: false }).await.unwrap();
            p.0.wait().unwrap();
            p = spawn(&f);
        }
        let mut late = f.client().await;
        late.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
        let mut v = View::default();
        v.until_end(&mut late).await;
        assert!(v.text == seen.text, "the same text");
        assert_eq!(v.text.len(), 12 + 1 + 4 + 12);
        if restart {
            late.call(Call::Shutdown { force: true }).await.unwrap();
        }
    }
    p.0.wait().unwrap();
}

#[tokio::test]
async fn sessions_are_listed_in_pages() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let mut ids = Vec::new();
    for n in 0..5 {
        let workspace = Some("w".repeat(4096));
        let title = format!("{n}");
        let v = c.call(Call::SessionCreate { title, workspace }).await.unwrap();
        ids.insert(0, v["session"].as_str().unwrap().to_owned());
    }
    let mut page = async |limit, offset| -> Vec<String> {
        let v = c.call(Call::SessionList { limit, offset }).await.unwrap();
        v.as_array().unwrap().iter().map(|s| s["id"].as_str().unwrap().to_owned()).collect()
    };
    let mut all = page(200, 0).await;
    all.sort();
    let mut want = ids.clone();
    want.sort();
    assert_eq!(all, want);
    assert_eq!([page(2, 0).await, page(2, 2).await, page(2, 4).await].concat(), page(9, 0).await);
    assert_eq!(page(u32::MAX, 5).await, [""; 0]);
    rusqlite::Connection::open(f.paths.journal())
        .unwrap()
        .execute_batch(
            "with recursive n(i) as (select 1 union all select i + 1 from n where i < 1100)
             insert into session select 'x' || i, 't', null, 1, 1, 1 from n",
        )
        .unwrap();
    assert_eq!(page(u32::MAX, 0).await.len(), MAX_LIST as usize, "in one answer");
    assert_eq!(page(u32::MAX, MAX_LIST).await.len(), 105);
}

#[tokio::test]
async fn answers_wait_for_a_client_that_does_not_read() {
    let f = fixture();
    let mut cfg = Config::new(f.paths.clone());
    cfg.outbox = 64 * 1024;
    f.serve_with(cfg);
    let mut c = f.client().await;
    for _ in 0..50 {
        session(&mut c).await;
    }
    let (n, pad) = (4000, " ".repeat(400));
    let requests: String = (0..n)
        .map(|i| format!("{{\"id\":{i},\"method\":\"session.list\",\"params\":{{}}}}{pad}\n"))
        .collect();
    // Under 2 MB of requests, for more than 30 MB of answers. They are not all taken.
    let mut taken = 0;
    for part in requests.as_bytes().chunks(4096) {
        match tokio::time::timeout(Duration::from_secs(1), c.send_raw(part)).await {
            Ok(r) => r.unwrap(),
            Err(_) => break,
        }
        taken += part.len();
    }
    assert!(taken < requests.len() * 3 / 4, "{taken}");
    // Another client is served meanwhile.
    f.client().await.call(Call::Status {}).await.unwrap();
}

// ---- following ----

#[tokio::test]
async fn the_second_follow_of_a_connection_replaces_the_first() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    let state = "{\"do\":\"state\",\"state\":\"running\"}\n";
    let many = AGENT.to_owned() + &state.repeat(20_000);
    replay(&mut c, &s, &f.script(&many), 0.0).await;
    quiet(&mut c).await;
    let last = journal(&f).session(&s).unwrap().unwrap().last_seq;
    c.call(Call::Follow { session: s.clone(), after: last }).await.unwrap();
    let live = AGENT.to_owned() + &state.replace("{\"do", "{\"after\":1,\"do").repeat(800);
    replay(&mut c, &s, &f.script(&live), 1.0).await;
    for _ in 0..3 {
        // The answer, then everything from the start, in order, nothing twice, live at the end.
        c.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
        c.queue.clear();
        let mut seen = 0;
        while seen < last + 20 {
            let m = c.next_within(LONG).await.unwrap().unwrap();
            let Message::Events { events, .. } = m else { panic!("{m:?}") };
            for e in events {
                assert_eq!(e.seq, seen + 1);
                seen = e.seq;
            }
        }
    }
    let mut v = View { seq: last, ..View::default() };
    c.call(Call::Unfollow { session: s.clone() }).await.unwrap();
    c.queue.clear();
    assert_eq!(c.next_within(Duration::from_millis(100)).await.unwrap(), None, "quiet");
    c.call(Call::Follow { session: s, after: last + 2 }).await.unwrap();
    v.seq = last + 2;
    v.until_end(&mut c).await;
}

/// The old follow may be anywhere: reading history, waiting for room, joining, live.
#[tokio::test(flavor = "multi_thread", worker_threads = 3)]
async fn nothing_of_a_follow_comes_after_the_answer_that_ended_it() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    let state = "{\"do\":\"state\",\"state\":\"running\"}\n";
    replay(&mut c, &s, &f.script(&(AGENT.to_owned() + &state.repeat(3000))), 0.0).await;
    quiet(&mut c).await;
    let mut live = AGENT.to_owned();
    live += &state.replace("{\"do", "{\"after\":1,\"do").repeat(60_000);
    let run = replay(&mut c, &s, &f.script(&live), 1.0).await;
    c.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
    for round in 0..400 {
        // Read as far as history, or to the live end, in turns.
        let far = if round % 10 == 0 { 3100 } else { 1 + round % 7 };
        c.queue.clear();
        let mut seen = 0;
        while seen < far {
            match c.next_within(LONG).await.unwrap().expect("events") {
                Message::Events { events, .. } => {
                    for e in events {
                        assert_eq!(e.seq, seen + 1, "round {round}");
                        seen = e.seq;
                    }
                }
                m => panic!("{m:?}"),
            }
        }
        if round < 200 {
            c.call(Call::Unfollow { session: s.clone() }).await.unwrap();
            c.queue.clear();
            c.call(Call::Status {}).await.unwrap();
            assert_eq!(c.queue.len(), 0, "round {round}: quiet after the answer");
        }
        c.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
    }
    c.call(Call::Cancel { session: s, run }).await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_connection_that_ends_without_reading_is_let_go() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let f = fixture();
    let mut cfg = Config::new(f.paths.clone());
    cfg.linger = Duration::from_millis(200);
    f.serve_with(cfg);
    let mut c = f.client().await;
    let s = session(&mut c).await;
    let big: String = (0..40).map(|i| item(&i.to_string(), &"x".repeat(100_000))).collect();
    replay(&mut c, &s, &f.script(&(AGENT.to_owned() + &big)), 0.0).await;
    quiet(&mut c).await;
    let token = std::fs::read_to_string(f.paths.token()).unwrap();
    let mut raw = stream(&f.paths).await;
    raw.write_all(
        format!(
            "{{\"id\":1,\"method\":\"hello\",\"params\":{{\"token\":\"{token}\",\"client\":\"x\",\"protocol\":1}}}}\n\
             {{\"id\":2,\"method\":\"session.follow\",\"params\":{{\"session\":\"{s}\"}}}}\n"
        )
        .as_bytes(),
    )
    .await
    .unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    raw.shutdown().await.unwrap();
    tokio::time::sleep(Duration::from_millis(600)).await;
    // What the system had taken is there; the rest of 4 MB was given up with the socket.
    let mut got = Vec::new();
    let _ = tokio::time::timeout(LONG, raw.read_to_end(&mut got)).await.expect("closed");
    assert!(got.len() < 3_000_000, "{}", got.len());
    assert_ne!(got.last(), Some(&b'\n'), "given up in the middle of a line");
    assert_eq!(c.call(Call::Status {}).await.unwrap()["clients"], 1);
}

#[cfg(unix)]
async fn stream(p: &Paths) -> tokio::net::UnixStream {
    tokio::net::UnixStream::connect(p.endpoint().unwrap()).await.unwrap()
}

#[tokio::test]
async fn a_client_that_leaves_does_not_wait_for_its_answer() {
    let f = fixture();
    let mut cfg = Config::new(f.paths.clone());
    cfg.linger = Duration::from_millis(200);
    f.serve_with(cfg);
    let mut c = f.client().await;
    let mut d = f.client().await;
    let lock = rusqlite::Connection::open(f.paths.journal()).unwrap();
    lock.execute_batch("begin immediate").unwrap();
    // It waits for the lock: ten seconds.
    let create = "{\"id\":9,\"method\":\"session.create\",\"params\":{\"title\":\"t\"}}\n";
    d.send_raw(create.as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_millis(100)).await;
    drop(d);
    let t = Instant::now();
    while c.call(Call::Status {}).await.unwrap()["clients"] != 1 {
        assert!(t.elapsed() < Duration::from_secs(3), "its place is free again");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    lock.execute_batch("rollback").unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn a_client_that_only_ends_its_side_is_answered() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let f = fixture();
    f.serve();
    f.client().await;
    let token = std::fs::read_to_string(f.paths.token()).unwrap();
    let mut raw = stream(&f.paths).await;
    let hello = format!(
        "{{\"id\":1,\"method\":\"hello\",\"params\":{{\"token\":\"{token}\",\"client\":\"x\",\"protocol\":1}}}}\n"
    );
    let create = "{\"id\":2,\"method\":\"session.create\",\"params\":{\"title\":\"t\"}}\n";
    raw.write_all((hello + create).as_bytes()).await.unwrap();
    raw.shutdown().await.unwrap();
    let mut got = String::new();
    tokio::time::timeout(LONG, raw.read_to_string(&mut got)).await.unwrap().unwrap();
    assert!(got.lines().count() == 2 && got.contains("\"session\":"), "{got}");
}

// ---- strangers ----

#[tokio::test]
async fn a_stranger_has_little_room_and_is_not_repeated() {
    let f = fixture();
    f.serve();
    f.client().await;
    let long = format!("{{\"id\":1,\"method\":\"{}\",\"params\":{{}}}}\n", "A".repeat(5000));
    let unended = "x".repeat(5000);
    let rude = "{\"id\":7,\"method\":\"SECRET\\u001b\",\"params\":{}}\n".to_owned();
    for (line, id, code) in [
        (long, 0, ErrorCode::Protocol),
        (unended, 0, ErrorCode::Protocol),
        (rude, 7, ErrorCode::BadRequest),
    ] {
        let mut c = Client::raw(&f.paths).await.unwrap();
        c.send_raw(line.as_bytes()).await.unwrap();
        let m = c.next_within(LONG).await.unwrap().unwrap();
        let Message::Err { id: i, error } = m else { panic!("{m:?}") };
        assert_eq!((i, error.code), (id, code));
        assert!(!error.message.contains("SECRET") && !error.message.contains('A'), "{error}");
        let t = Instant::now();
        assert!(matches!(c.next().await, Err(ClientError::Closed | ClientError::Io(_))));
        assert!(t.elapsed() < Duration::from_secs(1), "closed at once");
    }
    // Who is known has more room, and is not repeated either.
    let mut c = f.client().await;
    let s = SessionId("SECRET".repeat(2000));
    let e = refused(c.call(Call::Follow { session: s, after: 0 }).await);
    assert!(!e.message.contains("SECRET"));
    let path = f.script("{\"do\":\"SECRET\"}\n");
    let s = session(&mut c).await;
    let e = refused(c.call(Call::ReplayStart { session: s, path, speed: 0.0 }).await);
    assert!(!e.message.contains("SECRET"), "{e}");
}

#[tokio::test]
async fn silence_is_closed() {
    let f = fixture();
    let mut cfg = Config::new(f.paths.clone());
    cfg.hello_within = Duration::from_millis(300);
    f.serve_with(cfg);
    f.client().await;
    let mut silent = Vec::new();
    for _ in 0..20 {
        silent.push(Client::raw(&f.paths).await.unwrap());
    }
    for mut c in silent {
        let r = tokio::time::timeout(LONG, c.next()).await.expect("closed");
        assert!(matches!(r, Err(ClientError::Closed | ClientError::Io(_))));
    }
}

#[tokio::test]
async fn silence_keeps_nobody_out() {
    let f = fixture();
    let mut cfg = Config::new(f.paths.clone());
    // Nobody is closed for being silent in this test.
    cfg.hello_within = Duration::from_secs(600);
    f.serve_with(cfg);
    let mut c = f.client().await;
    let mut silent = Vec::new();
    for _ in 0..60 {
        silent.push(Client::raw(&f.paths).await.unwrap());
    }
    let t = Instant::now();
    f.client().await.call(Call::Status {}).await.unwrap();
    assert!(t.elapsed() < Duration::from_secs(2), "at once");
    c.call(Call::Status {}).await.unwrap();
    // The oldest made room; the last ones are still welcome to speak.
    let token = std::fs::read_to_string(f.paths.token()).unwrap();
    let mut open = 0;
    for (n, c) in silent.iter_mut().enumerate() {
        let hello = Call::Hello { token: token.clone(), client: "x".into(), protocol: PROTOCOL };
        match tokio::time::timeout(LONG, c.call(hello)).await.unwrap() {
            Ok(_) => open += 1,
            Err(_) => assert!(n < 30, "{n}"),
        }
    }
    assert!((30..=32).contains(&open), "{open}");
}

#[tokio::test]
async fn there_are_only_so_many_clients() {
    let f = fixture();
    f.serve();
    let mut all = Vec::new();
    for _ in 0..64 {
        all.push(f.client().await);
    }
    let one_more = async || Client::connect(&f.paths, "x").await;
    match one_more().await {
        Err(ClientError::Refused(e)) => assert_eq!(e.code, ErrorCode::Busy),
        r => panic!("{:?}", r.is_ok()),
    }
    all[0].call(Call::Status {}).await.unwrap();
    all.pop();
    let t = Instant::now();
    while one_more().await.is_err() {
        assert!(t.elapsed() < LONG, "the place is free again");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[tokio::test]
async fn every_host_has_its_own_token() {
    let (a, b) = (fixture(), fixture());
    a.serve();
    b.serve();
    let token = |f: &Fixture| std::fs::read_to_string(f.paths.token()).unwrap();
    for f in [&a, &b] {
        assert_eq!(token(f).len(), 64);
        let digits: std::collections::HashSet<_> = token(f).chars().collect();
        assert!(digits.len() > 4, "{}", token(f));
    }
    assert_ne!(token(&a), token(&b));
}

#[cfg(unix)]
#[tokio::test]
async fn a_host_that_runs_out_of_files_goes_on_and_is_not_busy_with_it() {
    let f = fixture();
    let mut few = Command::new("/bin/sh");
    few.args(["-c", "ulimit -n 32; exec \"$0\"", env!("CARGO_BIN_EXE_axiom-host")]);
    let (mut p, said) = spawn_heard(&f, &mut few);
    let mut c = f.client().await;
    let s = session(&mut c).await;
    let run = replay(&mut c, &s, &f.talker(10_000, 5), 1.0).await;
    let mut raw = Vec::new();
    for _ in 0..60 {
        raw.push(stream(&f.paths).await);
    }
    // It looks after its files meanwhile, which it cannot open.
    tokio::time::sleep(Duration::from_secs(4)).await;
    assert!(p.0.try_wait().unwrap().is_none(), "it still serves");
    c.call(Call::Cancel { session: s, run }).await.unwrap();
    drop(raw);
    let next = tokio::time::timeout(LONG, f.client()).await.unwrap();
    drop((next, c));
    p.0.kill().unwrap();
    let lines = std::io::read_to_string(said).unwrap();
    let tries = lines.lines().filter(|l| l.contains("accepting")).count();
    assert!((1..40).contains(&tries), "{tries}");
}

// ---- nobody reads what it says ----

/// Its stderr is closed at the other end: writing there is an error.
#[cfg(unix)]
fn spawn_unheard(f: &Fixture, command: &mut Command) -> Process {
    spawn_heard(f, command).0
}
#[cfg(unix)]
fn host_command() -> Command {
    Command::new(env!("CARGO_BIN_EXE_axiom-host"))
}

#[cfg(unix)]
#[tokio::test]
async fn it_works_though_nobody_reads_what_it_says() {
    let f = fixture();
    let mut p = spawn_unheard(&f, host_command().args(["--idle-exit", "1"]));
    let mut c = f.client().await;
    let s = session(&mut c).await;
    damage(&f, "update event set body='{'");
    for _ in 0..3 {
        // One connection each: a panic would take its place among the clients with it.
        let mut d = f.client().await;
        let r = d.call(Call::Follow { session: s.clone(), after: 0 });
        let e = refused(tokio::time::timeout(LONG, r).await.expect("an answer"));
        assert_eq!(e.message, "journal: damaged: event 1 cannot be read");
    }
    let t = Instant::now();
    while c.call(Call::Status {}).await.unwrap()["clients"] != 1 {
        assert!(t.elapsed() < LONG);
    }
    assert_eq!(c.call(Call::Status {}).await.unwrap()["faults"][0]["session"], s.0.as_str());
    drop(c);
    // It knows that the last one has left.
    let t = Instant::now();
    while p.0.try_wait().unwrap().is_none() {
        assert!(t.elapsed() < LONG, "idle");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(p.0.wait().unwrap().success());
}

#[cfg(unix)]
#[tokio::test]
async fn it_runs_out_of_files_and_starts_though_nobody_reads_what_it_says() {
    let f = fixture();
    let mut few = Command::new("/bin/sh");
    few.args(["-c", "ulimit -n 32; exec \"$0\"", env!("CARGO_BIN_EXE_axiom-host")]);
    let mut p = spawn_unheard(&f, &mut few);
    let mut c = f.client().await;
    let s = session(&mut c).await;
    replay(&mut c, &s, &f.talker(10_000, 5), 1.0).await;
    let mut raw = Vec::new();
    for _ in 0..60 {
        raw.push(stream(&f.paths).await);
    }
    tokio::time::sleep(Duration::from_millis(500)).await;
    c.call(Call::Status {}).await.unwrap();
    p.0.kill().unwrap();
    p.0.wait().unwrap();
    // What was open in it cannot be ended; that is said to nobody, and it starts.
    damage(&f, "update open set kind=9");
    let _p = spawn_unheard(&f, &mut host_command());
    let status = f.client().await.call(Call::Status {}).await.unwrap();
    assert_eq!(status["faults"][0]["session"], s.0.as_str());
}

#[cfg(unix)]
#[tokio::test]
async fn it_does_not_wait_for_whoever_could_read_what_it_says() {
    let f = fixture();
    // The other end is open, and nobody reads it.
    let (_p, _unread) = spawn_heard(&f, &mut host_command());
    let mut c = f.client().await;
    let s = session(&mut c).await;
    damage(&f, "update event set body='{'");
    // More words than a pipe holds.
    for _ in 0..1500 {
        let r = c.call(Call::Follow { session: s.clone(), after: 0 });
        refused(tokio::time::timeout(LONG, r).await.expect("an answer"));
    }
}

// ---- the journal ----

#[tokio::test]
async fn one_damaged_event_is_one_damaged_session() {
    let f = fixture();
    let mut p = spawn(&f);
    let mut c = f.client().await;
    let (bad, good) = (session(&mut c).await, session(&mut c).await);
    let script = f.script(&(AGENT.to_owned() + &item("i", "text") + FINISH));
    for s in [&bad, &good] {
        replay(&mut c, s, &script, 0.0).await;
    }
    quiet(&mut c).await;
    c.call(Call::Shutdown { force: false }).await.unwrap();
    p.0.wait().unwrap();
    damage(&f, &format!("update event set body='{{' where session='{bad}' and seq=1"));

    let mut p = spawn(&f);
    let mut c = f.client().await;
    let mut v = View::default();
    c.call(Call::Follow { session: good, after: 0 }).await.unwrap();
    v.until_end(&mut c).await;
    for _ in 0..2 {
        let e = refused(c.call(Call::Follow { session: bad.clone(), after: 0 }).await);
        assert_eq!(e.code, ErrorCode::Internal);
    }
    assert_eq!(c.next_within(Duration::from_millis(50)).await.unwrap(), None);
    let faults = c.call(Call::Status {}).await.unwrap()["faults"].clone();
    assert_eq!(faults[0]["session"], bad.0.as_str());
    assert!(faults[0]["words"].as_str().unwrap().contains("event 1 cannot be read"));
    // Beyond the damage it is readable, and it can be written.
    c.call(Call::Follow { session: bad.clone(), after: 1 }).await.unwrap();
    replay(&mut c, &bad, &script, 0.0).await;
    let mut v = View { seq: 1, ..View::default() };
    v.until_end(&mut c).await;
    // Repaired, it is whole again, and the host says so.
    damage(
        &f,
        &format!(
            "update event set body='{{\"type\":\"session.created\",\"title\":\"S\"}}'
             where session='{bad}' and seq=1"
        ),
    );
    let mut d = f.client().await;
    d.call(Call::Follow { session: bad.clone(), after: 0 }).await.unwrap();
    let mut v = View::default();
    v.until_end(&mut d).await;
    quiet(&mut c).await;
    let t = Instant::now();
    while c.call(Call::Status {}).await.unwrap()["faults"] != serde_json::json!([]) {
        assert!(t.elapsed() < LONG);
    }
    c.call(Call::Shutdown { force: true }).await.unwrap();
    p.0.wait().unwrap();
}

#[tokio::test]
async fn damage_found_on_the_way_ends_the_follow_in_words() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    let big: String = (0..20).map(|i| item(&i.to_string(), &"x".repeat(100_000))).collect();
    replay(&mut c, &s, &f.script(&(AGENT.to_owned() + &big)), 0.0).await;
    quiet(&mut c).await;
    damage(&f, &format!("update event set body='[]' where session='{s}' and seq=31"));
    c.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
    let mut last = 0;
    loop {
        match c.next_within(LONG).await.unwrap().expect("a fault") {
            Message::Events { events, .. } => last = events.last().unwrap().seq,
            Message::Fault { session, fault } => {
                assert_eq!(session, s);
                assert!(fault.contains("event 31"), "{fault}");
                break;
            }
            m => panic!("{m:?}"),
        }
    }
    assert!(last > 20 && last < 31, "{last}: what comes before the damage is shown");
    assert_eq!(c.next_within(Duration::from_millis(100)).await.unwrap(), None, "and no more");
}

#[tokio::test]
async fn a_session_that_cannot_be_written_says_so_and_works_again_when_it_can() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    c.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
    replay(&mut c, &s, &f.talker(10_000, 2), 1.0).await;
    let mut v = View::default();
    while v.text.values().next().is_none_or(String::is_empty) {
        v.take(c.next_within(LONG).await.unwrap().unwrap());
    }
    let sql = |q: &str| {
        rusqlite::Connection::open(f.paths.journal()).unwrap().execute_batch(q).unwrap();
    };
    sql("create trigger t before insert on event begin select raise(abort, 'no room'); end");
    let fault = loop {
        match c.next_within(LONG).await.unwrap().expect("a fault") {
            Message::Fault { session, fault } if session == s => break fault,
            m => v.take(m),
        }
    };
    assert!(fault.contains("no room"), "{fault}");
    quiet(&mut c).await;
    let status = c.call(Call::Status {}).await.unwrap();
    assert_eq!(status["faults"], serde_json::json!([{ "session": s, "words": fault }]));
    let list = c.call(Call::list()).await.unwrap();
    assert_eq!(list[0]["running"], false, "nothing runs in it");
    // Still refused, so nothing is accepted.
    let script = f.talker(5, 0);
    let start = Call::ReplayStart { session: s.clone(), path: script.clone(), speed: 0.0 };
    for call in [start, Call::Follow { session: s.clone(), after: v.seq }] {
        assert_eq!(refused(c.call(call).await).code, ErrorCode::Internal);
    }
    assert_eq!(c.next_within(Duration::from_millis(100)).await.unwrap(), None);
    // There is room again. The run that was cut short has ended, with what was seen of it.
    sql("drop trigger t");
    c.call(Call::Follow { session: s.clone(), after: v.seq }).await.unwrap();
    v.until_end(&mut c).await;
    assert_eq!(v.ended, Some(Outcome::Interrupted));
    assert!(v.text.values().next().unwrap().starts_with("w0 w1"));
    v.ended = None;
    replay(&mut c, &s, &script, 0.0).await;
    v.until_end(&mut c).await;
    assert_eq!(v.ended, Some(Outcome::Completed));
    let status = c.call(Call::Status {}).await.unwrap();
    assert_eq!(status["faults"], serde_json::json!([]));
}

#[tokio::test]
async fn a_session_is_closed_when_nobody_uses_it_and_goes_on_when_somebody_does() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    for _ in 0..5 {
        replay(&mut c, &s, &f.talker(3, 0), 0.0).await;
        quiet(&mut c).await;
    }
    let runs = history(&f, &s);
    let ends = runs.iter().filter(|b| matches!(b, Body::RunEnded { .. })).count();
    assert_eq!(ends, 5);
}

#[tokio::test]
async fn a_damaged_file_is_reported_after_an_unclean_end() {
    let f = fixture();
    axiom_host::paths::private_dir(&f.paths.run()).unwrap();
    let s = SessionId::new();
    let mut j = axiom_journal::Journal::open(&f.paths.journal()).unwrap();
    j.create(&s, "t", None, 1).unwrap();
    let events: Vec<_> = (2..400)
        .map(|seq| Event { seq, at: 1, body: Body::SessionTitled { title: "x".repeat(400) } })
        .collect();
    j.append(&s, &events).unwrap();
    drop(j);
    let mut bytes = std::fs::read(f.paths.journal()).unwrap();
    let middle = bytes.len() / 2 / 4096 * 4096;
    bytes[middle..middle + 4096].fill(7);
    std::fs::write(f.paths.journal(), bytes).unwrap();
    // The last host left its token: it did not stop in order.
    std::fs::write(f.paths.token(), "old").unwrap();
    let host = f.serve();
    let mut c = f.client().await;
    let t = Instant::now();
    loop {
        let status = c.call(Call::Status {}).await.unwrap();
        if status["damaged"].as_str().is_some_and(|d| d.contains("damaged")) {
            break;
        }
        assert!(t.elapsed() < LONG, "{status}");
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    session(&mut c).await;
    c.call(Call::Shutdown { force: false }).await.unwrap();
    host.await.unwrap().unwrap();
    // After an orderly end nothing is read through.
    let host = f.serve();
    let mut c = f.client().await;
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(c.call(Call::Status {}).await.unwrap().get("damaged").is_none());
    c.call(Call::Shutdown { force: false }).await.unwrap();
    host.await.unwrap().unwrap();
}

// ---- stopping ----

fn ended(f: &Fixture, s: &SessionId) -> Option<Outcome> {
    let last = history(f, s).into_iter().rev().take(2).collect::<Vec<_>>();
    match (&last[1], &last[0]) {
        (Body::RunEnded { outcome, .. }, Body::AgentState { state, .. }) => {
            assert!(!matches!(state, AgentState::Running | AgentState::Asking));
            Some(*outcome)
        }
        _ => None,
    }
}

#[tokio::test]
async fn every_run_ends_in_the_journal_when_the_host_stops() {
    let f = fixture();
    let host = f.serve();
    let mut c = f.client().await;
    let mut all = Vec::new();
    // Behind their backpressure (the journal is locked), in a long pause, and in between.
    let pause = f.script("{\"after\":50000,\"do\":\"finish\",\"outcome\":\"completed\"}\n");
    for script in
        [f.script(&asks(3000, 1000)), f.script(&asks(3000, 1000)), pause, f.talker(10_000, 5)]
    {
        let s = session(&mut c).await;
        replay(&mut c, &s, &script, 1.0).await;
        all.push(s);
    }
    let lock = rusqlite::Connection::open(f.paths.journal()).unwrap();
    lock.busy_timeout(LONG).unwrap();
    lock.execute_batch("begin immediate").unwrap();
    tokio::time::sleep(Duration::from_millis(1300)).await;
    let t = Instant::now();
    c.call(Call::Shutdown { force: true }).await.unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(!host.is_finished(), "nothing can have been written");
    lock.execute_batch("rollback").unwrap();
    tokio::time::timeout(LONG, host).await.unwrap().unwrap().unwrap();
    assert!(t.elapsed() < Duration::from_secs(8), "{:?}", t.elapsed());
    for s in &all {
        assert_eq!(ended(&f, s), Some(Outcome::Cancelled));
    }
    assert!(!f.paths.token().exists() && !f.paths.endpoint_file().exists());
}

#[tokio::test]
async fn followers_see_the_end_before_the_host_leaves() {
    let f = fixture();
    let host = f.serve();
    let (mut c, mut d) = (f.client().await, f.client().await);
    let s = session(&mut c).await;
    d.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
    replay(&mut c, &s, &f.talker(10_000, 5), 1.0).await;
    c.call(Call::Shutdown { force: true }).await.unwrap();
    let mut v = View::default();
    v.until_end(&mut d).await;
    assert_eq!(v.ended, Some(Outcome::Cancelled));
    while let Ok(m) = d.next_within(LONG).await {
        v.take(m.expect("closed"));
    }
    assert_eq!(v.state, Some(AgentState::Paused));
    host.await.unwrap().unwrap();
}

#[tokio::test]
async fn nothing_starts_once_the_host_is_stopping() {
    let f = fixture();
    let host = f.serve();
    let (mut c, mut d) = (f.client().await, f.client().await);
    let (s, busy) = (session(&mut c).await, session(&mut c).await);
    let script = f.talker(10_000, 5);
    replay(&mut c, &busy, &script, 1.0).await;
    // It cannot have stopped before it was asked: the end of the run cannot be written.
    let lock = rusqlite::Connection::open(f.paths.journal()).unwrap();
    lock.busy_timeout(LONG).unwrap();
    lock.execute_batch("begin immediate").unwrap();
    c.call(Call::Shutdown { force: true }).await.unwrap();
    let start = Call::ReplayStart { session: s.clone(), path: script.clone(), speed: 1.0 };
    let create = Call::SessionCreate { title: "late".into(), workspace: None };
    for call in [start, create] {
        assert_eq!(refused(d.call(call).await).code, ErrorCode::Busy);
    }
    lock.execute_batch("rollback").unwrap();
    tokio::time::timeout(LONG, host).await.unwrap().unwrap().unwrap();
    assert_eq!(history(&f, &s).len(), 1);
    assert_eq!(journal(&f).sessions(9, 0).unwrap().len(), 2);
}

#[tokio::test]
async fn a_long_pause_does_not_delay_a_cancel() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    c.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
    let script = "{\"after\":50000,\"do\":\"finish\",\"outcome\":\"completed\"}\n";
    let run = replay(&mut c, &s, &f.script(script), 1.0).await;
    let t = Instant::now();
    c.call(Call::Cancel { session: s, run }).await.unwrap();
    let mut v = View::default();
    v.until_end(&mut c).await;
    assert_eq!(v.ended, Some(Outcome::Cancelled));
    assert!(t.elapsed() < Duration::from_secs(2));
}

#[cfg(unix)]
#[tokio::test]
async fn a_signal_stops_it_in_order() {
    for signal in ["TERM", "INT", "HUP"] {
        let f = fixture();
        let mut p = spawn(&f);
        let mut c = f.client().await;
        let s = session(&mut c).await;
        replay(&mut c, &s, &f.talker(10_000, 5), 1.0).await;
        let kill = ["-s", signal, &p.0.id().to_string()].map(str::to_owned);
        assert!(Command::new("kill").args(kill).status().unwrap().success());
        assert!(p.0.wait().unwrap().success(), "{signal}");
        assert_eq!(ended(&f, &s), Some(Outcome::Cancelled), "{signal}");
        assert!(!f.paths.token().exists());
    }
}

// ---- its files ----

#[cfg(unix)]
async fn found_again(f: &Fixture, mut c: Client) -> Client {
    let t = Instant::now();
    let mut d = loop {
        match Client::connect(&f.paths, "x").await {
            Ok(d) => break d,
            Err(e) => assert!(t.elapsed() < LONG, "{e}"),
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    d.call(Call::Status {}).await.unwrap();
    c.call(Call::Status {}).await.unwrap();
    c
}
#[cfg(unix)]
fn looking(f: &Fixture) -> Config {
    let mut cfg = Config::new(f.paths.clone());
    cfg.look_every = Duration::from_millis(50);
    cfg
}

#[cfg(unix)]
#[tokio::test]
async fn what_clients_find_it_by_is_put_back() {
    let f = fixture();
    f.serve_with(looking(&f));
    let mut c = f.client().await;
    let socket = f.paths.endpoint().unwrap();
    std::fs::remove_dir_all(f.paths.run()).unwrap();
    c = found_again(&f, c).await;
    for file in [f.paths.token(), f.paths.endpoint_file(), socket.clone().into()] {
        std::fs::remove_file(file).unwrap();
        c = found_again(&f, c).await;
    }
    for file in [f.paths.token(), f.paths.endpoint_file()] {
        std::fs::write(file, "somebody else's").unwrap();
        c = found_again(&f, c).await;
    }
    // And nobody else can serve the directory meanwhile.
    std::fs::remove_file(f.paths.old_lock()).unwrap();
    std::fs::remove_file(f.paths.lock()).unwrap();
    let held = |p| std::fs::File::open(p).is_ok_and(|f: std::fs::File| f.try_lock().is_err());
    let t = Instant::now();
    while !held(f.paths.lock()) || !held(f.paths.old_lock()) {
        assert!(t.elapsed() < LONG);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    found_again(&f, c).await;
}

#[cfg(unix)]
#[tokio::test]
async fn a_short_place_that_was_cleaned_away_is_replaced() {
    let d = tempfile::tempdir().unwrap();
    let f = Fixture { paths: Paths::new(d.path().join("n".repeat(120))), dir: d };
    let host = f.serve_with(looking(&f));
    let c = f.client().await;
    let first = f.paths.endpoint().unwrap();
    std::fs::remove_dir_all(std::path::Path::new(&first).parent().unwrap()).unwrap();
    let mut c = found_again(&f, c).await;
    assert_ne!(f.paths.endpoint().unwrap(), first);
    c.call(Call::Shutdown { force: false }).await.unwrap();
    host.await.unwrap().unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn it_stops_in_order_when_they_cannot_be_put_back() {
    use axiom_host::HostError;
    // Another host took over; the journal went.
    for taken_over in [true, false] {
        let f = fixture();
        let mut cfg = looking(&f);
        // Time to take over before it looks again.
        cfg.look_every = Duration::from_secs(1);
        let host = f.serve_with(cfg);
        let mut c = f.client().await;
        let s = session(&mut c).await;
        replay(&mut c, &s, &f.talker(10_000, 5), 1.0).await;
        let mut other = None;
        if taken_over {
            std::fs::remove_dir_all(f.paths.run()).unwrap();
            axiom_host::paths::private_dir(&f.paths.run()).unwrap();
            let lock = std::fs::File::create(f.paths.old_lock()).unwrap();
            lock.try_lock().unwrap();
            std::fs::write(f.paths.token(), "another's").unwrap();
            other = Some(lock);
        } else {
            std::fs::rename(f.paths.journal(), f.dir.path().join("aside")).unwrap();
        }
        let r = tokio::time::timeout(LONG, host).await.unwrap().unwrap();
        assert!(matches!(r, Err(HostError::Lost(_))), "{r:?}");
        if taken_over {
            let cancelled =
                |b: &Body| matches!(b, Body::RunEnded { outcome: Outcome::Cancelled, .. });
            assert!(history(&f, &s).iter().any(cancelled));
        }
        let token = std::fs::read_to_string(f.paths.token());
        assert_eq!(token.is_ok_and(|t| t == "another's"), taken_over, "not its own to remove");
        drop(other);
    }
}

#[cfg(unix)]
#[tokio::test]
async fn what_may_pass_is_tried_again_and_then_it_stops() {
    use axiom_host::HostError;
    let f = fixture();
    let host = f.serve_with(looking(&f));
    let c = f.client().await;
    let blocked = || {
        std::fs::remove_dir_all(f.paths.run()).unwrap();
        std::fs::write(f.paths.run(), "not a directory").unwrap();
    };
    blocked();
    tokio::time::sleep(Duration::from_millis(200)).await;
    std::fs::remove_file(f.paths.run()).unwrap();
    let mut c = found_again(&f, c).await;
    let s = session(&mut c).await;
    replay(&mut c, &s, &f.talker(10_000, 5), 1.0).await;
    blocked();
    let r = tokio::time::timeout(LONG, host).await.unwrap().unwrap();
    assert!(matches!(r, Err(HostError::Lost(_))), "{r:?}");
    assert_eq!(ended(&f, &s), Some(Outcome::Cancelled));
}

#[tokio::test]
async fn a_host_from_before_schema_2_and_this_one_exclude_each_other() {
    use axiom_host::{Host, HostError};
    let f = fixture();
    axiom_host::paths::private_dir(&f.paths.run()).unwrap();
    let old = std::fs::File::create(f.paths.old_lock()).unwrap();
    old.try_lock().unwrap();
    let r = Host::start(Config::new(f.paths.clone()));
    assert!(matches!(r, Err(HostError::AlreadyRunning)));
    assert!(!f.paths.journal().exists(), "the journal is not touched");
    drop(old);
    f.serve();
    f.client().await;
    let old = std::fs::OpenOptions::new().write(true).open(f.paths.old_lock()).unwrap();
    assert!(old.try_lock().is_err());
}

// ---- scripts ----

#[tokio::test]
async fn only_so_much_runs_at_once() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    let pause = f.script("{\"after\":50000,\"do\":\"finish\",\"outcome\":\"completed\"}\n");
    let mut runs = Vec::new();
    for _ in 0..128 {
        runs.push(replay(&mut c, &s, &pause, 1.0).await);
    }
    let start = || Call::ReplayStart { session: s.clone(), path: pause.clone(), speed: 1.0 };
    assert_eq!(refused(c.call(start()).await).code, ErrorCode::Busy);
    for run in runs {
        c.call(Call::Cancel { session: s.clone(), run }).await.unwrap();
    }
    quiet(&mut c).await;
    c.call(start()).await.unwrap();
}

#[tokio::test]
async fn only_so_much_of_scripts_is_held_at_once() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    // 60 MiB of which only one line says something.
    let step = "{\"after\":50000,\"do\":\"finish\",\"outcome\":\"completed\"}\n";
    let big = f.script(&(step.to_owned() + &" ".repeat(60 * 1024 * 1024)));
    let first = replay(&mut c, &s, &big, 1.0).await;
    let second = replay(&mut c, &s, &big, 1.0).await;
    let start = || Call::ReplayStart { session: s.clone(), path: big.clone(), speed: 1.0 };
    assert_eq!(refused(c.call(start()).await).code, ErrorCode::Busy);
    assert_eq!(c.call(Call::Status {}).await.unwrap()["runs"], 2, "what was refused does not run");
    // A small one still fits.
    let small = replay(&mut c, &s, &f.script(step), 1.0).await;
    for run in [first, second, small] {
        c.call(Call::Cancel { session: s.clone(), run }).await.unwrap();
    }
    quiet(&mut c).await;
    c.call(start()).await.unwrap();
}

#[tokio::test]
async fn a_script_that_begins_a_name_twice_is_refused() {
    let f = fixture();
    f.serve();
    let mut c = f.client().await;
    let s = session(&mut c).await;
    let start = "{\"do\":\"start\",\"item\":\"p\",\"kind\":\"prose\"}\n";
    let end = "{\"do\":\"end\",\"item\":\"p\",\"outcome\":\"completed\"}\n";
    let ask =
        "{\"do\":\"ask\",\"request\":\"q\",\"what\":\"w\",\"subject\":\"s\",\"scope\":\"\"}\n";
    let answered = "{\"do\":\"answered\",\"request\":\"q\",\"answer\":\"a\"}\n";
    for (script, ok) in [
        ([start, start].concat(), false),
        ([ask, ask].concat(), false),
        ([start, end, start, end, ask, answered, ask, answered].concat(), true),
        // What is left open is ended with the run.
        ([start, ask].concat(), true),
    ] {
        let path = f.script(&script);
        let r = c.call(Call::ReplayStart { session: s.clone(), path, speed: 0.0 }).await;
        assert_eq!(r.is_ok(), ok, "{script}");
    }
    quiet(&mut c).await;
    let (mut begun, mut over) = (Vec::new(), Vec::new());
    for b in history(&f, &s) {
        match b {
            Body::ItemStarted { item, .. } => begun.push(item.0),
            Body::ItemEnded { item, .. } => over.push(item.0),
            Body::RequestOpened { request, .. } => begun.push(request.0),
            Body::RequestClosed { request, .. } => over.push(request.0),
            _ => {}
        }
    }
    begun.sort();
    over.sort();
    assert_eq!((begun.len(), begun), (over.len(), over));
}

// ---- the place of the socket ----

#[cfg(unix)]
#[tokio::test]
async fn a_client_finds_the_socket_whatever_its_own_environment() {
    let d = tempfile::tempdir().unwrap();
    let f = Fixture { paths: Paths::new(d.path().join("n".repeat(120))), dir: d };
    let tmp = tempfile::tempdir_in("/tmp").unwrap();
    let mut elsewhere = Command::new(env!("CARGO_BIN_EXE_axiom-host"));
    let mut p = spawn_as(&f, elsewhere.env("TMPDIR", tmp.path()));
    assert!(f.paths.endpoint().unwrap().starts_with(tmp.path().to_str().unwrap()));
    f.client().await.call(Call::Shutdown { force: false }).await.unwrap();
    p.0.wait().unwrap();
    assert_eq!(std::fs::read_dir(tmp.path()).unwrap().count(), 0, "nothing is left behind");
}
