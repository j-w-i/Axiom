//! Commands that go on while the agent does other things: a server, a watcher, a long build. An
//! agent's own, contained as any command, and ended with the agent.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::{Duration, Instant},
};

use serde_json::json;
use tokio::{io::AsyncReadExt, sync::watch};

use crate::{Output, shell, text::plain};

/// Of one agent at a time.
const MOST: usize = 8;
/// Output that has not been read; more pushes out the oldest.
const KEPT: usize = 64 * 1024;

#[derive(Debug, Clone, Default)]
pub struct Jobs(Arc<Mutex<All>>);

#[derive(Debug, Default)]
struct All {
    next: u32,
    jobs: BTreeMap<u32, One>,
    /// Told to end, and not ended yet.
    ending: Vec<Arc<Mutex<State>>>,
}

#[derive(Debug)]
struct One {
    command: String,
    began: Instant,
    state: Arc<Mutex<State>>,
    /// Dropped, the job ends.
    stop: watch::Sender<bool>,
}

#[derive(Debug, Default)]
struct State {
    fresh: VecDeque<u8>,
    lost: u64,
    ended: Option<String>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Asked to stop, or nobody has it any more.
async fn told(stop: &mut watch::Receiver<bool>) {
    let _ = stop.wait_for(|s| *s).await;
}

impl Jobs {
    pub fn start(&self, c: shell::Command) -> Output {
        let mut all = lock(&self.0);
        all.jobs.retain(|_, j| {
            let s = lock(&j.state);
            s.ended.is_none() || !s.fresh.is_empty()
        });
        if all.jobs.len() >= MOST {
            return Output::bad(format!("not run: there are {MOST} jobs; stop one"));
        }
        let command = c.command.clone();
        let mut p = match c.start() {
            Ok(p) => p,
            Err(e) => return Output::bad(e),
        };
        let (Some(mut out), Some(mut err)) = (p.stdout.take(), p.stderr.take()) else {
            return Output::bad("not run: it has no output");
        };
        let state = Arc::new(Mutex::new(State::default()));
        let (stop, mut stopped) = watch::channel(false);
        let s = state.clone();
        tokio::spawn(async move {
            let (mut a, mut b) = (vec![0; 16 * 1024], vec![0; 16 * 1024]);
            let (mut out_open, mut err_open) = (true, true);
            let mut look = tokio::time::interval(Duration::from_millis(250));
            let mut ended = None;
            while out_open || err_open || ended.is_none() {
                let n = tokio::select! {
                    r = out.read(&mut a), if out_open => {
                        out_open = matches!(r, Ok(n) if n > 0);
                        r.ok().map(|n| &a[..n])
                    }
                    r = err.read(&mut b), if err_open => {
                        err_open = matches!(r, Ok(n) if n > 0);
                        r.ok().map(|n| &b[..n])
                    }
                    r = p.wait(), if ended.is_none() => {
                        ended = Some(r);
                        None
                    }
                    _ = look.tick(), if ended.is_none() => {
                        p.look();
                        None
                    }
                    () = told(&mut stopped), if ended.is_none() => {
                        ended = Some(p.stop(Duration::from_secs(2)).await);
                        None
                    }
                };
                if let Some(bytes) = n {
                    let mut s = lock(&s);
                    s.fresh.extend(bytes);
                    let over = s.fresh.len().saturating_sub(KEPT);
                    s.fresh.drain(..over);
                    s.lost += over as u64;
                }
            }
            let words = match ended {
                Some(Ok(e)) => match (e.code, e.signal) {
                    (Some(c), _) => format!("ended: exit {c}"),
                    (_, Some(n)) => format!("ended by signal {n}"),
                    _ => "ended".into(),
                },
                _ => "its end is not known".into(),
            };
            lock(&s).ended = Some(words);
        });
        all.next += 1;
        let id = all.next;
        all.jobs.insert(id, One { command, began: Instant::now(), state, stop });
        Output {
            detail: json!({"summary": format!("job {id}")}),
            ..Output::ok(format!("job {id} runs; read what it says with job read\n"))
        }
    }

    /// What it has said since it was last read.
    pub async fn read(&self, id: u32, wait: Duration) -> Output {
        let Some(state) = lock(&self.0).jobs.get(&id).map(|j| j.state.clone()) else {
            return Output::bad(format!("there is no job {id}"));
        };
        let until = Instant::now() + wait;
        loop {
            {
                let s = lock(&state);
                if !s.fresh.is_empty() || s.ended.is_some() || Instant::now() >= until {
                    break;
                }
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        let mut s = lock(&state);
        let bytes: Vec<u8> = s.fresh.drain(..).collect();
        let lost = std::mem::take(&mut s.lost);
        let mut text = String::new();
        if lost > 0 {
            text += &format!("({lost} bytes before this were not read in time)\n");
        }
        text += &plain(&bytes);
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        let now = s.ended.clone().unwrap_or_else(|| "it runs".into());
        text += &format!("(job {id}: {now})\n");
        Output { detail: json!({"summary": now}), ..Output::ok(text) }
    }

    pub fn stop(&self, id: u32) -> Output {
        match lock(&self.0).jobs.get(&id) {
            Some(j) => {
                let _ = j.stop.send(true);
                Output::ok(format!("job {id} is being stopped\n"))
            }
            None => Output::bad(format!("there is no job {id}")),
        }
    }

    pub fn list(&self) -> Output {
        let all = lock(&self.0);
        let mut text = String::new();
        for (id, j) in &all.jobs {
            let now = lock(&j.state).ended.clone().unwrap_or_else(|| "runs".into());
            let first = j.command.lines().next().unwrap_or_default();
            let first: String = first.chars().take(100).collect();
            text += &format!("{id}: {first} ({now}, {} s)\n", j.began.elapsed().as_secs());
        }
        if text.is_empty() {
            text.push_str("(no jobs)\n");
        }
        Output::ok(text)
    }

    /// Those that still run, each in a few words.
    pub fn running(&self) -> Vec<String> {
        let all = lock(&self.0);
        let runs = all.jobs.iter().filter(|(_, j)| lock(&j.state).ended.is_none());
        runs.map(|(id, j)| {
            let first: String =
                j.command.lines().next().unwrap_or_default().chars().take(60).collect();
            format!("{id}: {first}")
        })
        .collect()
    }

    /// All of them end.
    pub fn end(&self) {
        let mut all = lock(&self.0);
        let jobs = std::mem::take(&mut all.jobs);
        all.ending.retain(|s| lock(s).ended.is_none());
        all.ending.extend(jobs.into_values().map(|j| j.state));
    }

    /// Until all that were told to end have ended.
    pub async fn ended(&self) {
        loop {
            {
                let mut all = lock(&self.0);
                all.ending.retain(|s| lock(s).ended.is_none());
                if all.ending.is_empty() && all.jobs.is_empty() {
                    return;
                }
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
    }
}
