//! A person's own terminals (decision 0022): their login shell on a pseudo-terminal, in a session's
//! folder, outside the sandbox as their own act. The screen is kept here by an emulator; a client is
//! sent text and colours only (T42). They live as long as the host, or until their person closes them.

use std::{
    collections::BTreeMap,
    io::{Read, Write},
    num::NonZeroU16,
    sync::{Arc, Mutex, MutexGuard, PoisonError},
    time::Duration,
};

use axiom_policy::{PersonsOwn, Place, Unsandboxed};
use axiom_proto::{ErrorCode, Screen, SessionId, Span, TerminalInfo, WireError};
use axiom_spawn::{Contain, Env, Job};
use tokio::sync::watch;

use crate::agents::bad;

/// At most, open at once.
pub const MOST: usize = 16;
/// Lines kept to scroll back through.
const BACK: usize = 5000;
/// Of a title.
const TITLE: usize = 200;
/// A screen is answered after this at the latest, changed or not.
const WAIT: Duration = Duration::from_secs(10);
/// Of what is typed at once.
const INPUT: usize = 64 * 1024;
const SIZE: std::ops::RangeInclusive<u16> = 2..=1000;

#[derive(Default)]
struct Said {
    title: String,
}

impl vt100::Callbacks for Said {
    fn set_window_title(&mut self, _: &mut vt100::Screen, title: &[u8]) {
        let t: String = String::from_utf8_lossy(title)
            .chars()
            .filter(|c| !c.is_control())
            .take(TITLE)
            .collect();
        self.title = t;
    }
}

struct Term {
    session: SessionId,
    shell: u32,
    /// Told, it ends with all it runs.
    stop: watch::Sender<bool>,
    pty: axiom_spawn::Pty,
    input: Mutex<std::fs::File>,
    screen: Mutex<vt100::Parser<Said>>,
    version: watch::Sender<u64>,
    ended: Mutex<Option<String>>,
}

#[derive(Default)]
pub struct Terminals {
    all: Mutex<BTreeMap<String, Arc<Term>>>,
}

fn lock<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(PoisonError::into_inner)
}

fn none() -> WireError {
    WireError::new(ErrorCode::NotFound, "no such terminal")
}

/// Where a person's shell is: theirs if it is one of the system's, the plain one otherwise.
fn shell() -> Result<Place, WireError> {
    let ok = |p: &str| {
        ["/bin/", "/usr/bin/", "/usr/local/bin/", "/opt/homebrew/bin/"]
            .iter()
            .any(|d| p.starts_with(d))
            && !p.contains("..")
            && std::path::Path::new(p).is_file()
    };
    let theirs = std::env::var("SHELL").ok().filter(|s| ok(s));
    Place::new(theirs.as_deref().unwrap_or("/bin/sh")).map_err(|e| bad(e.to_string()))
}

fn size(rows: u16, cols: u16) -> Result<(NonZeroU16, NonZeroU16), WireError> {
    let n = |v: u16| NonZeroU16::new(v).filter(|_| SIZE.contains(&v));
    n(rows).zip(n(cols)).ok_or_else(|| bad("a terminal is 2 to 1000 columns and rows"))
}

fn color(c: vt100::Color) -> Option<String> {
    match c {
        vt100::Color::Default => None,
        vt100::Color::Idx(i) => Some(i.to_string()),
        vt100::Color::Rgb(r, g, b) => Some(format!("#{r:02x}{g:02x}{b:02x}")),
    }
}

/// The screen as rows of spans of one look each.
fn rows(s: &vt100::Screen) -> Vec<Vec<Span>> {
    let (rows, cols) = s.size();
    let (rows, cols) = (rows.get(), cols.get());
    let mut out = Vec::with_capacity(usize::from(rows));
    for r in 0..rows {
        let mut row: Vec<Span> = Vec::new();
        for c in 0..cols {
            let Some(cell) = s.cell(r, c) else { continue };
            if cell.is_wide_continuation() {
                continue;
            }
            let text = if cell.has_contents() { cell.contents() } else { " " };
            let style = u8::from(cell.bold())
                | u8::from(cell.dim()) << 1
                | u8::from(cell.italic()) << 2
                | u8::from(cell.underline()) << 3
                | u8::from(cell.inverse()) << 4;
            let (fg, bg) = (color(cell.fgcolor()), color(cell.bgcolor()));
            match row.last_mut() {
                Some(l) if l.fg == fg && l.bg == bg && l.style == style => l.text.push_str(text),
                _ => row.push(Span { text: text.to_owned(), fg, bg, style }),
            }
        }
        // What is blank at the end is not sent.
        if let Some(l) = row.last_mut()
            && l.bg.is_none()
            && l.style & 24 == 0
        {
            let t = l.text.trim_end().len();
            l.text.truncate(t);
            if l.text.is_empty() {
                row.pop();
            }
        }
        out.push(row);
    }
    out
}

impl Terminals {
    fn get(&self, id: &str) -> Result<Arc<Term>, WireError> {
        lock(&self.all).get(id).cloned().ok_or_else(none)
    }

    pub fn list(&self) -> Vec<TerminalInfo> {
        lock(&self.all)
            .iter()
            .map(|(id, t)| TerminalInfo {
                terminal: id.clone(),
                session: t.session.clone(),
                title: t.title(),
                busy: t.busy(),
                ended: lock(&t.ended).clone(),
            })
            .collect()
    }

    /// Opens one in `folder`, real and the session's own. `path`, `home`, `user`, `tmp`: as programs
    /// are started.
    pub fn open(
        &self,
        session: SessionId,
        folder: Place,
        env: Env,
        size: (u16, u16),
        command: Option<String>,
        program: Option<Place>,
    ) -> Result<String, WireError> {
        let (cols, rows) = size;
        let (r, c) = self::size(rows, cols)?;
        if command.as_ref().is_some_and(|c| c.len() > INPUT || c.contains('\0')) {
            return Err(bad("that command cannot be typed"));
        }
        {
            let mut all = lock(&self.all);
            // Those that have ended give their room up.
            all.retain(|_, t| lock(&t.ended).is_none());
            if all.len() >= MOST {
                return Err(bad(format!("there are {MOST} terminals; close one")));
            }
        }
        let job = Job {
            program: match program {
                Some(p) => p,
                None => shell()?,
            },
            args: vec!["-l".into()],
            cwd: folder,
            env,
            stdin: true,
            contain: Contain::None(Unsandboxed::persons(PersonsOwn::Terminal)),
        };
        let (mut running, pty) = axiom_spawn::spawn_terminal(job, cols, rows)
            .map_err(|e| bad(format!("the terminal could not be opened: {e}")))?;
        let shell = running.id().unwrap_or(0);
        let mut reader = pty.reader().map_err(|e| bad(e.to_string()))?;
        let input = pty.writer().map_err(|e| bad(e.to_string()))?;
        let (stop, mut stopped) = watch::channel(false);
        let t = Arc::new(Term {
            session,
            shell,
            stop,
            pty,
            input: Mutex::new(input),
            screen: Mutex::new(vt100::Parser::new_with_callbacks(r, c, BACK, Said::default())),
            version: watch::Sender::new(1),
            ended: Mutex::new(None),
        });
        let id = format!("t_{shell}_{}", axiom_proto::now_ms());
        lock(&self.all).insert(id.clone(), t.clone());
        // What it writes is read on a thread of its own, until it is closed.
        let r = t.clone();
        std::thread::spawn(move || {
            let mut b = vec![0; 64 * 1024];
            loop {
                match reader.read(&mut b) {
                    Ok(0) | Err(_) => break,
                    Ok(n) => {
                        lock(&r.screen).process(&b[..n]);
                        r.version.send_modify(|v| *v += 1);
                    }
                }
            }
        });
        let w = t.clone();
        tokio::spawn(async move {
            let ended = tokio::select! {
                e = running.wait() => Some(e.ok()),
                _ = stopped.wait_for(|s| *s) => None,
            };
            let ended = match ended {
                Some(e) => e,
                None => running.stop(Duration::from_secs(2)).await.ok(),
            };
            let words = match ended {
                Some(e) => match (e.code, e.signal) {
                    (Some(c), _) => format!("ended: exit {c}"),
                    (_, Some(s)) => format!("ended by signal {s}"),
                    _ => "ended".into(),
                },
                None => "ended".into(),
            };
            *lock(&w.ended) = Some(words);
            w.version.send_modify(|v| *v += 1);
        });
        if let Some(c) = command {
            t.type_in(&format!("{c}\r"))?;
        }
        Ok(id)
    }

    pub fn input(&self, id: &str, text: &str) -> Result<(), WireError> {
        if text.len() > INPUT {
            return Err(bad("that is too much to type at once"));
        }
        self.get(id)?.type_in(text)
    }

    pub fn resize(&self, id: &str, cols: u16, rows: u16) -> Result<(), WireError> {
        let (r, c) = size(rows, cols)?;
        let t = self.get(id)?;
        lock(&t.screen).set_size(r, c);
        t.pty.resize(cols, rows).map_err(|e| bad(e.to_string()))?;
        t.version.send_modify(|v| *v += 1);
        Ok(())
    }

    pub fn scroll(&self, id: &str, back: u32) -> Result<(), WireError> {
        let t = self.get(id)?;
        lock(&t.screen).screen_mut().set_scrollback(usize::try_from(back).unwrap_or(usize::MAX));
        t.version.send_modify(|v| *v += 1);
        Ok(())
    }

    /// The screen once it is past `seen`, or after a while as it is.
    pub async fn screen(&self, id: &str, seen: u64) -> Result<Screen, WireError> {
        let t = self.get(id)?;
        let mut v = t.version.subscribe();
        let _ = tokio::time::timeout(WAIT, v.wait_for(|v| *v > seen)).await;
        Ok(t.shown(id))
    }

    pub async fn close(&self, id: &str, force: bool) -> Result<(), WireError> {
        let t = self.get(id)?;
        if t.busy() && !force && lock(&t.ended).is_none() {
            return Err(WireError::new(ErrorCode::Busy, format!("{} still runs in it", t.title())));
        }
        let _ = t.stop.send(true);
        lock(&self.all).remove(id);
        Ok(())
    }

    /// All of them end, with the host.
    pub fn end(&self) {
        let all = std::mem::take(&mut *lock(&self.all));
        for t in all.values() {
            let _ = t.stop.send(true);
        }
    }
}

impl Term {
    fn type_in(&self, text: &str) -> Result<(), WireError> {
        if lock(&self.ended).is_some() {
            return Err(bad("the terminal has ended"));
        }
        let mut w = lock(&self.input);
        w.write_all(text.as_bytes()).and_then(|()| w.flush()).map_err(|e| bad(e.to_string()))
    }

    fn busy(&self) -> bool {
        self.pty.foreground().is_some_and(|g| g != self.shell)
    }

    fn title(&self) -> String {
        let said = lock(&self.screen).callbacks().title.clone();
        if !said.is_empty() {
            return said;
        }
        let front = self.pty.foreground().unwrap_or(self.shell);
        axiom_spawn::usage(front).map(|u| u.name).unwrap_or_else(|| "Terminal".into())
    }

    fn shown(&self, id: &str) -> Screen {
        let version = *self.version.borrow();
        let p = lock(&self.screen);
        let s = p.screen();
        let back = s.scrollback();
        let (r, c) = s.cursor_position();
        let ended = lock(&self.ended).clone();
        Screen {
            terminal: id.to_owned(),
            version,
            cols: s.size().1.get(),
            rows: rows(s),
            cursor: (back == 0 && !s.hide_cursor() && ended.is_none()).then_some((r, c)),
            title: if p.callbacks().title.is_empty() {
                String::new()
            } else {
                p.callbacks().title.clone()
            },
            back: u32::try_from(back).unwrap_or(u32::MAX),
            most_back: u32::try_from(s.scrollback_len()).unwrap_or(u32::MAX),
            app_cursor: s.application_cursor(),
            bracketed: s.bracketed_paste(),
            ended,
        }
    }
}
