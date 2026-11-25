//! Commands. Contained by the profile they were judged with; their output is kept at both ends for
//! the model and whole, up to a size, in a file.

use std::time::{Duration, Instant};

use axiom_policy::{Place, Profile};
use axiom_spawn::{Contain, Env, Job, spawn};
use serde_json::json;
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    sync::mpsc,
};

use crate::{
    Output,
    text::{Ends, Lines},
};

#[derive(Debug)]
pub struct Command {
    pub command: String,
    pub cwd: Place,
    pub profile: Profile,
    pub limit: Duration,
    pub home: Place,
    pub path: Vec<Place>,
    pub scratch: Place,
}

const HEAD: usize = 12 * 1024;
const TAIL: usize = 36 * 1024;
/// Kept in a file at most.
const SPOOL: u64 = 32 * 1024 * 1024;

fn bash() -> &'static str {
    if std::path::Path::new("/bin/bash").exists() { "/bin/bash" } else { "/usr/bin/bash" }
}

impl Command {
    pub async fn run(self, heard: mpsc::Sender<String>) -> Output {
        let program = match Place::new(bash()) {
            Ok(p) => p,
            Err(e) => return Output::bad(e),
        };
        let mut env = Env::new(&self.path, &self.home, &self.scratch);
        if let Some(user) =
            self.home.name().chars().all(|c| c.is_ascii_graphic()).then(|| self.home.name())
        {
            let _ = env.set("USER", user);
            let _ = env.set("LOGNAME", user);
        }
        let job = Job {
            program,
            args: ["--noprofile", "--norc", "-c", &self.command].map(String::from).into(),
            cwd: self.cwd,
            env,
            stdin: false,
            contain: Contain::Sandbox(self.profile),
        };
        let began = Instant::now();
        let mut p = match spawn(job) {
            Ok(p) => p,
            Err(e) => return Output::bad(format!("not run: {e}")),
        };
        let (Some(mut out), Some(mut err)) = (p.stdout.take(), p.stderr.take()) else {
            return Output::bad("not run: it has no output");
        };
        let mut ends = Ends::new(HEAD, TAIL);
        let mut lines = Lines::default();
        let mut spool = Spool::new(&self.scratch).await;
        let (mut a, mut b) = (vec![0; 16 * 1024], vec![0; 16 * 1024]);
        let (mut out_open, mut err_open) = (true, true);
        let mut look = tokio::time::interval(Duration::from_millis(250));
        let end = tokio::time::sleep(self.limit);
        tokio::pin!(end);
        let mut timed_out = false;
        // Until the process has ended and both pipes are read: what it left running could hold them
        // open, and is ended with it.
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
                () = &mut end, if !timed_out => {
                    timed_out = true;
                    p.kill();
                    None
                }
            };
            if let Some(bytes) = n.filter(|b| !b.is_empty()) {
                ends.add(bytes);
                let l = lines.add(bytes);
                spool.add(bytes).await;
                // Who shows it may be behind: then the command waits. Who has left does not count.
                if !l.is_empty() {
                    let _ = heard.send(l).await;
                }
            }
        }
        let took = began.elapsed();
        let e = match ended {
            Some(Ok(e)) => e,
            Some(Err(e)) => return Output::bad(format!("its end is not known: {e}")),
            None => return Output::bad("its end is not known"),
        };
        let mut text = ends.text();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        let blocked = e.code != Some(0)
            && [
                "Operation not permitted",
                "Read-only file system",
                "Could not resolve host",
                "Permission denied",
            ]
            .iter()
            .any(|w| text.contains(w));
        let mut notes = String::new();
        match spool.file() {
            Some(f) if ends.cut() > 0 => notes += &format!("(all of it: {f})\n"),
            Some(f) => {
                let _ = tokio::fs::remove_file(f).await;
            }
            None => {}
        }
        for m in &e.made {
            notes += &format!(
                "(it made {}, which other programs take orders from: a person has been told)\n",
                m.display()
            );
        }
        match (timed_out, e.code, e.signal) {
            (true, ..) => {
                notes += &format!("(ended after {} s: it took too long)\n", self.limit.as_secs())
            }
            (_, Some(0), _) => {}
            (_, Some(c), _) => notes += &format!("(exit {c})\n"),
            (_, None, Some(s)) => notes += &format!("(ended by signal {s})\n"),
            _ => notes += "(ended)\n",
        }
        if blocked {
            text +=
                "(the sandbox may have prevented this: no network, writing only where allowed)\n";
        }
        if text.is_empty() && notes.is_empty() {
            notes.push_str("(no output)\n");
        }
        let mut rest = lines.end();
        if !rest.is_empty() && !rest.ends_with('\n') {
            rest.push('\n');
        }
        rest += &notes;
        text += &notes;
        Output {
            ok: e.code == Some(0) && !timed_out,
            text,
            rest: Some(rest),
            detail: json!({
                "exit": e.code, "signal": e.signal, "timed_out": timed_out, "left": e.left,
                "made": e.made.iter().map(|m| m.display().to_string()).collect::<Vec<_>>(),
                "took": u64::try_from(took.as_millis()).unwrap_or(u64::MAX), "bytes": ends.total,
            }),
        }
    }
}

/// The whole output, in a file of the scratch folder that did not exist before.
struct Spool {
    file: Option<(tokio::fs::File, std::path::PathBuf)>,
    left: u64,
}

impl Spool {
    async fn new(scratch: &Place) -> Self {
        let mut b = [0u8; 6];
        let _ = axiom_proto::random(&mut b);
        let name: String = b.iter().map(|x| format!("{x:02x}")).collect();
        let path = scratch.as_path().join(format!("output-{name}.txt"));
        let f = tokio::fs::OpenOptions::new().write(true).create_new(true).open(&path).await;
        Self { file: f.ok().map(|f| (f, path)), left: SPOOL }
    }

    async fn add(&mut self, b: &[u8]) {
        let n = usize::try_from(self.left).unwrap_or(usize::MAX).min(b.len());
        if let Some((f, _)) = &mut self.file
            && n > 0
        {
            if f.write_all(&b[..n]).await.is_err() {
                self.file = None;
            }
            self.left -= n as u64;
        }
    }

    fn file(&self) -> Option<String> {
        self.file.as_ref().map(|(_, p)| p.display().to_string())
    }
}
