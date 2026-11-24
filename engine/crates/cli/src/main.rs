//! `axiom`: everything a client can ask of the host, from a terminal.

use std::{io::Write, process::ExitCode};

use axiom_host::{client::Client, paths::Paths};
use axiom_proto::{Body, Call, Message, Outcome, SessionId};

const USAGE: &str = "usage:
  axiom status
  axiom sessions
  axiom new TITLE [WORKSPACE]
  axiom replay SESSION SCRIPT [SPEED]
  axiom follow SESSION
  axiom stop [--force]";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let rt = match tokio::runtime::Builder::new_current_thread().enable_all().build() {
        Ok(rt) => rt,
        Err(e) => return fail(&e),
    };
    match rt.block_on(run(&args.iter().map(String::as_str).collect::<Vec<_>>())) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => fail(e.as_ref()),
    }
}
fn fail(e: &dyn std::error::Error) -> ExitCode {
    eprintln!("axiom: {e}");
    ExitCode::FAILURE
}

async fn run(args: &[&str]) -> Result<(), Box<dyn std::error::Error>> {
    let call = match args {
        ["status"] => Call::Status {},
        ["sessions"] => Call::SessionList {},
        ["new", title] => Call::SessionCreate { title: (*title).into(), workspace: None },
        ["new", title, w] => {
            let w = std::fs::canonicalize(w)?;
            Call::SessionCreate { title: (*title).into(), workspace: Some(w.display().to_string()) }
        }
        ["replay", s, path, rest @ ..] => Call::ReplayStart {
            session: SessionId((*s).into()),
            path: std::fs::canonicalize(path)?.display().to_string(),
            speed: rest.first().map_or(Ok(1.0), |v| v.parse())?,
        },
        ["follow", s] => Call::Follow { session: SessionId((*s).into()), after: 0 },
        ["stop"] => Call::Shutdown { force: false },
        ["stop", "--force"] => Call::Shutdown { force: true },
        _ => return Err(USAGE.into()),
    };
    let paths = Paths::discover().ok_or("no data directory: set AXIOM_DATA")?;
    let mut c = Client::connect(&paths, concat!("axiom-cli/", env!("CARGO_PKG_VERSION")))
        .await
        .map_err(|e| format!("no host answers in {} ({e})", paths.data.display()))?;
    let follow = matches!(call, Call::Follow { .. });
    let answer = c.call(call).await?;
    if !follow {
        println!("{}", serde_json::to_string_pretty(&answer)?);
        return Ok(());
    }
    let mut out = std::io::stdout();
    let mut shown: std::collections::HashSet<String> = Default::default();
    loop {
        match c.next().await? {
            Message::Events { events, .. } => {
                for e in events {
                    match e.body {
                        Body::ItemDelta { item, text } => {
                            shown.insert(item.0);
                            write!(out, "{}", plain(&text))?;
                        }
                        Body::ItemEnded { item, text, outcome, .. } => {
                            if !shown.remove(&item.0) {
                                write!(out, "{}", plain(&text))?;
                            }
                            let note =
                                if outcome == Outcome::Completed { "" } else { " [cut short]" };
                            writeln!(out, "{note}")?;
                        }
                        Body::RunEnded { outcome, error, .. } => {
                            writeln!(out, "-- {outcome:?} {}", plain(&error.unwrap_or_default()))?;
                        }
                        Body::ItemStarted { name, .. } if !name.is_empty() => {
                            writeln!(out, "* {}", plain(&name))?;
                        }
                        Body::RequestOpened { what, subject, .. } => {
                            writeln!(out, "? {}: {}", plain(&what), plain(&subject))?;
                        }
                        _ => {}
                    }
                }
                out.flush()?;
            }
            Message::Resync { .. } => return Err("fell behind; run the command again".into()),
            _ => {}
        }
    }
}

/// What agents write never reaches the terminal as control codes (SI-15).
fn plain(s: &str) -> String {
    s.chars()
        .map(|c| if c.is_control() && c != '\n' && c != '\t' { '\u{fffd}' } else { c })
        .collect()
}
