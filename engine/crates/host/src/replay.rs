//! Plays a recorded script into a session. The agent's harness is always `replay`.

use std::{collections::HashMap, time::Duration};

use axiom_proto::{
    AgentId, AgentState, Answerable, Body, ErrorCode, Execution, ItemId, Outcome, RequestId, RunId,
    WireError,
    script::{Act, Step},
};
use tokio::sync::watch;

use crate::session::{Say, Session};

const MAX_SCRIPT: u64 = 64 * 1024 * 1024;
const MAX_PAUSE: Duration = Duration::from_secs(60);

pub async fn load(path: &str) -> Result<Vec<Step>, WireError> {
    let bad = |m: String| WireError::new(ErrorCode::BadRequest, m);
    let meta = tokio::fs::metadata(path).await.map_err(|e| bad(format!("{path}: {e}")))?;
    if !meta.is_file() || meta.len() > MAX_SCRIPT {
        return Err(bad(format!("{path} is not a file of at most {MAX_SCRIPT} bytes")));
    }
    let text = tokio::fs::read_to_string(path).await.map_err(|e| bad(format!("{path}: {e}")))?;
    text.lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty())
        .map(|(n, l)| Step::parse(l).map_err(|e| bad(format!("{path}:{}: {e}", n + 1))))
        .collect()
}

pub async fn play(
    s: Session,
    steps: Vec<Step>,
    speed: f64,
    run: RunId,
    agent: AgentId,
    mut cancel: watch::Receiver<bool>,
) {
    let (mut name, mut model) = ("Replay".to_owned(), None);
    if let Some(Act::Agent { name: n, model: m }) = steps.first().map(|s| &s.act) {
        name.clone_from(n);
        model.clone_from(m);
    }
    s.say(vec![
        Say::Event(Body::AgentAdded {
            agent: agent.clone(),
            name,
            execution: Execution {
                harness: "replay".into(),
                provider: None,
                model,
                connection: None,
                effort: None,
                environment: None,
            },
            parent: None,
        }),
        Say::Event(Body::RunStarted { run: run.clone(), agent: agent.clone() }),
        Say::Event(Body::AgentState { agent: agent.clone(), state: AgentState::Running }),
    ])
    .await;

    let mut items: HashMap<String, ItemId> = HashMap::new();
    let mut requests: HashMap<String, RequestId> = HashMap::new();
    let (mut outcome, mut error) = (Outcome::Completed, None);
    for step in steps {
        if speed > 0.0 && step.after > 0 {
            let ms = (step.after as f64 / speed).min(MAX_PAUSE.as_millis() as f64);
            tokio::select! {
                () = tokio::time::sleep(Duration::from_micros((ms * 1000.0) as u64)) => {}
                r = cancel.changed() => if r.is_err() { break },
            }
        }
        if *cancel.borrow() {
            outcome = Outcome::Cancelled;
            break;
        }
        let say = match step.act {
            Act::Agent { .. } => continue,
            Act::State { state } => Say::Event(Body::AgentState { agent: agent.clone(), state }),
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
                })
            }
            Act::Answered { request, answer } => match requests.remove(&request) {
                Some(request) => Say::Event(Body::RequestClosed { request, answer }),
                None => continue,
            },
            Act::Usage { usage, provenance } => {
                Say::Event(Body::Usage { agent: agent.clone(), usage, provenance })
            }
            Act::Finish { outcome: o, error: e } => {
                (outcome, error) = (o, e);
                break;
            }
        };
        s.say(vec![say]).await;
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
        requests
            .into_values()
            .map(|request| Say::Event(Body::RequestClosed { request, answer: "expired".into() })),
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
    s.say(end).await;
}
