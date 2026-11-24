use std::{
    collections::HashMap,
    io,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
    time::Duration,
};

use axiom_journal::Journal;
use axiom_proto::*;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::{Mutex, Notify, oneshot, watch},
};

use crate::{
    net::{Boxed, Listener},
    outbox::Outbox,
    paths::{Paths, private_dir, private_file},
    replay,
    session::{Cmd, Session},
    store::Store,
};

#[derive(Debug, thiserror::Error)]
pub enum HostError {
    #[error("another host already serves this data directory")]
    AlreadyRunning,
    #[error(transparent)]
    Io(#[from] io::Error),
    #[error(transparent)]
    Journal(#[from] axiom_journal::Error),
    #[error("the system has no randomness: {0}")]
    Random(String),
}

#[derive(Debug, Clone)]
pub struct Config {
    pub paths: Paths,
    /// Bytes that may wait for one client before it must resynchronise.
    pub outbox: usize,
    /// Stop after this long with no client and nothing running.
    pub idle_exit: Option<Duration>,
}
impl Config {
    pub fn new(paths: Paths) -> Self {
        Self { paths, outbox: 8 * 1024 * 1024, idle_exit: None }
    }
}

const HELLO_WITHIN: Duration = Duration::from_secs(5);
const PAGE: usize = 500;

struct State {
    cfg: Config,
    token: String,
    store: Store,
    sessions: Mutex<HashMap<SessionId, Session>>,
    runs: Mutex<HashMap<RunId, watch::Sender<bool>>>,
    clients: AtomicU64,
    ids: AtomicU64,
    stop: Notify,
    changed: Notify,
}

pub struct Host {
    st: Arc<State>,
    listener: Listener,
    // Held for the life of the host; the system releases it if the process dies.
    _lock: std::fs::File,
}

impl Host {
    pub fn start(cfg: Config) -> Result<Self, HostError> {
        let p = &cfg.paths;
        private_dir(&p.data)?;
        private_dir(&p.run())?;
        let lock =
            std::fs::OpenOptions::new().create(true).truncate(false).write(true).open(p.lock())?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Err(HostError::AlreadyRunning),
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
        let mut journal = Journal::open(&p.journal())?;
        journal.recover(now_ms())?;
        let token = token()?;
        let listener = Listener::bind(p)?;
        private_file(&p.endpoint_file(), &p.endpoint()?)?;
        private_file(&p.token(), &token)?;
        Ok(Self {
            st: Arc::new(State {
                store: Store::start(journal)?,
                token,
                sessions: Mutex::default(),
                runs: Mutex::default(),
                clients: AtomicU64::new(0),
                ids: AtomicU64::new(1),
                stop: Notify::new(),
                changed: Notify::new(),
                cfg,
            }),
            listener,
            _lock: lock,
        })
    }

    /// Serves until `host.shutdown`, idleness, or an unusable listener.
    pub async fn serve(mut self) -> Result<(), HostError> {
        let st = self.st.clone();
        let idle = {
            let st = st.clone();
            async move {
                let Some(d) = st.cfg.idle_exit else {
                    return std::future::pending().await;
                };
                loop {
                    let changed = st.changed.notified();
                    let quiet = st.clients.load(Relaxed) == 0 && st.runs.lock().await.is_empty();
                    if quiet {
                        tokio::select! {
                            () = tokio::time::sleep(d) => return,
                            () = changed => {}
                        }
                    } else {
                        changed.await;
                    }
                }
            }
        };
        tokio::pin!(idle);
        loop {
            tokio::select! {
                s = self.listener.accept() => {
                    let st = st.clone();
                    let s = s?;
                    tokio::spawn(async move {
                        st.clients.fetch_add(1, Relaxed);
                        st.changed.notify_waiters();
                        serve_client(&st, s).await;
                        st.clients.fetch_sub(1, Relaxed);
                        st.changed.notify_waiters();
                    });
                }
                () = st.stop.notified() => break,
                () = &mut idle => break,
            }
        }
        for (_, c) in st.runs.lock().await.drain() {
            let _ = c.send(true);
        }
        // Let the runs say how they ended; give up rather than hang.
        let _ = tokio::time::timeout(Duration::from_secs(2), async {
            tokio::time::sleep(Duration::from_millis(50)).await;
            for s in st.sessions.lock().await.values() {
                s.sync().await;
            }
        })
        .await;
        let _ = std::fs::remove_file(st.cfg.paths.token());
        Ok(())
    }
}

fn token() -> Result<String, HostError> {
    let mut b = [0u8; 32];
    axiom_proto::random(&mut b).map_err(HostError::Random)?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |d, (x, y)| d | (x ^ y)) == 0
}

async fn serve_client(st: &Arc<State>, stream: Boxed) {
    let id = st.ids.fetch_add(1, Relaxed);
    let out = Arc::new(Outbox::new(st.cfg.outbox));
    let (mut rd, mut wr) = tokio::io::split(stream);
    let writer = {
        let out = out.clone();
        tokio::spawn(async move {
            while let Some(b) = out.take().await {
                if wr.write_all(&b).await.is_err() || wr.flush().await.is_err() {
                    break;
                }
            }
            out.close();
            let _ = wr.shutdown().await;
        })
    };
    let mut lines = Lines::default();
    let mut buf = vec![0u8; 64 * 1024];
    let mut known = false;
    let mut following: HashMap<SessionId, tokio::task::JoinHandle<()>> = HashMap::new();
    let deadline = tokio::time::Instant::now() + HELLO_WITHIN;
    'conn: loop {
        let read = rd.read(&mut buf);
        let n = if known {
            read.await
        } else {
            match tokio::time::timeout_at(deadline, read).await {
                Ok(r) => r,
                Err(_) => break,
            }
        };
        let Ok(n) = n else { break };
        if n == 0 {
            break;
        }
        lines.push(&buf[..n]);
        loop {
            let line = match lines.next_line() {
                Ok(Some(l)) => l,
                Ok(None) => break,
                Err(e) => {
                    let error = WireError::new(ErrorCode::Protocol, e.to_string());
                    out.force(&Message::Err { id: 0, error });
                    break 'conn;
                }
            };
            let req: Request = match serde_json::from_slice(&line) {
                Ok(r) => r,
                Err(e) => {
                    let id = serde_json::from_slice::<Value>(&line)
                        .ok()
                        .and_then(|v| v.get("id")?.as_u64())
                        .unwrap_or(0);
                    let error = WireError::new(ErrorCode::BadRequest, e.to_string());
                    out.force(&Message::Err { id, error });
                    if known {
                        continue;
                    }
                    break 'conn;
                }
            };
            let answer = match (known, req.call) {
                (false, Call::Hello { token, protocol, .. }) => {
                    if !same(&token, &st.token) {
                        let error = WireError::new(ErrorCode::Unauthorised, "wrong token");
                        out.force(&Message::Err { id: req.id, error });
                        break 'conn;
                    }
                    if protocol != PROTOCOL {
                        let error = WireError::new(
                            ErrorCode::Protocol,
                            format!("this host speaks protocol {PROTOCOL}, the client {protocol}"),
                        );
                        out.force(&Message::Err { id: req.id, error });
                        break 'conn;
                    }
                    known = true;
                    Ok(json!({ "protocol": PROTOCOL, "version": env!("CARGO_PKG_VERSION") }))
                }
                (false, _) => {
                    let error = WireError::new(ErrorCode::Unauthorised, "say hello first");
                    out.force(&Message::Err { id: req.id, error });
                    break 'conn;
                }
                (true, Call::Follow { session, after }) => {
                    if let Some(t) = following.remove(&session) {
                        t.abort();
                    }
                    match open(st, &session).await {
                        Ok(s) => {
                            // The answer comes before the first event.
                            out.force(&Message::Ok { id: req.id, result: json!({}) });
                            let t =
                                tokio::spawn(follow(st.store.clone(), s, id, out.clone(), after));
                            following.insert(session, t);
                            continue;
                        }
                        Err(e) => Err(e),
                    }
                }
                (true, Call::Unfollow { session }) => {
                    if let Some(t) = following.remove(&session) {
                        t.abort();
                    }
                    if let Some(s) = st.sessions.lock().await.get(&session) {
                        s.cmd(Cmd::Unfollow { client: id }).await;
                    }
                    Ok(json!({}))
                }
                (true, call) => handle(st, call).await,
            };
            out.force(&match answer {
                Ok(result) => Message::Ok { id: req.id, result },
                Err(error) => Message::Err { id: req.id, error },
            });
        }
    }
    for (sid, t) in following {
        t.abort();
        if let Some(s) = st.sessions.lock().await.get(&sid) {
            s.cmd(Cmd::Unfollow { client: id }).await;
        }
    }
    out.close();
    // A client that stopped reading must not keep this task.
    let _ = tokio::time::timeout(Duration::from_secs(2), writer).await;
}

/// History first, at the client's pace; then joins the live events without a gap or a repeat.
async fn follow(store: Store, s: Session, client: u64, out: Arc<Outbox>, mut after: u64) {
    loop {
        let (joined, ok) = oneshot::channel();
        s.cmd(Cmd::Follow { client, out: out.clone(), after, joined }).await;
        if ok.await != Ok(false) {
            return;
        }
        match store.read(s.id.clone(), after, PAGE).await {
            Ok(events) => {
                let Some(last) = events.last().map(|e| e.seq) else {
                    // `after` is ahead of the journal: the client remembers another history.
                    out.force(&Message::Resync { session: s.id.clone(), resync: 0 });
                    return;
                };
                after = last;
                if !out.send(&Message::Events { session: s.id.clone(), events }).await {
                    return;
                }
            }
            Err(_) => {
                out.force(&Message::Resync { session: s.id.clone(), resync: 0 });
                return;
            }
        }
    }
}

fn internal(e: axiom_journal::Error) -> WireError {
    match e {
        axiom_journal::Error::NoSession(s) => {
            WireError::new(ErrorCode::NotFound, format!("no session {s}"))
        }
        e => WireError::new(ErrorCode::Internal, e.to_string()),
    }
}

async fn open(st: &State, id: &SessionId) -> Result<Session, WireError> {
    let mut all = st.sessions.lock().await;
    if let Some(s) = all.get(id) {
        return Ok(s.clone());
    }
    let info = st
        .store
        .session(id.clone())
        .await
        .map_err(internal)?
        .ok_or_else(|| WireError::new(ErrorCode::NotFound, format!("no session {id}")))?;
    let s = Session::open(id.clone(), info.last_seq, st.store.clone());
    all.insert(id.clone(), s.clone());
    Ok(s)
}

async fn handle(st: &Arc<State>, call: Call) -> Result<Value, WireError> {
    match call {
        Call::Hello { .. } => Err(WireError::new(ErrorCode::BadRequest, "already said hello")),
        Call::Follow { .. } | Call::Unfollow { .. } => {
            Err(WireError::new(ErrorCode::Internal, "handled by the connection"))
        }
        Call::Status {} => Ok(json!({
            "version": env!("CARGO_PKG_VERSION"),
            "pid": std::process::id(),
            "clients": st.clients.load(Relaxed),
            "runs": st.runs.lock().await.len(),
        })),
        Call::Shutdown { force } => {
            if !force && !st.runs.lock().await.is_empty() {
                return Err(WireError::new(ErrorCode::Busy, "agents are running"));
            }
            st.stop.notify_one();
            Ok(json!({}))
        }
        Call::SessionList {} => {
            let all = st.store.sessions().await.map_err(internal)?;
            serde_json::to_value(all)
                .map_err(|e| WireError::new(ErrorCode::Internal, e.to_string()))
        }
        Call::SessionCreate { title, workspace } => {
            if title.trim().is_empty() || title.len() > 500 {
                return Err(WireError::new(
                    ErrorCode::BadRequest,
                    "a title has between 1 and 500 bytes",
                ));
            }
            let id = SessionId::new();
            st.store.create(id.clone(), title, workspace, now_ms()).await.map_err(internal)?;
            Ok(json!({ "session": id }))
        }
        Call::ReplayStart { session, path, speed } => {
            if !speed.is_finite() || speed < 0.0 {
                return Err(WireError::new(ErrorCode::BadRequest, "speed is a number from 0"));
            }
            let s = open(st, &session).await?;
            let steps = replay::load(&path).await?;
            let (run, agent) = (RunId::new(), AgentId::new());
            let (tx, rx) = watch::channel(false);
            st.runs.lock().await.insert(run.clone(), tx);
            st.changed.notify_waiters();
            let (st2, r, a) = (st.clone(), run.clone(), agent.clone());
            tokio::spawn(async move {
                replay::play(s, steps, speed, r.clone(), a, rx).await;
                st2.runs.lock().await.remove(&r);
                st2.changed.notify_waiters();
            });
            Ok(json!({ "run": run, "agent": agent }))
        }
        Call::Cancel { run, .. } => match st.runs.lock().await.get(&run) {
            Some(c) => {
                let _ = c.send(true);
                Ok(json!({}))
            }
            None => Err(WireError::new(ErrorCode::NotFound, format!("no running {run}"))),
        },
    }
}
