//! A person's own terminal, through the socket: typed into, read as a screen, kept, closed.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::time::{Duration, Instant};

use axiom_host::{Config, client::Client};
use axiom_proto::*;

mod common;
use common::*;

async fn start(f: &Fixture) -> (Client, SessionId) {
    let w = f.dir.path().join("w");
    for d in ["w", "home"] {
        std::fs::create_dir(f.dir.path().join(d)).unwrap();
    }
    let mut c = Config::new(f.paths.clone());
    c.home = Some(f.dir.path().join("home"));
    c.shell = Some("/bin/sh".into());
    f.serve_with(c);
    let mut c = f.client().await;
    let w = Some(w.to_str().unwrap().to_owned());
    let v = c.call(Call::SessionCreate { title: "S".into(), workspace: w }).await.unwrap();
    (c, SessionId(v["session"].as_str().unwrap().into()))
}

fn text(s: &Screen) -> String {
    s.rows
        .iter()
        .map(|r| r.iter().map(|x| x.text.as_str()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Its screen once it shows `want`.
async fn until(c: &mut Client, t: &str, want: &str) -> Screen {
    let began = Instant::now();
    let mut seen = 0;
    loop {
        let v = c.call(Call::TerminalScreen { terminal: t.into(), seen }).await.unwrap();
        let s: Screen = serde_json::from_value(v).unwrap();
        if text(&s).contains(want) {
            return s;
        }
        assert!(began.elapsed() < Duration::from_secs(20), "{want:?} not in {:?}", text(&s));
        seen = s.version;
    }
}

#[tokio::test]
async fn a_terminal_is_typed_into_and_read_as_a_screen() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    let open = Call::TerminalOpen {
        session: s.clone(),
        cols: 200,
        rows: 10,
        command: Some("echo $((6*7)) && pwd".into()),
    };
    let v = c.call(open).await.unwrap();
    let t = v["terminal"].as_str().unwrap().to_owned();
    let screen = until(&mut c, &t, "42").await;
    assert_eq!((screen.cols, screen.rows.len()), (200, 10));
    let w = std::fs::canonicalize(f.dir.path().join("w")).unwrap();
    until(&mut c, &t, w.to_str().unwrap()).await;
    // Colours arrive as colours, and what a program writes as such goes nowhere.
    c.call(Call::TerminalInput {
        terminal: t.clone(),
        text: "printf '\\033[31mred\\033[0m \\033]52;c;aGk=\\007ok\\n'\r".into(),
    })
    .await
    .unwrap();
    let screen = until(&mut c, &t, "red ok").await;
    let red = screen.rows.iter().flatten().find(|x| x.text == "red").expect("its own span");
    assert_eq!(red.fg.as_deref(), Some("1"));
    assert!(
        text(&screen).lines().any(|l| l == "red ok"),
        "the clipboard's part is gone: {}",
        text(&screen)
    );
    // Something runs in front: it is busy, and is not closed without a second word.
    c.call(Call::TerminalInput { terminal: t.clone(), text: "sleep 65\r".into() }).await.unwrap();
    let mut busy = false;
    for _ in 0..100 {
        let all: Vec<TerminalInfo> =
            serde_json::from_value(c.call(Call::Terminals {}).await.unwrap()).unwrap();
        busy = all.iter().any(|x| x.terminal == t && x.busy);
        if busy {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(busy);
    let no = refused(c.call(Call::TerminalClose { terminal: t.clone(), force: false }).await);
    assert_eq!(no.code, ErrorCode::Busy);
    c.call(Call::TerminalResize { terminal: t.clone(), cols: 80, rows: 20 }).await.unwrap();
    c.call(Call::TerminalClose { terminal: t.clone(), force: true }).await.unwrap();
    let runs = || {
        let o = std::process::Command::new("ps").args(["-axo", "args="]).output().unwrap();
        String::from_utf8_lossy(&o.stdout).lines().any(|l| l.ends_with("sleep 65"))
    };
    let began = Instant::now();
    while runs() {
        assert!(began.elapsed() < Duration::from_secs(10), "what ran in it ends with it");
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let call = Call::TerminalInput { terminal: t, text: "x".into() };
    assert_eq!(refused(c.call(call).await).code, ErrorCode::NotFound);
    for bad in [(1, 10), (10, 0), (2000, 10)] {
        let open =
            Call::TerminalOpen { session: s.clone(), cols: bad.0, rows: bad.1, command: None };
        assert!(c.call(open).await.is_err(), "{bad:?}");
    }
}
