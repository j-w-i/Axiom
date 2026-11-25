//! `axiom`: everything a client can ask of the host, from a terminal.

use std::{
    collections::HashMap,
    io::{self, Read, Write},
    process::ExitCode,
    time::Duration,
};

use axiom_host::{client::Client, paths::Paths, signal};
use axiom_proto::{
    Access, AgentId, Answer, Answerable, Body, Call, Event, Execution, ItemId, ItemKind, Limit,
    Message, Outcome, RequestId, RunId, SecretText, SessionId, Unit,
};
use plain::{block, json, line};
use serde_json::Value;

mod plain;

const USAGE: &str = "usage: axiom [--json] COMMAND      --json: one line for each answer or event
  axiom status
  axiom limits                                  what the host keeps everything to
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
  axiom say [--wait] SESSION AGENT TEXT         --wait: prints the answer, and ends with
                                                0 done, 2 failed or cut short,
                                                3 a person is asked; 1 as ever
  axiom compact SESSION AGENT
  axiom branch [--files] SESSION AGENT ITEM NAME    another agent, from before a prompt or after an
                                                answer on; --files: with its copy's files then.
                                                Items: axiom --json follow SESSION
  axiom open FOLDER                             in the window, ready for a new session
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
                Ok(Ok(code)) => ExitCode::from(code),
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

async fn run(args: Vec<String>) -> Result<u8, Error> {
    // Before the command only, or an agent could not be told "--json".
    let mut args: Vec<&str> = args.iter().map(String::as_str).collect();
    let compact = args.first() == Some(&"--json");
    if compact {
        args.remove(0);
    }
    let wait = args.starts_with(&["say", "--wait"]) && args.len() == 5;
    if wait {
        args.remove(1);
    }
    let files = args.starts_with(&["branch", "--files"]);
    if files {
        args.remove(1);
    }
    let call = match args[..] {
        ["status"] => Call::Status {},
        ["limits"] => Call::Limits {},
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
        ["open", path] => {
            let path = std::fs::canonicalize(path).map_err(|e| format!("{path}: {e}"))?;
            Call::Open { path: path.display().to_string() }
        }
        ["compact", s, agent] => {
            Call::Compact { session: SessionId(s.into()), agent: AgentId(agent.into()) }
        }
        ["branch", s, agent, item, name] => Call::Branch {
            session: SessionId(s.into()),
            agent: AgentId(agent.into()),
            item: ItemId(item.into()),
            name: name.into(),
            files,
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
            words: None,
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
        return print(&Value::Array(sessions(&mut c).await?), compact);
    }
    if wait && let Call::Prompt { session, agent, .. } = &call {
        let of = (session.clone(), agent.clone());
        return waited(&mut c, call, of, compact).await;
    }
    let follow = matches!(call, Call::Follow { .. });
    let opens = matches!(call, Call::Open { .. });
    let told = matches!(call, Call::Limits {});
    let answer = c.call(call).await?;
    if opens && answer["windows"] == 0 {
        return Err("no window is open".into());
    }
    if told && !compact {
        return limits(serde_json::from_value(answer)?);
    }
    if !follow {
        return print(&answer, compact);
    }
    let mut shown = HashMap::new();
    loop {
        match c.next().await? {
            Message::Events { events, .. } => {
                let mut out = io::stdout().lock();
                for e in events {
                    if compact {
                        writeln!(out, "{}", json(&serde_json::to_string(&e)?))?;
                    } else {
                        event(&mut out, &mut shown, e)?;
                    }
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

/// In words, under what they are limits of.
fn limits(all: Vec<Limit>) -> Result<u8, Error> {
    let mut out = io::stdout().lock();
    let mut of = None;
    for l in all {
        if of.as_ref() != Some(&l.of) {
            writeln!(out, "{}{}", if of.is_some() { "\n" } else { "" }, line(&l.of))?;
        }
        writeln!(out, "  {:<58}  {}", line(&l.what), amount(l.most, l.unit))?;
        of = Some(l.of);
    }
    out.flush()?;
    Ok(0)
}

fn amount(most: Option<u64>, unit: Unit) -> String {
    let Some(n) = most else { return "no limit".into() };
    let in_whole = |of: &[(u64, &str)]| {
        let (size, name) = of.iter().find(|(s, _)| n > 0 && n.is_multiple_of(*s)).copied()?;
        let n = n / size;
        Some(if name.ends_with('B') {
            format!("{n} {name}")
        } else {
            format!("{n} {name}{}", if n == 1 { "" } else { "s" })
        })
    };
    let named = match unit {
        Unit::Bytes => in_whole(&[(1 << 30, "GiB"), (1 << 20, "MiB"), (1 << 10, "KiB"), (1, "B")]),
        Unit::Seconds => in_whole(&[(3600, "hour"), (60, "minute"), (1, "second")]),
        Unit::Count | Unit::Unknown => None,
    };
    named.unwrap_or_else(|| {
        let digits = n.to_string();
        let mut s = String::new();
        for (i, c) in digits.chars().enumerate() {
            if i > 0 && (digits.len() - i).is_multiple_of(3) {
                s.push(' ');
            }
            s.push(c);
        }
        s
    })
}

fn print(v: &Value, compact: bool) -> Result<u8, Error> {
    let mut out = io::stdout().lock();
    let text = if compact { serde_json::to_string(v)? } else { serde_json::to_string_pretty(v)? };
    writeln!(out, "{}", json(&text))?;
    out.flush()?;
    Ok(0)
}

/// After what is said too late for a run, how long another may take to begin.
const SETTLED: Duration = Duration::from_secs(2);
const FAILED: u8 = 2;
const ASKS: u8 = 3;

/// What has come of what was said.
#[derive(Debug, PartialEq)]
enum End {
    Ran(Outcome, Option<String>),
    /// A person is asked: nothing here answers (SI-2).
    Asks {
        request: RequestId,
        what: String,
        subject: String,
    },
}

/// Follows one agent from what is said to it to the end of it.
struct Watch {
    agent: AgentId,
    items: HashMap<String, String>,
    said: String,
    /// The run that is on, from the first that is told.
    run: Option<RunId>,
}
impl Watch {
    fn event(&mut self, body: Body) -> Option<End> {
        match body {
            Body::RunStarted { run, agent } if agent == self.agent => self.run = Some(run),
            Body::ItemStarted { item, agent, kind: ItemKind::Prose, .. } if agent == self.agent => {
                self.items.insert(item.0, String::new());
            }
            Body::ItemEnded { item, text, .. } if self.items.remove(&item.0).is_some() => {
                self.said = text;
            }
            Body::RequestOpened { request, agent, what, subject, .. } if agent == self.agent => {
                return Some(End::Asks { request, what, subject });
            }
            Body::RunEnded { run, outcome, error } if self.run.as_ref() == Some(&run) => {
                self.run = None;
                return Some(End::Ran(outcome, error));
            }
            _ => {}
        }
        None
    }
}

/// Says something and waits for what comes of it.
async fn waited(
    c: &mut Client,
    call: Call,
    of: (SessionId, AgentId),
    compact: bool,
) -> Result<u8, Error> {
    let (session, agent) = of;
    // From before it is said, so that nothing of it is missed.
    let known = sessions(c).await?;
    let known = known.iter().find(|s| s["id"] == session.0.as_str());
    let after = known.and_then(|s| s["last_seq"].as_u64()).unwrap_or(0);
    let told = c.call(call).await?;
    let queued = told["queued"] == true;
    let run = RunId(told["run"].as_str().unwrap_or_default().into());
    let mut w = Watch { agent, items: HashMap::new(), said: String::new(), run: Some(run) };
    // What it was asked before this was said.
    if queued {
        let waits = c.call(Call::Waiting {}).await?;
        let waits = waits.as_array().cloned().unwrap_or_default();
        let one = waits.iter().find(|x| {
            x["need"] == "asks"
                && x["session"] == session.0.as_str()
                && x["agent"] == w.agent.0.as_str()
        });
        if let Some(x) = one {
            let text = |k: &str| x[k].as_str().unwrap_or_default().to_owned();
            let (request, what) = (RequestId(text("request")), text("what"));
            return ends(&w, End::Asks { request, what, subject: text("subject") }, compact);
        }
    }
    c.call(Call::Follow { session, after }).await?;
    let mut end = None;
    loop {
        // One that was told while it worked may have heard it, or goes on with it in another run.
        let next = match &end {
            Some(_) => match tokio::time::timeout(SETTLED, c.next()).await {
                Ok(m) => m?,
                Err(_) => break,
            },
            None => c.next().await?,
        };
        match next {
            Message::Events { events, .. } => {
                for e in events {
                    if let Some(x) = w.event(e.body) {
                        end = Some(x);
                    } else if w.run.is_some() {
                        end = None;
                    }
                    let last = matches!(end, Some(End::Asks { .. })) || (end.is_some() && !queued);
                    if last && let Some(end) = end {
                        return ends(&w, end, compact);
                    }
                }
            }
            Message::Resync { .. } => {
                return Err("fell behind; what comes of it is not known".into());
            }
            Message::Fault { fault, .. } => return Err(fault.into()),
            _ => {}
        }
    }
    match end {
        Some(end) => ends(&w, end, compact),
        None => Err("what comes of it is not known".into()),
    }
}

fn ends(w: &Watch, end: End, compact: bool) -> Result<u8, Error> {
    let mut out = io::stdout().lock();
    let code = match &end {
        End::Ran(Outcome::Completed, _) => 0,
        End::Ran(..) => FAILED,
        End::Asks { .. } => ASKS,
    };
    if compact {
        let v = match &end {
            End::Ran(outcome, error) => {
                serde_json::json!({"outcome": outcome, "error": error, "text": w.said})
            }
            End::Asks { request, what, subject } => {
                serde_json::json!({"request": request, "what": what, "subject": subject})
            }
        };
        writeln!(out, "{}", json(&serde_json::to_string(&v)?))?;
    } else {
        if !w.said.is_empty() {
            writeln!(out, "{}", block(&w.said))?;
        }
        out.flush()?;
        match end {
            End::Ran(Outcome::Completed, _) => {}
            End::Ran(outcome, error) => {
                let why = line(&error.unwrap_or_default());
                writeln!(io::stderr(), "axiom: {outcome:?} {why}")?;
            }
            End::Asks { request, what, subject } => {
                let (r, what, subject) = (line(&request.0), line(&what), line(&subject));
                writeln!(io::stderr(), "axiom: a person is asked ({r}): {what}: {subject}")?;
            }
        }
    }
    out.flush()?;
    Ok(code)
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
    fn amounts_are_in_the_largest_whole_unit() {
        use Unit::{Bytes, Count, Seconds, Unknown};
        for (most, unit, want) in [
            (None, Count, "no limit"),
            (None, Bytes, "no limit"),
            (Some(0), Bytes, "0"),
            (Some(8), Count, "8"),
            (Some(999), Count, "999"),
            (Some(1000), Count, "1 000"),
            (Some(100_000), Count, "100 000"),
            (Some(1_234_567), Unknown, "1 234 567"),
            (Some(500), Bytes, "500 B"),
            (Some(1536), Bytes, "1536 B"),
            (Some(64 << 10), Bytes, "64 KiB"),
            (Some(8 << 20), Bytes, "8 MiB"),
            (Some(16 << 30), Bytes, "16 GiB"),
            (Some(1), Seconds, "1 second"),
            (Some(90), Seconds, "90 seconds"),
            (Some(600), Seconds, "10 minutes"),
            (Some(3600), Seconds, "1 hour"),
            (Some(4 * 3600), Seconds, "4 hours"),
        ] {
            assert_eq!(amount(most, unit), want);
        }
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
