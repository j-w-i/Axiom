//! What an agent can do itself. Each call is first planned: understood, worked out, judged. What was
//! judged is what is then done (`Step`), and nothing else: changes are made through the handles and
//! to the contents that were looked at, commands run as they were shown.

use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{Arc, Mutex, atomic::AtomicBool},
    time::Duration,
};

use axiom_policy::{
    Access, Action, Ask, Caps, Decision, Keep, Place, Policy, Profile, Reach, Verdict, rules::RunIn,
};
use axiom_proto::AgentId;
use serde::Deserialize;
use serde_json::{Value, json};
use tokio::sync::mpsc;

pub mod edit;
pub mod fs;
pub mod shell;
pub mod text;
pub mod walk;

use fs::{Content, FsError, Kind, Spot};

/// Where an agent works and what it may. From the session's record, never from the model (SI-10).
/// Every place in it is real: without links.
#[derive(Debug, Clone)]
pub struct Site {
    pub workspace: Place,
    pub caps: Caps,
    pub keep: Keep,
    pub access: Access,
    pub policy: Policy,
    /// What every program reads.
    pub system: Reach,
    /// This agent's own temporary folder.
    pub scratch: Place,
    /// Where programs are found.
    pub path: Vec<Place>,
    pub chain: Vec<AgentId>,
}

impl Site {
    fn program(&self, name: &str) -> Option<Place> {
        if name.contains('/') {
            return None;
        }
        self.path.iter().filter_map(|d| d.join(name).ok()).find(|p| p.as_path().is_file())
    }

    fn judge(&self, a: Action<'_>) -> Verdict {
        self.policy
            .decide(a, self.access, &self.caps, &self.keep, &self.workspace, &|n| self.program(n))
    }

    /// As the model and the person see it.
    fn shown(&self, p: &Place) -> String {
        match p.as_path().strip_prefix(self.workspace.as_path()) {
            Ok(r) if r.as_os_str().is_empty() => ".".into(),
            Ok(r) => r.display().to_string(),
            Err(_) => p.to_string(),
        }
    }
}

/// What this agent has read, so that it changes only what it knows.
#[derive(Debug, Clone, Default)]
pub struct Seen(Arc<Mutex<BTreeMap<Place, String>>>);

impl Seen {
    fn add(&self, p: &Place, digest: &str) {
        if let Ok(mut m) = self.0.lock() {
            m.insert(p.clone(), digest.to_owned());
        }
    }
    /// Everything has to be read again.
    pub fn forget(&self) {
        if let Ok(mut m) = self.0.lock() {
            m.clear();
        }
    }
    fn digest(&self, p: &Place) -> Option<String> {
        self.0.lock().ok()?.get(p).cloned()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

fn spec(name: &'static str, description: &'static str, props: Value, required: &[&str]) -> Spec {
    let schema = json!({"type": "object", "properties": props, "required": required, "additionalProperties": false});
    Spec { name, description, schema }
}

/// All tools, as they are offered to a model.
pub fn specs() -> Vec<Spec> {
    let s = |d: &str| json!({"type": "string", "description": d});
    let n = |d: &str| json!({"type": "integer", "minimum": 1, "description": d});
    let b = |d: &str| json!({"type": "boolean", "description": d});
    let path = || s("Relative to the workspace, or absolute.");
    vec![
        spec(
            "read",
            "Reads a text file. Lines are numbered. Read a file before changing it.",
            json!({"path": path(), "from": n("First line (1)."), "lines": n("How many lines (2000).")}),
            &["path"],
        ),
        spec(
            "write",
            "Writes a whole file, making folders as needed. A file that exists must have been read.",
            json!({"path": path(), "content": s("The whole content.")}),
            &["path", "content"],
        ),
        spec(
            "edit",
            "Replaces text in a file that has been read. The old text must be in it exactly once, unless all is true.",
            json!({"path": path(), "old": s("Exact text, without line numbers."), "new": s("What replaces it."), "all": b("Replace every one.")}),
            &["path", "old", "new"],
        ),
        spec(
            "patch",
            "Changes several files at once. The patch is *** Begin Patch, then for each file *** Add File: path with lines beginning +, *** Delete File: path, or *** Update File: path (then optionally *** Move to: path) with changes: @@ and a line to look behind, lines beginning with a space (kept), - (removed), + (added); then *** End Patch.",
            json!({"patch": s("The whole patch.")}),
            &["patch"],
        ),
        spec("list", "Lists a folder.", json!({"path": path()}), &[]),
        spec(
            "glob",
            "Finds files by name: *.rs, src/**/*.ts. Ignored files are left out.",
            json!({"pattern": s("What names must fit."), "path": path()}),
            &["pattern"],
        ),
        spec(
            "search",
            "Finds lines that fit a regular expression. Ignored files are left out.",
            json!({"pattern": s("Regular expression."), "path": path(), "only": s("Only in files whose name fits, as *.rs."), "around": n("Lines shown around each.")}),
            &["pattern"],
        ),
        spec(
            "shell",
            "Runs a command with bash, in a sandbox without a network, and returns its output. It cannot ask anything. Use the other tools for files.",
            json!({"command": s("The command."), "cwd": path(), "seconds": n("Ended after this long (120, at most 600).")}),
            &["command"],
        ),
    ]
}

/// What identifies a tool's meaning: any change of it is another tool (SI-19).
pub fn schema_digest(s: &Spec) -> String {
    let all = json!([s.name, s.description, s.schema]);
    fs::digest(all.to_string().as_bytes())
}

/// What a call comes to.
#[derive(Debug)]
pub enum Plan {
    /// Not done, and why; told to the model.
    No(String),
    /// Done without asking.
    Go(Step, Verdict),
    /// Done if a person says yes to exactly this.
    Ask(Box<Ask>, Step, Verdict),
}

/// A call that was understood and judged. Made only by [`plan`].
#[derive(Debug)]
pub struct Step(Op);

#[derive(Debug)]
enum Op {
    Read { spot: Spot, from: usize, lines: usize },
    Change(Vec<Change>),
    List(Spot),
    Glob { spot: Spot, pattern: String },
    Search { spot: Spot, pattern: String, only: Option<String>, around: usize },
    Shell(shell::Command),
}

#[derive(Debug)]
struct Change {
    place: Place,
    shown: String,
    /// `None`: folders on the way are missing too.
    spot: Option<Spot>,
    before: Option<Content>,
    after: Option<String>,
}

/// What came of it.
#[derive(Debug, Clone, PartialEq)]
pub struct Output {
    pub ok: bool,
    /// For the model.
    pub text: String,
    /// For the person, after what was shown as it came; `None`: what the model gets.
    pub rest: Option<String>,
    /// For the person: `diff`, `exit`, `took`.
    pub detail: Value,
}

impl Output {
    fn ok(text: String) -> Self {
        Self { ok: true, text, rest: None, detail: Value::Null }
    }
    fn bad(text: impl ToString) -> Self {
        Self { ok: false, text: text.to_string(), rest: None, detail: Value::Null }
    }
}

/// Files larger than this are not read whole.
const LARGEST: u64 = 8 * 1024 * 1024;
/// Of a file, at a time.
const READ: usize = 64 * 1024;
const FOUND: usize = 200;

macro_rules! args {
    ($name:ident { $($f:ident : $t:ty $(= $d:expr)?),* $(,)? }) => {
        #[derive(Deserialize)]
        #[serde(deny_unknown_fields)]
        struct $name { $($(#[serde(default = $d)])? $f: $t),* }
    };
}
fn one() -> usize {
    1
}
fn two_thousand() -> usize {
    2000
}
fn two_minutes() -> u64 {
    120
}
fn here() -> String {
    ".".into()
}
fn no() -> bool {
    false
}
fn zero() -> usize {
    0
}
fn none() -> Option<String> {
    None
}
args!(ReadArgs { path: String, from: usize = "one", lines: usize = "two_thousand" });
args!(WriteArgs { path: String, content: String });
args!(EditArgs { path: String, old: String, new: String, all: bool = "no" });
args!(PatchArgs { patch: String });
args!(ListArgs { path: String = "here" });
args!(GlobArgs { pattern: String, path: String = "here" });
args!(SearchArgs { pattern: String, path: String = "here", only: Option<String> = "none", around: usize = "zero" });
args!(ShellArgs { command: String, cwd: String = "here", seconds: u64 = "two_minutes" });

fn args<T: serde::de::DeserializeOwned>(v: &Value) -> Result<T, String> {
    T::deserialize(v).map_err(|e| format!("the arguments cannot be used: {e}"))
}

/// Understands and judges a call. Looks at files; changes nothing, starts nothing.
pub fn plan(tool: &str, v: &Value, site: &Site, seen: &Seen) -> Plan {
    if !site.caps.tools.has(tool) {
        return Plan::No(format!("there is no tool {tool} for this agent"));
    }
    match understand(tool, v, site, seen) {
        Ok(p) => p,
        Err(e) => Plan::No(e),
    }
}

fn understand(tool: &str, v: &Value, site: &Site, seen: &Seen) -> Result<Plan, String> {
    let look = |path: &str| -> Result<(Spot, Verdict), String> {
        let spot = find(site, &site.caps.read, path)?;
        let verdict = site.judge(Action::Read(&spot.place));
        Ok((spot, verdict))
    };
    let read = |op: Op, verdict: Verdict, what: String, place: &Place| {
        let scope = format!("reads {}", site.shown(place));
        Ok(settle(tool, site, Step(op), verdict, what, scope))
    };
    match tool {
        "read" => {
            let a: ReadArgs = args(v)?;
            let (spot, verdict) = look(&a.path)?;
            let place = spot.place.clone();
            read(
                Op::Read { spot, from: a.from, lines: a.lines.min(10_000) },
                verdict,
                site.shown(&place),
                &place,
            )
        }
        "list" => {
            let a: ListArgs = args(v)?;
            let (spot, verdict) = look(&a.path)?;
            let place = spot.place.clone();
            read(Op::List(spot), verdict, site.shown(&place), &place)
        }
        "glob" => {
            let a: GlobArgs = args(v)?;
            let (spot, verdict) = look(&a.path)?;
            let place = spot.place.clone();
            let what = a.pattern.clone();
            read(Op::Glob { spot, pattern: a.pattern }, verdict, what, &place)
        }
        "search" => {
            let a: SearchArgs = args(v)?;
            let (spot, verdict) = look(&a.path)?;
            let place = spot.place.clone();
            let what = a.pattern.clone();
            read(
                Op::Search { spot, pattern: a.pattern, only: a.only, around: a.around },
                verdict,
                what,
                &place,
            )
        }
        "write" => {
            let a: WriteArgs = args(v)?;
            let c = change(site, seen, &a.path, |_| Ok(Some(a.content)), true)?;
            changes(tool, site, vec![c])
        }
        "edit" => {
            let a: EditArgs = args(v)?;
            let c = change(
                site,
                seen,
                &a.path,
                |t| {
                    let t = t.ok_or("the file does not exist")?;
                    edit::replace(t, &a.old, &a.new, a.all).map(Some).map_err(|e| e.to_string())
                },
                true,
            )?;
            changes(tool, site, vec![c])
        }
        "patch" => {
            let a: PatchArgs = args(v)?;
            let mut all = Vec::new();
            for f in edit::parse_patch(&a.patch).map_err(|e| e.to_string())? {
                patched(site, seen, f, &mut all)?;
            }
            changes(tool, site, all)
        }
        "shell" => {
            let a: ShellArgs = args(v)?;
            run(site, a)
        }
        _ => Err(format!("there is no tool {tool}")),
    }
}

/// The real place of `path` among what may be read or written.
fn find(site: &Site, reach: &Reach, path: &str) -> Result<Spot, String> {
    let place = site.workspace.join(path).map_err(|e| format!("{path}: {e}"))?;
    // Not even whether it exists.
    if site.keep.hides(&place) {
        return Err(format!("{path} is kept from agents"));
    }
    let spot = fs::find(reach.roots(), &place).map_err(|e| e.to_string())?;
    if site.keep.hides(&spot.place) {
        return Err(format!("{path} is kept from agents"));
    }
    Ok(spot)
}

/// Of `path`, what it holds and what it is to hold.
fn change(
    site: &Site,
    seen: &Seen,
    path: &str,
    f: impl FnOnce(Option<&str>) -> Result<Option<String>, String>,
    known: bool,
) -> Result<Change, String> {
    let place = site.workspace.join(path).map_err(|e| format!("{path}: {e}"))?;
    let spot = match fs::find(site.caps.write.roots(), &place) {
        Ok(s) => Some(s),
        // Folders to be made: only where no link is on the way can the place be known beforehand.
        Err(FsError::Missing(_)) => None,
        Err(FsError::Outside(_)) => {
            let v = site.judge(Action::Write(&place));
            return Err(format!(
                "{path}: {}",
                if v.decision == Decision::Deny { v.why } else { "it is outside".into() }
            ));
        }
        Err(e) => return Err(e.to_string()),
    };
    let place = match &spot {
        Some(s) => s.place.clone(),
        None => deep(site, &place)?,
    };
    let before = match &spot {
        Some(s) if s.kind == Kind::File => {
            // What may not be read is not read to be changed either.
            if site.judge(Action::Read(&place)).decision == Decision::Deny {
                return Err(format!("{path} may not be read"));
            }
            Some(s.read(LARGEST).map_err(|e| e.to_string())?)
        }
        Some(s) if s.kind != Kind::Missing => return Err(format!("{path} is not a plain file")),
        _ => None,
    };
    if let (Some(b), true) = (&before, known) {
        match seen.digest(&place) {
            None => return Err(format!("read {path} before changing it")),
            Some(d) if d != b.digest => {
                return Err(format!("{path} has changed since you read it; read it again"));
            }
            Some(_) => {}
        }
    }
    let text = match &before {
        Some(b) => {
            Some(String::from_utf8(b.bytes.clone()).map_err(|_| format!("{path} is not text"))?)
        }
        None => None,
    };
    let after = f(text.as_deref())?;
    if after.as_ref().is_some_and(|a| a.len() as u64 > LARGEST) {
        return Err(format!("{path} would be too large"));
    }
    Ok(Change { shown: site.shown(&place), place, spot, before, after })
}

/// Where a name in folders that are still to be made will be: below the last that exists.
fn deep(site: &Site, place: &Place) -> Result<Place, String> {
    let mut up = place.parent();
    let mut rest = vec![place.name().to_owned()];
    while let Some(p) = up {
        match fs::find(site.caps.write.roots(), &p) {
            Ok(s) if s.kind == Kind::Folder => {
                rest.reverse();
                return s.place.join(&rest.join("/")).map_err(|e| e.to_string());
            }
            Ok(s) if s.kind != Kind::Missing => return Err(format!("{p} is not a folder")),
            Ok(_) | Err(FsError::Missing(_)) => {
                rest.push(p.name().to_owned());
                up = p.parent();
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    Err(format!("{place} is outside what this agent may write"))
}

fn patched(
    site: &Site,
    seen: &Seen,
    f: edit::FilePatch,
    all: &mut Vec<Change>,
) -> Result<(), String> {
    use edit::FilePatch as F;
    let mut add = |c: Change| {
        if all.iter().any(|x| x.place == c.place) {
            return Err(format!("{} is in the patch twice", c.shown));
        }
        all.push(c);
        Ok(())
    };
    match f {
        F::Add { path, text } => add(change(
            site,
            seen,
            &path,
            |t| match t {
                Some(_) => Err(format!("{path} exists already")),
                None => Ok(Some(text)),
            },
            false,
        )?),
        F::Remove { path } => add(change(
            site,
            seen,
            &path,
            |t| {
                t.ok_or(format!("{path} does not exist"))?;
                Ok(None)
            },
            true,
        )?),
        // What a hunk changes is named by the hunk itself: it need not have been read.
        F::Update { path, to: None, hunks } => add(change(
            site,
            seen,
            &path,
            |t| {
                let t = t.ok_or(format!("{path} does not exist"))?;
                edit::apply(t, &hunks, &path).map(Some).map_err(|e| e.to_string())
            },
            false,
        )?),
        F::Update { path, to: Some(to), hunks } => {
            let mut text = String::new();
            let gone = change(
                site,
                seen,
                &path,
                |t| {
                    let t = t.ok_or(format!("{path} does not exist"))?;
                    text = edit::apply(t, &hunks, &path).map_err(|e| e.to_string())?;
                    Ok(None)
                },
                false,
            )?;
            let new = change(
                site,
                seen,
                &to,
                |t| match t {
                    Some(_) => Err(format!("{to} exists already")),
                    None => Ok(Some(text)),
                },
                false,
            )?;
            add(new)?;
            add(gone)
        }
    }
}

/// Judges each file; the strictest decides for all.
fn changes(tool: &str, site: &Site, all: Vec<Change>) -> Result<Plan, String> {
    let mut worst: Option<Verdict> = None;
    for c in &all {
        let v = site.judge(Action::Write(&c.place));
        if v.decision == Decision::Deny {
            return Err(format!("{}: {}", c.shown, v.why));
        }
        if worst.as_ref().is_none_or(|w| v.decision > w.decision) {
            worst = Some(v);
        }
    }
    let Some(verdict) = worst else { return Err("nothing is changed".into()) };
    let subject: String = all.iter().map(diff).collect();
    let names: Vec<&str> = all.iter().map(|c| c.shown.as_str()).collect();
    let scope = format!("changes {}", names.join(", "));
    Ok(settle(tool, site, Step(Op::Change(all)), verdict, subject, scope))
}

fn diff(c: &Change) -> String {
    let before = c.before.as_ref().map(|b| String::from_utf8_lossy(&b.bytes).into_owned());
    text::diff(&c.shown, before.as_deref(), c.after.as_deref())
}

fn run(site: &Site, a: ShellArgs) -> Result<Plan, String> {
    if a.command.trim().is_empty() {
        return Err("the command is empty".into());
    }
    let spot = find(site, &site.caps.read, &a.cwd)?;
    if spot.kind != Kind::Folder {
        return Err(format!("{} is not a folder", a.cwd));
    }
    let cwd = spot.place;
    let verdict = site.judge(Action::Run { command: &a.command, cwd: &cwd });
    if verdict.decision == Decision::Deny {
        return Err(verdict.why);
    }
    let make = if verdict.run_in == RunIn::ReadOnly { Profile::read_only } else { Profile::of };
    let profile =
        make(&site.caps, &site.keep, &site.system, &site.scratch).map_err(|e| e.to_string())?;
    let writes: Vec<String> = profile
        .write()
        .roots()
        .iter()
        .filter(|p| **p != site.scratch)
        .map(|p| if *p == site.workspace { "the workspace".into() } else { p.to_string() })
        .collect();
    let scope = format!(
        "in {}; writes {}; no network",
        site.shown(&cwd),
        if writes.is_empty() { "nothing".into() } else { writes.join(", ") }
    );
    let c = shell::Command {
        command: a.command.clone(),
        cwd,
        profile,
        limit: Duration::from_secs(a.seconds.clamp(1, 600)),
        home: site.keep.home().clone(),
        path: site.path.clone(),
        scratch: site.scratch.clone(),
    };
    Ok(settle("shell", site, Step(Op::Shell(c)), verdict, a.command, scope))
}

fn settle(
    tool: &str,
    site: &Site,
    step: Step,
    verdict: Verdict,
    subject: String,
    scope: String,
) -> Plan {
    match verdict.decision {
        Decision::Deny => Plan::No(format!("{subject}: {}", verdict.why)),
        Decision::Allow => Plan::Go(step, verdict),
        Decision::Ask => {
            let schema =
                specs().iter().find(|s| s.name == tool).map(schema_digest).unwrap_or_default();
            let ask = Ask {
                chain: site.chain.clone(),
                workspace: site.workspace.to_string(),
                tool: tool.into(),
                schema,
                subject,
                scope: format!("{scope}. Asked because {}.", verdict.why),
                unsandboxed: false,
            };
            Plan::Ask(Box::new(ask), step, verdict)
        }
    }
}

impl Step {
    /// Whether it may change something: what must be on record before it is done.
    pub fn changes(&self) -> bool {
        matches!(self.0, Op::Change(_) | Op::Shell(_))
    }

    /// Does it. Dropping the future ends what it started. `heard` gets a command's output, plain, as it
    /// comes.
    pub async fn run(self, site: &Site, seen: &Seen, heard: mpsc::Sender<String>) -> Output {
        match self.0 {
            Op::Shell(c) => c.run(heard).await,
            op => {
                // Files are read and written off the runtime's own threads.
                let (site, seen) = (site.clone(), seen.clone());
                let stop = Stop(Arc::new(AtomicBool::new(false)));
                let flag = stop.0.clone();
                let out = tokio::task::spawn_blocking(move || files(op, &site, &seen, &flag)).await;
                drop(stop);
                out.unwrap_or_else(|_| Output::bad("the tool failed"))
            }
        }
    }
}

/// Tells a search that nobody waits for it any more.
struct Stop(Arc<AtomicBool>);
impl Drop for Stop {
    fn drop(&mut self) {
        self.0.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

fn files(op: Op, site: &Site, seen: &Seen, stop: &AtomicBool) -> Output {
    let may = |p: &Place| site.judge(Action::Read(p)).decision == Decision::Allow;
    let within =
        |spot: &Spot, f: &dyn Fn(&walk::Look<'_>) -> Result<String, String>| match spot.folder() {
            Ok(d) => {
                let look = walk::Look { dir: &d, place: &spot.place, may: &may, stop };
                f(&look).map_or_else(Output::bad, Output::ok)
            }
            Err(e) => Output::bad(e),
        };
    match op {
        Op::Read { spot, from, lines } => match spot.read(LARGEST) {
            Err(e) => Output::bad(e),
            Ok(c) => match std::str::from_utf8(&c.bytes) {
                Ok(t) if !t.contains('\0') => {
                    seen.add(&spot.place, &c.digest);
                    Output::ok(text::numbered(t, from, lines, READ))
                }
                _ => Output::bad(format!(
                    "{} is not text ({} bytes)",
                    site.shown(&spot.place),
                    c.bytes.len()
                )),
            },
        },
        Op::List(spot) => within(&spot, &|l| Ok(l.list(500))),
        Op::Glob { spot, pattern } => within(&spot, &|l| l.glob(&pattern, 500)),
        Op::Search { spot, pattern, only, around } => {
            within(&spot, &|l| l.search(&pattern, only.as_deref(), around, FOUND))
        }
        Op::Change(all) => make(all, site, seen),
        Op::Shell(_) => Output::bad("the tool failed"),
    }
}

/// Makes all changes or, as far as can be, none.
fn make(all: Vec<Change>, site: &Site, seen: &Seen) -> Output {
    let shown: String = all.iter().map(diff).collect();
    // Everything is looked at again before anything is changed.
    let mut ready = Vec::new();
    for c in all {
        let spot = match c.spot {
            Some(s) => s,
            None => match made(site, &c.place) {
                Ok(s) => s,
                Err(e) => return Output::bad(e),
            },
        };
        if let Some(b) = &c.before {
            match spot.read(LARGEST) {
                Ok(now) if now.digest == b.digest => {}
                Ok(_) => {
                    return Output::bad(format!(
                        "{} has changed meanwhile; read it again",
                        c.shown
                    ));
                }
                Err(e) => return Output::bad(e),
            }
        }
        ready.push((c.place, c.shown, spot, c.before, c.after));
    }
    let mut done: Vec<(Place, Spot, Option<Content>)> = Vec::new();
    for (place, shown, spot, before, after) in ready {
        let r = match &after {
            Some(t) => spot.write(t.as_bytes()),
            None => spot.remove(),
        };
        if let Err(e) = r {
            let mut lost = Vec::new();
            for (p, s, before) in done.into_iter().rev() {
                // Found anew: what was made may be another kind of thing now.
                let back = fs::find(site.caps.write.roots(), &p).and_then(|now| match &before {
                    Some(b) => now.write(&b.bytes),
                    None => now.remove(),
                });
                drop(s);
                if back.is_err() {
                    lost.push(site.shown(&p));
                }
            }
            let rest = if lost.is_empty() {
                "nothing was changed".into()
            } else {
                format!("could not be undone: {}", lost.join(", "))
            };
            return Output::bad(format!("{shown}: {e}; {rest}"));
        }
        if let Some(t) = &after {
            seen.add(&place, &fs::digest(t.as_bytes()));
        }
        done.push((place, spot, before));
    }
    let names: Vec<String> = done.iter().map(|d| site.shown(&d.0)).collect();
    Output {
        ok: true,
        text: format!("changed {}\n", names.join(", ")),
        rest: None,
        detail: json!({"diff": shown}),
    }
}

/// Makes the folders on the way, one by one, each looked at; the last must be where it was judged to
/// be.
fn made(site: &Site, place: &Place) -> Result<Spot, String> {
    let mut missing: Vec<Place> = Vec::new();
    let mut up = place.parent();
    while let Some(p) = up {
        match fs::find(site.caps.write.roots(), &p) {
            Ok(s) if s.kind == Kind::Folder => break,
            Ok(s) if s.kind != Kind::Missing => return Err(format!("{p} is not a folder")),
            Ok(_) | Err(FsError::Missing(_)) => {
                up = p.parent();
                missing.push(p);
            }
            Err(e) => return Err(e.to_string()),
        }
    }
    for p in missing.into_iter().rev() {
        let s = fs::find(site.caps.write.roots(), &p).map_err(|e| e.to_string())?;
        if s.place != p {
            return Err(format!("{p} leads elsewhere now"));
        }
        s.create_folder().map_err(|e| e.to_string())?;
    }
    let s = fs::find(site.caps.write.roots(), place).map_err(|e| e.to_string())?;
    if s.place != *place {
        return Err(format!("{place} leads elsewhere now"));
    }
    Ok(s)
}

/// The folders programs are found in and what every program reads, as far as they exist here.
pub fn system(home: &Place) -> (Vec<Place>, Reach) {
    let path = ["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin", "/bin", "/usr/sbin", "/sbin"];
    let read = [
        "/bin",
        "/sbin",
        "/usr",
        "/lib",
        "/lib32",
        "/lib64",
        "/opt",
        "/nix/store",
        "/System",
        "/Library/Apple",
        "/Library/Developer",
        "/Library/Frameworks",
        "/Library/Preferences/com.apple.dt.Xcode.plist",
        "/private/var/select",
        "/private/var/db/timezone",
        "/private/etc/localtime",
        "/private/etc/passwd",
        "/private/etc/ssl",
        "/private/etc/zshenv",
        "/private/etc/hosts",
        "/private/etc/protocols",
        "/private/etc/services",
        "/etc/alternatives",
        "/etc/ld.so.cache",
        "/etc/ld.so.conf",
        "/etc/ld.so.conf.d",
        "/etc/passwd",
        "/etc/group",
        "/etc/nsswitch.conf",
        "/etc/ssl",
        "/etc/ca-certificates",
        "/etc/localtime",
        "/etc/pki",
    ];
    // Tools a person installed for themself; what signs them in there stays hidden.
    let own = [
        ".cargo",
        ".rustup",
        ".local/bin",
        ".nvm",
        ".volta",
        ".pyenv",
        ".rbenv",
        ".bun",
        ".deno",
        "go",
        ".sdkman",
        "fvm",
    ];
    let real = |p: PathBuf| std::fs::canonicalize(p).ok().and_then(|p| Place::new(p).ok());
    let home_at = |s: &str| home.as_path().join(s);
    let bins = [".cargo/bin", ".local/bin", ".bun/bin", ".deno/bin", "go/bin", ".volta/bin"];
    let path = bins
        .iter()
        .map(|b| home_at(b))
        .chain(path.iter().map(PathBuf::from))
        .filter_map(real)
        .collect();
    let read =
        read.iter().map(PathBuf::from).chain(own.iter().map(|o| home_at(o))).filter_map(real);
    (path, Reach::of(read))
}
