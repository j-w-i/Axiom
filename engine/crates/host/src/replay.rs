//! Plays a recorded script into a session. The agent's harness is always `replay`.

use std::{
    collections::{HashMap, HashSet},
    io::Read,
    sync::Arc,
    time::Duration,
};

use axiom_proto::{
    AgentId, AgentState, Answerable, Body, Covers, ErrorCode, Execution, ItemId, Outcome,
    RequestId, RunId, WireError,
    script::{Act, Step},
};
use tokio::sync::{OwnedSemaphorePermit, Semaphore, watch};

use crate::session::{Closed, Refused, Say, Session};

const MAX_SCRIPT: u64 = 64 * 1024 * 1024;
const MAX_PAUSE: Duration = Duration::from_secs(60);

/// Scripts held at once, in KiB of their files. What they say takes a few times that in memory.
pub const HELD: usize = 128 * 1024;

pub struct Script {
    steps: Vec<Step>,
    _held: OwnedSemaphorePermit,
}

/// Reads a script, if `held` has room for it.
pub async fn load(path: String, held: Arc<Semaphore>) -> Result<Script, WireError> {
    tokio::task::spawn_blocking(move || read(&path, held))
        .await
        .map_err(|e| WireError::new(ErrorCode::Internal, e.to_string()))?
}

fn read(path: &str, held: Arc<Semaphore>) -> Result<Script, WireError> {
    let bad = |m: &str| WireError::new(ErrorCode::BadRequest, m);
    let not_a_script = || bad("the script is not a readable file of at most 64 MiB");
    // Opening a device may do something.
    if !std::fs::metadata(path).is_ok_and(|m| m.is_file()) {
        return Err(not_a_script());
    }
    let (file, len) = open(path).map_err(|_| not_a_script())?;
    if len > MAX_SCRIPT {
        return Err(not_a_script());
    }
    let kib = u32::try_from(len / 1024 + 1).map_err(|_| not_a_script())?;
    let held = held
        .try_acquire_many_owned(kib)
        .map_err(|_| WireError::new(ErrorCode::Busy, "too much is being replayed"))?;
    let mut text = String::new();
    file.take(len + 1).read_to_string(&mut text).map_err(|_| not_a_script())?;
    if text.len() as u64 > len {
        return Err(bad("the script grows while it is read"));
    }
    Ok(Script { steps: parse(&text)?, _held: held })
}

/// Opens what is a file, and says how long it is.
fn open(path: &str) -> std::io::Result<(std::fs::File, u64)> {
    let mut o = std::fs::OpenOptions::new();
    o.read(true);
    // The name may stand for a pipe by now, and opening one waits for its other end.
    #[cfg(unix)]
    std::os::unix::fs::OpenOptionsExt::custom_flags(&mut o, libc::O_NONBLOCK);
    let file = o.open(path)?;
    let meta = file.metadata()?;
    match meta.is_file() {
        true => Ok((file, meta.len())),
        false => Err(std::io::Error::other("not a file")),
    }
}

fn parse(text: &str) -> Result<Vec<Step>, WireError> {
    let bad = |m: String| WireError::new(ErrorCode::BadRequest, m);
    let mut steps = Vec::new();
    // Names in use: one given twice would leave the first without an end.
    let (mut items, mut requests) = (HashSet::new(), HashSet::new());
    for (n, l) in text.lines().enumerate().filter(|(_, l)| !l.trim().is_empty()) {
        // What the line holds is not repeated: it may be anything.
        let step = Step::parse(l).map_err(|_| bad(format!("line {} is not a step", n + 1)))?;
        let fresh = match &step.act {
            Act::Start { item, .. } => items.insert(item.clone()),
            Act::End { item, .. } => items.remove(item) || true,
            Act::Ask { request, .. } => requests.insert(request.clone()),
            Act::Answered { request, .. } => requests.remove(request) || true,
            _ => true,
        };
        if !fresh {
            return Err(bad(format!("line {} begins what has not ended", n + 1)));
        }
        steps.push(step);
    }
    Ok(steps)
}

pub struct Player {
    s: Session,
    script: Script,
    speed: f64,
    run: RunId,
    agent: AgentId,
}

/// Begins the run. It is in the journal for good when this returns.
pub async fn start(
    s: Session,
    script: Script,
    speed: f64,
    run: RunId,
    agent: AgentId,
) -> Result<Player, Refused> {
    let (mut name, mut model) = ("Replay".to_owned(), None);
    if let Some(Act::Agent { name: n, model: m }) = script.steps.first().map(|s| &s.act) {
        name.clone_from(n);
        model.clone_from(m);
    }
    s.say(vec![
        Say::Event(Body::AgentAdded {
            agent: agent.clone(),
            name,
            execution: Box::new(Execution {
                harness: "replay".into(),
                provider: None,
                model,
                connection: None,
                effort: None,
                environment: None,
                ..Execution::default()
            }),
            parent: None,
            access: None,
            apart: false,
            from: None,
            plan: false,
        }),
        Say::Event(Body::RunStarted { run: run.clone(), agent: agent.clone() }),
        Say::Event(Body::AgentState { agent: agent.clone(), state: AgentState::Running }),
    ])
    .await?;
    s.sync().await?;
    Ok(Player { s, script, speed, run, agent })
}

impl Player {
    /// Plays to the end, or until cancelled. How it ended is in the journal when this returns.
    pub async fn play(self, mut cancel: watch::Receiver<bool>) -> Result<(), Refused> {
        let Self { s, script: Script { steps, _held }, speed, run, agent } = self;
        let mut items: HashMap<String, ItemId> = HashMap::new();
        let mut requests: HashMap<String, RequestId> = HashMap::new();
        let (mut outcome, mut error) = (Outcome::Completed, None);
        for step in steps {
            if speed > 0.0 && step.after > 0 {
                let ms = (step.after as f64 / speed).min(MAX_PAUSE.as_millis() as f64);
                tokio::select! {
                    () = tokio::time::sleep(Duration::from_micros((ms * 1000.0) as u64)) => {}
                    _ = cancel.wait_for(|c| *c) => {}
                    () = s.closed() => return Err(Closed.into()),
                }
            }
            if *cancel.borrow() {
                outcome = Outcome::Cancelled;
                break;
            }
            let say = match step.act {
                Act::Agent { .. } => continue,
                Act::State { state } => {
                    Say::Event(Body::AgentState { agent: agent.clone(), state })
                }
                Act::Start { item, kind, name, args } => {
                    let id = ItemId::new();
                    items.insert(item, id.clone());
                    Say::Event(Body::ItemStarted {
                        item: id,
                        run: run.clone(),
                        agent: agent.clone(),
                        kind,
                        name,
                        args,
                        args_cut: 0,
                    })
                }
                Act::Text { item, text } => match items.get(&item) {
                    Some(id) => Say::Text(id.clone(), text),
                    None => continue,
                },
                Act::End { item, outcome, result } => match items.remove(&item) {
                    Some(id) => Say::End(id, outcome, result),
                    None => continue,
                },
                Act::Ask { request, what, subject, scope } => {
                    let id = RequestId::new();
                    requests.insert(request, id.clone());
                    Say::Event(Body::RequestOpened {
                        request: id,
                        agent: agent.clone(),
                        what,
                        subject,
                        scope,
                        // Nobody is waiting: this already happened.
                        answerable: Answerable::No,
                        digest: String::new(),
                        item: None,
                        session: false,
                    })
                }
                Act::Answered { request, answer } => match requests.remove(&request) {
                    Some(request) => Say::Event(Body::RequestClosed { request, answer }),
                    None => continue,
                },
                Act::Usage { usage, provenance } => Say::Event(Body::Usage {
                    agent: agent.clone(),
                    usage,
                    provenance,
                    of: Covers::Unknown,
                    run: Some(run.clone()),
                    on: None,
                    cost: None,
                    totals: Vec::new(),
                    cut: false,
                    aside: false,
                }),
                Act::Finish { outcome: o, error: e } => {
                    (outcome, error) = (o, e);
                    break;
                }
            };
            s.say(vec![say]).await?;
        }
        let mut end: Vec<_> = items
            .into_values()
            .map(|id| {
                Say::End(
                    id,
                    if outcome == Outcome::Completed { Outcome::Interrupted } else { outcome },
                    None,
                )
            })
            .collect();
        end.extend(
            requests.into_values().map(|request| {
                Say::Event(Body::RequestClosed { request, answer: "expired".into() })
            }),
        );
        end.push(Say::Event(Body::RunEnded { run, outcome, error }));
        end.push(Say::Event(Body::AgentState {
            agent,
            state: match outcome {
                Outcome::Failed => AgentState::Failed,
                Outcome::Completed => AgentState::Done,
                _ => AgentState::Paused,
            },
        }));
        s.say(end).await?;
        s.sync().await?;
        Ok(())
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    /// As when the name is given to a pipe after it was looked at.
    #[test]
    fn a_pipe_is_refused_without_waiting_for_its_other_end() {
        let d = tempfile::tempdir().unwrap();
        let pipe = d.path().join("pipe");
        assert!(std::process::Command::new("mkfifo").arg(&pipe).status().unwrap().success());
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || tx.send(open(pipe.to_str().unwrap()).is_err()));
        assert!(rx.recv_timeout(Duration::from_secs(5)).expect("not to wait"));
    }
}
