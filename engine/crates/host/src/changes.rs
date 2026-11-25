//! What has been changed in a session's folder since its last commit, as git has it. Git runs in the
//! sandbox, reading only and without a network, whatever a repository's configuration would have it
//! run (SI-6, SI-22); what is not tracked is read by handle.

use std::time::Duration;

use axiom_policy::{Caps, Place, Profile};
use axiom_proto::{AgentId, Changed, Changes, SessionId, WireError};
use axiom_spawn::{Contain, Env, Job, spawn};
use axiom_tools::{Site, fs, text};
use tokio::io::AsyncReadExt;

use crate::agents::{Agents, bad};

/// Of what git says, at most.
const MOST: usize = 2 * 1024 * 1024;
/// Of the names of what it tracks.
const TRACKED: usize = 32 * 1024 * 1024;
/// Of a file that is not tracked.
const FILE: u64 = 256 * 1024;
/// Files that are shown.
const FILES: usize = 1000;
const LONGEST: Duration = Duration::from_secs(20);
/// What a repository without a commit is compared with.
const NOTHING: &str = "4b825dc642cb6eb9a060e54bf8d69288fbee4904";

/// Where git is, if it is a program of the system's.
fn git(site: &Site) -> Option<Place> {
    site.path
        .iter()
        .filter(|d| !d.as_path().starts_with(site.keep.home().as_path()))
        .filter_map(|d| d.join("git").ok())
        .find(|p| p.as_path().is_file())
}

struct Git {
    program: Place,
    site: Site,
}

impl Git {
    /// What it says, and whether that was all.
    async fn run(&self, args: &[&str]) -> Result<Option<(Vec<u8>, bool)>, WireError> {
        self.run_to(args, MOST).await
    }

    async fn run_to(
        &self,
        args: &[&str],
        most: usize,
    ) -> Result<Option<(Vec<u8>, bool)>, WireError> {
        let s = &self.site;
        let caps = Caps { run: true, ..s.caps.clone() };
        let profile = Profile::read_only(&caps, &s.keep, &s.system, &s.scratch)
            .map_err(|e| bad(e.to_string()))?;
        // Nothing of a configuration that starts something, for where the sandbox would stop it only
        // with an error; and no file of it but the repository's.
        let first = [
            "--no-optional-locks",
            "--no-pager",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.untrackedCache=false",
            "-c",
            "diff.external=",
            "-c",
            "protocol.allow=never",
        ];
        let mut env = Env::new(&s.path, &s.scratch, &s.scratch);
        env.set("GIT_CONFIG_NOSYSTEM", "1").map_err(|e| bad(e.to_string()))?;
        let job = Job {
            program: self.program.clone(),
            args: first.iter().chain(args).map(|a| (*a).to_owned()).collect(),
            cwd: s.workspace.clone(),
            env,
            stdin: false,
            contain: Contain::Sandbox(profile),
        };
        let mut p = spawn(job).map_err(|e| bad(format!("git cannot be run: {e}")))?;
        let (Some(out), Some(mut err)) = (p.stdout.take(), p.stderr.take()) else {
            return Err(bad("git cannot be run"));
        };
        let mut said = Vec::new();
        let mut first = out.take(most as u64 + 1);
        let mut words = [0; 4096];
        let read = async {
            // What it complains of is not kept, and does not hold it up.
            let quiet = async { while err.read(&mut words).await.is_ok_and(|n| n > 0) {} };
            let (r, ()) = tokio::join!(first.read_to_end(&mut said), async {
                tokio::select! { () = quiet => {}, () = tokio::time::sleep(LONGEST) => {} }
            });
            r
        };
        let done = tokio::time::timeout(LONGEST, read).await;
        let whole = said.len() <= most && done.is_ok_and(|r| r.is_ok());
        if !whole {
            p.kill();
        }
        let ended = p.wait().await.map_err(|e| bad(e.to_string()))?;
        said.truncate(most);
        Ok((ended.code == Some(0) || !whole).then_some((said, whole)))
    }
}

/// What there is to say of the folder; `repository` is false if git has nothing to do with it.
pub async fn changes(
    agents: &std::sync::Arc<Agents>,
    session: &SessionId,
    workspace: String,
) -> Result<Changes, WireError> {
    let a = agents.clone();
    // Its scratch is nobody's but this session's.
    let id = AgentId(format!("git_{}", session.0));
    let site = tokio::task::spawn_blocking(move || a.looking(&workspace, &id))
        .await
        .map_err(|e| bad(e.to_string()))??;
    let none = Changes::default();
    if !site.workspace.as_path().join(".git").exists() {
        return Ok(none);
    }
    let Some(program) = git(&site) else { return Err(bad("git was not found")) };
    let git = Git { program, site };
    let Some((head, _)) = git
        .run(&[
            "status",
            "--porcelain=v1",
            "-z",
            "--branch",
            "--untracked-files=all",
            "--no-renames",
            "--ignore-submodules=all",
        ])
        .await?
    else {
        return Ok(none);
    };
    let (branch, mut files, untracked) = status(&head);
    let mut whole = files.len() < FILES;
    files.truncate(FILES);
    let since = match git.run(&["rev-parse", "--quiet", "--verify", "HEAD"]).await? {
        Some((h, _)) => String::from_utf8_lossy(&h).trim().to_owned(),
        None => NOTHING.into(),
    };
    let diff = git
        .run(&[
            "diff",
            &since,
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--ignore-submodules=all",
            "--src-prefix=",
            "--dst-prefix=",
            "--",
        ])
        .await?;
    let (diff, all) = diff.unwrap_or_default();
    whole &= all;
    let mut diff = String::from_utf8_lossy(&diff).into_owned();
    let site = git.site;
    let (more, rest) = tokio::task::spawn_blocking(move || new_files(&site, untracked))
        .await
        .map_err(|e| bad(e.to_string()))?;
    diff.push_str(&more);
    whole &= rest;
    Ok(Changes { repository: true, branch, files, diff, whole })
}

/// What git tracks in a folder, whatever it would ignore now. Nothing, where git has no say.
pub async fn tracked(site: &Site) -> Result<Vec<String>, WireError> {
    if !site.workspace.as_path().join(".git").is_dir() {
        return Ok(Vec::new());
    }
    let Some(program) = git(site) else { return Ok(Vec::new()) };
    let git = Git { program, site: site.clone() };
    let Some((all, whole)) = git.run_to(&["ls-files", "-z"], TRACKED).await? else {
        return Ok(Vec::new());
    };
    if !whole {
        return Err(bad("git did not say in time what it tracks, or it is too much"));
    }
    let names = all.split(|b| *b == 0).filter(|n| !n.is_empty());
    Ok(names.filter_map(|n| String::from_utf8(n.to_vec()).ok()).collect())
}

/// What git's status says: the branch, what has changed, and what of it is not tracked.
fn status(s: &[u8]) -> (Option<String>, Vec<Changed>, Vec<String>) {
    let (mut branch, mut files, mut untracked) = (None, Vec::new(), Vec::new());
    for e in s.split(|b| *b == 0).filter(|e| e.len() > 3) {
        let (xy, path) = e.split_at(3);
        let path = String::from_utf8_lossy(path).into_owned();
        if xy == b"## " {
            let name = path.split("...").next().unwrap_or_default();
            branch = Some(name.trim_start_matches("No commits yet on ").to_owned());
            continue;
        }
        let what = match (xy[0], xy[1]) {
            (b'?', _) => "new",
            (b'!', _) => continue,
            (b'U', _) | (_, b'U') | (b'A', b'A') | (b'D', b'D') => "conflict",
            (b'D', _) | (_, b'D') => "removed",
            (b'A', _) => "new",
            _ => "changed",
        };
        if xy[0] == b'?' {
            untracked.push(path.clone());
        }
        files.push(Changed { path, what: what.into() });
    }
    (branch, files, untracked)
}

/// What is not tracked, as new. A link, or what is kept from agents, is named and not shown.
fn new_files(site: &Site, all: Vec<String>) -> (String, bool) {
    let mut out = String::new();
    let mut whole = all.len() <= FILES;
    for path in all.into_iter().take(FILES) {
        if out.len() > MOST {
            return (out, false);
        }
        let Ok(place) = site.workspace.join(&path) else { continue };
        if site.keep.hides(&place) {
            continue;
        }
        let spot = fs::find(std::slice::from_ref(&site.workspace), &place);
        // Not what a link leads to: that is another file.
        let Ok(spot) = spot.map_err(drop).and_then(|s| (s.place == place).then_some(s).ok_or(()))
        else {
            continue;
        };
        match spot.read(FILE) {
            Ok(c) => match String::from_utf8(c.bytes) {
                Ok(t) if !t.contains('\0') => out.push_str(&text::diff(&path, None, Some(&t))),
                _ => {}
            },
            Err(fs::FsError::Large(..)) => whole = false,
            Err(_) => {}
        }
    }
    (out, whole)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_status_is_read() {
        let s = b"## main...origin/main [ahead 1]\0 M a.txt\0A  new.rs\0D  gone\0?? un tracked\0UU both\0!! ignored\0";
        let (branch, files, untracked) = status(s);
        assert_eq!(branch.as_deref(), Some("main"));
        let all: Vec<(&str, &str)> =
            files.iter().map(|f| (f.path.as_str(), f.what.as_str())).collect();
        assert_eq!(
            all,
            [
                ("a.txt", "changed"),
                ("new.rs", "new"),
                ("gone", "removed"),
                ("un tracked", "new"),
                ("both", "conflict")
            ]
        );
        assert_eq!(untracked, ["un tracked"]);
        assert_eq!(status(b"## No commits yet on trunk\0").0.as_deref(), Some("trunk"));
        assert_eq!(status(b"").1, []);
    }
}
