//! The command itself, against a host in this process.

// A test's helpers fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    io::{Read, Write},
    process::{Command, Output, Stdio},
    time::Duration,
};

use axiom_host::{Config, Host, client::Client, paths::Paths};
use axiom_proto::{Call, SessionId};

struct Fixture {
    dir: tempfile::TempDir,
    paths: Paths,
}
fn fixture() -> Fixture {
    let dir = tempfile::tempdir().unwrap();
    let paths = Paths::new(dir.path().join("d"));
    tokio::spawn(Host::start(Config::new(paths.clone())).unwrap().serve());
    Fixture { dir, paths }
}
impl Fixture {
    fn command(&self, args: &[&str]) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_axiom"));
        c.args(args).env("AXIOM_DATA", &self.paths.data);
        c.stdout(Stdio::piped()).stderr(Stdio::piped());
        c
    }
    fn run(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }
    async fn client(&self) -> Client {
        let c = Client::connect(&self.paths, "test");
        tokio::time::timeout(Duration::from_secs(10), c)
            .await
            .expect("a host that answers")
            .unwrap()
    }
    async fn session(&self, title: &str) -> SessionId {
        let call = Call::SessionCreate { title: title.into(), workspace: None };
        let v = self.client().await.call(call).await.unwrap();
        SessionId(v["session"].as_str().unwrap().into())
    }
    fn script(&self, steps: &str) -> String {
        let p = self.dir.path().join("script");
        std::fs::write(&p, steps).unwrap();
        p.to_str().unwrap().to_owned()
    }
}
fn clean(bytes: &[u8]) -> String {
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let bad = |c: char| c.is_control() && c != '\n' || matches!(c, '\u{202a}'..='\u{202e}');
    assert!(!text.contains(bad), "{text:?}");
    text
}

#[tokio::test(flavor = "multi_thread")]
async fn nothing_it_prints_moves_the_terminal() {
    let f = fixture();
    let s = f.session("a\u{9b}31m\u{202e}\u{7f}").await;
    let out = f.run(&["sessions"]);
    assert!(clean(&out.stdout).contains(r"a\u009b31m\u202e\u007f"));
    let evil = f.script("{\"do\":\"\\u001b]0;pwned\\u0007\\u001b[31mred\"}\n");
    for args in [
        vec!["replay", &s.0, &evil],
        vec!["follow", "\u{1b}[31m\u{202e}"],
        vec!["new", "t", "/\u{1b}[31m\nno such"],
        vec!["replay", &s.0, &evil, "\u{1b}"],
    ] {
        let out = f.run(&args);
        assert!(!out.status.success());
        let text = clean(&out.stderr);
        assert_eq!(text.lines().count(), 1, "{text:?}");
    }
    let usage = f.run(&["\u{1b}"]);
    assert!(clean(&usage.stderr).lines().count() > 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn it_stops_quietly_when_its_reader_has() {
    let f = fixture();
    for _ in 0..40 {
        f.client()
            .await
            .call(Call::SessionCreate { title: "t".into(), workspace: Some("w".repeat(4000)) })
            .await
            .unwrap();
    }
    // More than a pipe holds.
    let mut c = f.command(&["sessions"]).spawn().unwrap();
    drop(c.stdout.take());
    let mut said = String::new();
    c.stderr.take().unwrap().read_to_string(&mut said).unwrap();
    assert!(c.wait().unwrap().success());
    assert_eq!(said, "");
}

#[tokio::test(flavor = "multi_thread")]
async fn all_sessions_are_listed() {
    let f = fixture();
    for _ in 0..205 {
        f.session("t").await;
    }
    let all: serde_json::Value = serde_json::from_slice(&f.run(&["sessions"]).stdout).unwrap();
    assert_eq!(all.as_array().unwrap().len(), 205);
}

#[cfg(unix)]
#[tokio::test(flavor = "multi_thread")]
async fn a_signal_ends_it_even_while_nobody_reads() {
    let f = fixture();
    let s = f.session("t").await;
    let text = "x".repeat(900_000);
    let script = f.script(&format!(
        "{{\"do\":\"start\",\"item\":\"p\",\"kind\":\"prose\"}}\n{{\"do\":\"text\",\"item\":\"p\",\"text\":\"{text}\"}}\n{{\"do\":\"end\",\"item\":\"p\",\"outcome\":\"completed\"}}\n"
    ));
    assert!(f.run(&["replay", &s.0, &script, "0"]).status.success());
    for (signal, code) in [("INT", 130), ("TERM", 143), ("HUP", 129)] {
        let mut c = f.command(&["follow", &s.0]).spawn().unwrap();
        // Blocked: the pipe is full, and it is not read.
        tokio::time::sleep(Duration::from_millis(300)).await;
        let id = c.id().to_string();
        assert!(Command::new("kill").args(["-s", signal, &id]).status().unwrap().success());
        assert_eq!(c.wait().unwrap().code(), Some(code));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_key_for_bedrock_is_read_and_has_a_region() {
    let f = fixture();
    let connect = |args: &[&str]| {
        let mut c = f.command(&[&["connect", "Work", "bedrock"], args].concat());
        let mut c = c.stdin(Stdio::piped()).spawn().unwrap();
        c.stdin.take().unwrap().write_all(b"sk-canary-5\n").unwrap();
        c.wait_with_output().unwrap()
    };
    let here = "https://bedrock-runtime.eu-west-2.amazonaws.com";
    for bad in [&["--key"][..], &["https://evil.example", "--key"], &[here]] {
        let o = connect(bad);
        let said = clean(&o.stderr);
        assert!(!o.status.success() && said.contains("Amazon Bedrock"), "{said}");
        assert!(!said.contains("canary"));
    }
    let o = connect(&[here, "--key"]);
    assert!(o.status.success(), "{}", clean(&o.stderr));
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let all = clean(&f.run(&["connections"]).stdout);
    assert!(all.contains(here) && !all.contains("canary"), "{all}");
    // Its models are named by hand.
    let o = f.run(&["models", v["connection"].as_str().unwrap()]);
    assert_eq!((o.status.success(), clean(&o.stdout).trim()), (true, "[]"));
}

#[path = "../../engine/tests/common/server.rs"]
mod server;

struct Asked {
    f: Fixture,
    server: server::Server,
    session: SessionId,
    agent: String,
}

/// An agent on a server that answers from a script.
async fn asked(script: Vec<server::Reply>) -> Asked {
    let f = fixture();
    let server = server::Server::start(script).await;
    let w = f.dir.path().join("w");
    std::fs::create_dir(&w).unwrap();
    let w = std::fs::canonicalize(w).unwrap();
    let c = &mut f.client().await;
    let call =
        Call::SessionCreate { title: "t".into(), workspace: Some(w.to_str().unwrap().to_owned()) };
    let session = SessionId(c.call(call).await.unwrap()["session"].as_str().unwrap().into());
    let call = Call::ConnectionAdd {
        name: "here".into(),
        provider: "openai-compatible".into(),
        url: Some(server.url.clone()),
        secret: None,
    };
    let v = c.call(call).await.unwrap();
    let connection = v["connection"].as_str().unwrap().to_owned();
    let o = f.run(&["agent", &session.0, "Lead", &connection, "m", "ask"]);
    let v: serde_json::Value = serde_json::from_slice(&o.stdout).unwrap();
    let agent = v["agent"].as_str().unwrap().to_owned();
    Asked { f, server, session, agent }
}
impl Asked {
    fn say(&self, json: bool, text: &str) -> (Option<i32>, String, String) {
        let mut args = vec!["say", "--wait", &self.session.0, &self.agent, text];
        if json {
            args.insert(0, "--json");
        }
        let o = self.f.run(&args);
        (o.status.code(), clean(&o.stdout), clean(&o.stderr))
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn a_script_waits_for_the_answer() {
    let t = asked(vec![server::text(&["One", " answer."])]).await;
    assert_eq!(t.say(false, "first"), (Some(0), "One answer.\n".into(), String::new()));
    // The answer to this, not the one before.
    t.server.add(server::text(&["Another."]));
    let (code, out, _) = t.say(true, "--json");
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(
        (code, &v["text"], &v["outcome"]),
        (Some(0), &"Another.".into(), &"completed".into())
    );
    assert_eq!(out.lines().count(), 1);
    assert!(t.server.requests()[1]["messages"].to_string().contains("--json"), "words are words");
    let (code, _, err) = t.say(false, "nobody answers this");
    assert_eq!(code, Some(2), "{err}");
    assert!(!err.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn a_wait_ends_when_a_person_is_asked_and_answers_nothing() {
    let call = server::calls(&[("c1", "shell", r#"{"command":"touch made"}"#)]);
    let t = asked(vec![call, server::text(&["Done."])]).await;
    let (code, _, err) = t.say(false, "make it");
    assert_eq!(code, Some(3), "{err}");
    assert!(err.contains("touch made"), "{err}");
    // Said while it is asked: the same.
    let (code, out, _) = t.say(true, "and this");
    let v: serde_json::Value = serde_json::from_str(out.trim()).unwrap();
    assert_eq!(code, Some(3));
    assert!(v["request"].as_str().is_some_and(|r| !r.is_empty()), "{v}");
    let waits = t.f.run(&["--json", "waiting"]);
    let v: serde_json::Value = serde_json::from_slice(&waits.stdout).unwrap();
    assert_eq!(v.as_array().unwrap().len(), 1, "it is asked still");
    assert_eq!(clean(&waits.stdout).lines().count(), 1);
    assert!(!t.f.dir.path().join("w/made").exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn events_are_lines_of_json() {
    let t = asked(vec![server::text(&["Said\nin two lines."])]).await;
    assert_eq!(t.say(false, "go").0, Some(0));
    let mut p = t.f.command(&["--json", "follow", &t.session.0]).spawn().unwrap();
    let out = std::io::BufReader::new(p.stdout.take().unwrap());
    let mut found = false;
    for l in std::io::BufRead::lines(out) {
        let v: serde_json::Value = serde_json::from_str(&l.unwrap()).unwrap();
        found = found || v["body"]["text"] == "Said\nin two lines.";
        if v["body"]["type"] == "run.ended" {
            break;
        }
    }
    p.kill().unwrap();
    p.wait().unwrap();
    assert!(found);
}

#[tokio::test(flavor = "multi_thread")]
async fn a_folder_is_shown_in_the_windows_and_nothing_begins() {
    let f = fixture();
    let folder = f.dir.path().to_str().unwrap().to_owned();
    f.client().await;
    let o = f.run(&["open", &folder]);
    assert_eq!(o.status.code(), Some(1), "a terminal is no window");
    assert!(clean(&o.stderr).contains("no window"));
    let window = Client::connect(&f.paths, "axiom-app/test");
    let mut window = tokio::time::timeout(Duration::from_secs(10), window).await.unwrap().unwrap();
    assert_eq!(f.run(&["open", &folder]).status.code(), Some(0));
    let real = std::fs::canonicalize(&folder).unwrap();
    loop {
        let m = tokio::time::timeout(Duration::from_secs(10), window.next()).await.unwrap();
        if let axiom_proto::Message::Open { open } = m.unwrap() {
            assert_eq!(std::path::Path::new(&open), real);
            break;
        }
    }
    for bad in ["no/such/folder", "script"] {
        f.script("x");
        assert_eq!(
            f.run(&["open", f.dir.path().join(bad).to_str().unwrap()]).status.code(),
            Some(1)
        );
    }
    let call = Call::Open { path: "relative".into() };
    assert!(f.client().await.call(call).await.is_err());
    let all = f.client().await.call(Call::list()).await.unwrap();
    assert!(all.as_array().unwrap().is_empty(), "nothing has begun");
}
