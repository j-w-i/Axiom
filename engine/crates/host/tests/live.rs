//! The real Claude Code, once, by hand and with its owner's yes: it works on their account. Nothing
//! else starts it. `AXIOM_LIVE_CLAUDE=yes cargo test -p axiom-host --test live -- --ignored`; with
//! `AXIOM_LIVE_PROGRAMS=DIR` a program in that folder is used instead, to try this out; with
//! `AXIOM_LIVE_TOOLS=axiom` it works with Axiom's tools in place of its own.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::print_stdout)]

use std::time::{Duration, Instant};

use axiom_host::Config;
use axiom_proto::*;

mod common;
use common::*;

const SAYS: &str = "Make a file named made.txt that holds the one line: made. Then run the command `cat made.txt`. Then call the tool agent (of the server axiom) with do set to list. Then say in one sentence what each of the three gave.";

#[tokio::test]
#[ignore = "starts the real Claude Code, on its owner's account"]
async fn claude_code_works_once() {
    assert_eq!(std::env::var("AXIOM_LIVE_CLAUDE").as_deref(), Ok("yes"), "its owner has to say so");
    let f = fixture();
    let w = f.dir.path().join("w");
    std::fs::create_dir(&w).unwrap();
    let mut c = Config::new(f.paths.clone());
    c.home = std::env::var_os("HOME").map(Into::into);
    c.programs = std::env::var_os("AXIOM_LIVE_PROGRAMS").map(|p| vec![p.into()]);
    c.bridge = Some(env!("CARGO_BIN_EXE_axiom-host").into());
    f.serve_with(c);
    let mut c = f.client().await;
    let call =
        Call::SessionCreate { title: "Live".into(), workspace: Some(w.to_str().unwrap().into()) };
    let s = SessionId(c.call(call).await.unwrap()["session"].as_str().unwrap().into());
    c.call(Call::Follow { session: s.clone(), after: 0 }).await.unwrap();
    let execution = Execution {
        harness: "claude".into(),
        provider: None,
        model: std::env::var("AXIOM_LIVE_MODEL").ok(),
        connection: None,
        effort: None,
        environment: std::env::var("AXIOM_LIVE_TOOLS").ok(),
    };
    let add = |name: &str| Call::AgentAdd {
        session: s.clone(),
        name: name.into(),
        execution: execution.clone(),
        access: Access::Work,
        trust: false,
        instructions: vec![],
        apart: false,
    };
    let (lead, other) = (add("Lead"), add("Other"));
    let agent = AgentId(c.call(lead).await.unwrap()["agent"].as_str().unwrap().into());
    // There to be listed; never spoken to, so never started.
    c.call(other).await.unwrap();
    let call = Call::Prompt { session: s.clone(), agent, text: SAYS.into(), key: None };
    c.call(call).await.unwrap();
    let began = Instant::now();
    let (mut starts, mut ended) = (0, None);
    while ended.is_none() {
        assert!(began.elapsed() < Duration::from_secs(300), "too long");
        let Some(m) = c.next_within(Duration::from_secs(200)).await.unwrap() else { break };
        let Message::Events { events, .. } = m else { continue };
        for e in events {
            match e.body {
                Body::RequestOpened { request, what, subject, scope, digest, .. } => {
                    println!("ASKED {what}\n  {subject}\n  {scope}");
                    // One start is what was said yes to. Anything else is a no.
                    let start = what.starts_with("asks to start") && starts == 0;
                    starts += usize::from(start);
                    let answer = if start { Answer::Once } else { Answer::Deny };
                    let call =
                        Call::Answer { session: s.clone(), request, answer, digest, words: None };
                    c.call(call).await.unwrap();
                }
                Body::ItemStarted { kind, name, args, .. } => {
                    println!("{kind:?} {name} {}", args.unwrap_or_default());
                }
                Body::ItemEnded { outcome, text, result, .. } => {
                    println!("  {outcome:?} {text:?} {}", result.unwrap_or_default());
                }
                Body::Usage { usage, provenance, .. } => println!("{usage:?} {provenance:?}"),
                Body::RunEnded { outcome, error, .. } => ended = Some((outcome, error)),
                _ => {}
            }
        }
    }
    println!("ENDED {ended:?} after {:?}, {starts} starts", began.elapsed());
    println!("made.txt: {:?}", std::fs::read_to_string(w.join("made.txt")));
    // It ends with the host, and all it runs.
    c.call(Call::Shutdown { force: true }).await.unwrap();
    for _ in 0..1500 {
        if !f.paths.token().exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    assert_eq!(starts, 1);
    assert_eq!(ended.map(|e| e.0), Some(Outcome::Completed));
}
