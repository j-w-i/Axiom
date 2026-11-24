//! The command itself, against a host in this process.

// A test's helpers fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{
    io::Read,
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
