//! Copies of the folder for agents that work apart (decision 0014): made when one is first spoken
//! to, and brought back into the folder only as a person says, with what they have seen.

use std::{
    collections::{BTreeMap, BTreeSet},
    io::{self, Write},
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
};

use axiom_engine::Deed;
use axiom_policy::{Place, audit::Containment, rules::Decision};
use axiom_proto::{
    AgentId, CopyBack, CopyChanges, CopyFile, CopySeen, ErrorCode, RequestId, SessionId, WireError,
};
use axiom_tools::{
    Site,
    copy::{self, Files, From, Is, Seen, Taking, Was, What, alike},
    points::{self, Delta},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::{OwnedMutexGuard, watch};

use crate::{
    agents::{Agents, Apart, Setup, bad, plain, real},
    changes,
    paths::{private_dir, private_file},
    store::Store,
};

/// What an agent that works apart is told of it.
pub const WORDS: &str = "You work in a copy of the project's folder that is yours alone. What you change is not in the project until a person brings it in, file by file; other agents do not see it before that. You cannot commit: leave your work as it is. What git ignores (dependencies, build output) was not copied.";

/// Of a diff, what is shown.
const DIFF: usize = 2 * 1024 * 1024;
const FILES: usize = 2000;
/// Of what is put on record of a change.
const NAMES: usize = 200;
/// Of one agent's copy: at how many points of its conversation it is kept, how much that may come to,
/// and how long one may take.
pub const POINTS: usize = 50;
pub const KEPT: u64 = 256 * 1024 * 1024;
const TAKES: std::time::Duration = std::time::Duration::from_secs(5);

fn internal(words: impl std::fmt::Display) -> WireError {
    WireError::new(ErrorCode::Internal, words.to_string())
}
fn busy(words: &str) -> WireError {
    WireError::new(ErrorCode::Busy, words)
}

/// What is known of a copy, in Axiom's own folder. What is about to be done is written here before it
/// is done: whatever ends half way is recognised by what the files are ([`Known::settle`]).
#[derive(Default, Serialize, Deserialize)]
struct Known {
    files: Files,
    /// Files that could not be copied.
    #[serde(default)]
    odd: usize,
    /// The last that was brought in.
    #[serde(default)]
    last: Option<Last>,
    /// What is being taken over from the folder.
    #[serde(default)]
    taking: Taking,
    /// Where both had changed when the copy was brought up to date: what the copy had then.
    #[serde(default)]
    clashes: BTreeMap<String, Option<String>>,
}

#[derive(Serialize, Deserialize)]
struct Last {
    id: String,
    items: Vec<Undo>,
}

#[derive(Clone, Serialize, Deserialize)]
struct Undo {
    path: String,
    /// What was there, and where that is kept.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    before: Option<Is>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kept: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    mode: Option<u32>,
    /// What it is when it was brought in.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    after: Option<Is>,
}

impl Known {
    /// Makes what is known true again, whatever was begun and not ended.
    fn settle(&mut self, own: &Own) -> Result<(), WireError> {
        let (f, c) =
            (copy::folder(&own.folder).map_err(words)?, copy::folder(&own.copy).map_err(words)?);
        for u in self.last.iter().flat_map(|l| &l.items) {
            let there = copy::now(&f, &u.path).map_err(words)?;
            let cur = self.files.get(&u.path).map(|w| &w.is);
            // Only while it is one of the two: the copy may have been brought up to date since.
            let ours = alike(cur, u.before.as_ref()) || alike(cur, u.after.as_ref());
            let one =
                alike(there.as_ref(), u.before.as_ref()) || alike(there.as_ref(), u.after.as_ref());
            if ours && one && cur != there.as_ref() {
                set(&mut self.files, &u.path, there);
            }
        }
        for (path, is) in std::mem::take(&mut self.taking) {
            let here = copy::now(&c, &path).map_err(words)?;
            if alike(here.as_ref(), is.as_ref()) {
                set(&mut self.files, &path, here);
            }
        }
        Ok(())
    }

    /// What of the last can be put back still.
    fn back(&self, own: &Own) -> Result<Option<CopyBack>, WireError> {
        let Some(l) = &self.last else { return Ok(None) };
        let f = copy::folder(&own.folder).map_err(words)?;
        let mut files = 0;
        for u in &l.items {
            let there = copy::now(&f, &u.path).map_err(words)?;
            files += usize::from(alike(there.as_ref(), u.after.as_ref()));
        }
        Ok((files > 0).then(|| CopyBack { id: l.id.clone(), files }))
    }
}

fn set(files: &mut Files, path: &str, is: Option<Is>) {
    match is {
        // No stamp: the copy's own is another file.
        Some(is) => files.insert(path.to_owned(), Was { is, stamp: None }),
        None => files.remove(path),
    };
}

/// What a copy was when so many turns of its agent's conversation had been.
#[derive(Serialize, Deserialize)]
struct Point {
    n: u64,
    /// What counted as copied then: the name of the file it is in.
    base: String,
    delta: Delta,
}

type Base = BTreeMap<String, Is>;

/// The points of a copy, in a folder of Axiom's own, the oldest first.
#[derive(Default, Serialize, Deserialize)]
struct Points {
    all: Vec<Point>,
}

impl Points {
    /// The last that is not before `lo` nor after `n`.
    fn at(&self, lo: u64, n: u64) -> Option<&Point> {
        self.all.iter().rev().find(|p| (lo..=n).contains(&p.n))
    }
    fn base(dir: &Path, name: &str) -> Result<Base, WireError> {
        serde_json::from_slice(&std::fs::read(dir.join(name)).map_err(internal)?).map_err(internal)
    }
    fn load(dir: &Path) -> Result<Self, WireError> {
        match std::fs::read(dir.join(POINTS_FILE)) {
            Ok(b) => serde_json::from_slice(&b).map_err(internal),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(internal(e)),
        }
    }

    /// Lets go of the oldest until they are few and small enough, and of what none of them needs.
    fn keep(&mut self, dir: &Path) -> Result<(), WireError> {
        let sizes: BTreeMap<String, u64> = std::fs::read_dir(dir)
            .map_err(internal)?
            .flatten()
            .filter_map(|e| {
                let size = e.metadata().map_or(0, |m| m.len());
                Some((e.file_name().into_string().ok()?, size))
            })
            .collect();
        let mut bases: BTreeMap<String, Base> = BTreeMap::new();
        for p in &self.all {
            if !bases.contains_key(&p.base) {
                bases.insert(p.base.clone(), Self::base(dir, &p.base)?);
            }
        }
        let needed = loop {
            let mut needed: BTreeSet<String> = BTreeSet::new();
            for p in &self.all {
                needed.insert(p.base.clone());
                needed.extend(p.delta.values().flatten().map(points::name));
            }
            // What was copied, as far as it is kept: of files that are something else by now.
            needed.extend(bases.values().flat_map(Base::values).map(points::name));
            let size: u64 = needed.iter().filter_map(|n| sizes.get(n)).sum();
            if self.all.is_empty() || (self.all.len() <= POINTS && size <= KEPT) {
                break needed;
            }
            let gone = self.all.remove(0);
            if !self.all.iter().any(|p| p.base == gone.base) {
                bases.remove(&gone.base);
            }
        };
        let text = serde_json::to_string(self).map_err(internal)?;
        private_file(&dir.join(POINTS_FILE), &text).map_err(internal)?;
        for name in sizes.keys().filter(|n| *n != POINTS_FILE && !needed.contains(*n)) {
            let _ = std::fs::remove_file(dir.join(name));
        }
        Ok(())
    }
}

const POINTS_FILE: &str = "points.json";

pub struct Copies {
    data: PathBuf,
    /// Where what copies were at points of their agents' conversations is kept.
    points: PathBuf,
    root: Option<PathBuf>,
    /// One thing at a time.
    turn: Arc<tokio::sync::Mutex<()>>,
}

/// One agent's.
#[derive(Clone)]
struct Own {
    folder: Place,
    copy: Place,
    /// What is known of it, and the folder of what was there before.
    file: PathBuf,
    before: PathBuf,
    /// What it was at points of its agent's conversation.
    points: PathBuf,
}

/// Work off the runtime's threads that ends when nobody waits for it any more.
async fn work<T: Send + 'static>(
    turn: OwnedMutexGuard<()>,
    f: impl FnOnce(&AtomicBool) -> Result<T, WireError> + Send + 'static,
) -> Result<T, WireError> {
    struct Stops(Arc<AtomicBool>);
    impl Drop for Stops {
        fn drop(&mut self) {
            self.0.store(true, Relaxed);
        }
    }
    let stops = Stops(Arc::default());
    let stop = stops.0.clone();
    // Nobody else's turn before this has ended, also when nobody waits for it any more.
    let work = tokio::task::spawn_blocking(move || {
        let r = f(&stop);
        drop(turn);
        r
    });
    let r = work.await.map_err(internal)?;
    drop(stops);
    r
}

fn words(e: impl std::fmt::Display) -> WireError {
    bad(e.to_string())
}

impl Copies {
    pub fn new(data: &Path, home: Option<&Path>) -> Self {
        Self {
            data: data.join("copies"),
            points: data.join("points"),
            root: home.map(|h| h.join(".axiom-work")),
            turn: Arc::default(),
        }
    }

    async fn turn(&self) -> OwnedMutexGuard<()> {
        self.turn.clone().lock_owned().await
    }

    /// Where all of them are.
    pub fn root(&self) -> Option<PathBuf> {
        self.root.clone()
    }

    fn own(&self, folder: &str, s: &SessionId, a: &AgentId, make: bool) -> Result<Own, WireError> {
        let root = self.root.as_ref().ok_or_else(|| bad("there is no home folder"))?;
        let mut at = root.clone();
        let steps = [plain(&s.0), plain(&a.0)];
        if steps.iter().any(String::is_empty) {
            return Err(bad("this has no name for a folder"));
        }
        if make {
            private_dir(&at).map_err(internal)?;
        }
        // Step by step, and none of it a link to elsewhere.
        let mut here = real(&at)?;
        for n in &steps {
            at = here.as_path().join(n);
            if make {
                private_dir(&at).map_err(internal)?;
            }
            let next = here.join(n).map_err(words)?;
            if real(&at)? != next {
                return Err(bad("the copy's folder leads elsewhere"));
            }
            here = next;
        }
        let folder = real(Path::new(folder))?;
        if folder.covers_folded(&real(root)?) {
            return Err(bad("the folder holds the copies themselves"));
        }
        let name = steps.join("-");
        let file = self.data.join(format!("{name}.json"));
        let before = self.data.join(format!("{name}.before"));
        let points = self.points.join(&name);
        Ok(Own { folder, copy: here, file, before, points })
    }
}

impl Own {
    fn known(&self) -> Result<Option<Known>, WireError> {
        match std::fs::read(&self.file) {
            Ok(b) => serde_json::from_slice(&b).map(Some).map_err(internal),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(internal(e)),
        }
    }
    fn keep(&self, k: &Known) -> Result<(), WireError> {
        let text = serde_json::to_string(k).map_err(internal)?;
        if let Some(d) = self.file.parent() {
            private_dir(d).map_err(internal)?;
        }
        private_file(&self.file, &text).map_err(internal)
    }
    /// What was kept before others is not needed any more.
    fn forget_but(&self, id: Option<String>) {
        let Ok(all) = std::fs::read_dir(&self.before) else { return };
        for e in all.flatten() {
            if e.file_name().to_str().map(str::to_owned) != id {
                let _ = std::fs::remove_dir_all(e.path());
            }
        }
    }

    fn remove(&self) -> Result<(), WireError> {
        for d in [self.copy.as_path(), &self.before, &self.points] {
            match std::fs::remove_dir_all(d) {
                Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(internal(e)),
                _ => {}
            }
        }
        match std::fs::remove_file(&self.file) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(internal(e)),
            _ => Ok(()),
        }
    }
}

/// How the folder is looked at: as an agent that only reads, by the folder's own rules.
async fn looking(
    agents: &Arc<Agents>,
    folder: &str,
    agent: &AgentId,
) -> Result<(Site, Vec<String>), WireError> {
    let (a, f) = (agents.clone(), folder.to_owned());
    let id = AgentId(format!("copy_{}", agent.0));
    let site =
        tokio::task::spawn_blocking(move || a.looking(&f, &id)).await.map_err(internal)??;
    let tracked = changes::tracked(&site).await?;
    Ok((site, tracked))
}

/// The agent as it is to work: in its copy, which is made if there is none.
pub async fn ready(
    agents: &Arc<Agents>,
    store: &Store,
    session: &SessionId,
    agent: &AgentId,
    mut setup: Setup,
    mut cancel: watch::Receiver<bool>,
) -> Result<Setup, WireError> {
    let Some(apart) = setup.apart.clone() else { return Ok(setup) };
    if apart.made && Path::new(&setup.workspace).is_dir() {
        return Ok(setup);
    }
    let make = async {
        let turn = agents.copies.turn().await;
        let (site, tracked) = looking(agents, &apart.folder, agent).await?;
        let (a, s, ag, folder) =
            (agents.clone(), session.clone(), agent.clone(), apart.folder.clone());
        work(turn, move |stop| {
            let own = a.copies.own(&folder, &s, &ag, true)?;
            // What is there was not made to its end, or nothing is known of it: nobody has worked in
            // it, since that begins here.
            own.remove()?;
            let own = a.copies.own(&folder, &s, &ag, true)?;
            let may = |p: &Place| site.reads(p);
            let from = From { folder: &own.folder, tracked: &tracked, may: &may, stop };
            match copy::make(&from, &own.copy, crate::limits::copied()) {
                Ok((files, odd)) => {
                    own.keep(&Known { files, odd, ..Known::default() })?;
                    Ok(own.copy.to_string())
                }
                Err(e) => {
                    let _ = own.remove();
                    Err(bad(format!("no copy of the folder could be made: {e}")))
                }
            }
        })
        .await
    };
    let place = tokio::select! {
        p = make => p?,
        _ = cancel.wait_for(|c| *c) => return Err(bad("it was stopped")),
    };
    setup.workspace = place;
    setup.apart = Some(Apart { made: true, ..apart });
    keep(store, session, agent, &setup).await?;
    Ok(setup)
}

/// Keeps what an agent's copy is, where its conversation has `n` turns. What cannot be kept is not, and
/// nothing waits for it.
pub async fn point(
    agents: &Arc<Agents>,
    session: &SessionId,
    agent: &AgentId,
    setup: &Setup,
    n: u64,
) -> Result<bool, WireError> {
    if !setup.apart.as_ref().is_some_and(|a| a.made) {
        return Ok(false);
    }
    let kept = async {
        let turn = agents.copies.turn().await;
        let (a, s, ag, setup) = (agents.clone(), session.clone(), agent.clone(), setup.clone());
        work(turn, move |stop| {
            let (own, known) = need(own_of(&a, &s, &ag, &setup)?)?;
            for d in [&a.copies.points, &own.points] {
                private_dir(d).map_err(internal)?;
            }
            let mut all = Points::load(&own.points)?;
            all.all.retain(|p| p.n != n);
            let delta =
                points::keep(&own.folder, &own.copy, &known.files, &own.points, FILES, stop);
            if let Some(delta) = delta.map_err(words)? {
                let base: Base =
                    known.files.iter().map(|(p, w)| (p.clone(), w.is.clone())).collect();
                let text = serde_json::to_string(&base).map_err(internal)?;
                let base = format!("{}.base", axiom_tools::fs::digest(text.as_bytes()));
                if !own.points.join(&base).exists() {
                    private_file(&own.points.join(&base), &text).map_err(internal)?;
                }
                all.all.push(Point { n, base, delta });
            }
            all.keep(&own.points)?;
            Ok(all.all.last().is_some_and(|p| p.n == n))
        })
        .await
    };
    tokio::time::timeout(TAKES, kept).await.unwrap_or(Ok(false))
}

/// Whether what an agent's copy was is kept, for a place in its conversation.
pub async fn has_point(
    agents: &Arc<Agents>,
    session: &SessionId,
    agent: &AgentId,
    setup: &Setup,
    (lo, n): (u64, u64),
) -> Result<bool, WireError> {
    let turn = agents.copies.turn().await;
    let (a, s, ag, setup) = (agents.clone(), session.clone(), agent.clone(), setup.clone());
    work(turn, move |_| {
        let Some(own) = own_of(&a, &s, &ag, &setup)? else { return Ok(false) };
        Ok(Points::load(&own.points)?.at(lo, n).is_some())
    })
    .await
}

/// Whose copy a new one is made from, and for which place in their conversation.
pub struct Then {
    pub agent: AgentId,
    pub setup: Setup,
    pub at: (u64, u64),
}

/// Makes a copy for an agent, as another's was then: where it is.
pub async fn again(
    agents: &Arc<Agents>,
    store: &Store,
    session: &SessionId,
    then: Then,
    to: &AgentId,
) -> Result<String, WireError> {
    let turn = agents.copies.turn().await;
    let subject = format!("another made for {}, as it was earlier", to.0);
    record(agents, store, session, &then.agent, then.setup.folder(), subject).await?;
    let (a, s, to) = (agents.clone(), session.clone(), to.clone());
    work(turn, move |stop| {
        let (own, known) = need(own_of(&a, &s, &then.agent, &then.setup)?)?;
        let all = Points::load(&own.points)?;
        let (lo, n) = then.at;
        let point = all.at(lo, n).ok_or_else(|| bad("what its files were there is not kept"))?;
        let base = Points::base(&own.points, &point.base)?;
        let mut files = base.clone();
        for (path, is) in &point.delta {
            match is {
                Some(is) => files.insert(path.clone(), is.clone()),
                None => files.remove(path),
            };
        }
        let new = a.copies.own(own.folder.as_str(), &s, &to, true)?;
        let from = points::Sources { store: &own.points, copy: &own.copy, folder: &own.folder };
        let made = points::again(&files, &from, &new.copy, crate::limits::copied(), stop);
        let kept = made.map_err(words).and_then(|made| {
            // What counted as copied then counts so now.
            let counted = |(path, is): (String, Is)| {
                let stamp = made.get(&path).filter(|w| w.is == is).and_then(|w| w.stamp.clone());
                (path, Was { is, stamp })
            };
            // A clash that the first one has not settled is none of this one's doing either.
            let clashes = known
                .clashes
                .into_iter()
                .filter(|(path, _)| base.get(path) == known.files.get(path).map(|w| &w.is));
            let known = Known { clashes: clashes.collect(), odd: known.odd, ..Known::default() };
            new.keep(&Known { files: base.clone().into_iter().map(counted).collect(), ..known })
        });
        match kept {
            Ok(()) => Ok(new.copy.to_string()),
            Err(e) => {
                let _ = new.remove();
                Err(e)
            }
        }
    })
    .await
}

/// Throws away the copy of an agent there has never been.
pub async fn unmake(agents: &Arc<Agents>, session: &SessionId, agent: &AgentId, folder: &str) {
    let turn = agents.copies.turn().await;
    let (a, s, ag, folder) = (agents.clone(), session.clone(), agent.clone(), folder.to_owned());
    let _ = work(turn, move |_| a.copies.own(&folder, &s, &ag, false)?.remove()).await;
}

async fn keep(
    store: &Store,
    session: &SessionId,
    agent: &AgentId,
    s: &Setup,
) -> Result<(), WireError> {
    let text = serde_json::to_string(s).map_err(internal)?;
    store.put_agent(session.clone(), agent.clone(), text).await.map_err(internal)
}

/// Of an agent that works apart and has been spoken to, its copy.
fn own_of(
    agents: &Agents,
    session: &SessionId,
    agent: &AgentId,
    setup: &Setup,
) -> Result<Option<Own>, WireError> {
    let Some(apart) = &setup.apart else {
        return Err(bad("the agent works in the folder itself"));
    };
    if !apart.made || !Path::new(&setup.workspace).is_dir() {
        return Ok(None);
    }
    let own = agents.copies.own(&apart.folder, session, agent, false)?;
    if own.copy.as_str() != setup.workspace {
        return Err(bad("the copy is elsewhere now"));
    }
    Ok(Some(own))
}

fn need(own: Option<Own>) -> Result<(Own, Known), WireError> {
    let own = own.ok_or_else(|| bad("there is no copy: the agent has not been spoken to"))?;
    let mut known = own.known()?.ok_or_else(|| bad("what was copied is not known any more"))?;
    known.settle(&own)?;
    Ok((own, known))
}

/// The agents of the session, as they are kept.
pub type Others = Vec<(AgentId, Setup)>;

pub async fn shown(
    agents: &Arc<Agents>,
    session: SessionId,
    agent: AgentId,
    all: Others,
) -> Result<CopyChanges, WireError> {
    let turn = agents.copies.turn().await;
    let a = agents.clone();
    work(turn, move |stop| {
        let setup = all.iter().find(|o| o.0 == agent).map(|o| &o.1);
        let setup = setup.ok_or_else(|| bad("no such agent in this session"))?;
        let Some(own) = own_of(&a, &session, &agent, setup)? else {
            return Ok(CopyChanges::default());
        };
        let (own, known) = need(Some(own))?;
        let look = |own: &Own, known: &Known| {
            copy::changes(&own.folder, &own.copy, &known.files, FILES, stop).map_err(words)
        };
        let mine = look(&own, &known)?;
        let mut others: Vec<(String, BTreeSet<String>)> = Vec::new();
        for (id, s) in all.iter().filter(|o| o.0 != agent && o.1.apart.is_some()) {
            // One that cannot be looked at keeps nothing of this one from being shown.
            let Ok(Some(o)) = own_of(&a, &session, id, s) else { continue };
            let Ok((o, k)) = need(Some(o)) else { continue };
            let Ok(theirs) = look(&o, &k) else { continue };
            others.push((s.name.clone(), theirs.all.into_iter().map(|c| c.path).collect()));
        }
        let mut room = DIFF;
        let mut files = Vec::new();
        for c in mine.all {
            let diff = copy::diff(&own.folder, &own.copy, &c).filter(|d| d.len() <= room);
            room -= diff.as_ref().map_or(0, String::len);
            let what = match c.what {
                What::New => "new",
                What::Changed => "changed",
                What::Removed => "removed",
            };
            let names = others.iter().filter(|o| o.1.contains(&c.path)).map(|o| o.0.clone());
            let digest = c.is.as_ref().map(|i| i.digest.clone());
            files.push(CopyFile {
                what: what.into(),
                run: c.is.as_ref().is_some_and(|i| i.run),
                there: c.there.map(|i| i.digest),
                diff,
                clash: c.clash,
                unsettled: known.clashes.get(&c.path).is_some_and(|d| *d == digest),
                never: c.never.map(str::to_owned),
                runs: c.runs,
                others: names.collect(),
                path: c.path,
                digest,
            });
        }
        Ok(CopyChanges {
            made: true,
            files,
            whole: mine.whole,
            ignored: mine.ignored,
            odd: mine.odd + known.odd,
            back: known.back(&own)?,
        })
    })
    .await
}

/// Puts on record what a person has done (SI-23), before it is done.
async fn record(
    agents: &Agents,
    store: &Store,
    session: &SessionId,
    agent: &AgentId,
    folder: &str,
    subject: String,
) -> Result<(), WireError> {
    let deed = Deed {
        chain: vec![agent.clone()],
        workspace: folder.to_owned(),
        tool: "copy".into(),
        subject,
        decision: Decision::Allow,
        rules: vec!["a person".into()],
        answered: None,
    };
    agents.record(store, session, deed, Containment::Axiom).await.map_err(internal)
}

fn named<'a>(what: &str, all: impl ExactSizeIterator<Item = &'a str>) -> String {
    let n = all.len();
    let mut out = format!("{what} {}", all.take(NAMES).collect::<Vec<_>>().join(", "));
    if n > NAMES {
        out.push_str(&format!(" and {} more", n - NAMES));
    }
    out
}

fn left(path: &str, why: impl Into<String>) -> Value {
    json!({"path": path, "why": why.into()})
}

/// Kept before anything is touched, and there after a power loss.
fn put_by(dir: &Path, name: &str, bytes: &[u8]) -> io::Result<()> {
    let mut f = std::fs::File::create_new(dir.join(name))?;
    f.write_all(bytes)?;
    f.sync_all()
}

pub async fn bring(
    agents: &Arc<Agents>,
    store: &Store,
    session: SessionId,
    agent: AgentId,
    setup: Setup,
    files: Vec<CopySeen>,
) -> Result<Value, WireError> {
    if files.is_empty() || files.len() > FILES {
        return Err(bad("between 1 and 2000 files are brought in at once"));
    }
    let turn = agents.copies.turn().await;
    let subject: Vec<String> = files
        .iter()
        .map(|f| match &f.digest {
            Some(d) => format!("{} ({})", f.path, d.get(..12).unwrap_or(d)),
            None => format!("{} (removed)", f.path),
        })
        .collect();
    let subject = named("brought in:", subject.iter().map(String::as_str));
    record(agents, store, &session, &agent, setup.folder(), subject).await?;
    let a = agents.clone();
    work(turn, move |_| {
        let (own, mut known) = need(own_of(&a, &session, &agent, &setup)?)?;
        let last = Last { id: RequestId::new().0, items: Vec::new() };
        let kept = own.before.join(plain(&last.id));
        if let Some(d) = own.before.parent() {
            private_dir(d).map_err(internal)?;
        }
        for d in [&own.before, &kept] {
            private_dir(d).map_err(internal)?;
        }
        let mut not = Vec::new();
        let mut items: Vec<Undo> = Vec::new();
        for (n, f) in files.iter().enumerate() {
            if items.iter().any(|u| u.path == f.path) {
                continue;
            }
            let seen = Seen { digest: f.digest.as_deref(), run: f.run, there: f.there.as_deref() };
            let was = known.files.get(&f.path);
            let p = match copy::plan(&own.folder, &own.copy, &f.path, was, seen) {
                Ok(p) => p,
                Err(why) => {
                    not.push(left(&f.path, why));
                    continue;
                }
            };
            let name = p.bytes.as_ref().map(|_| n.to_string());
            if let (Some(b), Some(name)) = (&p.bytes, &name)
                && let Err(e) = put_by(&kept, name, b)
            {
                not.push(left(&f.path, format!("what is there cannot be kept: {e}")));
                continue;
            }
            items.push(Undo {
                path: f.path.clone(),
                before: p.there,
                kept: name,
                mode: p.mode,
                after: p.is,
            });
        }
        if items.is_empty() {
            let _ = std::fs::remove_dir_all(&kept);
            return Ok(json!({ "brought": [], "left": not }));
        }
        // On record what is to be, and how it was: what ends from here on is made good by that.
        known.last = Some(Last { items, ..last });
        if let Err(e) = own.keep(&known) {
            let _ = std::fs::remove_dir_all(&kept);
            return Err(e);
        }
        let mut brought = Vec::new();
        if let Some(l) = &mut known.last {
            l.items.retain(|u| {
                let done = copy::bring(
                    &own.folder,
                    &own.copy,
                    &u.path,
                    u.after.as_ref(),
                    u.before.as_ref(),
                );
                match done {
                    Ok(()) => brought.push(u.path.clone()),
                    Err(why) => not.push(left(&u.path, why)),
                }
                brought.last() == Some(&u.path)
            });
        }
        known.settle(&own)?;
        known.clashes.retain(|p, _| !brought.contains(p));
        if brought.is_empty() {
            known.last = None;
        }
        own.keep(&known)?;
        own.forget_but(known.last.as_ref().map(|l| plain(&l.id)));
        Ok(json!({ "brought": brought, "left": not }))
    })
    .await
}

pub async fn back(
    agents: &Arc<Agents>,
    store: &Store,
    session: SessionId,
    agent: AgentId,
    setup: Setup,
    id: String,
) -> Result<Value, WireError> {
    let turn = agents.copies.turn().await;
    let subject = format!("put back what was there before {id}");
    record(agents, store, &session, &agent, setup.folder(), subject).await?;
    let a = agents.clone();
    work(turn, move |_| {
        let (own, mut known) = need(own_of(&a, &session, &agent, &setup)?)?;
        let Some(mut l) = known.last.take().filter(|l| l.id == id) else {
            return Err(bad("something else has been brought in since"));
        };
        let kept = own.before.join(plain(&l.id));
        let f = copy::folder(&own.folder).map_err(words)?;
        let (mut done, mut not) = (Vec::new(), Vec::new());
        let mut stay = Vec::new();
        for u in std::mem::take(&mut l.items) {
            let there = copy::now(&f, &u.path).map_err(words);
            // Put back already, before something ended it.
            if there.is_ok_and(|t| alike(t.as_ref(), u.before.as_ref())) {
                done.push(u);
                continue;
            }
            let put = match &u.kept {
                Some(n) => match std::fs::read(kept.join(n)) {
                    Ok(b) => copy::back(&own.folder, &u.path, Some((&b, u.mode)), u.after.as_ref()),
                    Err(e) => Err(format!("what was there cannot be read: {e}")),
                },
                None => copy::back(&own.folder, &u.path, None, u.after.as_ref()),
            };
            match put {
                Ok(()) => done.push(u),
                Err(why) => {
                    not.push(left(&u.path, why));
                    stay.push(u);
                }
            }
        }
        let paths: Vec<String> = done.iter().map(|u| u.path.clone()).collect();
        // While it is known what it was and what it became.
        known.last = Some(Last { id: l.id.clone(), items: done });
        known.settle(&own)?;
        known.last = (!stay.is_empty()).then_some(Last { id: l.id, items: stay });
        own.keep(&known)?;
        own.forget_but(known.last.as_ref().map(|l| plain(&l.id)));
        Ok(json!({ "back": paths, "left": not }))
    })
    .await
}

/// Nothing of the agent runs meanwhile: the caller sees to that.
pub async fn renew(
    agents: &Arc<Agents>,
    store: &Store,
    session: SessionId,
    agent: AgentId,
    setup: Setup,
) -> Result<Value, WireError> {
    let turn = agents.copies.turn().await;
    let (site, tracked) = looking(agents, setup.folder(), &agent).await?;
    let subject = "brought up to date with the folder".to_owned();
    record(agents, store, &session, &agent, setup.folder(), subject).await?;
    let a = agents.clone();
    work(turn, move |stop| {
        let (own, mut known) = need(own_of(&a, &session, &agent, &setup)?)?;
        let beside = a.beside(&agent)?;
        let may = |p: &Place| site.reads(p);
        let from = From { folder: &own.folder, tracked: &tracked, may: &may, stop };
        let news = copy::news(&from, &own.copy, &mut known.files, crate::limits::copied()).map_err(words)?;
        let mut not: Vec<Value> = news.left.iter().map(|(p, w)| left(p, w.as_str())).collect();
        // On record before it is done: what is taken over is the folder's, and nobody's change.
        known.taking = news.taking;
        own.keep(&known)?;
        // For who goes back to before this.
        if own.points.is_dir() {
            let was = known.taking.keys().filter_map(|p| Some((p.as_str(), &known.files.get(p)?.is)));
            points::keep_these(&own.copy, was, &own.points).map_err(words)?;
        }
        let (c, f) =
            (copy::folder(&own.copy).map_err(words)?, copy::folder(&own.folder).map_err(words)?);
        let mut taken = 0;
        for (path, is) in std::mem::take(&mut known.taking) {
            match copy::take(&own.folder, &own.copy, &path, is.as_ref()) {
                Ok(w) => {
                    match w {
                        Some(w) => known.files.insert(path, w),
                        None => known.files.remove(&path),
                    };
                    taken += 1;
                }
                Err(e) => not.push(left(&path, e.to_string())),
            }
        }
        let digest = |p: &str| -> Result<Option<String>, WireError> {
            Ok(copy::now(&c, p).map_err(words)?.map(|i| i.digest))
        };
        // Those of before that nobody has settled are told of again.
        let mut clashes = BTreeSet::new();
        for (path, had) in std::mem::take(&mut known.clashes) {
            if digest(&path)? == had && !news.clashes.contains_key(&path) {
                let there = copy::now(&f, &path).map_err(words)?;
                if copy::now(&c, &path).map_err(words)? != there {
                    known.clashes.insert(path.clone(), had);
                    clashes.insert(path);
                }
            }
        }
        for (path, is) in news.clashes {
            known.clashes.insert(path.clone(), digest(&path)?);
            set(&mut known.files, &path, is);
            clashes.insert(path);
        }
        for path in &clashes {
            if known.files.contains_key(path)
                && let Err(e) = copy::beside(&own.folder, &beside, path)
            {
                not.push(left(path, format!("it is not beside the copy: {e}")));
            }
        }
        own.keep(&known)?;
        let say = (!clashes.is_empty()).then(|| {
            format!(
                "The project has changed since your copy was made, in files you have changed too: {}. \
                 What the project has now is in {beside}, under the same names; what is not there it \
                 has removed. Work its changes into yours.",
                named("", clashes.iter().map(String::as_str)).trim()
            )
        });
        Ok(json!({ "taken": taken, "clashes": clashes, "left": not, "say": say }))
    })
    .await
}

/// Nothing of the agent runs meanwhile: the caller sees to that.
pub async fn remove(
    agents: &Arc<Agents>,
    store: &Store,
    session: SessionId,
    agent: AgentId,
    mut setup: Setup,
    lose: bool,
) -> Result<Setup, WireError> {
    let turn = agents.copies.turn().await;
    let (a, s, ag, set_up) = (agents.clone(), session.clone(), agent.clone(), setup.clone());
    let own = work(turn, move |stop| {
        let Some(own) = own_of(&a, &s, &ag, &set_up)? else { return Ok(None) };
        if !lose {
            let (_, known) = need(Some(own.clone()))?;
            let c =
                copy::changes(&own.folder, &own.copy, &known.files, FILES, stop).map_err(words)?;
            let only = c.all.len() + c.ignored + c.odd;
            if only > 0 || !c.whole {
                return Err(busy(&format!("{only} files or folders are in it only")));
            }
        }
        Ok(Some(own))
    })
    .await?;
    let turn = agents.copies.turn().await;
    let subject = match lose {
        true => "the copy thrown away, with what only it had",
        false => "the copy thrown away; the folder has all of it",
    };
    record(agents, store, &session, &agent, setup.folder(), subject.into()).await?;
    // First: an agent that is spoken to now gets another.
    if let Some(apart) = &mut setup.apart {
        apart.made = false;
        setup.workspace.clone_from(&apart.folder);
    }
    keep(store, &session, &agent, &setup).await?;
    if let Some(own) = own {
        work(turn, move |_| own.remove()).await?;
    }
    Ok(setup)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    /// The oldest go, and what none of the others needs.
    #[test]
    fn what_points_come_to_is_bounded() {
        let d = tempfile::tempdir().unwrap();
        let is = |n: u64| Is { digest: format!("d{n}"), link: false, run: false };
        let file = |name: &str, size: u64| {
            std::fs::File::create(d.path().join(name)).unwrap().set_len(size).unwrap();
        };
        std::fs::write(d.path().join("b.base"), r#"{"f":{"digest":"was"}}"#).unwrap();
        file("was", 1);
        file("nobodys", 1);
        let mut all = Points::default();
        for n in 0..5 {
            file(&format!("d{n}"), KEPT / 4);
            let delta = [("f".to_owned(), Some(is(n)))].into();
            all.all.push(Point { n, base: "b.base".into(), delta });
        }
        // The same again costs nothing.
        let delta = [("g".to_owned(), Some(is(4)))].into();
        all.all.push(Point { n: 5, base: "b.base".into(), delta });
        all.keep(d.path()).unwrap();
        assert_eq!(all.all.iter().map(|p| p.n).collect::<Vec<_>>(), [2, 3, 4, 5]);
        let names = || {
            let all = std::fs::read_dir(d.path()).unwrap();
            let mut all: Vec<_> =
                all.map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
            all.sort();
            all
        };
        assert_eq!(names(), ["b.base", "d2", "d3", "d4", "points.json", "was"]);
        assert_eq!(Points::load(d.path()).unwrap().all.len(), 4);
        all.all.clear();
        all.keep(d.path()).unwrap();
        assert_eq!(names(), ["points.json"]);
    }
}
