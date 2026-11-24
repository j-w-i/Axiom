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

/// `n` requests one after the other: each waits for the drive.
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
async fn silence_is_closed_and_takes_no_place_for_long() {
    let f = fixture();
    let mut cfg = Config::new(f.paths.clone());
    cfg.hello_within = Duration::from_millis(300);
    f.serve_with(cfg);
    f.client().await;
    let mut silent = Vec::new();
    for _ in 0..40 {
        silent.push(Client::raw(&f.paths).await.unwrap());
    }
    // More than may wait at once; the next one is let in when they are closed.
    tokio::time::timeout(LONG, f.client()).await.unwrap().call(Call::Status {}).await.unwrap();
    for mut c in silent {
        let r = tokio::time::timeout(LONG, c.next()).await.expect("closed");
        assert!(matches!(r, Err(ClientError::Closed | ClientError::Io(_))));
    }
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
async fn a_host_that_runs_out_of_files_goes_on() {
    let f = fixture();
    let mut few = Command::new("/bin/sh");
    few.args(["-c", "ulimit -n 32; exec \"$0\"", env!("CARGO_BIN_EXE_axiom-host")]);
    let mut p = spawn_as(&f, &mut few);
    let mut c = f.client().await;
    let s = session(&mut c).await;
    let run = replay(&mut c, &s, &f.talker(10_000, 5), 1.0).await;
    let mut raw = Vec::new();
    for _ in 0..60 {
        raw.push(stream(&f.paths).await);
    }
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(p.0.try_wait().unwrap().is_none(), "it still serves");
    c.call(Call::Cancel { session: s, run }).await.unwrap();
    drop(raw);
    let next = tokio::time::timeout(LONG, f.client()).await.unwrap();
    drop(next);
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
    replay(&mut c, &busy, &f.script(&asks(3000, 0)), 1.0).await;
    let script = f.talker(10_000, 5);
    c.call(Call::Shutdown { force: true }).await.unwrap();
    for _ in 0..20 {
        let start = Call::ReplayStart { session: s.clone(), path: script.clone(), speed: 1.0 };
        match d.call(start).await {
            Err(ClientError::Refused(e)) => assert_eq!(e.code, ErrorCode::Busy),
            Err(ClientError::Closed | ClientError::Io(_)) => break,
            r => panic!("{r:?}"),
        }
    }
    host.await.unwrap().unwrap();
    assert_eq!(history(&f, &s).len(), 1);
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

// ---- scripts ----

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
