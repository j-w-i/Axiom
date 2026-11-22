//! Integrated terminals (ADR-0018 §4; M4-TERM-001): the person's shell in a PTY
//! (`portable-pty`), its output streamed as UTF-8 that never splits a character, coalesced to at
//! most one event per 16 ms, with a bounded backlog for a client that reconnects. A terminal is
//! not an agent tool and passes no gate.

use axiom_protocol::terminal::{ListedTerminal, TerminalId, TerminalInfo};
use axiom_protocol::WorkspaceId;
use portable_pty::{native_pty_system, ChildKiller, CommandBuilder, MasterPty, PtySize};
use std::collections::{BTreeMap, HashMap};
use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{sync_channel, RecvTimeoutError};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

/// Output kept per terminal for a client that reconnects.
pub const BACKLOG_BYTES: usize = 1024 * 1024;
/// At most one output event per terminal in this interval.
pub const COALESCE: Duration = Duration::from_millis(16);
/// Hang-up, then a kill after this long.
pub const CLOSE_GRACE: Duration = Duration::from_secs(2);
/// Largest output event: a burst longer than this is sent in several (AUD-036).
pub const EVENT_MAX_BYTES: usize = 256 * 1024;
/// Output of one terminal sent but not yet delivered (held by the event relay and the
/// connections' queues). Past this the terminal stops reading its PTY, so the program waits
/// (backpressure), instead of the daemon buffering without limit (AUD-036).
pub const OUTPUT_BUDGET_BYTES: usize = 4 * 1024 * 1024;
/// How long output waits for the budget. A relay that delivers nothing for this long (a stuck
/// client) does not freeze the terminal: further output is not sent as events, the backlog keeps
/// it, and the `seq` gap tells a client to redraw from `terminal/list`.
pub const BUDGET_WAIT: Duration = Duration::from_secs(2);

#[derive(Debug, thiserror::Error)]
pub enum TermError {
    #[error("There is no terminal {0}.")]
    NotFound(String),
    #[error("The terminal could not be started: {0}")]
    Spawn(String),
    #[error("{0}")]
    Invalid(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl From<TermError> for axiom_protocol::RpcError {
    fn from(e: TermError) -> Self {
        use axiom_protocol::ErrorKind;
        let kind = match &e {
            TermError::NotFound(_) => ErrorKind::NotFound,
            TermError::Spawn(_) => ErrorKind::SpawnFailed,
            TermError::Invalid(_) => ErrorKind::InvalidParams,
            TermError::Io(_) => ErrorKind::Io,
        };
        axiom_protocol::RpcError::new(kind, e.to_string())
    }
}

/// The bytes of a terminal's output budget one event holds; released when the last copy of the
/// event is dropped (every subscriber has received and let go of it).
#[derive(Debug)]
pub struct OutputPermit {
    bytes: usize,
    budget: Arc<Budget>,
}

impl Drop for OutputPermit {
    fn drop(&mut self) {
        let mut held = lock(&self.budget.held);
        *held = held.saturating_sub(self.bytes);
        self.budget.cv.notify_all();
    }
}

impl PartialEq for OutputPermit {
    fn eq(&self, other: &Self) -> bool {
        self.bytes == other.bytes
    }
}

/// One terminal's output budget (bytes sent as events and not yet released).
#[derive(Debug, Default)]
struct Budget {
    held: Mutex<usize>,
    cv: std::sync::Condvar,
}

impl Budget {
    /// Takes `n` bytes, waiting up to [`BUDGET_WAIT`] while the budget is spent; `None` if it
    /// stayed spent (the event is then not sent). An event larger than the budget is let through
    /// when nothing else is held.
    fn take(self: &Arc<Self>, n: usize) -> Option<OutputPermit> {
        let end = Instant::now() + BUDGET_WAIT;
        let mut held = lock(&self.held);
        while *held > 0 && *held + n > OUTPUT_BUDGET_BYTES {
            let left = end.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            held = self
                .cv
                .wait_timeout(held, left)
                .map(|(g, _)| g)
                .unwrap_or_else(|p| p.into_inner().0);
        }
        *held += n;
        Some(OutputPermit {
            bytes: n,
            budget: self.clone(),
        })
    }
}

/// What a terminal reports.
#[derive(Debug, Clone, PartialEq)]
pub enum TermEvent {
    Output {
        workspace_id: WorkspaceId,
        terminal_id: TerminalId,
        seq: u64,
        data: String,
        /// This event's share of the terminal's output budget: keep the event (or this) until
        /// its data is delivered, then drop it.
        permit: Arc<OutputPermit>,
    },
    Exited {
        workspace_id: WorkspaceId,
        terminal_id: TerminalId,
        code: Option<i32>,
        /// The shell's process id (its process group and session), now ended.
        pid: Option<u32>,
    },
}

/// How to start one terminal (resolved by the daemon).
#[derive(Debug, Clone)]
pub struct Spec {
    pub workspace_id: WorkspaceId,
    /// The program and its arguments; `None`: the person's shell.
    pub argv: Option<Vec<String>>,
    pub cwd: PathBuf,
    /// The complete environment (the daemon's plus the project's).
    pub env: BTreeMap<String, String>,
    pub cols: u16,
    pub rows: u16,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|p| p.into_inner())
}

/// Decodes UTF-8 across reads: a character split between two reads is held until it is whole;
/// invalid bytes become U+FFFD.
#[derive(Default)]
pub struct Utf8Stream {
    pending: Vec<u8>,
}

impl Utf8Stream {
    pub fn push(&mut self, bytes: &[u8]) -> String {
        self.pending.extend_from_slice(bytes);
        let mut out = String::new();
        loop {
            match std::str::from_utf8(&self.pending) {
                Ok(s) => {
                    out.push_str(s);
                    self.pending.clear();
                    return out;
                }
                Err(e) => {
                    let good = e.valid_up_to();
                    // The prefix was just validated.
                    out.push_str(std::str::from_utf8(&self.pending[..good]).unwrap_or_default());
                    match e.error_len() {
                        // Incomplete character at the end: keep it for the next read.
                        None => {
                            self.pending.drain(..good);
                            return out;
                        }
                        Some(n) => {
                            out.push('\u{FFFD}');
                            self.pending.drain(..good + n);
                        }
                    }
                }
            }
        }
    }
}

/// Removes and returns at most `max` bytes from the front of `s`, cut at a character boundary.
fn take_prefix(s: &mut String, max: usize) -> String {
    if s.len() <= max {
        return std::mem::take(s);
    }
    let mut cut = max;
    while !s.is_char_boundary(cut) {
        cut -= 1;
    }
    let rest = s.split_off(cut);
    std::mem::replace(s, rest)
}

/// The kept output, trimmed from the front at a character boundary.
#[derive(Default)]
struct Backlog {
    text: String,
    seq: u64,
}

impl Backlog {
    fn push(&mut self, s: &str) {
        self.text.push_str(s);
        if self.text.len() > BACKLOG_BYTES {
            let mut cut = self.text.len() - BACKLOG_BYTES;
            while !self.text.is_char_boundary(cut) {
                cut += 1;
            }
            self.text.drain(..cut);
        }
    }
}

pub struct Terminal {
    pub id: TerminalId,
    pub workspace_id: WorkspaceId,
    pub title: String,
    pub pid: Option<u32>,
    master: Mutex<Box<dyn MasterPty + Send>>,
    writer: Mutex<Box<dyn Write + Send>>,
    killer: Mutex<Box<dyn ChildKiller + Send + Sync>>,
    backlog: Mutex<Backlog>,
    running: AtomicBool,
    exit_code: Mutex<Option<i32>>,
    budget: Arc<Budget>,
}

impl Terminal {
    pub fn info(&self) -> TerminalInfo {
        TerminalInfo {
            terminal_id: self.id.clone(),
            workspace_id: self.workspace_id.clone(),
            title: self.title.clone(),
            pid: self.pid,
            running: self.running.load(Ordering::SeqCst),
            exit_code: *lock(&self.exit_code),
        }
    }

    pub fn listed(&self) -> ListedTerminal {
        let b = lock(&self.backlog);
        ListedTerminal {
            info: self.info(),
            backlog: b.text.clone(),
            seq: b.seq,
        }
    }

    pub fn write(&self, data: &str) -> Result<(), TermError> {
        let mut w = lock(&self.writer);
        w.write_all(data.as_bytes())?;
        w.flush()?;
        Ok(())
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<(), TermError> {
        if cols == 0 || rows == 0 {
            return Err(TermError::Invalid(
                "A terminal needs at least one row and one column.".into(),
            ));
        }
        lock(&self.master)
            .resize(PtySize {
                rows,
                cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| TermError::Io(std::io::Error::other(e.to_string())))
    }

    /// Output bytes sent as events and not yet released (AUD-036).
    pub fn held_output_bytes(&self) -> usize {
        *lock(&self.budget.held)
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }

    /// Hangs up the process group; kills the terminal's session if it is still there after the
    /// grace period.
    pub fn close(self: &Arc<Self>) {
        if !self.running.load(Ordering::SeqCst) {
            return;
        }
        #[cfg(unix)]
        if let Some(pid) = self.pid {
            use nix::sys::signal::{killpg, Signal};
            use nix::unistd::Pid;
            let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGHUP);
            let me = self.clone();
            std::thread::spawn(move || {
                let end = Instant::now() + CLOSE_GRACE;
                while Instant::now() < end {
                    if !me.running.load(Ordering::SeqCst) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(20));
                }
                me.kill_session();
            });
            return;
        }
        #[cfg(windows)]
        if let Some(pid) = self.pid {
            // ConPTY: killing the shell leaves what it started; end the whole tree (AUD-101).
            let killed = std::process::Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/T", "/F"])
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .status()
                .is_ok_and(|s| s.success());
            if killed {
                return;
            }
        }
        let _ = lock(&self.killer).kill();
    }

    /// SIGKILL to the shell's process group and, on Linux, to every other group of its session
    /// (jobs an interactive shell put in groups of their own).
    #[cfg(unix)]
    fn kill_session(&self) {
        use nix::sys::signal::{killpg, Signal};
        use nix::unistd::Pid;
        let Some(pid) = self.pid else { return };
        let _ = killpg(Pid::from_raw(pid as i32), Signal::SIGKILL);
        for g in session_groups(pid) {
            let _ = killpg(Pid::from_raw(g as i32), Signal::SIGKILL);
        }
    }
}

/// The process groups of the session `sid` (Linux: from `/proc`; elsewhere none known).
#[cfg(target_os = "linux")]
fn session_groups(sid: u32) -> Vec<u32> {
    let mut out = Vec::new();
    if let Ok(dir) = std::fs::read_dir("/proc") {
        for e in dir.flatten() {
            let Ok(pid) = e.file_name().to_string_lossy().parse::<u32>() else {
                continue;
            };
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{pid}/stat")) else {
                continue;
            };
            // After the command's closing parenthesis: state ppid pgrp session …
            let Some(rest) = stat.rsplit_once(')').map(|(_, r)| r) else {
                continue;
            };
            let f: Vec<&str> = rest.split_whitespace().collect();
            if f.get(3).and_then(|s| s.parse::<u32>().ok()) == Some(sid) {
                if let Some(g) = f.get(2).and_then(|s| s.parse::<u32>().ok()) {
                    if !out.contains(&g) {
                        out.push(g);
                    }
                }
            }
        }
    }
    out
}

#[cfg(all(unix, not(target_os = "linux")))]
fn session_groups(_sid: u32) -> Vec<u32> {
    Vec::new()
}

/// Every terminal of the daemon.
pub struct Terminals {
    map: Mutex<HashMap<TerminalId, Arc<Terminal>>>,
    next: AtomicU64,
    events: tokio::sync::broadcast::Sender<TermEvent>,
}

impl Default for Terminals {
    fn default() -> Self {
        Self {
            map: Mutex::new(HashMap::new()),
            next: AtomicU64::new(1),
            events: tokio::sync::broadcast::channel(4096).0,
        }
    }
}

/// The person's login shell: `$SHELL`, else `/bin/sh` (`-l` on macOS, as Terminal.app starts it).
#[cfg(not(windows))]
pub fn default_shell(env: &BTreeMap<String, String>) -> Vec<String> {
    let shell = env
        .get("SHELL")
        .filter(|s| !s.is_empty())
        .cloned()
        .unwrap_or_else(|| "/bin/sh".into());
    if cfg!(target_os = "macos") {
        vec![shell, "-l".into()]
    } else {
        vec![shell]
    }
}

/// The person's shell on Windows: `%COMSPEC%` (cmd.exe). Windows support is deferred.
#[cfg(windows)]
pub fn default_shell(_env: &BTreeMap<String, String>) -> Vec<String> {
    vec![std::env::var("COMSPEC").unwrap_or_else(|_| "cmd.exe".into())]
}

impl Terminals {
    pub fn subscribe(&self) -> tokio::sync::broadcast::Receiver<TermEvent> {
        self.events.subscribe()
    }

    pub fn get(&self, id: &TerminalId) -> Result<Arc<Terminal>, TermError> {
        lock(&self.map)
            .get(id)
            .cloned()
            .ok_or_else(|| TermError::NotFound(id.0.clone()))
    }

    pub fn of_workspace(&self, ws: &WorkspaceId) -> Vec<Arc<Terminal>> {
        let mut v: Vec<_> = lock(&self.map)
            .values()
            .filter(|t| &t.workspace_id == ws)
            .cloned()
            .collect();
        v.sort_by_key(|t| t.id.0.trim_start_matches('t').parse::<u64>().unwrap_or(0));
        v
    }

    /// Closes and forgets the workspace's terminals.
    pub fn close_workspace(&self, ws: &WorkspaceId) {
        let gone: Vec<_> = {
            let mut m = lock(&self.map);
            let ids: Vec<_> = m
                .values()
                .filter(|t| &t.workspace_id == ws)
                .map(|t| t.id.clone())
                .collect();
            ids.into_iter().filter_map(|id| m.remove(&id)).collect()
        };
        for t in gone {
            t.close();
        }
    }

    pub fn close_all(&self) {
        let all: Vec<_> = lock(&self.map).drain().map(|(_, t)| t).collect();
        for t in all {
            t.close();
        }
    }

    /// Closes every terminal and waits (blocking, at most the grace period and a little more)
    /// until they have ended, killing what ignores the hang-up: for the daemon's shutdown, which
    /// must not exit before the escalation has run (AUD-027).
    pub fn close_all_and_wait(&self) {
        let all: Vec<_> = lock(&self.map).drain().map(|(_, t)| t).collect();
        for t in &all {
            t.close();
        }
        let end = Instant::now() + CLOSE_GRACE + Duration::from_millis(500);
        while Instant::now() < end && all.iter().any(|t| t.is_running()) {
            std::thread::sleep(Duration::from_millis(20));
        }
        #[cfg(unix)]
        for t in all.iter().filter(|t| t.is_running()) {
            t.kill_session();
        }
    }

    /// Closes one terminal and forgets it.
    pub fn close(&self, id: &TerminalId) -> Result<(), TermError> {
        let t = lock(&self.map)
            .remove(id)
            .ok_or_else(|| TermError::NotFound(id.0.clone()))?;
        t.close();
        Ok(())
    }

    pub fn create(&self, spec: Spec) -> Result<Arc<Terminal>, TermError> {
        if spec.cols == 0 || spec.rows == 0 {
            return Err(TermError::Invalid(
                "A terminal needs at least one row and one column.".into(),
            ));
        }
        let argv = match &spec.argv {
            Some(a) if !a.is_empty() => a.clone(),
            Some(_) => return Err(TermError::Invalid("argv is empty.".into())),
            None => default_shell(&spec.env),
        };
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: spec.rows,
                cols: spec.cols,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(|e| TermError::Spawn(e.to_string()))?;
        let mut cmd = CommandBuilder::new(&argv[0]);
        cmd.args(&argv[1..]);
        cmd.cwd(&spec.cwd);
        cmd.env_clear();
        for (k, v) in &spec.env {
            cmd.env(k, v);
        }
        cmd.env("TERM", "xterm-256color");
        cmd.env("COLORTERM", "truecolor");
        cmd.env("TERM_PROGRAM", "Axiom");
        let mut child = pair
            .slave
            .spawn_command(cmd)
            .map_err(|e| TermError::Spawn(format!("{}: {e}", argv[0])))?;
        drop(pair.slave);
        let reader = pair
            .master
            .try_clone_reader()
            .map_err(|e| TermError::Spawn(e.to_string()))?;
        let writer = pair
            .master
            .take_writer()
            .map_err(|e| TermError::Spawn(e.to_string()))?;
        let id = TerminalId(format!("t{}", self.next.fetch_add(1, Ordering::SeqCst)));
        let title = std::path::Path::new(&argv[0])
            .file_name()
            .map_or_else(|| argv[0].clone(), |n| n.to_string_lossy().into_owned());
        let t = Arc::new(Terminal {
            id: id.clone(),
            workspace_id: spec.workspace_id.clone(),
            title,
            pid: child.process_id(),
            master: Mutex::new(pair.master),
            writer: Mutex::new(writer),
            killer: Mutex::new(child.clone_killer()),
            backlog: Mutex::new(Backlog::default()),
            running: AtomicBool::new(true),
            exit_code: Mutex::new(None),
            budget: Arc::default(),
        });
        lock(&self.map).insert(id, t.clone());

        // Reader → (bounded: the shell waits when output piles up) → coalescer.
        let (tx, rx) = sync_channel::<Vec<u8>>(64);
        std::thread::Builder::new()
            .name(format!("pty-read-{}", t.id.0))
            .spawn(move || {
                let mut r = reader;
                let mut buf = vec![0u8; 64 * 1024];
                loop {
                    match r.read(&mut buf) {
                        Ok(0) | Err(_) => break,
                        Ok(n) => {
                            if tx.send(buf[..n].to_vec()).is_err() {
                                break;
                            }
                        }
                    }
                }
            })?;
        let events = self.events.clone();
        let term = t.clone();
        std::thread::Builder::new()
            .name(format!("pty-out-{}", t.id.0))
            .spawn(move || {
                let mut decoder = Utf8Stream::default();
                let mut pending = String::new();
                let mut last = Instant::now() - COALESCE;
                let mut open = true;
                while open || !pending.is_empty() {
                    let wait = if pending.is_empty() {
                        Duration::from_secs(3600)
                    } else {
                        COALESCE.saturating_sub(last.elapsed())
                    };
                    if open && pending.len() < EVENT_MAX_BYTES {
                        match rx.recv_timeout(wait) {
                            Ok(bytes) => pending.push_str(&decoder.push(&bytes)),
                            Err(RecvTimeoutError::Timeout) => {}
                            Err(RecvTimeoutError::Disconnected) => {
                                open = false;
                                pending.push_str(&decoder.push(&[]));
                            }
                        }
                    }
                    let full = pending.len() >= EVENT_MAX_BYTES;
                    if !pending.is_empty() && (full || last.elapsed() >= COALESCE || !open) {
                        let data = take_prefix(&mut pending, EVENT_MAX_BYTES);
                        let seq = {
                            let mut b = lock(&term.backlog);
                            b.push(&data);
                            b.seq += 1;
                            b.seq
                        };
                        // Waiting here stops reading the PTY: the program waits too.
                        if let Some(permit) = term.budget.take(data.len()) {
                            let _ = events.send(TermEvent::Output {
                                workspace_id: term.workspace_id.clone(),
                                terminal_id: term.id.clone(),
                                seq,
                                data,
                                permit: Arc::new(permit),
                            });
                        }
                        last = Instant::now();
                    }
                }
                // The output has ended: wait for the process and report it.
                // A process ended by a signal has no exit code.
                let code = child
                    .wait()
                    .ok()
                    .filter(|s| s.signal().is_none())
                    .map(|s| s.exit_code() as i32);
                *lock(&term.exit_code) = code;
                term.running.store(false, Ordering::SeqCst);
                let _ = events.send(TermEvent::Exited {
                    workspace_id: term.workspace_id.clone(),
                    terminal_id: term.id.clone(),
                    code,
                    pid: term.pid,
                });
            })?;
        Ok(t)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utf8_split_across_reads_and_invalid_bytes() {
        let mut d = Utf8Stream::default();
        let pi = "π😀".as_bytes();
        assert_eq!(d.push(&pi[..1]), "");
        assert_eq!(d.push(&pi[1..4]), "π");
        assert_eq!(d.push(&pi[4..]), "😀");
        assert_eq!(d.push(b"a\xffb"), "a\u{FFFD}b");
    }

    #[test]
    fn prefixes_are_cut_at_character_boundaries() {
        let mut s = "aé".repeat(3);
        assert_eq!(take_prefix(&mut s, 2), "a");
        assert_eq!(s, "éaéaé");
        assert_eq!(take_prefix(&mut s, 100), "éaéaé");
        assert!(s.is_empty());
    }

    #[test]
    fn the_budget_waits_then_gives_up_and_is_released_on_drop() {
        let b: Arc<Budget> = Arc::default();
        let big = b.take(OUTPUT_BUDGET_BYTES).unwrap();
        // One event larger than the budget passes when nothing else is held.
        drop(big);
        let a = b.take(OUTPUT_BUDGET_BYTES - 10).unwrap();
        let t = Instant::now();
        assert!(b.take(100).is_none());
        assert!(t.elapsed() >= BUDGET_WAIT);
        // Released by another thread while waiting: the wait ends early.
        let b2 = b.clone();
        let h = std::thread::spawn(move || b2.take(100).is_some());
        std::thread::sleep(Duration::from_millis(50));
        drop(a);
        assert!(h.join().unwrap());
        assert_eq!(*lock(&b.held), 0);
    }

    #[test]
    fn backlog_is_bounded_at_a_character_boundary() {
        let mut b = Backlog::default();
        b.push(&"é".repeat(BACKLOG_BYTES));
        assert!(b.text.len() <= BACKLOG_BYTES);
        assert!(b.text.starts_with('é'));
    }
}
