use std::{
    collections::{HashMap, VecDeque},
    io,
    path::PathBuf,
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
    sync::{Notify, OwnedMutexGuard, OwnedSemaphorePermit, Semaphore, oneshot, watch},
    task::{JoinHandle, JoinSet},
    time::{Instant, sleep_until, timeout},
};

use crate::{
    agents::{self, Agents, Setup, Spoken},
    connections::{Connections, MemoryVault, Vault},
    faults::{Access, Faults},
    log,
    net::{self, Boxed, Listener},
    outbox::{Follower, Outbox},
    paths::{Lock, Paths, Same, private_dir, private_file},
    replay,
    session::{Joined, Opened, Refused, Say, Session, encode, lines},
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
    #[error("stopped, because its files are not its own any more: {0}")]
    Lost(io::Error),
}

#[derive(Clone)]
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
    /// How often it looks whether clients still find it.
    pub look_every: Duration,
    /// The person's home folder: what signs them in there is kept from agents.
    pub home: Option<PathBuf>,
    /// Where agents have their temporary folders. Not inside the data directory, which they never
    /// see.
    pub scratch: PathBuf,
    pub vault: Arc<dyn Vault>,
}
impl Config {
    pub fn new(paths: Paths) -> Self {
        let scratch = paths.scratch();
        Self {
            paths,
            outbox: 8 * 1024 * 1024,
            idle_exit: None,
            hello_within: Duration::from_secs(5),
            linger: Duration::from_secs(2),
            look_every: Duration::from_secs(3),
            home: home(),
            scratch,
            vault: Arc::new(MemoryVault::default()),
        }
    }
}

fn home() -> Option<PathBuf> {
    let var = if cfg!(windows) { "USERPROFILE" } else { "HOME" };
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

/// Connections that have not shown the token. One more closes the oldest.
const STRANGERS: usize = 32;
/// Connections that have.
const CLIENTS: usize = 64;
/// All of them, with those that are being closed. More wait to be accepted.
const CONNECTIONS: usize = 128;
/// Runs at once.
const RUNS: usize = 128;
/// How many times in a row it may fail to look after its files.
const LOOKS: u32 = 10;
/// Bytes read ahead of a request that is being answered.
const AHEAD: usize = 64 * 1024;
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
    scripts: Arc<Semaphore>,
    stop: Notify,
    /// Connections or runs came or went.
    changed: Notify,
    /// Connections end.
    over: watch::Sender<bool>,
    agents: Arc<Agents>,
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
    async fn open(self: &Arc<Self>, id: &SessionId) -> Result<Session, WireError> {
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
        let slot = slot.lock_owned().await;
        if let Some(s) = slot.as_ref().and_then(Opened::session) {
            return Ok(s);
        }
        // Whoever asked may leave. What has begun ends all the same, and the next one waits for it.
        let (st, id) = (self.clone(), id.clone());
        match tokio::spawn(async move { st.open_in(slot, &id).await }).await {
            Ok(r) => r.and_then(|s| s.ok_or_else(none)),
            Err(e) => Err(internal(e.to_string())),
        }
    }

    async fn open_in(
        &self,
        mut slot: OwnedMutexGuard<Option<Opened>>,
        id: &SessionId,
    ) -> Result<Option<Session>, WireError> {
        if let Some(before) = slot.take() {
            before.end().await;
        }
        match self.store.open(id.clone(), now_ms()).await {
            Ok(Some(seq)) => {
                self.faults.clear(id, Access::Write);
                let (s, opened) =
                    Session::open(id.clone(), seq, self.store.clone(), self.faults.clone());
                *slot = Some(opened);
                Ok(Some(s))
            }
            Ok(None) => Ok(None),
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
        if runs.live.len() >= RUNS {
            return Err(WireError::new(ErrorCode::Busy, "too much is running"));
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
    listener: Option<Listener>,
    journal: Same,
    // Released after the socket is gone.
    lock: Lock,
    old_lock: Lock,
}

fn lock(p: &std::path::Path) -> Result<Lock, HostError> {
    Lock::take(p)?.ok_or(HostError::AlreadyRunning)
}

impl Host {
    pub fn start(cfg: Config) -> Result<Self, HostError> {
        let p = &cfg.paths;
        private_dir(&p.data)?;
        private_dir(&p.run())?;
        let (lock, old_lock) = (lock(&p.lock())?, lock(&p.old_lock())?);
        // A host that stops removes its token.
        let unclean = p.token().exists();
        let mut journal = Journal::open(&p.journal())?;
        let same = Same::of(&p.journal())?;
        let faults = Faults::default();
        for (id, e) in journal.recover(now_ms())?.1 {
            faults.note(&id, Access::Write, &e);
        }
        let token = token()?;
        let listener = Listener::bind(p)?;
        private_file(&p.endpoint_file(), listener.path())?;
        private_file(&p.token(), &token)?;
        let connections = Connections::open(&p.data.join("connections.json"), cfg.vault.clone())?;
        let agents =
            Agents::new(connections, p, cfg.home.clone(), cfg.scratch.clone(), listener.path());
        let st = Arc::new(State {
            agents: Arc::new(agents),
            store: Store::start(journal, &p.journal())?,
            token,
            faults: Arc::new(faults),
            damaged: OnceLock::new(),
            sessions: Mutex::default(),
            runs: Mutex::default(),
            connections: AtomicU64::new(0),
            clients: Arc::new(Semaphore::new(CLIENTS)),
            scripts: Arc::new(Semaphore::new(replay::HELD)),
            stop: Notify::new(),
            changed: Notify::new(),
            over: watch::Sender::new(false),
            cfg,
        });
        if unclean {
            check(&st)?;
        }
        Ok(Self { st, listener: Some(listener), journal: same, lock, old_lock })
    }

    /// Clients find the host by its files, which anybody may remove (a cleaner of temporary files, a
    /// hand). Puts back what is gone; fails for good if that would not be safe.
    fn put_back(&mut self) -> Result<(), Gone> {
        let p = &self.st.cfg.paths;
        if !self.journal.still() {
            return Err(Gone::Lost(io::Error::other("the journal was removed or replaced")));
        }
        private_dir(&p.data)?;
        private_dir(&p.run())?;
        for (held, path) in [(&mut self.lock, p.lock()), (&mut self.old_lock, p.old_lock())] {
            if !held.still() {
                *held = Lock::take(&path)?.ok_or_else(|| {
                    Gone::Lost(io::Error::other("another host serves this directory"))
                })?;
            }
        }
        if !self.listener.as_ref().is_some_and(Listener::still) {
            // The old one first: it cleans up the name they may share.
            self.listener = None;
            self.listener = Some(Listener::bind(p)?);
            log::note("its socket was gone; there is a new one".into());
        }
        let path = self.listener.as_ref().map_or("", Listener::path);
        for (file, text) in [(p.endpoint_file(), path), (p.token(), &self.st.token)] {
            if !holds(&file, text)? {
                private_file(&file, text)?;
            }
        }
        Ok(())
    }

    /// Serves until `host.shutdown` or idleness.
    pub async fn serve(self) -> Result<(), HostError> {
        self.serve_until(std::future::pending()).await
    }

    /// Or until `stop`, which is `host.shutdown` with force.
    pub async fn serve_until(mut self, stop: impl Future<Output = ()>) -> Result<(), HostError> {
        let st = self.st.clone();
        let strangers = Arc::new(Strangers::default());
        let mut connections = JoinSet::new();
        let mut look = tokio::time::interval(st.cfg.look_every);
        look.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let (mut lost, mut failed) = (None, 0u32);
        tokio::pin!(stop);
        {
            let idle = idle(&st);
            tokio::pin!(idle);
            loop {
                let room = connections.len() < CONNECTIONS;
                tokio::select! {
                    s = next(&mut self.listener), if room => {
                        let (st, place) = (st.clone(), strangers.admit());
                        let here = Here::new(&st);
                        connections.spawn(async move {
                            if let Ok(s) = net::carry(s) {
                                connection(&st, s, place).await;
                            }
                            drop(here);
                        });
                    }
                    Some(_) = connections.join_next() => {}
                    _ = look.tick() => match self.put_back() {
                        Ok(()) => failed = 0,
                        // Too many open files, a full disk: it may pass.
                        Err(Gone::NotNow(e)) if failed < LOOKS || too_many_files(&e) => {
                            log::note(format!("looking after its files: {e}"));
                            // Others may hold them for as long as they like. Nothing is lost by it.
                            failed += u32::from(!too_many_files(&e));
                        }
                        Err(Gone::NotNow(e) | Gone::Lost(e)) => {
                            lost = Some(HostError::Lost(e));
                            break;
                        }
                    },
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
        // Only what is still its own: there may be another host by now.
        let p = &st.cfg.paths;
        let path = self.listener.as_ref().map_or("", Listener::path);
        for (file, text) in [(p.token(), st.token.as_str()), (p.endpoint_file(), path)] {
            if holds(&file, text).unwrap_or(false) {
                let _ = std::fs::remove_file(file);
            }
        }
        lost.map_or(Ok(()), Err)
    }
}

/// Why the host's files are not as they were.
enum Gone {
    Lost(io::Error),
    NotNow(io::Error),
}
impl From<io::Error> for Gone {
    fn from(e: io::Error) -> Self {
        Gone::NotNow(e)
    }
}

fn too_many_files(e: &io::Error) -> bool {
    #[cfg(unix)]
    return matches!(e.raw_os_error(), Some(libc::EMFILE | libc::ENFILE));
    // ERROR_TOO_MANY_OPEN_FILES
    #[cfg(windows)]
    return e.raw_os_error() == Some(4);
}

fn holds(file: &std::path::Path, text: &str) -> io::Result<bool> {
    match std::fs::read(file) {
        Ok(t) => Ok(t == text.as_bytes()),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e),
    }
}

/// Counts a connection, however it ends.
struct Here(Arc<State>);
impl Here {
    fn new(st: &Arc<State>) -> Self {
        st.connections.fetch_add(1, Relaxed);
        st.changed.notify_waiters();
        Self(st.clone())
    }
}
impl Drop for Here {
    fn drop(&mut self) {
        self.0.connections.fetch_sub(1, Relaxed);
        self.0.changed.notify_waiters();
    }
}

/// Who is connected and has not shown the token, oldest first.
#[derive(Default)]
struct Strangers {
    list: Mutex<VecDeque<(u64, oneshot::Sender<()>)>>,
    ids: AtomicU64,
}
impl Strangers {
    fn list(&self) -> MutexGuard<'_, VecDeque<(u64, oneshot::Sender<()>)>> {
        self.list.lock().unwrap_or_else(PoisonError::into_inner)
    }
    /// One more. The oldest goes if they are too many: silence must not keep anybody out.
    fn admit(self: &Arc<Self>) -> Stranger {
        let id = self.ids.fetch_add(1, Relaxed);
        let (tx, leave) = oneshot::channel();
        let mut list = self.list();
        if list.len() >= STRANGERS {
            // Dropped, its other end resolves.
            list.pop_front();
        }
        list.push_back((id, tx));
        Stranger { all: self.clone(), id, leave }
    }
}
struct Stranger {
    all: Arc<Strangers>,
    id: u64,
    leave: oneshot::Receiver<()>,
}
impl Drop for Stranger {
    fn drop(&mut self) {
        self.all.list().retain(|(id, _)| *id != self.id);
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

/// The next connection. An error of the listener (too many open files, say) passes or it does not;
/// the sessions do not end because of it.
async fn next(l: &mut Option<Listener>) -> net::Raw {
    let Some(l) = l else {
        return std::future::pending().await;
    };
    let mut pause = Duration::from_millis(20);
    loop {
        match l.accept().await {
            Ok(s) => return s,
            Err(e) => {
                log::note(format!("accepting a connection: {e}"));
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
            let mut words = e.to_string();
            limits::clip(&mut words, limits::TITLE);
            let _ = st.damaged.set(words.clone());
            log::note(words);
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

/// The writing half of a connection. However the reading half ends, it holds nothing after that.
struct Writer {
    out: Arc<Outbox>,
    task: JoinHandle<()>,
}
impl Drop for Writer {
    fn drop(&mut self) {
        self.task.abort();
        self.out.abandon();
    }
}

async fn connection(st: &Arc<State>, stream: Boxed, stranger: Stranger) {
    let out = Arc::new(Outbox::new(st.cfg.outbox));
    let (rd, mut wr) = tokio::io::split(stream);
    let mut writer = {
        let (out, o) = (out.clone(), out.clone());
        let task = tokio::spawn(async move {
            while let Some(b) = out.take().await {
                if wr.write_all(&b).await.is_err() || wr.flush().await.is_err() {
                    break;
                }
            }
            out.abandon();
            let _ = wr.shutdown().await;
        });
        Writer { out: o, task }
    };
    let mut over = st.over.subscribe();
    let reader = Reading {
        rd,
        buf: vec![0; 64 * 1024],
        lines: Lines::new(limits::HELLO_LINE),
        linger: st.cfg.linger,
        until: None,
    };
    tokio::select! {
        () = requests(st, reader, &out, stranger) => {}
        _ = over.wait_for(|o| *o) => {}
    }
    out.close();
    // A client that stopped reading must not keep the socket.
    let _ = timeout(st.cfg.linger, &mut writer.task).await;
}

/// The reading half of a connection.
struct Reading {
    rd: ReadHalf<Boxed>,
    buf: Vec<u8>,
    lines: Lines,
    linger: Duration,
    /// The client has ended its side; what it asked may take until then.
    until: Option<Instant>,
}

impl Reading {
    /// False if no more comes.
    async fn more(&mut self) -> bool {
        if self.until.is_some() {
            return false;
        }
        let n = self.rd.read(&mut self.buf).await;
        self.took(n)
    }
    fn took(&mut self, n: io::Result<usize>) -> bool {
        match n {
            Ok(n @ 1..) => {
                self.lines.push(&self.buf[..n]);
                true
            }
            _ => {
                self.until = Some(Instant::now() + self.linger);
                false
            }
        }
    }
    /// Does `f`, unless the client leaves: then `f` has a moment more, and is given up.
    async fn during<T>(&mut self, f: impl Future<Output = T>) -> Option<T> {
        tokio::pin!(f);
        loop {
            let until = self.until;
            let listens = until.is_none() && self.lines.waiting() < AHEAD;
            tokio::select! {
                biased;
                r = &mut f => return Some(r),
                n = self.rd.read(&mut self.buf), if listens => {
                    self.took(n);
                }
                () = sleep_until(until.unwrap_or_else(Instant::now)), if until.is_some() => {
                    return None;
                }
            }
        }
    }
}

/// Reads requests until the client ends, or does what ends it.
async fn requests(st: &Arc<State>, mut r: Reading, out: &Arc<Outbox>, stranger: Stranger) {
    // The place this connection takes: a stranger's, until it shows the token.
    let mut stranger = Some(stranger);
    let mut _place = None;
    // Keeps what is followed open.
    let mut following: HashMap<SessionId, Session> = HashMap::new();
    let deadline = Instant::now() + st.cfg.hello_within;
    loop {
        let more = match &mut stranger {
            None => r.more().await,
            Some(s) => tokio::select! {
                // What it has said counts before how long it took.
                biased;
                m = r.more() => m,
                _ = &mut s.leave => return,
                () = sleep_until(deadline) => return,
            },
        };
        if !more {
            return;
        }
        loop {
            // Answers are bounded by not asking for more of them.
            if r.during(out.room()).await != Some(true) {
                return;
            }
            let line = match r.lines.next_line() {
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
                    if stranger.is_none() {
                        continue;
                    }
                    return;
                }
            };
            match (stranger.is_none(), req.call) {
                (false, call) => match hello(st, call) {
                    Ok(client) => {
                        _place = Some(client);
                        stranger = None;
                        r.lines.set_limit(MAX_LINE);
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
                    match r.during(follow(st, &session, after)).await {
                        Some(Ok((s, page))) => {
                            // The answer comes before the first event.
                            out.answer(req.id, Ok(json!({})));
                            tokio::spawn(feed(st.clone(), s.clone(), f, after, page));
                            following.insert(session, s);
                        }
                        Some(Err(e)) => {
                            out.unfollow(&session);
                            out.answer(req.id, Err(e));
                        }
                        None => return,
                    }
                }
                (true, Call::Unfollow { session }) => {
                    out.unfollow(&session);
                    following.remove(&session);
                    out.answer(req.id, Ok(json!({})));
                }
                (true, call) => match r.during(handle(st, call)).await {
                    Some(answer) => out.answer(req.id, answer),
                    None => return,
                },
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
    st: &Arc<State>,
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

async fn blocking<T: Send + 'static>(
    f: impl FnOnce() -> T + Send + 'static,
) -> Result<T, WireError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| internal(e.to_string()))
}

/// The folder a session is about.
async fn workspace(st: &Arc<State>, id: &SessionId) -> Result<String, WireError> {
    let s = st.store.session(id.clone()).await.map_err(|e| internal(e.to_string()))?;
    let s = s.ok_or_else(|| WireError::new(ErrorCode::NotFound, "no such session"))?;
    s.workspace.ok_or_else(|| agents::bad("the session has no folder to work in"))
}

async fn stored(st: &Arc<State>, id: &SessionId) -> Result<Vec<(AgentId, String)>, WireError> {
    st.store.agents(id.clone()).await.map_err(|e| internal(e.to_string()))
}

async fn setup(st: &Arc<State>, id: &SessionId, agent: &AgentId) -> Result<Setup, WireError> {
    let all = stored(st, id).await?;
    let (_, text) = all
        .into_iter()
        .find(|(a, _)| a == agent)
        .ok_or_else(|| WireError::new(ErrorCode::NotFound, "no such agent in this session"))?;
    serde_json::from_str(&text).map_err(|e| internal(e.to_string()))
}

async fn keep(
    st: &Arc<State>,
    id: &SessionId,
    agent: &AgentId,
    s: &Setup,
) -> Result<(), WireError> {
    let text = serde_json::to_string(s).map_err(|e| internal(e.to_string()))?;
    st.store.put_agent(id.clone(), agent.clone(), text).await.map_err(|e| internal(e.to_string()))
}

/// Says it, for good.
async fn said(st: &Arc<State>, id: &SessionId, all: Vec<Say>) -> Result<(), WireError> {
    let s = st.open(id).await?;
    s.say(all).await.map_err(|e| agents::bad(e.to_string()))?;
    s.sync().await.map_err(|_| st.closed(id))?;
    Ok(())
}

async fn prompt(
    st: &Arc<State>,
    session: SessionId,
    agent: AgentId,
    text: String,
) -> Result<Value, WireError> {
    let setup = setup(st, &session, &agent).await?;
    let slot = st.agents.slot(&session, &agent);
    let turnstile = match slot.speak().await? {
        Spoken::Heard(ear) => {
            // It is in the journal before it is said to be heard.
            said(st, &session, axiom_engine::said(&ear.run, &agent, &text)).await?;
            let run = ear.tell(text);
            return Ok(json!({ "run": run, "queued": true }));
        }
        Spoken::Start(t) => t,
    };
    let s = st.open(&session).await?;
    let run = RunId::new();
    let (live, cancel) = Live::new(st, &session, &run)?;
    // By itself: a client that leaves now does not leave a run that never ends.
    let (told, begun) = oneshot::channel();
    let (st, r) = (st.clone(), run.clone());
    tokio::spawn(async move {
        let begin = async {
            let (tx, rx) = tokio::sync::mpsc::channel(1);
            drop(tx);
            let mut a = agents::load(&st.agents, &st.store, s, &agent, &setup, rx).await?;
            a.begin(&r, text).await.map_err(|e| match e {
                axiom_engine::Refused::Closed => st.closed(&session),
                axiom_engine::Refused::This(why) => internal(why),
            })?;
            a.inbox = turnstile.enter(&r);
            Ok(a)
        };
        let mut a = match begin.await {
            Ok(a) => a,
            Err(e) => {
                let _ = told.send(Err(e));
                return;
            }
        };
        let _ = told.send(Ok(()));
        let mut lives = vec![live];
        let next = || {
            let run = RunId::new();
            let (live, cancel) = Live::new(&st, &session, &run)?;
            lives.clear();
            lives.push(live);
            Ok((run, cancel))
        };
        agents::work(&mut a, &slot, (r, cancel), next).await;
        // Its session first: what it said last is in the journal.
        drop(a);
    });
    begun.await.map_err(|_| internal("the run did not start"))??;
    Ok(json!({ "run": run, "queued": false }))
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
            let (run, agent) = (RunId::new(), AgentId::new());
            let (live, cancel) = Live::new(st, &session, &run)?;
            let script = replay::load(path, st.scripts.clone()).await?;
            // By itself: a client that leaves now does not leave a run that never ends.
            let (told, begun) = oneshot::channel();
            let (st, r, a) = (st.clone(), run.clone(), agent.clone());
            tokio::spawn(async move {
                // Answered when it is in the journal for good that the run started.
                match replay::start(s, script, speed, r, a).await {
                    Ok(p) => {
                        let _ = told.send(Ok(()));
                        // Refused, the session has no more to say; the journal has what it said.
                        let _ = p.play(cancel).await;
                    }
                    Err(e) => {
                        let _ = told.send(Err(match e {
                            Refused::Closed(_) => st.closed(&session),
                            Refused::TooLong(e) => {
                                WireError::new(ErrorCode::BadRequest, e.to_string())
                            }
                        }));
                    }
                }
                drop(live);
            });
            begun.await.map_err(|_| internal("the run did not start"))??;
            Ok(json!({ "run": run, "agent": agent }))
        }
        Call::AgentAdd { session, name, execution, access } => {
            let workspace = workspace(st, &session).await?;
            if agents::full(stored(st, &session).await?.len()) {
                return Err(agents::bad("the session has as many agents as it may"));
            }
            let a = st.agents.clone();
            let setup = blocking(move || a.setup(name, execution, access, &workspace)).await??;
            let agent = AgentId::new();
            keep(st, &session, &agent, &setup).await?;
            said(st, &session, agents::added(&agent, &setup)).await?;
            Ok(json!({ "agent": agent }))
        }
        Call::AgentSet { session, agent, execution, access } => {
            let mut setup = setup(st, &session, &agent).await?;
            if st.agents.slot(&session, &agent).busy().await {
                return Err(WireError::new(ErrorCode::Busy, "the agent is at work"));
            }
            setup.execution = execution.unwrap_or(setup.execution);
            setup.access = access.unwrap_or(setup.access);
            let a = st.agents.clone();
            let setup = blocking(move || {
                a.setup(setup.name, setup.execution, setup.access, &setup.workspace)
            })
            .await??;
            keep(st, &session, &agent, &setup).await?;
            // What was allowed was allowed to it as it was.
            st.agents.desk(&session).forget();
            said(st, &session, agents::changed(&agent, &setup)).await?;
            Ok(json!({}))
        }
        Call::Prompt { session, agent, text } => {
            if text.trim().is_empty() || limits::text_len(&text) > limits::TEXT {
                return Err(agents::bad("what is said has between 1 byte and 1 MiB"));
            }
            prompt(st, session, agent, text).await
        }
        Call::Answer { session, request, answer, digest } => {
            st.agents.answer(&session, &request, answer, &digest).map(|()| json!({}))
        }
        Call::Connections {} => {
            serde_json::to_value(st.agents.connections.list()).map_err(|e| internal(e.to_string()))
        }
        Call::ConnectionAdd { name, provider, url, secret } => {
            let a = st.agents.clone();
            let id = blocking(move || a.connections.add(name, provider, url, secret.map(|s| s.0)))
                .await??;
            Ok(json!({ "connection": id }))
        }
        Call::ConnectionRemove { connection } => {
            let a = st.agents.clone();
            blocking(move || a.connections.remove(&connection)).await??;
            Ok(json!({}))
        }
        Call::Models { connection } => {
            let a = st.agents.clone();
            let client = blocking(move || a.connections.client(&connection)).await??;
            let all = client.list_models().await.map_err(|e| agents::bad(e.to_string()))?;
            let quirks = |id: &str| client.profile().quirks(id);
            let all: Vec<_> = all
                .into_iter()
                .map(|m| {
                    let mut id = m.id;
                    limits::clip(&mut id, limits::TITLE);
                    axiom_proto::ModelInfo { context: quirks(&id).context, name: None, id }
                })
                .collect();
            serde_json::to_value(all).map_err(|e| internal(e.to_string()))
        }
        Call::Audit { after, limit } => {
            let all =
                st.store.audit(after, limit.min(500)).await.map_err(|e| internal(e.to_string()))?;
            let mut size = 0;
            let all: Vec<Value> = all
                .iter()
                .take_while(|t| {
                    size += t.len();
                    size < 4 * 1024 * 1024
                })
                .map(|t| serde_json::from_str(t))
                .collect::<Result<_, _>>()
                .map_err(|e| internal(e.to_string()))?;
            Ok(Value::Array(all))
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{paths::Paths, session::Say};

    fn host(d: &tempfile::TempDir) -> (Arc<State>, Host) {
        let h = Host::start(Config::new(Paths::new(d.path().join("d")))).unwrap();
        (h.st.clone(), h)
    }
    async fn session(st: &Arc<State>) -> SessionId {
        let id = SessionId::new();
        st.store.create(id.clone(), "t".into(), None, 1).await.unwrap();
        id
    }
    fn started(item: &str) -> Say {
        Say::Event(Body::ItemStarted {
            item: ItemId(item.into()),
            run: RunId("r".into()),
            agent: AgentId("a".into()),
            kind: ItemKind::Prose,
            name: String::new(),
            args: None,
            args_cut: 0,
        })
    }

    #[tokio::test]
    async fn a_session_is_not_opened_again_before_it_is_closed() {
        let d = tempfile::tempdir().unwrap();
        let (st, _host) = host(&d);
        let id = session(&st).await;
        let s = st.open(&id).await.unwrap();
        s.say(vec![started("p")]).await.unwrap();
        s.sync().await.unwrap();
        // Its last text waits behind a lock when nobody holds it any more.
        let lock = rusqlite::Connection::open(st.cfg.paths.journal()).unwrap();
        lock.execute_batch("begin immediate").unwrap();
        s.say(vec![Say::Text(ItemId("p".into()), "last".into())]).await.unwrap();
        drop(s);
        let again = st.open(&id);
        tokio::pin!(again);
        assert!(timeout(Duration::from_millis(150), &mut again).await.is_err());
        lock.execute_batch("rollback").unwrap();
        let s = timeout(Duration::from_secs(5), again).await.unwrap().unwrap();
        s.say(vec![started("q")]).await.unwrap();
        s.sync().await.unwrap();
        let history = st.store.read(id.clone(), 0).await.unwrap();
        let ended = history.iter().find_map(|e| match &e.body {
            Body::ItemEnded { text, .. } => Some(text.as_str()),
            _ => None,
        });
        assert_eq!(ended, Some("last"), "nothing said is lost between the two");
    }

    #[tokio::test]
    async fn an_open_that_nobody_waits_for_ends_before_the_next_begins() {
        let d = tempfile::tempdir().unwrap();
        let (st, _host) = host(&d);
        let id = session(&st).await;
        let s = st.open(&id).await.unwrap();
        s.say(vec![started("p")]).await.unwrap();
        s.sync().await.unwrap();
        let lock = rusqlite::Connection::open(st.cfg.paths.journal()).unwrap();
        lock.execute_batch("begin immediate").unwrap();
        s.say(vec![Say::Text(ItemId("p".into()), "last".into())]).await.unwrap();
        drop(s);
        // Both the end of the one before and what recovery writes are behind the lock.
        for _ in 0..3 {
            assert!(timeout(Duration::from_millis(80), st.open(&id)).await.is_err());
        }
        lock.execute_batch("rollback").unwrap();
        let s = timeout(Duration::from_secs(5), st.open(&id)).await.unwrap().unwrap();
        s.say(vec![started("q")]).await.unwrap();
        s.sync().await.unwrap();
        let history = st.store.read(id.clone(), 0).await.unwrap();
        let ends = history.iter().filter(|e| matches!(e.body, Body::ItemEnded { .. })).count();
        assert_eq!(ends, 1, "{history:?}");
    }

    #[tokio::test]
    async fn one_session_is_open_once_however_many_ask_and_is_forgotten_after() {
        let d = tempfile::tempdir().unwrap();
        let (st, _host) = host(&d);
        let id = session(&st).await;
        let (a, b, c) = tokio::join!(st.open(&id), st.open(&id), st.open(&id));
        for s in [a.unwrap(), b.unwrap(), c.unwrap()] {
            // Two writers would both give the next number.
            s.say(vec![started(&ItemId::new().0)]).await.unwrap();
            s.sync().await.unwrap();
        }
        for _ in 0..20 {
            let other = session(&st).await;
            st.open(&other).await.unwrap();
            let t = Instant::now();
            while !st.sessions()[&other]
                .try_lock()
                .is_ok_and(|s| s.as_ref().is_none_or(Opened::ended))
            {
                assert!(t.elapsed() < Duration::from_secs(5));
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }
        assert!(st.open(&SessionId::new()).await.is_err());
        st.open(&id).await.unwrap();
        assert!(st.sessions().len() <= 2, "{}", st.sessions().len());
    }

    #[cfg(unix)]
    #[test]
    fn too_few_files_is_told_from_what_else_fails() {
        for (code, few) in [(libc::EMFILE, true), (libc::ENFILE, true), (libc::ENOSPC, false)] {
            assert_eq!(too_many_files(&io::Error::from_raw_os_error(code)), few);
        }
        assert!(!too_many_files(&io::Error::other("x")));
    }

    #[tokio::test]
    async fn the_oldest_stranger_goes() {
        let all = Arc::new(Strangers::default());
        let mut here: Vec<_> = (0..STRANGERS).map(|_| all.admit()).collect();
        let gone =
            |s: &mut Stranger| s.leave.try_recv() == Err(oneshot::error::TryRecvError::Closed);
        assert!(!here.iter_mut().any(gone));
        // One that is known by now has made room.
        drop(here.remove(0));
        here.push(all.admit());
        assert!(!here.iter_mut().any(gone));
        here.push(all.admit());
        assert!(gone(&mut here[0]) && !here[1..].iter_mut().any(gone));
        assert_eq!(all.list().len(), STRANGERS);
        drop(here);
        assert!(all.list().is_empty());
    }

    #[tokio::test]
    async fn a_connection_that_panics_leaves_nothing_behind() {
        let d = tempfile::tempdir().unwrap();
        let (st, _host) = host(&d);
        let out = Arc::new(Outbox::new(1000));
        let (here, o) = (Here::new(&st), out.clone());
        let (held, released) = oneshot::channel::<()>();
        let task = tokio::spawn(async move {
            // As the writer holds the socket.
            let task = tokio::spawn(async move {
                let _held = held;
                std::future::pending().await
            });
            let _writer = Writer { out: o, task };
            let _here = here;
            tokio::task::yield_now().await;
            panic!("as a defect would");
        });
        assert!(task.await.is_err());
        assert_eq!(st.connections.load(Relaxed), 0);
        assert!(timeout(Duration::from_secs(5), released).await.unwrap().is_err(), "let go");
        assert!(!out.room().await, "closed");
    }
}
