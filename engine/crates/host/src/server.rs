use std::{
    collections::HashMap,
    io,
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, PoisonError,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
    time::Duration,
};

use axiom_journal::{Journal, Reader};
use axiom_proto::*;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt, ReadHalf},
    sync::{Notify, OwnedSemaphorePermit, Semaphore, watch},
    task::JoinSet,
    time::{Instant, timeout, timeout_at},
};

use crate::{
    faults::{Access, Faults},
    net::{self, Boxed, Listener},
    outbox::{Follower, Outbox},
    paths::{Paths, lock_file, private_dir, private_file},
    replay,
    session::{Joined, Opened, Session, encode, lines},
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
    /// A connection that has not shown the token by then is closed.
    pub hello_within: Duration,
    /// How long a connection that has ended may take to be written out.
    pub linger: Duration,
}
impl Config {
    pub fn new(paths: Paths) -> Self {
        Self {
            paths,
            outbox: 8 * 1024 * 1024,
            idle_exit: None,
            hello_within: Duration::from_secs(5),
            linger: Duration::from_secs(2),
        }
    }
}

/// Connections that have not shown the token. The next ones wait to be accepted.
const STRANGERS: usize = 32;
/// Connections that have.
const CLIENTS: usize = 64;
/// How long runs may take to say how they ended when the host stops. Logout allows little more.
const ENDINGS: Duration = Duration::from_secs(10);

struct State {
    cfg: Config,
    token: String,
    store: Store,
    faults: Arc<Faults>,
    /// What the check after an unclean end found wrong with the journal's file.
    damaged: OnceLock<String>,
    sessions: Mutex<HashMap<SessionId, Arc<Slot>>>,
    runs: Mutex<Runs>,
    connections: AtomicU64,
    clients: Arc<Semaphore>,
    stop: Notify,
    /// Connections or runs came or went.
    changed: Notify,
    /// Connections end.
    over: watch::Sender<bool>,
}

type Slot = tokio::sync::Mutex<Option<Opened>>;

#[derive(Default)]
struct Runs {
    closing: bool,
    live: HashMap<RunId, (SessionId, watch::Sender<bool>)>,
}

impl State {
    fn runs(&self) -> MutexGuard<'_, Runs> {
        self.runs.lock().unwrap_or_else(PoisonError::into_inner)
    }
    fn sessions(&self) -> MutexGuard<'_, HashMap<SessionId, Arc<Slot>>> {
        self.sessions.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Nothing starts after this. False, and no change, if it would disturb something.
    fn close(&self, force: bool) -> bool {
        let mut runs = self.runs();
        if !force && !runs.live.is_empty() {
            return false;
        }
        runs.closing = true;
        true
    }

    /// The session, opened from the journal if nobody has it open.
    async fn open(&self, id: &SessionId) -> Result<Session, WireError> {
        let none = || WireError::new(ErrorCode::NotFound, "no such session");
        // Longer than any of ours; not worth keeping, even in words about it.
        if id.0.len() > 100 {
            return Err(none());
        }
        let slot = {
            let mut all = self.sessions();
            let unused = |s: &Arc<Slot>| {
                Arc::strong_count(s) == 1
                    && s.try_lock().is_ok_and(|s| s.as_ref().is_none_or(Opened::ended))
            };
            all.retain(|_, s| !unused(s));
            all.entry(id.clone()).or_default().clone()
        };
        // Only those who want this session wait for it.
        let mut slot = slot.lock().await;
        if let Some(s) = slot.as_ref().and_then(Opened::session) {
            return Ok(s);
        }
        if let Some(before) = slot.take() {
            before.end().await;
        }
        match self.store.open(id.clone(), now_ms()).await {
            Ok(Some(seq)) => {
                self.faults.clear(id, Access::Write);
                let (s, opened) =
                    Session::open(id.clone(), seq, self.store.clone(), self.faults.clone());
                *slot = Some(opened);
                Ok(s)
            }
            Ok(None) => Err(none()),
            Err(e) => Err(internal(self.faults.note(id, Access::Write, &e))),
        }
    }

    /// Why a session that was open is closed.
    fn closed(&self, id: &SessionId) -> WireError {
        internal(self.faults.words(id).unwrap_or_else(|| "the session is closed".into()))
    }
}

/// A run in the list of what runs, until it is dropped.
struct Live {
    st: Arc<State>,
    run: RunId,
}
impl Live {
    fn new(
        st: &Arc<State>,
        session: &SessionId,
        run: &RunId,
    ) -> Result<(Self, watch::Receiver<bool>), WireError> {
        let (tx, rx) = watch::channel(false);
        let mut runs = st.runs();
        if runs.closing {
            return Err(stopping());
        }
        runs.live.insert(run.clone(), (session.clone(), tx));
        drop(runs);
        st.changed.notify_waiters();
        Ok((Self { st: st.clone(), run: run.clone() }, rx))
    }
}
impl Drop for Live {
    fn drop(&mut self) {
        self.st.runs().live.remove(&self.run);
        self.st.changed.notify_waiters();
    }
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
        let lock = lock_file(&p.lock())?;
        match lock.try_lock() {
            Ok(()) => {}
            Err(std::fs::TryLockError::WouldBlock) => return Err(HostError::AlreadyRunning),
            Err(std::fs::TryLockError::Error(e)) => return Err(e.into()),
        }
        // A host that stops removes its token.
        let unclean = p.token().exists();
        let mut journal = Journal::open(&p.journal())?;
        let faults = Faults::default();
        for (id, e) in journal.recover(now_ms())?.1 {
            faults.note(&id, Access::Write, &e);
        }
        let token = token()?;
        let listener = Listener::bind(p)?;
        private_file(&p.endpoint_file(), listener.path())?;
        private_file(&p.token(), &token)?;
        let st = Arc::new(State {
            store: Store::start(journal, &p.journal())?,
            token,
            faults: Arc::new(faults),
            damaged: OnceLock::new(),
            sessions: Mutex::default(),
            runs: Mutex::default(),
            connections: AtomicU64::new(0),
            clients: Arc::new(Semaphore::new(CLIENTS)),
            stop: Notify::new(),
            changed: Notify::new(),
            over: watch::Sender::new(false),
            cfg,
        });
        if unclean {
            check(&st)?;
        }
        Ok(Self { st, listener, _lock: lock })
    }

    /// Serves until `host.shutdown` or idleness.
    pub async fn serve(self) -> Result<(), HostError> {
        self.serve_until(std::future::pending()).await
    }

    /// Or until `stop`, which is `host.shutdown` with force.
    pub async fn serve_until(mut self, stop: impl Future<Output = ()>) -> Result<(), HostError> {
        let st = self.st.clone();
        let strangers = Arc::new(Semaphore::new(STRANGERS));
        let mut connections = JoinSet::new();
        tokio::pin!(stop);
        {
            let idle = idle(&st);
            tokio::pin!(idle);
            loop {
                tokio::select! {
                    (s, stranger) = next(&mut self.listener, &strangers) => {
                        let st = st.clone();
                        connections.spawn(async move {
                            st.connections.fetch_add(1, Relaxed);
                            st.changed.notify_waiters();
                            if let Ok(s) = net::carry(s) {
                                connection(&st, s, stranger).await;
                            }
                            st.connections.fetch_sub(1, Relaxed);
                            st.changed.notify_waiters();
                        });
                    }
                    Some(_) = connections.join_next() => {}
                    () = st.stop.notified() => break,
                    () = &mut idle => break,
                    () = &mut stop => break,
                }
            }
        }
        st.close(true);
        for (_, c) in st.runs().live.values() {
            let _ = c.send(true);
        }
        // Each run says how it ended, and waits until that is in the journal.
        let _ = timeout(ENDINGS, async {
            loop {
                let changed = st.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if st.runs().live.is_empty() {
                    break;
                }
                changed.await;
            }
        })
        .await;
        // The last answers and events reach whoever still reads.
        st.over.send_replace(true);
        while connections.join_next().await.is_some() {}
        let slots: Vec<_> = st.sessions().drain().collect();
        let _ = timeout(ENDINGS, async {
            for (_, slot) in slots {
                if let Some(s) = slot.lock().await.take() {
                    s.end().await;
                }
            }
        })
        .await;
        let _ = std::fs::remove_file(st.cfg.paths.token());
        let _ = std::fs::remove_file(st.cfg.paths.endpoint_file());
        Ok(())
    }
}

/// Resolves when nobody has been here for long enough, if that is a reason to stop.
async fn idle(st: &State) {
    let Some(d) = st.cfg.idle_exit else {
        return std::future::pending().await;
    };
    loop {
        let changed = st.changed.notified();
        let quiet = st.connections.load(Relaxed) == 0 && st.runs().live.is_empty();
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

/// The next connection, when there is room for one more stranger. An error of the listener (too
/// many open files, say) passes or it does not; the sessions do not end because of it.
async fn next(l: &mut Listener, strangers: &Arc<Semaphore>) -> (net::Raw, OwnedSemaphorePermit) {
    let Ok(permit) = strangers.clone().acquire_owned().await else {
        // Nobody closes it.
        return std::future::pending().await;
    };
    let mut pause = Duration::from_millis(20);
    loop {
        match l.accept().await {
            Ok(s) => return (s, permit),
            Err(e) => {
                eprintln!("axiom-host: accepting a connection: {e}");
                tokio::time::sleep(pause).await;
                pause = (pause * 2).min(Duration::from_secs(1));
            }
        }
    }
}

/// Looks through the journal's file, after the last host did not stop in order.
fn check(st: &Arc<State>) -> io::Result<()> {
    let (st, path) = (st.clone(), st.cfg.paths.journal());
    std::thread::Builder::new().name("journal check".into()).spawn(move || {
        if let Err(e) = Reader::open(&path).and_then(|r| r.check()) {
            eprintln!("axiom-host: {e}");
            let mut words = e.to_string();
            limits::clip(&mut words, limits::TITLE);
            let _ = st.damaged.set(words);
        }
    })?;
    Ok(())
}

fn token() -> Result<String, HostError> {
    let mut b = [0u8; 32];
    axiom_proto::random(&mut b).map_err(HostError::Random)?;
    Ok(b.iter().map(|x| format!("{x:02x}")).collect())
}

fn same(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |d, (x, y)| d | (x ^ y)) == 0
}

async fn connection(st: &Arc<State>, stream: Boxed, stranger: OwnedSemaphorePermit) {
    let out = Arc::new(Outbox::new(st.cfg.outbox));
    let (rd, mut wr) = tokio::io::split(stream);
    let mut writer = {
        let out = out.clone();
        tokio::spawn(async move {
            while let Some(b) = out.take().await {
                if wr.write_all(&b).await.is_err() || wr.flush().await.is_err() {
                    break;
                }
            }
            out.abandon();
            let _ = wr.shutdown().await;
        })
    };
    let mut over = st.over.subscribe();
    tokio::select! {
        () = requests(st, rd, &out, stranger) => {}
        _ = over.wait_for(|o| *o) => {}
    }
    out.close();
    // A client that stopped reading must not keep the socket.
    if timeout(st.cfg.linger, &mut writer).await.is_err() {
        writer.abort();
    }
    out.abandon();
}

/// Reads requests until the client ends, or does what ends it.
async fn requests(
    st: &Arc<State>,
    mut rd: ReadHalf<Boxed>,
    out: &Arc<Outbox>,
    stranger: OwnedSemaphorePermit,
) {
    let mut lines = Lines::new(limits::HELLO_LINE);
    let mut buf = vec![0u8; 64 * 1024];
    // The place this connection takes: a stranger's, until it shows the token.
    let mut _place = stranger;
    let mut known = false;
    // Keeps what is followed open.
    let mut following: HashMap<SessionId, Session> = HashMap::new();
    let deadline = Instant::now() + st.cfg.hello_within;
    loop {
        let read = rd.read(&mut buf);
        let n = if known {
            read.await
        } else {
            match timeout_at(deadline, read).await {
                Ok(r) => r,
                Err(_) => return,
            }
        };
        match n {
            Ok(n @ 1..) => lines.push(&buf[..n]),
            _ => return,
        }
        loop {
            // Answers are bounded by not asking for more of them.
            if !out.room().await {
                return;
            }
            let line = match lines.next_line() {
                Ok(Some(l)) => l,
                Ok(None) => break,
                Err(e) => {
                    out.answer(0, Err(WireError::new(ErrorCode::Protocol, e.to_string())));
                    return;
                }
            };
            let req: Request = match serde_json::from_slice(&line) {
                Ok(r) => r,
                Err(_) => {
                    let error = WireError::new(ErrorCode::BadRequest, "not a request");
                    out.answer(id_of(&line), Err(error));
                    if known {
                        continue;
                    }
                    return;
                }
            };
            match (known, req.call) {
                (false, call) => match hello(st, call) {
                    Ok(client) => {
                        _place = client;
                        known = true;
                        lines.set_limit(MAX_LINE);
                        let version = env!("CARGO_PKG_VERSION");
                        out.answer(req.id, Ok(json!({ "protocol": PROTOCOL, "version": version })));
                    }
                    Err(e) => {
                        out.answer(req.id, Err(e));
                        return;
                    }
                },
                (true, Call::Follow { session, after }) => {
                    following.remove(&session);
                    let f = Follower::begin(out, &session);
                    match follow(st, &session, after).await {
                        Ok((s, page)) => {
                            // The answer comes before the first event.
                            out.answer(req.id, Ok(json!({})));
                            tokio::spawn(feed(st.clone(), s.clone(), f, after, page));
                            following.insert(session, s);
                        }
                        Err(e) => {
                            out.unfollow(&session);
                            out.answer(req.id, Err(e));
                        }
                    }
                }
                (true, Call::Unfollow { session }) => {
                    out.unfollow(&session);
                    following.remove(&session);
                    out.answer(req.id, Ok(json!({})));
                }
                (true, call) => out.answer(req.id, handle(st, call).await),
            }
        }
    }
}

/// The number of a request that could not be read, without reading the rest of it.
fn id_of(line: &[u8]) -> u64 {
    #[derive(Deserialize)]
    struct Id {
        id: u64,
    }
    serde_json::from_slice(line).map_or(0, |i: Id| i.id)
}

/// A place among the clients, for who shows the token.
fn hello(st: &State, call: Call) -> Result<OwnedSemaphorePermit, WireError> {
    let Call::Hello { token, protocol, .. } = call else {
        return Err(WireError::new(ErrorCode::Unauthorised, "say hello first"));
    };
    if !same(&token, &st.token) {
        return Err(WireError::new(ErrorCode::Unauthorised, "wrong token"));
    }
    if protocol != PROTOCOL {
        let m = format!("this host speaks protocol {PROTOCOL}, the client {protocol}");
        return Err(WireError::new(ErrorCode::Protocol, m));
    }
    st.clients
        .clone()
        .try_acquire_owned()
        .map_err(|_| WireError::new(ErrorCode::Busy, "too many clients"))
}

/// Opens a session and reads the first page, so that what cannot be read is an error answer.
async fn follow(
    st: &State,
    id: &SessionId,
    after: u64,
) -> Result<(Session, Vec<Event>), WireError> {
    let s = st.open(id).await?;
    match st.store.read(id.clone(), after).await {
        Ok(page) => Ok((s, page)),
        Err(e) => Err(internal(st.faults.note(id, Access::Read, &e))),
    }
}

/// History first, at the client's pace; then joins the live events without a gap or a repeat.
async fn feed(st: Arc<State>, s: Session, f: Follower, from: u64, mut page: Vec<Event>) {
    let mut after = from;
    let words = loop {
        let sent = match encode(&page).and_then(|e| lines(&s.id, &e)) {
            Ok(l) => l,
            Err(e) => break st.faults.note(&s.id, Access::Read, &e),
        };
        for (last, line) in sent {
            if !f.send(&line).await {
                return;
            }
            after = last;
        }
        match s.join(f.clone(), after).await {
            Ok(Joined::Yes) => {
                if from == 0 {
                    // All of it could be read.
                    st.faults.clear(&s.id, Access::Read);
                }
                return;
            }
            Ok(Joined::Ahead) => return f.resync(0),
            Ok(Joined::Behind(a)) => after = a,
            Err(_) => break st.closed(&s.id).message,
        }
        match st.store.read(s.id.clone(), after).await {
            Ok(p) if p.is_empty() => {
                let why = format!("event {after} is not the last, but nothing follows it");
                break st.faults.note(&s.id, Access::Read, &why);
            }
            Ok(p) => page = p,
            Err(e) => break st.faults.note(&s.id, Access::Read, &e),
        }
    };
    f.fault(&words);
}

fn internal(words: impl Into<String>) -> WireError {
    WireError::new(ErrorCode::Internal, words)
}
fn stopping() -> WireError {
    WireError::new(ErrorCode::Busy, "the host is stopping")
}

async fn handle(st: &Arc<State>, call: Call) -> Result<Value, WireError> {
    match call {
        Call::Hello { .. } => Err(WireError::new(ErrorCode::BadRequest, "already said hello")),
        Call::Follow { .. } | Call::Unfollow { .. } => Err(internal("handled by the connection")),
        Call::Status {} => {
            let mut status = json!({
                "version": env!("CARGO_PKG_VERSION"),
                "pid": std::process::id(),
                "clients": CLIENTS - st.clients.available_permits(),
                "runs": st.runs().live.len(),
                "faults": st.faults.list(),
            });
            if let Some(d) = st.damaged.get() {
                status["damaged"] = d.as_str().into();
            }
            Ok(status)
        }
        Call::Shutdown { force } => {
            if !st.close(force) {
                return Err(WireError::new(ErrorCode::Busy, "agents are running"));
            }
            st.stop.notify_one();
            Ok(json!({}))
        }
        Call::SessionList { limit, offset } => {
            let mut all = st
                .store
                .sessions(limit.min(MAX_LIST), offset)
                .await
                .map_err(|e| internal(e.to_string()))?;
            // What the journal has as running, and this host does not run, has been cut short.
            let runs = st.runs();
            for s in &mut all {
                s.running &= runs.live.values().any(|(id, _)| *id == s.id);
            }
            drop(runs);
            serde_json::to_value(all).map_err(|e| internal(e.to_string()))
        }
        Call::SessionCreate { title, workspace } => {
            if title.trim().is_empty() || limits::text_len(&title) > limits::TITLE {
                return Err(WireError::new(
                    ErrorCode::BadRequest,
                    "a title has between 1 and 500 bytes",
                ));
            }
            if workspace.as_ref().is_some_and(|w| limits::text_len(w) > limits::WORKSPACE) {
                return Err(WireError::new(
                    ErrorCode::BadRequest,
                    "a workspace's name has at most 4096 bytes",
                ));
            }
            if st.runs().closing {
                return Err(stopping());
            }
            let id = SessionId::new();
            st.store
                .create(id.clone(), title, workspace, now_ms())
                .await
                .map_err(|e| internal(e.to_string()))?;
            Ok(json!({ "session": id }))
        }
        Call::ReplayStart { session, path, speed } => {
            if !speed.is_finite() || speed < 0.0 {
                return Err(WireError::new(ErrorCode::BadRequest, "speed is a number from 0"));
            }
            let s = st.open(&session).await?;
            let steps = replay::load(&path).await?;
            let (run, agent) = (RunId::new(), AgentId::new());
            let (live, cancel) = Live::new(st, &session, &run)?;
            // Answered when it is in the journal for good that the run started.
            let p = replay::start(s, steps, speed, run.clone(), agent.clone())
                .await
                .map_err(|_| st.closed(&session))?;
            tokio::spawn(async move {
                // Closed, the session has no more to say; what it has is in the journal.
                let _ = p.play(cancel).await;
                drop(live);
            });
            Ok(json!({ "run": run, "agent": agent }))
        }
        Call::Cancel { session, run } => match st.runs().live.get(&run) {
            Some((s, c)) if *s == session => {
                let _ = c.send(true);
                Ok(json!({}))
            }
            _ => Err(WireError::new(ErrorCode::NotFound, "no such run in this session")),
        },
    }
}
