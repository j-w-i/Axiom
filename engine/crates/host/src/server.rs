use std::{
    collections::{HashMap, VecDeque},
    io,
    path::PathBuf,
    pin::Pin,
    sync::{
        Arc, Mutex, MutexGuard, OnceLock, PoisonError,
        atomic::{AtomicU64, Ordering::Relaxed},
    },
    time::Duration,
};

use axiom_engine::Deed;
use axiom_journal::{Journal, Reader};
use axiom_model::chatgpt::Places;
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
    agents::{self, Agents, Apart, Finished, Holder, Setup, Spoken, Switch, Told},
    catalogue, changes,
    connections::{Connections, MemoryVault, Vault},
    copies::{self, Then},
    faults::{Access, Faults},
    files, log,
    net::{self, Boxed, Listener},
    outbox::{Follower, Outbox},
    paths::{Lock, Paths, Same, private_dir, private_file},
    peers, planning,
    programs::{Programs, Said},
    replay,
    session::{Joined, Opened, Refused, Say, Session, encode, lines},
    signin::Signins,
    store::Store,
    waiting::{self, Waiting},
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
    /// Where harnesses are looked for; `None`: where programs usually are.
    pub programs: Option<Vec<PathBuf>>,
    /// The host's own program, if installed programs are to have tools of Axiom's.
    pub bridge: Option<PathBuf>,
    pub vault: Arc<dyn Vault>,
    /// Where a ChatGPT plan is signed in to and used.
    pub chatgpt: Places,
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
            programs: None,
            bridge: None,
            vault: Arc::new(MemoryVault::default()),
            chatgpt: Places::default(),
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
pub(crate) const CLIENTS: usize = 64;
/// All of them, with those that are being closed. More wait to be accepted.
const CONNECTIONS: usize = 128;
/// Runs at once.
pub(crate) const RUNS: usize = 128;
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
    waiting: Arc<Waiting>,
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
    /// The folder a person asked for last in a terminal.
    open: watch::Sender<String>,
    agents: Arc<Agents>,
    signins: Arc<Signins>,
    folders: crate::folders::Folders,
}

type Slot = tokio::sync::Mutex<Option<Opened>>;

#[derive(Default)]
struct Runs {
    closing: bool,
    live: HashMap<RunId, (SessionId, watch::Sender<bool>)>,
    /// What else is not to be ended in the middle.
    at_work: usize,
}
impl Runs {
    fn quiet(&self) -> bool {
        self.live.is_empty() && self.at_work == 0
    }
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
        if !force && !runs.quiet() {
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
                let (s, opened) = Session::open(
                    id.clone(),
                    seq,
                    self.store.clone(),
                    self.faults.clone(),
                    self.waiting.clone(),
                );
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
        let waiting = Arc::new(Waiting::default());
        waiting.load(axiom_journal::Reader::open(&p.journal())?.unseen()?);
        let listener = Listener::bind(p)?;
        private_file(&p.endpoint_file(), listener.path())?;
        private_file(&p.token(), &token)?;
        let file = p.data.join("connections.json");
        let connections = Connections::open(&file, cfg.vault.clone(), cfg.chatgpt.clone())?;
        let agents = Agents::new(
            Arc::new(connections),
            p,
            cfg.home.clone(),
            cfg.scratch.clone(),
            Programs::open(&p.data.join("programs.json"), cfg.vault.clone(), cfg.programs.clone())?,
            cfg.bridge.clone(),
            listener.path(),
        );
        let st = Arc::new(State {
            agents: Arc::new(agents),
            signins: Arc::default(),
            store: Store::start(journal, &p.journal())?,
            token,
            faults: Arc::new(faults),
            waiting,
            damaged: OnceLock::new(),
            sessions: Mutex::default(),
            runs: Mutex::default(),
            connections: AtomicU64::new(0),
            clients: Arc::new(Semaphore::new(CLIENTS)),
            scripts: Arc::new(Semaphore::new(replay::HELD)),
            stop: Notify::new(),
            changed: Notify::new(),
            over: watch::Sender::new(false),
            open: watch::Sender::new(String::new()),
            folders: crate::folders::Folders::open(&p.data.join("folders.json"))?,
            cfg,
        });
        let switch: std::sync::Weak<dyn Switch> = Arc::downgrade(&st) as _;
        let _ = st.agents.switch.set(switch);
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
                if st.runs().quiet() {
                    break;
                }
                changed.await;
            }
        })
        .await;
        // What runs between runs: it ends, and lets its session go.
        st.signins.end();
        st.agents.end().await;
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
        let quiet = st.connections.load(Relaxed) == 0 && st.runs().quiet();
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
    // An installed program's: nothing is opened for it but its own tool, and that ends with it.
    let mut peer: Option<String> = None;
    let mut asked = JoinSet::new();
    // Keeps what is followed open.
    let mut following: HashMap<SessionId, Session> = HashMap::new();
    let many = Arc::new(Semaphore::new(SLOW));
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
                (false, Call::PeerHello { grant }) => {
                    let place = st.clients.clone().try_acquire_owned();
                    match (st.agents.granted(&grant), place) {
                        (Some(_), Ok(place)) => {
                            _place = Some(place);
                            peer = Some(grant);
                            stranger = None;
                            out.answer(req.id, Ok(json!({ "tools": tools_of() })));
                        }
                        (None, _) => {
                            let e = WireError::new(ErrorCode::Unauthorised, "wrong key");
                            out.answer(req.id, Err(e));
                            return;
                        }
                        (_, Err(_)) => {
                            let e = WireError::new(ErrorCode::Busy, "too many clients");
                            out.answer(req.id, Err(e));
                            return;
                        }
                    }
                }
                (true, call) if peer.is_some() => {
                    while asked.try_join_next().is_some() {}
                    // Asked for each time: the key may be gone by now.
                    let who = peer.as_deref().and_then(|k| st.agents.granted(k));
                    let Some(who) = who else {
                        let e =
                            WireError::new(ErrorCode::Unauthorised, "the key is not good any more");
                        out.answer(req.id, Err(e));
                        return;
                    };
                    let Call::Peer { tool, args } = call else {
                        let words = "this key opens nothing else";
                        out.answer(req.id, Err(WireError::new(ErrorCode::Unauthorised, words)));
                        continue;
                    };
                    if asked.len() >= SLOW {
                        let words = "too much is asked at once";
                        out.answer(req.id, Err(WireError::new(ErrorCode::Busy, words)));
                        continue;
                    }
                    let (st, out) = (st.clone(), out.clone());
                    asked.spawn(async move {
                        out.answer(req.id, by_peer(&st, who, tool, args).await);
                    });
                }
                (false, call) => match hello(st, call) {
                    Ok((client, window)) => {
                        _place = Some(client);
                        stranger = None;
                        r.lines.set_limit(MAX_LINE);
                        let version = env!("CARGO_PKG_VERSION");
                        out.answer(req.id, Ok(json!({ "protocol": PROTOCOL, "version": version })));
                        tokio::spawn(tells(st.waiting.watch(), out.clone()));
                        if window {
                            tokio::spawn(opens(st.open.subscribe(), out.clone()));
                        }
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
                (true, call) if slow(&call) || copies_first(st, &call).await => {
                    match many.clone().try_acquire_owned() {
                        Ok(place) => {
                            let (st, out) = (st.clone(), out.clone());
                            tokio::spawn(async move {
                                out.answer(req.id, handle(&st, call).await);
                                drop(place);
                            });
                        }
                        Err(_) => out.answer(
                            req.id,
                            Err(WireError::new(ErrorCode::Busy, "too much is asked at once")),
                        ),
                    }
                }
                (true, call) => match r.during(handle(st, call)).await {
                    Some(answer) => out.answer(req.id, answer),
                    None => return,
                },
            }
        }
    }
}

/// What waits for a person is something else now: told as soon as there is room, the last word of it.
async fn tells(mut waiting: watch::Receiver<usize>, out: Arc<Outbox>) {
    loop {
        tokio::select! {
            c = waiting.changed() => if c.is_err() || !out.room().await {
                return;
            },
            () = out.closed() => return,
        }
        let n = *waiting.borrow_and_update();
        if !out.tell(&Message::Waiting { waiting: n }) {
            return;
        }
    }
}

/// A folder is to be shown: the last that was asked for.
async fn opens(mut open: watch::Receiver<String>, out: Arc<Outbox>) {
    loop {
        tokio::select! {
            c = open.changed() => if c.is_err() || !out.room().await {
                return;
            },
            () = out.closed() => return,
        }
        let path = open.borrow_and_update().clone();
        if !out.tell(&Message::Open { open: path }) {
            return;
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
/// A place, and whether the client has a window.
fn hello(st: &State, call: Call) -> Result<(OwnedSemaphorePermit, bool), WireError> {
    let Call::Hello { token, protocol, client } = call else {
        return Err(WireError::new(ErrorCode::Unauthorised, "say hello first"));
    };
    if !same(&token, &st.token) {
        return Err(WireError::new(ErrorCode::Unauthorised, "wrong token"));
    }
    if protocol != PROTOCOL {
        let m = format!("this host speaks protocol {PROTOCOL}, the client {protocol}");
        return Err(WireError::new(ErrorCode::Protocol, m));
    }
    let place = st.clients.clone().try_acquire_owned();
    let place = place.map_err(|_| WireError::new(ErrorCode::Busy, "too many clients"))?;
    Ok((place, client.starts_with(axiom_proto::WINDOW)))
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

/// Done to its end, also when who asked is gone.
async fn to_its_end<F>(
    st: &Arc<State>,
    f: impl FnOnce(Arc<State>) -> F + Send + 'static,
) -> Result<Value, WireError>
where
    F: Future<Output = Result<Value, WireError>> + Send + 'static,
{
    let at_work = AtWork::new(st)?;
    let work = f(st.clone());
    let work = tokio::spawn(async move {
        let r = work.await;
        drop(at_work);
        r
    });
    work.await.map_err(|e| internal(e.to_string()))?
}

/// Something the host does not end in the middle of.
struct AtWork(Arc<State>);
impl AtWork {
    fn new(st: &Arc<State>) -> Result<Self, WireError> {
        let mut runs = st.runs();
        if runs.closing {
            return Err(WireError::new(ErrorCode::Busy, "the host is stopping"));
        }
        runs.at_work += 1;
        drop(runs);
        Ok(Self(st.clone()))
    }
}
impl Drop for AtWork {
    fn drop(&mut self) {
        self.0.runs().at_work -= 1;
        self.0.changed.notify_waiters();
    }
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

/// How long a provider may take to list its models.
const LISTING: Duration = Duration::from_secs(20);
/// Slow calls of one client at a time.
const SLOW: usize = 4;

/// What waits for a network, a drive or a system's store, and that nothing else waits for.
fn slow(c: &Call) -> bool {
    matches!(
        c,
        Call::Models { .. }
            | Call::Look { .. }
            | Call::Harnesses {}
            | Call::ProgramSet { .. }
            | Call::ProgramRemove { .. }
            | Call::PlanAccept { .. }
            | Call::Changes { .. }
            | Call::CopyChanges { .. }
            | Call::CopyBring { .. }
            | Call::CopyBack { .. }
            | Call::CopyRenew { .. }
            | Call::CopyRemove { .. }
            | Call::BranchLook { .. }
            | Call::Branch { .. }
            | Call::ConnectionAdd { .. }
            | Call::ConnectionRemove { .. }
            | Call::FileRead { .. }
            | Call::FileList { .. }
            | Call::FileFind { .. }
            | Call::FileSearch { .. }
            | Call::FileOpen { .. }
            | Call::FileShow { .. }
            | Call::Editors {}
    )
}

/// The folder a person looks at: the session's, or the copy an agent works in.
async fn looked_at(
    st: &Arc<State>,
    session: &SessionId,
    agent: Option<&AgentId>,
) -> Result<files::Here, WireError> {
    let mut folder = workspace(st, session).await?;
    if let Some(a) = agent {
        let s = setup(st, session, a).await?;
        if s.apart.as_ref().is_some_and(|a| a.made) {
            folder = s.workspace;
        }
    }
    let a = st.agents.clone();
    let id = AgentId(format!("person_{}", session.0));
    let site = tokio::task::spawn_blocking(move || a.looking(&folder, &id))
        .await
        .map_err(|e| internal(e.to_string()))??;
    Ok(files::Here { site })
}

/// Whether a process of a tree has lost its parent: it is not the first, and its parent is none of
/// the tree.
fn left(pid: u32, parent: u32, root: Option<u32>, pids: &[u32]) -> bool {
    Some(pid) != root && !pids.contains(&parent)
}

/// What each process of a tree uses. One whose parent is not in the tree and is not who started it
/// has lost its parent.
fn processes(pids: &[u32]) -> Vec<ProcessInfo> {
    #[cfg(unix)]
    {
        let all: Vec<axiom_spawn::Usage> =
            pids.iter().filter_map(|p| axiom_spawn::usage(*p)).collect();
        let root = pids.first().copied();
        all.iter()
            .map(|u| ProcessInfo {
                pid: u.pid,
                parent: u.parent,
                name: u.name.clone(),
                cpu_ms: u.cpu_ms,
                memory: u.memory,
                orphan: left(u.pid, u.parent, root, pids),
            })
            .collect()
    }
    #[cfg(not(unix))]
    {
        let _ = pids;
        Vec::new()
    }
}

/// Done off the runtime's threads.
async fn off<T: Send + 'static>(
    f: impl FnOnce() -> Result<T, WireError> + Send + 'static,
) -> Result<T, WireError> {
    tokio::task::spawn_blocking(f).await.map_err(|e| internal(e.to_string()))?
}

fn value(v: impl serde::Serialize) -> Result<Value, WireError> {
    serde_json::to_value(v).map_err(|e| internal(e.to_string()))
}

/// Puts on record what a person had the host do for them (SI-23), before it is done.
async fn person(
    st: &Arc<State>,
    session: &SessionId,
    folder: &str,
    tool: &str,
    subject: String,
) -> Result<(), WireError> {
    let deed = Deed {
        chain: vec![],
        workspace: folder.to_owned(),
        tool: tool.into(),
        subject,
        decision: axiom_policy::Decision::Allow,
        rules: vec!["a person".into()],
        answered: None,
    };
    st.agents
        .record(&st.store, session, deed, axiom_policy::audit::Containment::None)
        .await
        .map_err(|e| internal(e.to_string()))
}

/// What is said to an agent whose copy of the folder is made, or looked through, first.
async fn copies_first(st: &Arc<State>, c: &Call) -> bool {
    let Call::Prompt { session, agent, .. } = c else { return false };
    let s = setup(st, session, agent).await;
    s.is_ok_and(|s| s.apart.is_some())
}

/// Most bytes of a client's own name for a prompt.
const KEY: usize = 100;

async fn prompt(
    st: &Arc<State>,
    session: SessionId,
    agent: AgentId,
    text: String,
    key: Option<String>,
    told: Option<Told>,
) -> Result<Value, WireError> {
    let slot = st.agents.slot(&session, &agent);
    // As it is told, and as it is shown: by whom.
    let (by, shown) = match &told {
        Some(t) => (t.by.clone(), Some((t.by.clone(), t.text.clone()))),
        None => (String::new(), None),
    };
    let spoken = slot.speak().await?;
    // Looked for while nobody else can speak: said again, it is not heard again.
    if let Some(v) = key.as_deref().and_then(|k| slot.answered(k)) {
        return Ok(v);
    }
    let turnstile = match spoken {
        Spoken::Heard(ear) => {
            // In the journal, and waiting to be heard, before it is said to be.
            let item = ItemId::new();
            let show = shown.as_ref().map_or(&text, |s| &s.1);
            said(st, &session, axiom_engine::said(&item, &ear.run, &agent, show, &by)).await?;
            st.store
                .put_pending(session, agent, item.clone(), text.clone())
                .await
                .map_err(|e| internal(e.to_string()))?;
            let answer = json!({ "run": ear.tell(text, item), "queued": true });
            if let Some(k) = key {
                slot.remember(k, answer.clone());
            }
            return Ok(answer);
        }
        Spoken::Start(t) => t,
    };
    if told.is_none() {
        slot.wakes.store(0, Relaxed);
    }
    let chain = told.map(|t| t.chain).unwrap_or_default();
    let word = axiom_engine::Word { text, item: None, shown };
    let run = start(st, session, agent, turnstile, Opening::Said { word, key, chain }).await?;
    Ok(json!({ "run": run, "queued": false }))
}

/// Where a conversation is to be branched.
struct Place {
    setup: Setup,
    /// How many turns a branch has, and whether what it is branched at is in them.
    n: u64,
    with: bool,
    /// Whose files those turns were about, and between which of its turns they are the same.
    files: copies::Then,
    /// What they were is kept.
    kept: bool,
    /// They are what they are now.
    now: bool,
}

async fn place(
    st: &Arc<State>,
    session: &SessionId,
    agent: &AgentId,
    item: &ItemId,
) -> Result<Place, WireError> {
    let no = |e: axiom_journal::Error| internal(e.to_string());
    let of = setup(st, session, agent).await?;
    if of.execution.harness != "axiom" {
        return Err(agents::bad(
            "its program keeps its own conversation, and cannot go back in it",
        ));
    }
    let n = st.store.cut(session.clone(), agent.clone(), item.clone()).await.map_err(no)?;
    let (n, with) = n.ok_or_else(|| agents::bad("the conversation cannot be branched here"))?;
    // Whose turns they are: a branch's first ones are the other's, and so are their files.
    let (mut who, mut whose) = (agent.clone(), of.clone());
    let mut kept = false;
    let mut at;
    loop {
        let all = st.store.messages(session.clone(), who.clone()).await.map_err(no)?;
        at = (axiom_engine::context::since(&all, n).map_err(|e| internal(e.to_string()))?, n);
        let total = all.len() as u64;
        if whose.apart.is_some() {
            kept = copies::has_point(&st.agents, session, &who, &whose, at).await?;
        }
        let now = n == total && whose.apart.as_ref().is_some_and(|a| a.made);
        match whose.from.clone().filter(|f| !kept && !now && n <= f.turns) {
            Some(f) => {
                whose = setup(st, session, &f.agent).await?;
                who = f.agent;
            }
            None => {
                let files = copies::Then { agent: who, setup: whose, at };
                return Ok(Place {
                    setup: of,
                    n,
                    with,
                    files,
                    kept: kept || now,
                    now: now && !kept,
                });
            }
        }
    }
}

async fn branch(
    st: Arc<State>,
    session: SessionId,
    agent: AgentId,
    item: ItemId,
    name: String,
    files: bool,
) -> Result<Value, WireError> {
    let p = place(&st, &session, &agent, &item).await?;
    if agents::full(stored(&st, &session).await?.len()) {
        return Err(agents::bad("the session has as many agents as it may"));
    }
    if files && !p.kept {
        return Err(agents::bad("what its files were there is not kept"));
    }
    let a = st.agents.clone();
    let written = a.written(&st.store).await?;
    let mut new = p.setup.clone();
    new.name = name;
    // What was handed in is the other's.
    new.planned = None;
    new.from = Some(agents::Origin { agent: agent.clone(), turns: p.n });
    if let Some(apart) = &mut new.apart {
        apart.made = false;
        new.workspace.clone_from(&apart.folder);
    }
    let mut new = blocking(move || a.setup(new, &written)).await??;
    let to = AgentId::new();
    let folder = new.folder().to_owned();
    if files {
        let slot = st.agents.slot(&session, &p.files.agent);
        // As it is now, if nothing of it is at work.
        let _held = if p.now { Some(slot.hold().await?) } else { None };
        if p.now {
            let Then { agent, setup, .. } = &p.files;
            if !copies::point(&st.agents, &session, agent, setup, p.n).await? {
                return Err(agents::bad("what its files are cannot be kept"));
            }
        }
        new.workspace = copies::again(&st.agents, &st.store, &session, p.files, &to).await?;
        if let Some(apart) = &mut new.apart {
            apart.made = true;
        }
    }
    let made = async {
        let text = serde_json::to_string(&new).map_err(|e| internal(e.to_string()))?;
        let kept = st.store.branch(session.clone(), agent.clone(), p.n, to.clone(), text).await;
        kept.map_err(|e| internal(e.to_string()))?;
        let from = Branched { agent, item, with: p.with };
        if let Err(e) = said(&st, &session, agents::added(&to, &new, Some(from))).await {
            // Nobody knows of it.
            let _ = st.store.remove_agent(session.clone(), to.clone()).await;
            return Err(e);
        }
        Ok(json!({ "agent": to }))
    };
    let r = made.await;
    if r.is_err() && files {
        copies::unmake(&st.agents, &session, &to, &folder).await;
    }
    r
}

/// Why a run begins.
enum Opening {
    /// Something is said to the agent: by a person, or by those in the chain.
    Said { word: axiom_engine::Word, key: Option<String>, chain: Vec<AgentId> },
    /// What others have come to is told to it.
    Told,
    /// Nothing is told to it: its conversation is summarised.
    Compact,
}

/// Runs of an agent that others' ends may begin, one after another, with no person in between.
pub(crate) const WAKES: u32 = 8;
/// Ends that come together are told together.
const TOGETHER: Duration = Duration::from_millis(300);

/// Tells an agent that is not at work what it is owed and has not asked for. One that is at work may
/// ask itself, and is looked at again when it ends.
fn deliver(
    st: Arc<State>,
    session: SessionId,
    agent: AgentId,
) -> Pin<Box<dyn Future<Output = ()> + Send>> {
    // Boxed: the run it may start may end in another.
    Box::pin(async move {
        tokio::time::sleep(TOGETHER).await;
        let slot = st.agents.slot(&session, &agent);
        if st.runs().closing || !peers::has_finished(&st.agents, &session, &slot) {
            return;
        }
        let Ok(Spoken::Start(turnstile)) = slot.speak().await else { return };
        // What went wrong would go wrong again, and whoever was stopped stays so: it is told when it
        // is spoken to the next time.
        let well = slot.last.borrow().as_ref().is_some_and(|l| l.outcome == Outcome::Completed);
        if !well || slot.wakes.fetch_add(1, Relaxed) >= WAKES {
            return;
        }
        let _ = start(&st, session, agent, turnstile, Opening::Told).await;
    })
}

/// How long after a run nothing of it is to have gone on, for a person to be told.
const SETTLED: Duration = Duration::from_secs(2);

/// Keeps, for its person, what an agent came to; if they were who began it, and it does not go on.
async fn unseen(st: Arc<State>, session: SessionId, agent: AgentId) {
    let slot = st.agents.slot(&session, &agent);
    let end = slot.last.borrow().clone();
    tokio::time::sleep(SETTLED).await;
    let Some(end) = end else { return };
    let same = slot.last.borrow().as_ref().is_some_and(|l| Arc::ptr_eq(l, &end));
    // Of one that others told, they are told; one that is owed something goes on by itself.
    let own = slot.chain.lock().unwrap_or_else(PoisonError::into_inner).len() <= 1;
    let owed = !slot.owed().is_empty() && end.outcome == Outcome::Completed;
    if !same || !own || owed || slot.busy().await {
        return;
    }
    let error = end.error.as_deref().unwrap_or_default();
    let (kind, words) = match end.outcome {
        Outcome::Completed => (waiting::FINISHED, waiting::begins(&end.text)),
        Outcome::Failed if error.starts_with("billing") => (waiting::LIMIT, waiting::begins(error)),
        Outcome::Failed => (waiting::FAILED, waiting::begins(error)),
        // A person did it, or the host is ending.
        _ => return,
    };
    let at = now_ms();
    let one = axiom_journal::Unseen {
        session: session.clone(),
        agent: agent.clone(),
        kind: kind.into(),
        words: words.clone(),
        at,
    };
    let (id, a) = (session.clone(), agent.clone());
    if st.store.put_unseen(id, a, kind, words, at).await.is_ok() {
        st.waiting.unseen(one);
        // It was spoken to this moment.
        if slot.busy().await && st.waiting.seen(&session, &agent) {
            let _ = st.store.drop_unseen(session, agent).await;
        }
    }
}

/// What an agent is owed and others have come to: at once no longer owed, and waiting to be heard.
/// How it is shown.
async fn collect(
    st: &Arc<State>,
    session: &SessionId,
    agent: &AgentId,
    slot: &agents::Slot,
) -> Result<Vec<(ItemId, String, String)>, WireError> {
    let live = |r: &RunId| st.runs().live.contains_key(r);
    slot.load_owed(&st.store, session, agent, live).await?;
    let done = peers::finished(&st.agents, session, slot);
    if done.is_empty() {
        return Ok(Vec::new());
    }
    let names = stored(st, session).await?;
    let name = |id: &AgentId| {
        let text = names.iter().find(|n| n.0 == *id).map(|n| n.1.as_str());
        let setup = text.and_then(|t| serde_json::from_str::<Setup>(t).ok());
        setup.map_or_else(|| id.0.clone(), |s| s.name)
    };
    let mut items = Vec::new();
    let mut shown = Vec::new();
    for (child, end, runs) in done {
        let (item, name) = (ItemId::new(), name(&child));
        let text = peers::report(&name, &end);
        items.push((runs, item.clone(), axiom_engine::from_agent(&name, &text)));
        shown.push((item, name, text));
    }
    let told = st.store.deliver(session.clone(), agent.clone(), items).await;
    let told = told.map_err(|e| internal(e.to_string()))?;
    shown.retain(|s| told.contains(&s.0));
    Ok(shown)
}

async fn start(
    st: &Arc<State>,
    session: SessionId,
    agent: AgentId,
    turnstile: Box<agents::Turnstile>,
    opening: Opening,
) -> Result<RunId, WireError> {
    let slot = st.agents.slot(&session, &agent);
    let setup = setup(st, &session, &agent).await?;
    let s = st.open(&session).await?;
    let run = RunId::new();
    let (live, cancel) = Live::new(st, &session, &run)?;
    let compact = matches!(opening, Opening::Compact);
    let (word, key) = match opening {
        Opening::Said { word, key, chain } => {
            let mut now = slot.chain.lock().unwrap_or_else(PoisonError::into_inner);
            *now = chain;
            now.push(agent.clone());
            (Some(word), key)
        }
        // As deep as it was when it told them.
        Opening::Told | Opening::Compact => {
            let mut now = slot.chain.lock().unwrap_or_else(PoisonError::into_inner);
            if now.is_empty() {
                now.push(agent.clone());
            }
            (None, None)
        }
    };
    let first = run.clone();
    if let Some(k) = &key {
        slot.remember(k.clone(), json!({ "run": run, "queued": false }));
    }
    // By itself: a client that leaves now does not leave a run that never ends.
    let (told, begun) = oneshot::channel();
    let st = st.clone();
    tokio::spawn(async move {
        let refused = |e| match e {
            axiom_engine::Refused::Closed => st.closed(&session),
            axiom_engine::Refused::This(why) => internal(why),
        };
        let mut setup = setup;
        let begin = async {
            // It is at work again: what it came to before is behind it.
            if st.waiting.seen(&session, &agent) {
                let _ = st.store.drop_unseen(session.clone(), agent.clone()).await;
            }
            let (a, c) = (&st.agents, cancel.clone());
            setup = copies::ready(a, &st.store, &session, &agent, setup.clone(), c).await?;
            // What its files are, for who goes back to before this.
            if setup.apart.is_some() && setup.execution.harness == "axiom" && !compact {
                let n = st.store.turns(session.clone(), agent.clone()).await;
                let n = n.map_err(|e| internal(e.to_string()))?;
                if let Err(e) = copies::point(&st.agents, &session, &agent, &setup, n).await {
                    log::note(format!("keeping what a copy is: {}", e.message));
                }
            }
            // What is taken in here is answered: nothing is while room is made.
            let owed =
                if compact { Vec::new() } else { collect(&st, &session, &agent, &slot).await? };
            if word.is_none() && owed.is_empty() && !compact {
                return Err(agents::bad("there is nothing to tell it"));
            }
            let rx = turnstile.enter(&run, &cancel);
            let mut a =
                agents::Worker::load(&st.agents, &st.store, s.clone(), &agent, &setup, rx).await?;
            if compact {
                a.compacts();
            }
            // In the order in which it is told.
            let owed = owed
                .into_iter()
                .flat_map(|(item, by, show)| axiom_engine::said(&item, &run, &agent, &show, &by));
            a.begin(&run, owed.collect(), word).await.map_err(refused)?;
            Ok(a)
        };
        let mut a = match begin.await {
            Ok(a) => a,
            Err(e) => {
                if let Some(k) = key {
                    slot.forget(&k);
                }
                slot.leave().await;
                let _ = told.send(Err(e));
                return;
            }
        };
        let _ = told.send(Ok(()));
        let (mut run, mut cancel, mut live) = (run, cancel, live);
        let mut runs = Vec::new();
        let (mut outcome, mut error, mut text);
        loop {
            runs.push(run.clone());
            let (mut inbox, e, last) = a.run(run, cancel.clone()).await;
            (outcome, error, text) = (e.outcome, e.error, last);
            let stopped = *cancel.borrow();
            if stopped {
                slot.stop_jobs();
                // Who stops an agent stops what it began (SI-24), and is owed nothing of it.
                let began = slot.forgive();
                let _ = st.store.drop_owed(session.clone(), agent.clone(), began.clone()).await;
                for r in began {
                    st.stop(&r);
                }
            }
            if !slot.after(&mut inbox, stopped).await {
                break;
            }
            // What was said too late for that run begins another, from what is kept: whatever
            // went wrong in memory is not carried over.
            run = RunId::new();
            let next = async {
                let (l, c) = Live::new(&st, &session, &run)?;
                (live, cancel) = (l, c);
                slot.renamed(&run, &cancel).await;
                agents::Worker::load(&st.agents, &st.store, s.clone(), &agent, &setup, inbox).await
            };
            a = match next.await {
                Ok(a) => a,
                Err(_) => {
                    slot.leave().await;
                    break;
                }
            };
            if a.begin(&run, Vec::new(), None).await.is_err() {
                slot.leave().await;
                break;
            }
        }
        // A person asked for it, and for nothing else: there is nothing to be told of.
        let only_compacted = compact && runs.len() == 1;
        let end = Finished { runs, outcome, error, text };
        // On the drive before anybody is told: who is owed it is told once, also by another host.
        let body = serde_json::to_string(&end).unwrap_or_default();
        let parents =
            st.store.end_owed(session.clone(), agent.clone(), end.runs.clone(), body).await;
        slot.finish(end);
        if !only_compacted {
            tokio::spawn(unseen(st.clone(), session.clone(), agent.clone()));
        }
        for p in parents.unwrap_or_default().into_iter().chain([agent]) {
            tokio::spawn(deliver(st.clone(), session.clone(), p));
        }
        // Its session before its place among what runs: what it said last is in the journal.
        drop(s);
        drop(live);
    });
    begun.await.map_err(|_| internal("the run did not start"))??;
    Ok(first)
}

impl Switch for State {
    fn tell(
        self: Arc<Self>,
        session: SessionId,
        agent: AgentId,
        told: Told,
    ) -> Pin<Box<dyn Future<Output = Result<RunId, WireError>> + Send>> {
        Box::pin(async move {
            let text = axiom_engine::from_agent(&told.by, &told.text);
            let v = prompt(&self, session, agent, text, None, Some(told)).await?;
            Ok(RunId(v["run"].as_str().unwrap_or_default().to_owned()))
        })
    }
    fn stop(&self, run: &RunId) {
        if let Some((_, c)) = self.runs().live.get(run) {
            let _ = c.send(true);
        }
    }
}

/// What a program is given of Axiom, as it is told: what is Axiom's to do, and nothing its own tools do
/// (decision 0017).
fn tools_of() -> Vec<Value> {
    let shown = |s: axiom_tools::Spec| json!({"name": s.name, "description": s.description, "schema": s.schema});
    peers::specs().into_iter().map(shown).collect()
}

/// What an installed program asks for.
async fn by_peer(
    st: &Arc<State>,
    who: Holder,
    tool: String,
    args: Value,
) -> Result<Value, WireError> {
    if tool != peers::TOOL {
        return Err(WireError::new(ErrorCode::Unauthorised, "this key opens nothing else"));
    }
    let Holder { session, agent } = who;
    let s = st.open(&session).await?;
    let floor =
        agents::Floor { agents: st.agents.clone(), store: st.store.clone(), session: s, agent };
    let o = peers::call(&floor, &st.agents.desk(&session), &args).await;
    Ok(json!({"ok": o.ok, "text": o.text}))
}

async fn handle(st: &Arc<State>, call: Call) -> Result<Value, WireError> {
    match call {
        Call::Hello { .. } | Call::PeerHello { .. } => {
            Err(WireError::new(ErrorCode::BadRequest, "already said hello"))
        }
        Call::Peer { .. } => Err(agents::bad("only an agent's own program may ask that")),
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
        Call::Limits {} => {
            serde_json::to_value(crate::limits::kept()).map_err(|e| internal(e.to_string()))
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
        Call::AgentAdd { session, name, execution, access, trust, instructions, apart, plan } => {
            let workspace = workspace(st, &session).await?;
            if agents::full(stored(st, &session).await?.len()) {
                return Err(agents::bad("the session has as many agents as it may"));
            }
            let a = st.agents.clone();
            let written = a.written(&st.store).await?;
            let setup = blocking(move || {
                let (resume, began) = (None, false);
                let apart = match apart {
                    true => {
                        let folder = agents::real(workspace.as_ref())?.to_string();
                        Some(Apart { folder, made: false })
                    }
                    false => None,
                };
                let s = Setup {
                    name,
                    execution,
                    access,
                    workspace,
                    instructions,
                    trust,
                    resume,
                    began,
                    apart,
                    from: None,
                    plan,
                    planned: None,
                };
                a.setup(s, &written)
            })
            .await??;
            let agent = AgentId::new();
            keep(st, &session, &agent, &setup).await?;
            if let Err(e) = said(st, &session, agents::added(&agent, &setup, None)).await {
                // Nobody knows of it.
                let _ = st.store.remove_agent(session, agent).await;
                return Err(e);
            }
            Ok(json!({ "agent": agent }))
        }
        Call::Changes { session } => {
            let folder = workspace(st, &session).await?;
            let c = changes::changes(&st.agents, &session, folder).await?;
            serde_json::to_value(c).map_err(|e| internal(e.to_string()))
        }
        Call::Seen { session, agent } => {
            if st.waiting.seen(&session, &agent) {
                st.store.drop_unseen(session, agent).await.map_err(|e| internal(e.to_string()))?;
            }
            Ok(json!({}))
        }
        Call::Waiting {} => {
            let mut all = st.waiting.list();
            let mut known: HashMap<SessionId, (String, Vec<(AgentId, String)>)> = HashMap::new();
            for w in &mut all {
                if !known.contains_key(&w.session) {
                    let s = st.store.session(w.session.clone()).await;
                    let title = s.ok().flatten().map(|s| s.title).unwrap_or_default();
                    let agents = stored(st, &w.session).await.unwrap_or_default();
                    known.insert(w.session.clone(), (title, agents));
                }
                if let Some((title, agents)) = known.get(&w.session) {
                    w.title.clone_from(title);
                    let setup = agents.iter().find(|a| a.0 == w.agent);
                    let setup = setup.and_then(|a| serde_json::from_str::<Setup>(&a.1).ok());
                    w.name = setup.map(|s| s.name).unwrap_or_default();
                }
            }
            serde_json::to_value(all).map_err(|e| internal(e.to_string()))
        }
        Call::CopyChanges { session, agent } => {
            let mut all = Vec::new();
            for (id, text) in stored(st, &session).await? {
                all.push((id, serde_json::from_str(&text).map_err(|e| internal(e.to_string()))?));
            }
            let c = copies::shown(&st.agents, session, agent, all).await?;
            serde_json::to_value(c).map_err(|e| internal(e.to_string()))
        }
        Call::CopyBring { session, agent, files } => {
            let s = setup(st, &session, &agent).await?;
            to_its_end(st, move |st| async move {
                copies::bring(&st.agents, &st.store, session, agent, s, files).await
            })
            .await
        }
        Call::CopyBack { session, agent, back } => {
            let s = setup(st, &session, &agent).await?;
            to_its_end(st, move |st| async move {
                copies::back(&st.agents, &st.store, session, agent, s, back).await
            })
            .await
        }
        Call::CopyRenew { session, agent } => {
            to_its_end(st, move |st| async move {
                let slot = st.agents.slot(&session, &agent);
                let _held = slot.hold().await?;
                slot.still().await?;
                let s = setup(&st, &session, &agent).await?;
                copies::renew(&st.agents, &st.store, session, agent, s).await
            })
            .await
        }
        Call::CopyRemove { session, agent, lose } => {
            to_its_end(st, move |st| async move {
                let slot = st.agents.slot(&session, &agent);
                let _held = slot.hold().await?;
                slot.still().await?;
                let s = setup(&st, &session, &agent).await?;
                copies::remove(&st.agents, &st.store, session, agent, s, lose).await?;
                Ok(json!({}))
            })
            .await
        }
        Call::Harnesses {} => {
            let a = st.agents.clone();
            let written = a.written(&st.store).await?;
            let all = blocking(move || a.harnesses(&written)).await??;
            serde_json::to_value(all).map_err(|e| internal(e.to_string()))
        }
        Call::Programs {} => {
            serde_json::to_value(st.agents.programs.list()).map_err(|e| internal(e.to_string()))
        }
        Call::ProgramSet { harness, account, name, path, args, vars } => {
            let a = st.agents.clone();
            let written = a.written(&st.store).await?;
            let said = Said { harness, account, name, path, args, vars };
            let harness = blocking(move || a.program(said, &written)).await??;
            Ok(json!({ "harness": harness }))
        }
        Call::ProgramRemove { harness, account } => {
            let a = st.agents.clone();
            let remove = move || {
                a.programs.remove(&harness, account.as_deref())?;
                // What another program of that name says is another matter.
                a.catalogue.forget(|l| l.harness == harness);
                Ok::<_, WireError>(())
            };
            blocking(remove).await??;
            Ok(json!({}))
        }
        Call::Look { path } => {
            let a = st.agents.clone();
            let all = blocking(move || a.look(&path)).await??;
            serde_json::to_value(all).map_err(|e| internal(e.to_string()))
        }
        Call::AgentSet { session, agent, execution, access, plan } => {
            let slot = st.agents.slot(&session, &agent);
            // Nothing begins meanwhile.
            let _held = slot.hold().await?;
            let mut setup = setup(st, &session, &agent).await?;
            if execution.as_ref().is_some_and(|e| e.harness != setup.execution.harness) {
                // What it knows is in the harness it has.
                return Err(agents::bad("an agent keeps its harness; add another agent instead"));
            }
            if execution.as_ref().is_some_and(|e| e.connection != setup.execution.connection) {
                // What was said is kept with the account it was said on.
                setup.resume = None;
            }
            setup.execution = execution.unwrap_or(setup.execution);
            setup.access = match (access, plan) {
                (Some(a), _) => a,
                (None, Some(true)) => axiom_proto::Access::Read,
                (None, _) => setup.access,
            };
            // Who may do something else is not asked for a plan any more.
            setup.plan = plan.unwrap_or(setup.plan && access.is_none());
            let a = st.agents.clone();
            let written = a.written(&st.store).await?;
            let setup = blocking(move || a.setup(setup, &written)).await??;
            keep(st, &session, &agent, &setup).await?;
            // What was allowed was allowed to it as it was.
            st.agents.desk(&session).forget();
            slot.changed().await;
            said(st, &session, agents::changed(&agent, &setup)).await?;
            Ok(json!({}))
        }
        Call::PlanAccept { session, agent, item, access, text } => {
            if text.as_ref().is_some_and(|t| t.trim().is_empty() || t.len() > planning::MOST) {
                return Err(agents::bad("a plan has between 1 and 65536 bytes"));
            }
            let slot = st.agents.slot(&session, &agent);
            {
                // Nothing begins meanwhile.
                let _held = slot.hold().await?;
                let mut setup = setup(st, &session, &agent).await?;
                if !setup.plan || setup.planned.as_ref() != Some(&item) {
                    return Err(agents::bad("this is not the plan the agent has handed in last"));
                }
                (setup.plan, setup.planned, setup.access) = (false, None, access);
                let a = st.agents.clone();
                let written = a.written(&st.store).await?;
                let setup = blocking(move || a.setup(setup, &written)).await??;
                keep(st, &session, &agent, &setup).await?;
                st.agents.desk(&session).forget();
                slot.changed().await;
                said(st, &session, agents::changed(&agent, &setup)).await?;
            }
            prompt(st, session, agent, planning::accepted(text), None, None).await
        }
        Call::Open { path } => {
            // A folder, by its whole name: what a window is given is what will be worked in.
            let whole = path.len() <= 4096 && std::path::Path::new(&path).is_absolute();
            let there = whole && tokio::fs::metadata(&path).await.is_ok_and(|m| m.is_dir());
            if !there {
                return Err(agents::bad("a folder is opened by its whole path"));
            }
            let windows = st.open.receiver_count();
            st.open.send_replace(path);
            Ok(json!({ "windows": windows }))
        }
        Call::BranchLook { session, agent, item } => {
            let p = place(st, &session, &agent, &item).await?;
            let info = BranchInfo { apart: p.setup.apart.is_some(), files: p.kept };
            serde_json::to_value(info).map_err(|e| internal(e.to_string()))
        }
        Call::Branch { session, agent, item, name, files } => {
            to_its_end(st, move |st| branch(st, session, agent, item, name, files)).await
        }
        Call::Compact { session, agent } => {
            if setup(st, &session, &agent).await?.execution.harness != "axiom" {
                return Err(agents::bad("its program keeps its own conversation"));
            }
            let slot = st.agents.slot(&session, &agent);
            let Spoken::Start(turnstile) = slot.speak().await? else {
                return Err(WireError::new(ErrorCode::Busy, "the agent is at work"));
            };
            // Nobody says anything while the place is held. What waits is to be answered, and not
            // only to be part of a summary.
            let waits = st.store.pending(session.clone(), agent.clone()).await;
            if !waits.map_err(|e| internal(e.to_string()))?.is_empty() {
                return Err(agents::bad("something said to it waits to be heard: say more first"));
            }
            let run = start(st, session, agent, turnstile, Opening::Compact).await?;
            Ok(json!({ "run": run }))
        }
        Call::Prompt { session, agent, text, key } => {
            if key.as_ref().is_some_and(|k| k.len() > KEY) {
                return Err(agents::bad("a prompt's name has at most 100 bytes"));
            }
            if text.trim().is_empty() || limits::text_len(&text) > limits::TEXT {
                return Err(agents::bad("what is said has between 1 byte and 1 MiB"));
            }
            prompt(st, session, agent, text, key, None).await
        }
        Call::Answer { session, request, answer, digest, words } => {
            let words = words.filter(|w| !w.trim().is_empty());
            let asked = st.waiting.asked(&session, &request);
            st.agents.answer(&session, &request, answer, &digest, words.clone())?;
            // A program of the person's own has no way to be told why with its no: it is told after.
            if let (Some(words), Some(agent)) = (words, asked)
                && let Ok(s) = setup(st, &session, &agent).await
                && s.execution.harness != "axiom"
                // Not if it was not to be started at all.
                && st.agents.slot(&session, &agent).has_program().await
            {
                let text = format!("I said no to what you asked for: {words}");
                prompt(st, session, agent, text, None, None).await?;
            }
            Ok(json!({}))
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
            // To its end, also when who asked is gone.
            let leave = tokio::spawn(st.agents.connections.clone().leave(connection.clone()));
            let ended = leave.await.map_err(|e| internal(e.to_string()))??;
            let a = st.agents.clone();
            let forget = move || {
                a.catalogue.forget(|l| l.connection.as_deref() == Some(&connection));
                a.programs.forget(&connection)
            };
            blocking(forget).await??;
            Ok(ended.map_or_else(|| json!({}), |e| json!({ "ended": e })))
        }
        Call::SigninBegin { connection } => {
            let c = st.agents.connections.clone();
            let (signin, url) = st.signins.begin(c, connection).await?;
            Ok(json!({ "signin": signin, "url": url }))
        }
        Call::SigninState { signin } => {
            serde_json::to_value(st.signins.state(&signin)?).map_err(|e| internal(e.to_string()))
        }
        Call::SigninCancel { signin } => st.signins.cancel(&signin).map(|()| json!({})),
        Call::Models { connection } => {
            let (a, of) = (st.agents.clone(), connection.clone());
            let client = blocking(move || a.connections.client(&connection)).await??;
            // A provider that does not list them in one answer: they are named by hand.
            let all = match timeout(LISTING, client.list_models()).await {
                Err(_) => return Err(agents::bad("the provider did not list its models in time")),
                Ok(Err(axiom_model::CallError::Encode(_))) => Vec::new(),
                Ok(r) => r.map_err(|e| agents::bad(e.to_string()))?,
            };
            let a = st.agents.clone();
            let all: Vec<ModelInfo> = all
                .into_iter()
                .map(|m| catalogue::fit(catalogue::about(client.profile(), m)))
                .collect();
            let keep = all.clone();
            if !keep.is_empty() {
                blocking(move || a.catalogue.put("axiom", Some(&of), keep)).await?;
            }
            serde_json::to_value(all).map_err(|e| internal(e.to_string()))
        }
        Call::ModelsKnown {} => {
            serde_json::to_value(st.agents.catalogue.list()).map_err(|e| internal(e.to_string()))
        }
        Call::ModelAbout { connection, model } => {
            if model.trim().is_empty() || limits::text_len(&model) > limits::TITLE {
                return Err(agents::bad("a model's name has between 1 and 500 bytes"));
            }
            let a = st.agents.clone();
            let about = move || {
                if let Some(m) = a.catalogue.find("axiom", Some(&connection), &model) {
                    return Ok(m);
                }
                let client = a.connections.client(&connection)?;
                let named = axiom_model::ModelInfo::named(model);
                Ok::<_, WireError>(catalogue::about(client.profile(), named))
            };
            serde_json::to_value(blocking(about).await??).map_err(|e| internal(e.to_string()))
        }
        Call::FileRead { session, agent, path } => {
            let here = looked_at(st, &session, agent.as_ref()).await?;
            value(off(move || here.read(&path)).await?)
        }
        Call::FileList { session, agent, path } => {
            let here = looked_at(st, &session, agent.as_ref()).await?;
            value(off(move || here.list(&path)).await?)
        }
        Call::FileFind { session, agent, words } => {
            let here = looked_at(st, &session, agent.as_ref()).await?;
            value(off(move || here.find(&words)).await?)
        }
        Call::FileSearch { session, agent, words, case, word, regex } => {
            let here = looked_at(st, &session, agent.as_ref()).await?;
            value(off(move || here.search(&words, case, word, regex)).await?)
        }
        Call::FileOpen { session, agent, path, line, editor } => {
            let here = looked_at(st, &session, agent.as_ref()).await?;
            let folder = here.site.workspace.to_string();
            let file = off(move || here.file(&path)).await?;
            let written = st.agents.written(&st.store).await?;
            let w = st.agents.editors(written)?;
            let named = editor.clone().unwrap_or_else(|| "the first editor found".into());
            person(st, &session, &folder, "editor", format!("{named} opens {file}")).await?;
            off(move || w.open(&file, line, editor.as_deref())).await?;
            Ok(json!({}))
        }
        Call::FileShow { session, agent, path } => {
            let here = looked_at(st, &session, agent.as_ref()).await?;
            let folder = here.site.workspace.to_string();
            let file = off(move || here.file(&path)).await?;
            let written = st.agents.written(&st.store).await?;
            let w = st.agents.editors(written)?;
            person(st, &session, &folder, "reveal", format!("the file manager shows {file}"))
                .await?;
            off(move || w.reveal(&file)).await?;
            Ok(json!({}))
        }
        Call::Editors {} => {
            let written = st.agents.written(&st.store).await?;
            let w = st.agents.editors(written)?;
            value(off(move || Ok(w.editors())).await?)
        }
        Call::Folders {} => value(st.folders.list()),
        Call::FolderSet { path, name, pinned, execution, access, commands } => {
            // As sessions name it, so that it is theirs; what it is, is checked by the system.
            if !std::path::Path::new(&path).is_absolute()
                || limits::text_len(&path) > limits::WORKSPACE
            {
                return Err(agents::bad("a folder is named by its whole path"));
            }
            let said = crate::folders::Said { path, name, pinned, execution, access, commands };
            value(st.folders.set(said)?)
        }
        Call::FolderRemove { id } => {
            st.folders.remove(&id)?;
            Ok(json!({}))
        }
        Call::Jobs {} => {
            let mut all = Vec::new();
            for (session, agent, jobs) in st.agents.jobs() {
                for j in jobs {
                    all.push(JobInfo {
                        session: session.clone(),
                        agent: agent.clone(),
                        job: j.job,
                        command: j.command,
                        age: j.age,
                        ended: j.ended,
                        last: j.last,
                        processes: processes(&j.processes),
                    });
                }
            }
            value(all)
        }
        Call::JobStop { session, agent, job } => {
            st.agents.stop_job(&session, &agent, job)?;
            Ok(json!({}))
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
mod left_behind {
    #[test]
    fn a_process_whose_parent_is_gone_was_left_behind() {
        let tree = [10, 11, 12];
        assert!(!super::left(10, 1, Some(10), &tree), "the first is started by the host");
        assert!(!super::left(11, 10, Some(10), &tree));
        assert!(super::left(12, 1, Some(10), &tree));
        assert!(super::left(12, 99, Some(10), &tree));
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
    async fn who_is_gone_is_told_nothing_more() {
        let (tx, rx) = watch::channel(0);
        let out = Arc::new(Outbox::new(1 << 20));
        let task = tokio::spawn(tells(rx, out.clone()));
        tx.send_replace(3);
        let line = timeout(Duration::from_secs(5), out.take()).await.unwrap().unwrap();
        assert_eq!(line, b"{\"waiting\":3}\n");
        out.close();
        timeout(Duration::from_secs(5), task).await.unwrap().unwrap();
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
