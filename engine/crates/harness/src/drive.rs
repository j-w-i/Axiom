//! One attempt: a process, what is said between it and an adapter, and its end. Everything between a
//! harness and who listens to it is bounded: while nobody takes the events it is not read, and a
//! harness that is not read waits.

use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use axiom_proto::{Lines, Outcome};
use axiom_spawn::{Ended, Running};
use serde_json::Value;
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    process::{ChildStderr, ChildStdin, ChildStdout},
    sync::{OwnedSemaphorePermit, Semaphore, mpsc, oneshot},
    time::{Instant, sleep_until, timeout},
};

use crate::{
    event::{Answer, By, Done, Event, Gone, Level, Probe, Refused, Settled, Took, Why},
    frames::{Frame, Frames, Tail, Way, cut, frame},
    ledger::{Ledger, MOST_ASKED, size, weight},
    talk::{Out, Talk, Word},
};

/// A longer line ends the attempt.
pub const MOST_LINE: usize = 8 * 1024 * 1024;
/// A longer prompt is refused.
pub const MOST_PROMPT: usize = 1024 * 1024;
/// A question that is about more is refused: it could not be shown whole.
pub const MOST_SUBJECT: usize = 256 * 1024;
/// Events that wait for who listens, in bytes and in number.
const WAITING: usize = 4 * 1024 * 1024;
const EVENTS: usize = 1024;
/// Lines that wait for the harness to read them.
const UNREAD: usize = 128;
/// Lines that are no messages, until the attempt ends. Programs greet, and libraries warn.
const GARBAGE: u32 = 16;
const STDERR: usize = 8 * 1024;

/// How long things may take.
#[derive(Debug, Clone)]
pub struct Limits {
    /// Until the session is there. Harnesses start servers of their own.
    pub opening: Duration,
    /// From asking a turn to end until everything is ended.
    pub cancel: Duration,
    /// From asking a process to end until it is ended.
    pub stop: Duration,
    /// Until one line is read by the harness.
    pub write: Duration,
    /// A person has for an answer; then it is no. Nothing: as long as they like.
    pub ask: Option<Duration>,
    /// Without a word while at work and nothing is asked.
    pub silence: Option<Duration>,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            opening: Duration::from_secs(90),
            cancel: Duration::from_secs(10),
            stop: Duration::from_secs(3),
            write: Duration::from_secs(60),
            ask: None,
            silence: Some(Duration::from_secs(45 * 60)),
        }
    }
}

type Reply<T> = oneshot::Sender<Result<T, Refused>>;

#[derive(Debug)]
pub(crate) enum Command {
    Prompt(String, By, Reply<Took>),
    Answer(String, Answer, Reply<()>),
    /// Ends the turn; with `true`, everything after it.
    Stop(bool),
    Shut,
}

/// Says things to an attempt. Dropped, it changes nothing.
#[derive(Debug, Clone)]
pub struct Handle(mpsc::Sender<Command>);

impl Handle {
    async fn ask<T>(&self, c: impl FnOnce(Reply<T>) -> Command) -> Result<T, Refused> {
        let (tx, rx) = oneshot::channel();
        self.0.send(c(tx)).await.map_err(|_| Refused::Closed)?;
        rx.await.map_err(|_| Refused::Closed)?
    }

    /// Nothing more is taken.
    pub fn is_over(&self) -> bool {
        self.0.is_closed()
    }

    /// Begins a turn, or is heard in the one that runs, if the harness can do that.
    pub async fn prompt(&self, text: &str, by: By) -> Result<Took, Refused> {
        if text.trim().is_empty() {
            return Err(Refused::Empty);
        }
        if text.len() > MOST_PROMPT {
            return Err(Refused::TooLong(MOST_PROMPT));
        }
        self.ask(|r| Command::Prompt(text.to_owned(), by, r)).await
    }

    /// A person's answer.
    pub async fn answer(&self, question: &str, a: Answer) -> Result<(), Refused> {
        self.ask(|r| Command::Answer(question.to_owned(), a, r)).await
    }

    /// Ends the turn and keeps the session. If it does not end in time, everything is ended.
    pub async fn interrupt(&self) -> Result<(), Refused> {
        self.0.send(Command::Stop(false)).await.map_err(|_| Refused::Closed)
    }

    /// Ends the turn, if the harness can be asked to, and then everything it runs (SI-24).
    pub async fn cancel(&self) -> Result<(), Refused> {
        self.0.send(Command::Stop(true)).await.map_err(|_| Refused::Closed)
    }

    /// Lets it end in order, and after a while ends it.
    pub async fn shut(&self) -> Result<(), Refused> {
        self.0.send(Command::Shut).await.map_err(|_| Refused::Closed)
    }
}

/// What an attempt tells. Dropped, the attempt ends and all it runs.
#[derive(Debug)]
pub struct Events(mpsc::Receiver<(Event, OwnedSemaphorePermit)>);

impl Events {
    /// The next. Nothing after [`Event::Closed`].
    pub async fn next(&mut self) -> Option<Event> {
        Some(self.0.recv().await?.0)
    }
}

pub(crate) struct Given<F> {
    pub process: Running,
    pub talk: Box<dyn Talk>,
    pub frames: F,
    pub limits: Limits,
    /// Every question is answered with no.
    pub read_only: bool,
    pub person_only: bool,
    pub probed: Option<oneshot::Sender<Probe>>,
}

/// Starts driving; nothing if the process has no pipes.
pub(crate) fn drive<F: Frames>(mut g: Given<F>) -> Option<(Handle, Events)> {
    let (stdin, out, err) =
        (g.process.stdin.take()?, g.process.stdout.take()?, g.process.stderr.take()?);
    let (commands, told) = mpsc::channel(16);
    let (events, heard) = mpsc::channel(EVENTS);
    let (writer, to_write) = mpsc::channel(UNREAD);
    let failed = tokio::spawn(write(stdin, to_write, g.limits.write));
    let now = Instant::now();
    let a = Attempt {
        room: Arc::new(Semaphore::new(WAITING)),
        lines: Lines::new(MOST_LINE),
        ledger: Ledger::default(),
        waiting: VecDeque::new(),
        tail: Tail::default(),
        errors: VecDeque::new(),
        writer: Some(writer),
        events,
        ready: false,
        garbage: 0,
        opening: now + g.limits.opening,
        heard: now,
        stopping: None,
        answers: BTreeMap::new(),
        over: None,
        g,
    };
    tokio::spawn(a.run(out, err, told, failed));
    Some((Handle(commands), Events(heard)))
}

async fn write(
    mut to: ChildStdin,
    mut lines: mpsc::Receiver<String>,
    limit: Duration,
) -> Option<String> {
    while let Some(l) = lines.recv().await {
        match timeout(limit, to.write_all(l.as_bytes())).await {
            Ok(Ok(())) => {}
            Ok(Err(e)) => return Some(format!("it cannot be told anything: {e}")),
            Err(_) => return Some("it does not read what it is told".into()),
        }
    }
    None
}

struct Attempt<F> {
    g: Given<F>,
    room: Arc<Semaphore>,
    lines: Lines,
    ledger: Ledger,
    /// Of one line, at most.
    waiting: VecDeque<Event>,
    tail: Tail,
    errors: VecDeque<u8>,
    /// Dropped, its input is closed.
    writer: Option<mpsc::Sender<String>>,
    events: mpsc::Sender<(Event, OwnedSemaphorePermit)>,
    ready: bool,
    garbage: u32,
    opening: Instant,
    heard: Instant,
    /// Until when the turn must have ended, and whether everything ends with it.
    stopping: Option<(Instant, bool)>,
    /// Until when questions must have been answered.
    answers: BTreeMap<String, Instant>,
    over: Option<Over>,
}

/// How the attempt is to end.
struct Over {
    why: Why,
    words: Option<String>,
    outcome: Outcome,
    /// Whether the process may end by itself first.
    gently: bool,
}

struct Wrong {
    until: Instant,
    why: Why,
    words: String,
}

impl Wrong {
    fn new(why: Why, words: String) -> Self {
        let mut w = Self { until: Instant::now(), why, words };
        w.wait();
        w
    }
    fn wait(&mut self) {
        self.until = Instant::now() + Duration::from_millis(500);
    }
}

/// What woke the attempt up.
enum Woke {
    Room(Option<OwnedSemaphorePermit>),
    Out(std::io::Result<usize>),
    Err(std::io::Result<usize>),
    Told(Option<Command>),
    Ended(std::io::Result<Ended>),
    Failed(Option<String>),
    Unheard,
    Look,
    Late,
}

impl<F: Frames> Attempt<F> {
    async fn run(
        mut self,
        mut out: ChildStdout,
        mut err: ChildStderr,
        mut told: mpsc::Receiver<Command>,
        mut failed: tokio::task::JoinHandle<Option<String>>,
    ) {
        let mut buf = vec![0; 16 * 1024];
        let mut ebuf = vec![0; 16 * 1024];
        let (mut out_open, mut err_open, mut told_open, mut writing) = (true, true, true, true);
        let mut look = tokio::time::interval(Duration::from_millis(250));
        let mut ended: Option<Ended> = None;
        // Something is wrong with it. It is said when it has had a moment to end, so that its end can
        // be told instead.
        let mut wrong: Option<Wrong> = None;
        let mut o = Out::default();
        self.g.talk.open(&mut o);
        self.take(o).await;
        while self.over.is_none() {
            if self.waiting.is_empty() {
                match self.lines.next_line() {
                    Ok(Some(l)) => {
                        self.line(&l).await;
                        continue;
                    }
                    Ok(None) => {}
                    Err(e) => {
                        self.fail(Why::Nonsense, format!("it wrote no line: {e}"));
                        continue;
                    }
                }
                if !out_open && let Some(e) = &ended {
                    self.fail(Why::Died, format!("it ended by itself ({})", code(e)));
                    continue;
                }
            }
            let late = self.late(wrong.as_ref());
            let next = self.waiting.front().map(weight);
            let idle = next.is_none();
            let woke = tokio::select! {
                r = room(&self.room, &self.events, next.unwrap_or(0)), if !idle => Woke::Room(r),
                r = out.read(&mut buf), if out_open && idle => Woke::Out(r),
                r = err.read(&mut ebuf), if err_open => Woke::Err(r),
                c = told.recv(), if told_open => Woke::Told(c),
                r = self.g.process.wait(), if ended.is_none() => Woke::Ended(r),
                r = &mut failed, if writing => Woke::Failed(r.unwrap_or(None)),
                () = self.events.closed() => Woke::Unheard,
                _ = look.tick(), if ended.is_none() => Woke::Look,
                () = sleep_until(late) => Woke::Late,
            };
            match woke {
                Woke::Room(p) => {
                    self.deliver(p);
                    wrong.as_mut().map(Wrong::wait);
                }
                Woke::Out(Ok(n)) if n > 0 => {
                    self.heard = Instant::now();
                    self.lines.push(&buf[..n]);
                    wrong.as_mut().map(Wrong::wait);
                }
                Woke::Out(_) => {
                    out_open = false;
                    wrong.get_or_insert(Wrong::new(Why::Died, "it closed its output".into()));
                }
                Woke::Err(Ok(n)) if n > 0 => self.error(&ebuf[..n]),
                Woke::Err(_) => err_open = false,
                Woke::Told(Some(c)) => self.command(c).await,
                Woke::Told(None) => told_open = false,
                Woke::Ended(Ok(e)) => {
                    let words = format!("it ended by itself ({})", code(&e));
                    wrong = Some(Wrong::new(Why::Died, words));
                    ended = Some(e);
                }
                Woke::Ended(Err(e)) => self.fail(Why::Died, format!("its end is not known: {e}")),
                Woke::Failed(words) => {
                    writing = false;
                    if let Some(w) = words {
                        wrong.get_or_insert(Wrong::new(Why::Deaf, w));
                    }
                }
                Woke::Unheard => self.end(Why::Unheard, None, Outcome::Cancelled, false),
                Woke::Look => self.g.process.look(),
                Woke::Late => self.overdue(wrong.as_ref()).await,
            }
        }
        self.finish(out, err, err_open, ended).await;
    }

    /// When something is next too late.
    fn late(&self, wrong: Option<&Wrong>) -> Instant {
        let far = Instant::now() + Duration::from_secs(3600);
        let mut all = vec![far];
        all.extend(self.stopping.map(|s| s.0));
        all.extend(self.answers.values());
        // While nobody listens it is not read, and it is no fault of its own that it says nothing.
        if self.waiting.is_empty() {
            all.extend(wrong.map(|w| w.until));
            all.extend((!self.ready).then_some(self.opening));
            all.extend(self.silent());
        }
        all.into_iter().min().unwrap_or(far)
    }

    fn silent(&self) -> Option<Instant> {
        let quiet = self.ledger.turn() && self.ledger.asked() == 0;
        self.g.limits.silence.filter(|_| quiet).map(|s| self.heard + s)
    }

    async fn overdue(&mut self, wrong: Option<&Wrong>) {
        let now = Instant::now();
        if self.stopping.is_some_and(|s| s.0 <= now) {
            let words = Some("it did not stop when it was asked to".into());
            return self.end(Why::Asked, words, Outcome::Cancelled, false);
        }
        if self.waiting.is_empty() {
            if let Some(w) = wrong.filter(|w| w.until <= now) {
                return self.fail(w.why, w.words.clone());
            }
            if !self.ready && self.opening <= now {
                return self.fail(Why::Silent, "it did not begin in time");
            }
            if self.silent().is_some_and(|s| s <= now) {
                return self.fail(Why::Silent, "it said nothing for too long");
            }
        }
        let late: Vec<String> =
            self.answers.iter().filter(|a| *a.1 <= now).map(|a| a.0.clone()).collect();
        for id in late {
            self.refuse(&id, Settled::TimedOut).await;
        }
    }

    /// Answers a question with no.
    async fn refuse(&mut self, id: &str, how: Settled) {
        self.answers.remove(id);
        let mut o = Out::default();
        if self.g.talk.answer(id, &Answer::Refuse, &mut o).is_ok() {
            o.event(Event::AskClosed { id: id.to_owned(), how });
        }
        self.take(o).await;
    }

    async fn command(&mut self, c: Command) {
        let mut o = Out::default();
        match c {
            Command::Prompt(text, by, reply) => {
                let r = if self.g.person_only && by != By::Person {
                    Err(Refused::PersonOnly)
                } else if self.stopping.is_some() {
                    Err(Refused::Busy)
                } else {
                    self.g.talk.prompt(&text, by, &mut o)
                };
                self.take(o).await;
                let _ = reply.send(r);
            }
            Command::Answer(id, a, reply) => {
                let r = self.g.talk.answer(&id, &a, &mut o);
                if r.is_ok() {
                    o.event(Event::AskClosed { id, how: Settled::Answered });
                }
                self.take(o).await;
                let _ = reply.send(r);
            }
            Command::Stop(all) => {
                if !self.ledger.turn() {
                    if all || !self.ready {
                        self.end(Why::Asked, None, Outcome::Cancelled, false);
                    }
                    return;
                }
                if let Some(s) = &mut self.stopping {
                    s.1 |= all;
                    return;
                }
                if self.g.talk.interrupt(&mut o) {
                    self.stopping = Some((Instant::now() + self.g.limits.cancel, all));
                    self.take(o).await;
                } else {
                    self.end(Why::Asked, None, Outcome::Cancelled, false);
                }
            }
            Command::Shut => {
                self.g.talk.close(&mut o);
                self.take(o).await;
                self.end(Why::Asked, None, Outcome::Cancelled, true);
            }
        }
    }

    async fn line(&mut self, line: &[u8]) {
        if line.iter().all(u8::is_ascii_whitespace) {
            return;
        }
        let read = serde_json::from_slice::<Value>(line).ok();
        let f = frame(Way::In, line, read.as_ref());
        let raw = cut(&f.text, 4096).0.to_owned();
        if !self.keep(f).await {
            return;
        }
        let mut o = Out::default();
        match read {
            Some(v) => self.g.talk.heard(v, &mut o),
            _ => o.bad = true,
        }
        if o.bad {
            self.garbage += 1;
            if self.garbage > GARBAGE {
                return self.fail(Why::Nonsense, "it writes what is not its protocol");
            }
            o.event(Event::Unknown { what: "no message".into(), raw, bytes: 0 });
        }
        self.take(o).await;
    }

    async fn keep(&mut self, f: Frame) -> bool {
        self.tail.add(&f);
        let kept = timeout(self.g.limits.write, self.g.frames.keep(f)).await;
        let kept = matches!(kept, Ok(Ok(())));
        if !kept {
            self.fail(Why::Journal, "what was said could not be put on record");
        }
        kept
    }

    /// Does what an adapter wants done.
    async fn take(&mut self, o: Out) {
        let mut all = VecDeque::from([o]);
        while let Some(o) = all.pop_front() {
            for v in o.send {
                if !self.send(&v).await {
                    return;
                }
            }
            for w in o.say {
                if let Word::Event(Event::Asked(q)) = &w {
                    let why = if self.g.read_only {
                        Some("it may only read")
                    } else if self.stopping.is_some() {
                        Some("it is being stopped")
                    } else if size(&q.subject) > MOST_SUBJECT
                        || q.tool.as_ref().is_some_and(|t| !self.ledger.is_whole(t))
                    {
                        Some("it is about more than can be shown")
                    } else if self.ledger.asked() >= MOST_ASKED {
                        Some("too much is asked at once")
                    } else {
                        None
                    };
                    if let Some(why) = why {
                        let mut o = Out::default();
                        o.notice(Level::Warning, format!("Not allowed, {why}: {}", q.title));
                        let _ = self.g.talk.answer(&q.id, &Answer::Refuse, &mut o);
                        all.push_back(o);
                        continue;
                    }
                }
                self.word(w);
            }
            if let Some(p) = o.probe
                && let Some(tx) = self.g.probed.take()
            {
                let _ = tx.send(p);
                self.end(Why::Asked, None, Outcome::Completed, true);
            }
            if let Some((why, words)) = o.end {
                self.fail(why, words);
            }
        }
    }

    fn word(&mut self, w: Word) {
        let mut out = Vec::new();
        if let Err(f) = self.ledger.take(w, &mut out) {
            self.fail(Why::Flood, f.0);
        }
        for e in out {
            match &e {
                Event::Ready(_) => self.ready = true,
                Event::Asked(q) => {
                    if let Some(limit) = self.g.limits.ask {
                        self.answers.insert(q.id.clone(), Instant::now() + limit);
                    }
                }
                Event::AskClosed { id, .. } => {
                    self.answers.remove(id);
                }
                Event::Done(_) => {
                    if let Some((_, all)) = self.stopping.take()
                        && all
                    {
                        self.end(Why::Asked, None, Outcome::Cancelled, false);
                    }
                }
                _ => {}
            }
            self.waiting.push_back(e);
        }
    }

    async fn send(&mut self, v: &Value) -> bool {
        let line = v.to_string();
        let f = frame(Way::Out, line.as_bytes(), Some(v));
        if !self.keep(f).await {
            return false;
        }
        let Some(w) = &self.writer else { return false };
        match w.try_send(line + "\n") {
            Ok(()) => true,
            // It may have died: that is told when it is known.
            Err(mpsc::error::TrySendError::Closed(_)) => true,
            Err(mpsc::error::TrySendError::Full(_)) => {
                self.fail(Why::Deaf, "it does not read what it is told");
                false
            }
        }
    }

    fn error(&mut self, b: &[u8]) {
        self.errors.extend(b);
        let over = self.errors.len().saturating_sub(STDERR);
        self.errors.drain(..over);
    }

    fn deliver(&mut self, p: Option<OwnedSemaphorePermit>) {
        // Every prompt and every answer makes one: from then on it is its turn to speak.
        self.heard = Instant::now();
        if let Some(p) = p
            && let Some(e) = self.waiting.pop_front()
        {
            // There is room, and nobody else sends.
            let _ = self.events.try_send((e, p));
        }
    }

    fn fail(&mut self, why: Why, words: impl Into<String>) {
        self.end(why, Some(words.into()), Outcome::Failed, false);
    }

    /// The first reason stands.
    fn end(&mut self, why: Why, words: Option<String>, outcome: Outcome, gently: bool) {
        self.over.get_or_insert(Over { why, words, outcome, gently });
    }

    async fn finish(
        mut self,
        out: ChildStdout,
        err: ChildStderr,
        err_open: bool,
        ended: Option<Ended>,
    ) {
        let Some(over) = self.over.take() else { return };
        self.writer = None;
        let grace = self.g.limits.stop;
        // What it still says is not listened to, but it must be able to say it.
        let (out, err) = (tokio::spawn(drain(out)), tokio::spawn(drain(err)));
        let by_itself = match (&ended, over.gently) {
            (None, true) => timeout(grace, self.g.process.wait()).await.ok().and_then(Result::ok),
            _ => None,
        };
        let ended = match ended.or(by_itself) {
            Some(e) => Some(e),
            None => self.g.process.stop(grace).await.ok(),
        };
        out.abort();
        if err_open && let Ok(Ok(last)) = timeout(Duration::from_secs(1), err).await {
            self.error(&last);
        }
        let mut last = Vec::new();
        let d = Done {
            outcome: over.outcome,
            stop: format!("{:?}", over.why).to_lowercase(),
            error: over.words.clone(),
        };
        self.ledger.close(d, &mut last);
        self.waiting.extend(last);
        let stderr: Vec<u8> = self.errors.iter().copied().collect();
        self.waiting.push_back(Event::Closed(Box::new(Gone {
            why: over.why,
            words: over.words,
            ended,
            frames: self.tail.take(),
            stderr: String::from_utf8_lossy(&stderr).into_owned(),
        })));
        while let Some(e) = self.waiting.front() {
            // Nothing runs any more: who listens may take as long as they like.
            match room(&self.room, &self.events, weight(e)).await {
                Some(p) => self.deliver(Some(p)),
                None => return,
            }
        }
    }
}

/// Waits until an event of this size may be sent. Nothing: nobody listens.
async fn room(
    s: &Arc<Semaphore>,
    to: &mpsc::Sender<(Event, OwnedSemaphorePermit)>,
    weight: usize,
) -> Option<OwnedSemaphorePermit> {
    let n = u32::try_from(weight.min(WAITING)).unwrap_or(1);
    let p = s.clone().acquire_many_owned(n).await.ok()?;
    // Its place is free until the next send, which is ours.
    drop(to.reserve().await.ok()?);
    Some(p)
}

/// Reads to the end, and returns the last of it.
async fn drain(mut from: impl AsyncRead + Unpin) -> Vec<u8> {
    let (mut buf, mut last) = (vec![0; 16 * 1024], Vec::new());
    while let Ok(n) = from.read(&mut buf).await {
        if n == 0 {
            break;
        }
        last.extend_from_slice(&buf[..n]);
        let over = last.len().saturating_sub(STDERR);
        last.drain(..over);
    }
    last
}

fn code(e: &Ended) -> String {
    match (e.code, e.signal) {
        (Some(c), _) => format!("exit {c}"),
        (_, Some(s)) => format!("signal {s}"),
        _ => "how is not known".into(),
    }
}
