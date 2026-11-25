//! Copies of the folder for agents that work apart (decision 0014): made when one is first spoken
//! to, and brought back into the folder only as a person says, with what they have seen.

use std::{
    io,
    path::{Path, PathBuf},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
};

use axiom_engine::Deed;
use axiom_policy::{Place, audit::Containment, rules::Decision};
use axiom_proto::{AgentId, CopyChanges, CopyFile, CopySeen, ErrorCode, SessionId, WireError};
use axiom_tools::{
    Site,
    copy::{self, Before, Files, From, Is, Most, Was, What},
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use tokio::sync::watch;

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

fn internal(words: impl std::fmt::Display) -> WireError {
    WireError::new(ErrorCode::Internal, words.to_string())
}
fn busy(words: &str) -> WireError {
    WireError::new(ErrorCode::Busy, words)
}

/// What is known of a copy, in Axiom's own folder.
#[derive(Default, Serialize, Deserialize)]
struct Known {
    files: Files,
    /// The last that was brought in.
    #[serde(default)]
    before: Vec<Undo>,
}

#[derive(Serialize, Deserialize)]
struct Undo {
    path: String,
    /// Where what was there is kept, by its name beside this file; absent: nothing was.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    kept: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    after: Option<Is>,
    #[serde(default)]
    run: bool,
    /// What the folder's had been copied as.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    was: Option<Was>,
}

pub struct Copies {
    data: PathBuf,
    root: Option<PathBuf>,
    /// One thing at a time.
    turn: tokio::sync::Mutex<()>,
}

/// One agent's.
#[derive(Clone)]
struct Own {
    folder: Place,
    copy: Place,
    /// What is known of it, and the folder of what was there before.
    file: PathBuf,
    before: PathBuf,
}

/// Work off the runtime's threads that ends when nobody waits for it any more.
async fn work<T: Send + 'static>(
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
    let r = tokio::task::spawn_blocking(move || f(&stop)).await.map_err(internal)?;
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
            root: home.map(|h| h.join(".axiom-work")),
            turn: tokio::sync::Mutex::default(),
        }
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
        let name = steps.join("-");
        let file = self.data.join(format!("{name}.json"));
        Ok(Own { folder: real(Path::new(folder))?, copy: here, file, before: self.data.join(name) })
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
    fn remove(&self) -> Result<(), WireError> {
        for r in
            [std::fs::remove_dir_all(self.copy.as_path()), std::fs::remove_dir_all(&self.before)]
        {
            match r {
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
        let _turn = agents.copies.turn.lock().await;
        let (site, tracked) = looking(agents, &apart.folder, agent).await?;
        let (a, s, ag, folder) =
            (agents.clone(), session.clone(), agent.clone(), apart.folder.clone());
        work(move |stop| {
            let own = a.copies.own(&folder, &s, &ag, true)?;
            // What is there was not made to its end, or nothing is known of it: nobody has worked in
            // it, since that begins here.
            own.remove()?;
            let own = a.copies.own(&folder, &s, &ag, true)?;
            let may = |p: &Place| site.reads(p);
            let from = From { folder: &own.folder, tracked: &tracked, may: &may, stop };
            match copy::make(&from, &own.copy, Most::default()) {
                Ok(files) => {
                    own.keep(&Known { files, before: vec![] })?;
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
    let known = own.known()?.ok_or_else(|| bad("what was copied is not known any more"))?;
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
    let _turn = agents.copies.turn.lock().await;
    let a = agents.clone();
    work(move |stop| {
        let setup = all.iter().find(|o| o.0 == agent).map(|o| &o.1);
        let setup = setup.ok_or_else(|| bad("no such agent in this session"))?;
        let Some(own) = own_of(&a, &session, &agent, setup)? else {
            return Ok(CopyChanges::default());
        };
        let (own, known) = need(Some(own))?;
        let look = |own: &Own, known: &Known| {
            copy::changes(&own.folder, &own.copy, &known.files, FILES, stop).map_err(words)
        };
        let (mine, mut whole) = look(&own, &known)?;
        let mut others: Vec<(String, Vec<String>)> = Vec::new();
        for (id, s) in all.iter().filter(|o| o.0 != agent) {
            if s.apart.as_ref().is_none_or(|p| p.folder != setup.folder()) {
                continue;
            }
            // One that cannot be looked at keeps nothing of this one from being shown.
            let Ok(Some(o)) = own_of(&a, &session, id, s) else { continue };
            let Ok((o, k)) = need(Some(o)) else { continue };
            let Ok((theirs, _)) = look(&o, &k) else { continue };
            others.push((s.name.clone(), theirs.into_iter().map(|c| c.path).collect()));
        }
        let mut diff = String::new();
        let mut files = Vec::new();
        for c in &mine {
            if diff.len() < DIFF {
                if let Some(d) = copy::diff(&own.folder, &own.copy, c) {
                    diff.push_str(&d);
                }
            } else {
                whole = false;
            }
            let what = match c.what {
                What::New => "new",
                What::Changed => "changed",
                What::Removed => "removed",
            };
            let names = others.iter().filter(|o| o.1.contains(&c.path)).map(|o| o.0.clone());
            files.push(CopyFile {
                path: c.path.clone(),
                what: what.into(),
                digest: c.is.as_ref().map(|i| i.digest.clone()),
                clash: c.clash,
                never: c.never.map(str::to_owned),
                runs: c.runs,
                others: names.collect(),
            });
        }
        Ok(CopyChanges { made: true, files, diff, whole, back: known.before.len() })
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
    let _turn = agents.copies.turn.lock().await;
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
    work(move |_| {
        let (own, mut known) = need(own_of(&a, &session, &agent, &setup)?)?;
        // What is known now is what can be put back: what was before that no longer is.
        let old = own.before.clone();
        let mut done: Vec<Undo> = Vec::new();
        let (mut brought, mut not) = (Vec::new(), Vec::new());
        let fresh = old.with_extension("new");
        let _ = std::fs::remove_dir_all(&fresh);
        if let Some(d) = own.before.parent() {
            private_dir(d).map_err(internal)?;
        }
        private_dir(&fresh).map_err(internal)?;
        for (n, f) in files.iter().enumerate() {
            if brought.contains(&f.path) {
                continue;
            }
            let was = known.files.get(&f.path).cloned();
            let seen = f.digest.as_deref();
            // Kept before it is touched: what cannot be kept is not touched.
            let kept = match copy::peek(&own.folder, &f.path) {
                Ok(Some(b)) => {
                    let name = n.to_string();
                    if let Err(e) = std::fs::write(fresh.join(&name), &b) {
                        not.push(left(&f.path, format!("what is there cannot be kept: {e}")));
                        continue;
                    }
                    Some((name, b))
                }
                Ok(None) => None,
                Err(why) => {
                    not.push(left(&f.path, why));
                    continue;
                }
            };
            match copy::bring(&own.folder, &own.copy, &f.path, was.as_ref(), seen) {
                // Another than was kept would be put back.
                Ok(b) if b.bytes.as_deref() != kept.as_ref().map(|k| k.1.as_slice()) => {
                    brought.push(f.path.clone());
                    set(&mut known.files, &f.path, b.after);
                }
                Ok(b) => {
                    set(&mut known.files, &f.path, b.after.clone());
                    done.push(Undo {
                        path: f.path.clone(),
                        kept: kept.map(|k| k.0),
                        after: b.after,
                        run: b.run,
                        was,
                    });
                    brought.push(f.path.clone());
                }
                Err(why) => not.push(left(&f.path, why)),
            }
        }
        if brought.is_empty() {
            let _ = std::fs::remove_dir_all(&fresh);
        } else {
            let _ = std::fs::remove_dir_all(&old);
            std::fs::rename(&fresh, &old).map_err(internal)?;
            known.before = done;
            own.keep(&known)?;
        }
        Ok(json!({ "brought": brought, "left": not }))
    })
    .await
}

fn set(files: &mut Files, path: &str, is: Option<Is>) {
    match is {
        Some(is) => files.insert(path.to_owned(), Was { is, stamp: None }),
        None => files.remove(path),
    };
}

pub async fn back(
    agents: &Arc<Agents>,
    store: &Store,
    session: SessionId,
    agent: AgentId,
    setup: Setup,
) -> Result<Value, WireError> {
    let _turn = agents.copies.turn.lock().await;
    let (a, s, ag, set_up) = (agents.clone(), session.clone(), agent.clone(), setup.clone());
    let (own, mut known) = work(move |_| need(own_of(&a, &s, &ag, &set_up)?)).await?;
    let subject = named("put back:", known.before.iter().map(|u| u.path.as_str()));
    record(agents, store, &session, &agent, setup.folder(), subject).await?;
    work(move |_| {
        let (mut done, mut not) = (Vec::new(), Vec::new());
        for u in std::mem::take(&mut known.before) {
            let bytes = match &u.kept {
                Some(n) => match std::fs::read(own.before.join(n)) {
                    Ok(b) => Some(b),
                    Err(e) => {
                        not.push(left(&u.path, e.to_string()));
                        continue;
                    }
                },
                None => None,
            };
            let b = Before { path: u.path.clone(), bytes, after: u.after.clone(), run: u.run };
            match copy::back(&own.folder, &b) {
                Ok(()) => {
                    match &u.was {
                        Some(w) => known.files.insert(u.path.clone(), w.clone()),
                        None => known.files.remove(&u.path),
                    };
                    done.push(u.path);
                }
                Err(why) => {
                    not.push(left(&u.path, why));
                    known.before.push(u);
                }
            }
        }
        own.keep(&known)?;
        Ok(json!({ "back": done, "left": not }))
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
    let _turn = agents.copies.turn.lock().await;
    let (site, tracked) = looking(agents, setup.folder(), &agent).await?;
    let subject = "brought up to date with the folder".to_owned();
    record(agents, store, &session, &agent, setup.folder(), subject).await?;
    let a = agents.clone();
    work(move |stop| {
        let (own, mut known) = need(own_of(&a, &session, &agent, &setup)?)?;
        let beside = a.beside(&agent)?;
        let may = |p: &Place| site.reads(p);
        let from = From { folder: &own.folder, tracked: &tracked, may: &may, stop };
        let r = copy::renew(&from, &own.copy, &mut known.files, &beside, Most::default());
        // Also what was done before it went wrong.
        own.keep(&known)?;
        let r = r.map_err(words)?;
        let say = (!r.clashes.is_empty()).then(|| {
            format!(
                "The project has changed since your copy was made, in files you have changed too: {}. \
                 What the project has now is in {beside}, under the same names; what is not there it \
                 has removed. Work its changes into yours.",
                named("", r.clashes.iter().map(String::as_str)).trim()
            )
        });
        Ok(json!({ "taken": r.taken.len(), "clashes": r.clashes, "say": say }))
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
    let _turn = agents.copies.turn.lock().await;
    let (a, s, ag, set_up) = (agents.clone(), session.clone(), agent.clone(), setup.clone());
    let own = work(move |stop| {
        let Some(own) = own_of(&a, &s, &ag, &set_up)? else { return Ok(None) };
        if !lose {
            let (_, known) = need(Some(own.clone()))?;
            let (c, _) =
                copy::changes(&own.folder, &own.copy, &known.files, FILES, stop).map_err(words)?;
            if !c.is_empty() {
                return Err(busy(&format!(
                    "{} changed files in it are not in the folder",
                    c.len()
                )));
            }
        }
        Ok(Some(own))
    })
    .await?;
    let subject = match lose {
        true => "the copy thrown away, with what was changed in it",
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
        work(move |_| own.remove()).await?;
    }
    Ok(setup)
}
