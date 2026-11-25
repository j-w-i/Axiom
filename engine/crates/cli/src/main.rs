//! `axiom`: everything a client can ask of the host, from a terminal.

use std::{
    collections::HashMap,
    io::{self, Read, Write},
    process::ExitCode,
    time::Duration,
};

use axiom_host::{client::Client, paths::Paths, signal};
use axiom_proto::{
    Access, AgentId, Answer, Answerable, Body, Call, Event, Execution, Message, Outcome, RequestId,
    RunId, SecretText, SessionId,
};
use plain::{block, json, line};
use serde_json::Value;

mod plain;

const USAGE: &str = "usage:
  axiom status
  axiom sessions
  axiom waiting                                 what waits for a person
  axiom new TITLE [WORKSPACE]
  axiom replay SESSION SCRIPT [SPEED]
  axiom follow SESSION
  axiom connections
  axiom connect NAME PROVIDER [URL] [--key]     a key is read from standard input
  axiom disconnect CONNECTION
  axiom models CONNECTION
  axiom agent SESSION NAME CONNECTION MODEL read|ask|work
  axiom set SESSION AGENT read|ask|work
  axiom say SESSION AGENT TEXT
  axiom answer SESSION REQUEST DIGEST once|session|deny
  axiom cancel SESSION RUN
  axiom audit [AFTER]
  axiom stop [--force]";

/// Longer is no key.
const KEY: u64 = 16 * 1024;

fn access(s: &str) -> Result<Access, Error> {
    match s {
        "read" => Ok(Access::Read),
        "ask" => Ok(Access::Ask),
        "work" => Ok(Access::Work),
        _ => Err(USAGE.into()),
    }
}

/// Never from the arguments: every process of this machine can read those (SI-3).
fn key() -> Result<SecretText, Error> {
    let mut s = String::new();
    io::stdin().take(KEY + 1).read_to_string(&mut s)?;
    let t = s.trim().to_owned();
    if t.is_empty() || s.len() as u64 > KEY {
        return Err("a key is read from standard input: between 1 and 16384 bytes".into());
    }
    Ok(SecretText(t))
}

/// A host that is busy with strangers lets a client in sooner than this.
const ANSWER_WITHIN: Duration = Duration::from_secs(10);

type Error = Box<dyn std::error::Error + Send + Sync>;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match start(args) {
        Ok(code) => code,
        Err(e) => fail(&e),
    }
}

/// The work has a thread of its own, because it may wait for a terminal that does not read. A
/// signal ends the process all the same.
fn start(args: Vec<String>) -> io::Result<ExitCode> {
    let runtime = || tokio::runtime::Builder::new_current_thread().enable_all().build();
    let (done, finished) = tokio::sync::oneshot::channel();
    let work = runtime()?;
    std::thread::Builder::new().spawn(move || done.send(work.block_on(run(args))))?;
    runtime()?.block_on(async {
        let stop = signal::stop()?;
        Ok(tokio::select! {
            r = finished => match r {
                Ok(Ok(())) => ExitCode::SUCCESS,
                Ok(Err(e)) if pipe(e.as_ref()) => ExitCode::SUCCESS,
                Ok(Err(e)) => fail(e.as_ref()),
                Err(e) => fail(&e),
            },
            // As a shell says it.
            n = stop => ExitCode::from(128 + u8::try_from(n).unwrap_or(0)),
        })
    })
}

/// Whoever read our output has had enough.
fn pipe(e: &(dyn std::error::Error + 'static)) -> bool {
    e.downcast_ref::<io::Error>().is_some_and(|e| e.kind() == io::ErrorKind::BrokenPipe)
}

fn fail(e: &(dyn std::error::Error + 'static)) -> ExitCode {
    let text = e.to_string();
    let text = if text == USAGE { text } else { line(&text) };
    let _ = writeln!(io::stderr(), "axiom: {text}");
    ExitCode::FAILURE
}

async fn run(args: Vec<String>) -> Result<(), Error> {
    let call = match args.iter().map(String::as_str).collect::<Vec<_>>()[..] {
        ["status"] => Call::Status {},
        ["sessions"] => Call::list(),
        ["waiting"] => Call::Waiting {},
        ["new", title] => Call::SessionCreate { title: title.into(), workspace: None },
        ["new", title, w] => {
            let w = std::fs::canonicalize(w).map_err(|e| format!("{w}: {e}"))?;
            Call::SessionCreate { title: title.into(), workspace: Some(w.display().to_string()) }
        }
        ["replay", s, path, ref rest @ ..] => Call::ReplayStart {
            session: SessionId(s.into()),
            path: std::fs::canonicalize(path)
                .map_err(|e| format!("{path}: {e}"))?
                .display()
                .to_string(),
            speed: rest.first().map_or(Ok(1.0), |v| v.parse())?,
        },
        ["follow", s] => Call::Follow { session: SessionId(s.into()), after: 0 },
        ["connections"] => Call::Connections {},
        ["connect", name, provider, ref rest @ ..] => {
            let url = rest.iter().find(|a| **a != "--key").map(|u| (*u).to_owned());
            let secret = if rest.contains(&"--key") { Some(key()?) } else { None };
            Call::ConnectionAdd { name: name.into(), provider: provider.into(), url, secret }
        }
        ["disconnect", c] => Call::ConnectionRemove { connection: c.into() },
        ["models", c] => Call::Models { connection: c.into() },
        ["agent", s, name, c, model, a] => Call::AgentAdd {
            session: SessionId(s.into()),
            name: name.into(),
            execution: Execution {
                harness: "axiom".into(),
                provider: None,
                model: Some(model.into()),
                connection: Some(c.into()),
                effort: None,
                environment: None,
            },
            access: access(a)?,
            trust: false,
            instructions: Vec::new(),
            apart: false,
        },
        ["set", s, agent, a] => Call::AgentSet {
            session: SessionId(s.into()),
            agent: AgentId(agent.into()),
            execution: None,
            access: Some(access(a)?),
        },
        ["say", s, agent, text] => Call::Prompt {
            session: SessionId(s.into()),
            agent: AgentId(agent.into()),
            text: text.into(),
            key: None,
        },
        ["answer", s, r, digest, a] => Call::Answer {
            session: SessionId(s.into()),
            request: RequestId(r.into()),
            answer: match a {
                "once" => Answer::Once,
                "session" => Answer::Session,
                "deny" => Answer::Deny,
                _ => return Err(USAGE.into()),
            },
            digest: digest.into(),
        },
        ["cancel", s, r] => Call::Cancel { session: SessionId(s.into()), run: RunId(r.into()) },
        ["audit"] => Call::Audit { after: 0, limit: 200 },
        ["audit", n] => Call::Audit { after: n.parse()?, limit: 200 },
        ["stop"] => Call::Shutdown { force: false },
        ["stop", "--force"] => Call::Shutdown { force: true },
        _ => return Err(USAGE.into()),
    };
    let paths = Paths::discover().ok_or("no data directory: set AXIOM_DATA")?;
    let no =
        |e: &dyn std::error::Error| format!("no host answers in {} ({e})", paths.data.display());
    let client = Client::connect(&paths, concat!("axiom-cli/", env!("CARGO_PKG_VERSION")));
    let mut c = tokio::time::timeout(ANSWER_WITHIN, client)
        .await
        .map_err(|e| no(&e))?
        .map_err(|e| no(&e))?;
    if let Call::SessionList { .. } = call {
        return print(&Value::Array(sessions(&mut c).await?));
    }
    let follow = matches!(call, Call::Follow { .. });
    let answer = c.call(call).await?;
    if !follow {
        return print(&answer);
    }
    let mut shown = HashMap::new();
    loop {
        match c.next().await? {
            Message::Events { events, .. } => {
                let mut out = io::stdout().lock();
                for e in events {
                    event(&mut out, &mut shown, e)?;
                }
                out.flush()?;
            }
            Message::Resync { .. } => return Err("fell behind; run the command again".into()),
            Message::Fault { fault, .. } => return Err(fault.into()),
            _ => {}
        }
    }
}

/// All of them, a page at a time.
async fn sessions(c: &mut Client) -> Result<Vec<Value>, Error> {
    let mut all = Vec::new();
    loop {
        let offset = u32::try_from(all.len())?;
        let limit = axiom_proto::MAX_LIST;
        match c.call(Call::SessionList { limit, offset }).await? {
            Value::Array(page) if !page.is_empty() => all.extend(page),
            _ => return Ok(all),
        }
    }
}

fn print(v: &Value) -> Result<(), Error> {
    let mut out = io::stdout().lock();
    writeln!(out, "{}", json(&serde_json::to_string_pretty(v)?))?;
    Ok(out.flush()?)
}

/// `shown` is the text of each unfinished item that is on the screen.
fn event(out: &mut impl Write, shown: &mut HashMap<String, String>, e: Event) -> io::Result<()> {
    match e.body {
        Body::ItemDelta { item, text } => {
            write!(out, "{}", block(&text))?;
            shown.entry(item.0).or_default().push_str(&text);
        }
        Body::ItemEnded { item, text, outcome, .. } => {
            let before = shown.remove(&item.0).unwrap_or_default();
            // The final text replaces the deltas: usually it goes on where they stopped.
            let same: usize = before
                .chars()
                .zip(text.chars())
                .take_while(|(a, b)| a == b)
                .map(|(c, _)| c.len_utf8())
                .sum();
            let from = if same < before.len() {
                writeln!(out)?;
                text[..same].rfind('\n').map_or(0, |i| i + 1)
            } else {
                same
            };
            let note = if outcome == Outcome::Completed { "" } else { " [cut short]" };
            writeln!(out, "{}{note}", block(&text[from..]))?;
        }
        Body::RunEnded { outcome, error, .. } => {
            writeln!(out, "-- {outcome:?} {}", line(&error.unwrap_or_default()))?;
        }
        Body::ItemStarted { name, .. } if !name.is_empty() => {
            writeln!(out, "* {}", line(&name))?;
        }
        Body::RequestOpened { request, what, subject, scope, answerable, digest, .. } => {
            let scope = if scope.is_empty() { scope } else { format!(" ({})", line(&scope)) };
            writeln!(out, "? {}: {}{scope}", line(&what), line(&subject))?;
            if answerable == Answerable::Live && !digest.is_empty() {
                writeln!(out, "  answer {} {}", line(&request.0), line(&digest))?;
            }
        }
        Body::AgentAdded { agent, name, .. } => {
            writeln!(out, "+ {} {}", line(&agent.0), line(&name))?;
        }
        Body::RunStarted { run, .. } => writeln!(out, "-- run {}", line(&run.0))?,
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use axiom_proto::{AgentId, Answerable, ItemId, RequestId};

    use super::*;

    fn screen(bodies: Vec<Body>) -> String {
        let (mut out, mut shown) = (Vec::new(), HashMap::new());
        for body in bodies {
            event(&mut out, &mut shown, Event { seq: 1, at: 1, body }).unwrap();
        }
        assert!(shown.is_empty());
        String::from_utf8(out).unwrap()
    }
    fn delta(text: &str) -> Body {
        Body::ItemDelta { item: ItemId("i".into()), text: text.into() }
    }
    fn end(text: &str) -> Body {
        let item = ItemId("i".into());
        Body::ItemEnded {
            item,
            outcome: Outcome::Completed,
            text: text.into(),
            result: None,
            result_cut: 0,
        }
    }

    #[test]
    fn the_rest_of_a_live_item_is_printed() {
        assert_eq!(screen(vec![delta("w0 "), delta("w1 "), end("w0 w1 w2 w3")]), "w0 w1 w2 w3\n");
        assert_eq!(screen(vec![end("all of it")]), "all of it\n");
        assert_eq!(screen(vec![delta("all"), end("all")]), "all\n");
        // What was shown and is not so any more stays, and the truth follows.
        let cut = screen(vec![delta("né\n[more is not kept]"), end("né\n[9 more bytes]")]);
        assert_eq!(cut, "né\n[more is not kept]\n[9 more bytes]\n");
    }

    #[test]
    fn an_approval_is_one_line_with_its_scope() {
        let ask = Body::RequestOpened {
            request: RequestId::new(),
            agent: AgentId::new(),
            what: "Run".into(),
            subject: "ls\n-- Completed \nsafe\u{202e}txt".into(),
            scope: "this\nfolder".into(),
            answerable: Answerable::Live,
            digest: String::new(),
            item: None,
            session: false,
        };
        assert_eq!(
            screen(vec![ask]),
            "? Run: ls\\n-- Completed \\nsafe\\u{202e}txt (this\\nfolder)\n"
        );
    }
}
