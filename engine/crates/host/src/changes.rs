//! What has been changed in a session's folder since its last commit, as git has it. Git runs in the
//! sandbox, reading only and without a network, whatever a repository's configuration would have it
//! run (SI-6, SI-22); what is not tracked is read by handle.

use std::time::Duration;

use axiom_policy::{Caps, Place, Profile};
use axiom_proto::{
    AgentId, Author, Changed, Changes, CommitInfo, CommitShown, Committed, CopySeen, SessionId,
    WireError,
};
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

/// What a git that a person had run may write (decision 0018).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Writes {
    /// The repository's own `.git`, but for what is kept in it.
    Git,
    /// That, and the folder: a fast-forward.
    Folder,
}

/// What git said when it was done: whether it did it, and in its own words why not.
struct Said {
    ok: bool,
    err: String,
}

impl Git {
    /// The repository's own `.git`, if it is a folder: a worktree's or a submodule's is a file
    /// that leads elsewhere, which is not written here.
    fn git_dir(&self) -> Result<Place, WireError> {
        let d = self.site.workspace.join(".git").map_err(|e| bad(e.to_string()))?;
        match std::fs::symlink_metadata(d.as_path()) {
            Ok(m) if m.is_dir() => Ok(d),
            _ => Err(bad(
                "this repository keeps its history elsewhere (a worktree or a submodule): commit in a terminal",
            )),
        }
    }

    /// Runs git as a person's own act, writing what `writes` says, with `input` on its standard
    /// input. Its output is not kept; its complaint is.
    async fn act(
        &self,
        args: &[&str],
        writes: Writes,
        input: Option<&[u8]>,
    ) -> Result<Said, WireError> {
        let s = &self.site;
        let caps = Caps { run: true, ..s.caps.clone() };
        let profile = Profile::persons_git(
            &caps,
            &s.keep,
            &s.system,
            &s.scratch,
            &self.git_dir()?,
            writes == Writes::Folder,
        )
        .map_err(|e| bad(e.to_string()))?;
        let first = [
            "--no-pager",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "diff.external=",
            "-c",
            "protocol.allow=never",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "tag.gpgsign=false",
            "-c",
            "core.editor=false",
        ];
        let mut env = Env::new(&s.path, &s.scratch, &s.scratch);
        env.set("GIT_CONFIG_NOSYSTEM", "1").map_err(|e| bad(e.to_string()))?;
        let job = Job {
            program: self.program.clone(),
            args: first.iter().chain(args).map(|a| (*a).to_owned()).collect(),
            cwd: s.workspace.clone(),
            env,
            stdin: input.is_some(),
            contain: Contain::Sandbox(profile),
        };
        let mut p = spawn(job).map_err(|e| bad(format!("git cannot be run: {e}")))?;
        if let (Some(mut i), Some(text)) = (p.stdin.take(), input) {
            use tokio::io::AsyncWriteExt;
            let _ = i.write_all(text).await;
        }
        let (Some(mut out), Some(err)) = (p.stdout.take(), p.stderr.take()) else {
            return Err(bad("git cannot be run"));
        };
        let mut words = Vec::new();
        let mut err = err.take(16 * 1024);
        let read = async {
            let mut sink = [0; 4096];
            let quiet = async { while out.read(&mut sink).await.is_ok_and(|n| n > 0) {} };
            let _ = tokio::join!(quiet, err.read_to_end(&mut words));
        };
        if tokio::time::timeout(LONGEST * 3, read).await.is_err() {
            p.kill();
        }
        let ended = p.wait().await.map_err(|e| bad(e.to_string()))?;
        Ok(Said { ok: ended.code == Some(0), err: text::plain(&words).trim().to_owned() })
    }

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

/// Folders that are never looked into for repositories: what is built or fetched.
const NOT_IN: &[&str] = &["node_modules", "target", "build", "dist", "vendor", ".venv", "venv"];
/// Repositories found in one folder, at most.
const REPOSITORIES: usize = 32;

/// The repositories of a folder: its own (`""`), and those at most two levels down, never in a
/// hidden or built folder, and never through a link.
pub fn repositories(root: &Place) -> Vec<String> {
    let mut out = Vec::new();
    let is_repo = |p: &std::path::Path| std::fs::symlink_metadata(p.join(".git")).is_ok();
    if is_repo(root.as_path()) {
        out.push(String::new());
    }
    let mut todo = vec![(root.as_path().to_path_buf(), String::new(), 0)];
    while let Some((dir, rel, depth)) = todo.pop() {
        if depth >= 2 {
            continue;
        }
        let Ok(all) = std::fs::read_dir(&dir) else { continue };
        let mut names: Vec<String> = all
            .flatten()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().to_str().map(str::to_owned))
            .filter(|n| !n.starts_with('.') && !NOT_IN.contains(&n.as_str()))
            .collect();
        names.sort();
        for n in names.into_iter().rev().take(1000) {
            let r = if rel.is_empty() { n.clone() } else { format!("{rel}/{n}") };
            let p = dir.join(&n);
            if is_repo(&p) {
                out.push(r.clone());
            }
            todo.push((p, r, depth + 1));
        }
        if out.len() >= REPOSITORIES {
            break;
        }
    }
    out.sort();
    out.truncate(REPOSITORIES);
    out
}

/// A branch or a commit as a person names it: never something git would take for an option.
pub fn revision(r: &str) -> Result<&str, WireError> {
    let ok = !r.is_empty()
        && r.len() <= 200
        && !r.starts_with('-')
        && !r.contains("..")
        && r.chars().all(|c| c.is_ascii_alphanumeric() || "._/@^~-{}".contains(c));
    if ok { Ok(r) } else { Err(bad("that is no name of a branch or a commit")) }
}

/// Git, as a person looks at a repository of a session's folder.
async fn looking_at(
    agents: &std::sync::Arc<Agents>,
    session: &SessionId,
    workspace: String,
    repository: Option<String>,
) -> Result<Option<Git>, WireError> {
    let a = agents.clone();
    // Its scratch is nobody's but this session's.
    let id = AgentId(format!("git_{}", session.0));
    let site = tokio::task::spawn_blocking(move || {
        let top = a.looking(&workspace, &id)?;
        let Some(r) = repository.filter(|r| !r.is_empty()) else { return Ok(top) };
        if !repositories(&top.workspace).contains(&r) {
            return Err(bad("no such repository in the folder"));
        }
        a.looking(&format!("{workspace}/{r}"), &id)
    })
    .await
    .map_err(|e| bad(e.to_string()))??;
    if !site.workspace.as_path().join(".git").exists() {
        return Ok(None);
    }
    let Some(program) = git(&site) else { return Err(bad("git was not found")) };
    Ok(Some(Git { program, site }))
}

/// What there is to say of the folder; `repository` is false if git has nothing to do with it.
pub async fn changes(
    agents: &std::sync::Arc<Agents>,
    session: &SessionId,
    workspace: String,
    repository: Option<String>,
    against: Option<(String, bool)>,
) -> Result<Changes, WireError> {
    let top = Place::new(&workspace).map_err(|e| bad(e.to_string()))?;
    let found = tokio::task::spawn_blocking(move || repositories(&top))
        .await
        .map_err(|e| bad(e.to_string()))?;
    let none = Changes { repositories: found.clone(), ..Changes::default() };
    // Of a folder that is no repository but holds some, the first is shown.
    let repository =
        repository.or_else(|| found.first().cloned().filter(|_| !found.contains(&String::new())));
    let Some(git) = looking_at(agents, session, workspace, repository).await? else {
        return Ok(none);
    };
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
    let head =
        |h: Option<(Vec<u8>, bool)>| h.map(|(h, _)| String::from_utf8_lossy(&h).trim().to_owned());
    let since = match &against {
        None => head(git.run(&["rev-parse", "--quiet", "--verify", "HEAD"]).await?)
            .unwrap_or_else(|| NOTHING.into()),
        Some((r, base)) => {
            let r = revision(r)?;
            let found = if *base {
                git.run(&["merge-base", "HEAD", "--end-of-options", r]).await?
            } else {
                git.run(&[
                    "rev-parse",
                    "--quiet",
                    "--verify",
                    "--end-of-options",
                    &format!("{r}^{{commit}}"),
                ])
                .await?
            };
            head(found).ok_or_else(|| bad(format!("{r} is not known to this repository")))?
        }
    };
    let (upstream, ahead, behind) = upstream(&git).await?;
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
    let default_branch = default_branch(&git).await?;
    let hooks = has_hooks(&git.site.workspace);
    // What each is now, so that a commit takes it as it was shown.
    let (root, paths) =
        (git.site.workspace.clone(), files.iter().map(|f| f.path.clone()).collect::<Vec<_>>());
    let digests = tokio::task::spawn_blocking(move || {
        paths.iter().map(|p| digest_now(&root, p).ok().flatten()).collect::<Vec<_>>()
    })
    .await
    .map_err(|e| bad(e.to_string()))?;
    for (f, d) in files.iter_mut().zip(digests) {
        f.digest = d;
    }
    let (remote, remote_url) = match remote_of(&git, branch.as_deref()).await? {
        Some((r, u)) => (Some(r), u),
        None => (None, None),
    };
    let site = git.site;
    let (more, rest) = tokio::task::spawn_blocking(move || new_files(&site, untracked))
        .await
        .map_err(|e| bad(e.to_string()))?;
    diff.push_str(&more);
    whole &= rest;
    Ok(Changes {
        repository: true,
        branch,
        files,
        diff,
        whole,
        repositories: none.repositories,
        upstream,
        ahead,
        behind,
        against: against.map(|(r, base)| if base { format!("where {r} parted") } else { r }),
        default_branch,
        hooks,
        remote,
        remote_url,
    })
}

/// The branch the one that is out follows, and how far apart they are, as of the last fetch.
async fn upstream(git: &Git) -> Result<(Option<String>, Option<u32>, Option<u32>), WireError> {
    let name =
        git.run(&["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{upstream}"]).await?;
    let Some(name) = name.map(|(n, _)| String::from_utf8_lossy(&n).trim().to_owned()) else {
        return Ok((None, None, None));
    };
    let counts = git.run(&["rev-list", "--left-right", "--count", "HEAD...@{upstream}"]).await?;
    let counts = counts.map(|(c, _)| String::from_utf8_lossy(&c).into_owned()).unwrap_or_default();
    let mut n = counts.split_whitespace().filter_map(|x| x.parse().ok());
    Ok((Some(name), n.next(), n.next()))
}

/// The unit separators git is asked to put between fields and commits.
const FIELD: char = '\u{1f}';
const ONE: char = '\u{1e}';
const FORMAT: &str = "--format=%H%x1f%an%x1f%at%x1f%s%x1e";

fn commits(said: &[u8]) -> Vec<CommitInfo> {
    String::from_utf8_lossy(said)
        .split(ONE)
        .filter_map(|c| {
            let mut f = c.trim_start_matches('\n').split(FIELD);
            Some(CommitInfo {
                id: f.next()?.to_owned(),
                author: f.next()?.to_owned(),
                at: f.next()?.parse().ok()?,
                subject: f.next()?.to_owned(),
            })
        })
        .filter(|c| c.id.len() >= 40)
        .collect()
}

/// The last commits, newest first.
pub async fn log(
    agents: &std::sync::Arc<Agents>,
    session: &SessionId,
    workspace: String,
    repository: Option<String>,
    from: Option<String>,
    most: u32,
) -> Result<Vec<CommitInfo>, WireError> {
    let Some(git) = looking_at(agents, session, workspace, repository).await? else {
        return Err(bad("the folder is not a git repository"));
    };
    let n = format!("-n{}", most.clamp(1, 500));
    let mut args = vec!["log", "--no-color", FORMAT, &n];
    let from = from.as_deref().map(revision).transpose()?;
    if let Some(r) = from {
        args.extend(["--end-of-options", r]);
    }
    let said = git.run(&args).await?;
    Ok(said.map(|(s, _)| commits(&s)).unwrap_or_default())
}

/// One commit, with what it changed.
pub async fn show(
    agents: &std::sync::Arc<Agents>,
    session: &SessionId,
    workspace: String,
    repository: Option<String>,
    commit: &str,
) -> Result<CommitShown, WireError> {
    let commit = revision(commit)?;
    let Some(git) = looking_at(agents, session, workspace, repository).await? else {
        return Err(bad("the folder is not a git repository"));
    };
    let c = format!("{commit}^{{commit}}");
    let head = git.run(&["log", "-n1", "--no-color", FORMAT, "--end-of-options", &c]).await?;
    let info = head.map(|(h, _)| commits(&h)).unwrap_or_default().into_iter().next();
    let info = info.ok_or_else(|| bad(format!("{commit} is not known to this repository")))?;
    let message =
        git.run(&["log", "-n1", "--no-color", "--format=%B", "--end-of-options", &c]).await?;
    let message =
        message.map(|(m, _)| String::from_utf8_lossy(&m).trim_end().to_owned()).unwrap_or_default();
    let diff = git
        .run(&[
            "show",
            "--format=",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--no-renames",
            "--ignore-submodules=all",
            "--src-prefix=",
            "--dst-prefix=",
            "--end-of-options",
            &c,
        ])
        .await?;
    let (diff, whole) = diff.unwrap_or_default();
    Ok(CommitShown {
        commit: info,
        message,
        diff: String::from_utf8_lossy(&diff).into_owned(),
        whole,
    })
}

/// Branches, its own first and then the remotes'.
pub async fn branches(
    agents: &std::sync::Arc<Agents>,
    session: &SessionId,
    workspace: String,
    repository: Option<String>,
) -> Result<Vec<String>, WireError> {
    let Some(git) = looking_at(agents, session, workspace, repository).await? else {
        return Ok(Vec::new());
    };
    let said = git
        .run(&[
            "for-each-ref",
            "--format=%(refname:short)",
            "--count=500",
            "refs/heads",
            "refs/remotes",
        ])
        .await?;
    let said = said.map(|(s, _)| String::from_utf8_lossy(&s).into_owned()).unwrap_or_default();
    Ok(said.lines().filter(|l| !l.is_empty() && !l.ends_with("/HEAD")).map(str::to_owned).collect())
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
        files.push(Changed { path, what: what.into(), digest: None });
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

/// The branch that is the repository's own: what its remote calls `HEAD`, or `main` or `master`.
async fn default_branch(git: &Git) -> Result<Option<String>, WireError> {
    let remote =
        git.run(&["symbolic-ref", "--quiet", "--short", "refs/remotes/origin/HEAD"]).await?;
    if let Some((r, _)) = remote {
        let r = String::from_utf8_lossy(&r).trim().to_owned();
        if let Some(b) = r.strip_prefix("origin/") {
            return Ok(Some(b.to_owned()));
        }
    }
    for b in ["main", "master"] {
        let r = format!("refs/heads/{b}");
        if git.run(&["show-ref", "--verify", "--quiet", &r]).await?.is_some() {
            return Ok(Some(b.into()));
        }
    }
    Ok(None)
}

/// Whether the repository has hooks of its own, which Axiom does not run.
fn has_hooks(root: &Place) -> bool {
    let r = root.as_path();
    let own = std::fs::read_dir(r.join(".git/hooks"))
        .into_iter()
        .flatten()
        .flatten()
        .any(|e| !e.file_name().to_string_lossy().ends_with(".sample"));
    let config = std::fs::read_to_string(r.join(".git/config")).unwrap_or_default();
    own || r.join(".husky").exists()
        || r.join(".pre-commit-config.yaml").exists()
        || config.to_lowercase().contains("hookspath")
}

/// What the person's `~/.gitconfig` says of them: name, address, and whether commits are signed.
/// Only its own lines: what it includes is not followed.
pub fn identity(home: &std::path::Path) -> (Option<Author>, bool) {
    let Ok(text) = std::fs::read_to_string(home.join(".gitconfig")) else { return (None, false) };
    let (mut section, mut name, mut email, mut sign) = (String::new(), None, None, false);
    for l in text.lines().take(10_000) {
        let l = l.trim();
        if l.starts_with('[') {
            section = l.trim_matches(['[', ']']).trim().to_lowercase();
            continue;
        }
        let Some((k, v)) = l.split_once('=') else { continue };
        let (k, v) = (k.trim().to_lowercase(), v.trim().trim_matches('"').to_owned());
        match (section.as_str(), k.as_str()) {
            ("user", "name") => name = Some(v),
            ("user", "email") => email = Some(v),
            ("commit", "gpgsign") => sign = v.eq_ignore_ascii_case("true"),
            _ => {}
        }
    }
    (name.zip(email).map(|(name, email)| Author { name, email }), sign)
}

fn plain_line(s: &str, what: &str) -> Result<(), WireError> {
    if s.trim().is_empty() || s.len() > 200 || s.chars().any(char::is_control) {
        return Err(bad(format!("{what} must be a line of at most 200 bytes")));
    }
    Ok(())
}

/// A file as it is now, by the same digest as copies have: absent if there is none.
fn digest_now(root: &Place, path: &str) -> Result<Option<String>, WireError> {
    let at = crate::files::below(root, path)?;
    let spot = fs::find(std::slice::from_ref(root), &at).map_err(|e| bad(e.to_string()))?;
    if spot.place != at {
        return Err(bad(format!("{path} leads elsewhere")));
    }
    match spot.read(64 * 1024 * 1024) {
        Ok(c) => Ok(Some(c.digest)),
        Err(fs::FsError::Missing(_)) => Ok(None),
        Err(e) => Err(bad(e.to_string())),
    }
}

/// A person's commit (decision 0018): the files they chose, as they saw them.
#[allow(clippy::too_many_arguments)]
pub async fn commit(
    agents: &std::sync::Arc<Agents>,
    session: &SessionId,
    workspace: String,
    repository: Option<String>,
    files: Vec<CopySeen>,
    message: String,
    branch: Option<String>,
    onto_default: bool,
    author: Option<Author>,
    home: Option<std::path::PathBuf>,
) -> Result<Committed, WireError> {
    if files.is_empty() || files.len() > FILES {
        return Err(bad(format!("choose between 1 and {FILES} files")));
    }
    if message.trim().is_empty() || message.len() > 64 * 1024 || message.contains('\0') {
        return Err(bad("a commit needs a message, of at most 64 KiB"));
    }
    let (theirs, signs) = home.as_deref().map(identity).unwrap_or((None, false));
    let who = author
        .or(theirs)
        .ok_or_else(|| bad("say who commits: git has no name and address for you"))?;
    plain_line(&who.name, "a name")?;
    plain_line(&who.email, "an address")?;
    let Some(git) = looking_at(agents, session, workspace, repository).await? else {
        return Err(bad("the folder is not a git repository"));
    };
    git.git_dir()?;
    let root = git.site.workspace.clone();
    // Each as it was shown, or nothing.
    let paths: Vec<String> = files.iter().map(|f| f.path.clone()).collect();
    let (r, seen) = (root.clone(), files.clone());
    let now = tokio::task::spawn_blocking(move || {
        seen.iter().map(|f| digest_now(&r, &f.path)).collect::<Result<Vec<_>, _>>()
    })
    .await
    .map_err(|e| bad(e.to_string()))??;
    for (f, d) in files.iter().zip(&now) {
        if f.digest != *d {
            return Err(bad(format!("{} has changed since you saw it: look again", f.path)));
        }
    }
    let on = git.run(&["rev-parse", "--abbrev-ref", "HEAD"]).await?;
    let on = on.map(|(b, _)| String::from_utf8_lossy(&b).trim().to_owned()).unwrap_or_default();
    if let Some(b) = &branch {
        revision(b)?;
        if b.contains(['^', '~', '{', '}', '@']) {
            return Err(bad("that is no name of a branch"));
        }
        let made = git.act(&["switch", "-q", "-c", b], Writes::Git, None).await?;
        if !made.ok {
            return Err(bad(format!("the branch was not made: {}", made.err)));
        }
    } else if !onto_default && default_branch(&git).await?.is_some_and(|d| d == on) {
        return Err(bad(format!("{on} is the repository's own branch: say so, or name a new one")));
    }
    // What git does not know yet is told to it first.
    let tracked = tracked(&git.site).await?;
    let fresh: Vec<&str> =
        paths.iter().map(String::as_str).filter(|p| !tracked.iter().any(|t| t == p)).collect();
    if !fresh.is_empty() {
        let mut args = vec!["add", "--"];
        args.extend(fresh.iter().copied());
        let added = git.act(&args, Writes::Git, None).await?;
        if !added.ok {
            return Err(bad(format!("git did not take them: {}", added.err)));
        }
    }
    let (n, e) = (format!("user.name={}", who.name), format!("user.email={}", who.email));
    let mut args =
        vec!["-c", &n, "-c", &e, "commit", "--only", "--no-verify", "-q", "-F", "-", "--"];
    args.extend(paths.iter().map(String::as_str));
    let done = git.act(&args, Writes::Git, Some(message.as_bytes())).await?;
    if !done.ok {
        return Err(bad(format!("nothing was committed: {}", done.err)));
    }
    let head = git.run(&["log", "-n1", "--no-color", FORMAT]).await?;
    let commit =
        head.map(|(h, _)| commits(&h)).unwrap_or_default().into_iter().next().unwrap_or_default();
    let (r, seen) = (root.clone(), files.clone());
    let after = tokio::task::spawn_blocking(move || {
        seen.iter().map(|f| digest_now(&r, &f.path).ok().flatten()).collect::<Vec<_>>()
    })
    .await
    .map_err(|e| bad(e.to_string()))?;
    let changed = files
        .iter()
        .zip(after)
        .filter(|(f, d)| f.digest != *d)
        .map(|(f, _)| f.path.clone())
        .collect();
    Ok(Committed { commit, branch, hooks: has_hooks(&root), unsigned: signs, changed })
}

/// The remote a branch talks to (its own, or `origin`), with its address.
async fn remote_of(
    git: &Git,
    branch: Option<&str>,
) -> Result<Option<(String, Option<String>)>, WireError> {
    let named = match branch {
        Some(b) if revision(b).is_ok() => {
            let k = format!("branch.{b}.remote");
            git.run(&["config", "--get", &k])
                .await?
                .map(|(r, _)| String::from_utf8_lossy(&r).trim().to_owned())
        }
        _ => None,
    };
    let remote = named.unwrap_or_else(|| "origin".into());
    if revision(&remote).is_err() {
        return Ok(None);
    }
    let k = format!("remote.{remote}.url");
    let Some((url, _)) = git.run(&["config", "--get", &k]).await? else { return Ok(None) };
    Ok(Some((remote, Some(String::from_utf8_lossy(&url).trim().to_owned()))))
}

/// Keys a repository's own configuration may hold for Axiom to talk to its remote outside the
/// sandbox: names of remotes and branches, the layout, the person's name. Anything else might name
/// a program to run there (decision 0018).
fn key_may(key: &str) -> bool {
    let k = key.to_ascii_lowercase();
    let (section, rest) = k.split_once('.').unwrap_or((&k, ""));
    let last = rest.rsplit('.').next().unwrap_or(rest);
    let sub = rest.contains('.');
    match section {
        "remote" => {
            sub && matches!(last, "url" | "pushurl" | "fetch" | "push" | "tagopt" | "prune")
        }
        "branch" => sub && matches!(last, "remote" | "merge" | "rebase" | "pushremote"),
        "core" => {
            !sub && matches!(
                last,
                "repositoryformatversion"
                    | "filemode"
                    | "bare"
                    | "logallrefupdates"
                    | "ignorecase"
                    | "precomposeunicode"
                    | "symlinks"
                    | "autocrlf"
                    | "eol"
                    | "safecrlf"
                    | "sparsecheckout"
            )
        }
        "extensions" => true,
        "user" => !sub && matches!(last, "name" | "email"),
        "pull" => !sub && matches!(last, "rebase" | "ff"),
        "push" => !sub && matches!(last, "default" | "autosetupremote"),
        "fetch" => !sub && matches!(last, "prune" | "prunetags"),
        "init" => !sub && last == "defaultbranch",
        "gc" => !sub && matches!(last, "auto" | "autopacklimit" | "reflogexpire" | "pruneexpire"),
        "lfs" => sub && matches!(last, "access" | "locksverify"),
        "submodule" => sub && matches!(last, "url" | "active"),
        _ => false,
    }
}

/// The repository's own configuration, as git reads it without the person's or the system's:
/// the first key that may not be there, if there is one.
async fn config_refused(git: &Git) -> Result<Option<String>, WireError> {
    let Some((all, whole)) = git.run(&["config", "--list", "--show-scope", "-z"]).await? else {
        return Ok(None);
    };
    if !whole {
        return Ok(Some("(more than can be read)".into()));
    }
    let parts: Vec<&[u8]> = all.split(|b| *b == 0).collect();
    for pair in parts.chunks(2) {
        let [scope, entry] = pair else { continue };
        let scope = String::from_utf8_lossy(scope);
        if !matches!(scope.as_ref(), "local" | "worktree") {
            continue;
        }
        let entry = String::from_utf8_lossy(entry);
        let key = entry.split('\n').next().unwrap_or_default();
        if !key_may(key) {
            return Ok(Some(key.to_owned()));
        }
        if key.to_ascii_lowercase().ends_with("url") {
            let value = entry.split_once('\n').map_or("", |v| v.1);
            if value.starts_with("ext::")
                || value.contains("::")
                || value.trim_start().starts_with('-')
            {
                return Ok(Some(key.to_owned()));
            }
        }
    }
    Ok(None)
}

impl Git {
    /// Runs git outside the sandbox as the person's own act, to talk to a remote with their own
    /// credentials (decision 0018): no hooks, no programs of the repository's, asks nobody.
    async fn outside(&self, args: &[&str], env: Env) -> Result<Said, WireError> {
        let first = [
            "--no-pager",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "protocol.allow=never",
            "-c",
            "protocol.https.allow=always",
            "-c",
            "protocol.ssh.allow=always",
            "-c",
            "protocol.file.allow=always",
            "-c",
            "submodule.recurse=false",
            "-c",
            "core.askPass=",
        ];
        let job = Job {
            program: self.program.clone(),
            args: first.iter().chain(args).map(|a| (*a).to_owned()).collect(),
            cwd: self.site.workspace.clone(),
            env,
            stdin: false,
            contain: Contain::None(axiom_policy::Unsandboxed::persons(
                axiom_policy::PersonsOwn::Remote,
            )),
        };
        let mut p = spawn(job).map_err(|e| bad(format!("git cannot be run: {e}")))?;
        let (Some(mut out), Some(err)) = (p.stdout.take(), p.stderr.take()) else {
            return Err(bad("git cannot be run"));
        };
        let mut words = Vec::new();
        let mut err = err.take(16 * 1024);
        let read = async {
            let mut sink = [0; 4096];
            let quiet = async { while out.read(&mut sink).await.is_ok_and(|n| n > 0) {} };
            let _ = tokio::join!(quiet, err.read_to_end(&mut words));
        };
        if tokio::time::timeout(Duration::from_secs(300), read).await.is_err() {
            p.kill();
        }
        let ended = p.wait().await.map_err(|e| bad(e.to_string()))?;
        Ok(Said { ok: ended.code == Some(0), err: text::plain(&words).trim().to_owned() })
    }
}

/// Where a push or a pull goes: the remote, its address, the branch that is out and the one it
/// is sent to or taken from. Refused if the repository's configuration holds what may run.
async fn talking(git: &Git) -> Result<(String, String, String, String), WireError> {
    if let Some(k) = config_refused(git).await? {
        return Err(bad(format!(
            "the repository's own configuration says {k}, which Axiom does not run with your credentials: push or pull in a terminal"
        )));
    }
    let on = git.run(&["symbolic-ref", "--quiet", "--short", "HEAD"]).await?;
    let on = on.map(|(b, _)| String::from_utf8_lossy(&b).trim().to_owned());
    let on = on.filter(|b| revision(b).is_ok()).ok_or_else(|| bad("no branch is out"))?;
    let (remote, url) =
        remote_of(git, Some(&on)).await?.ok_or_else(|| bad("the repository has no remote"))?;
    let url = url.unwrap_or_default();
    let merge = git.run(&["config", "--get", &format!("branch.{on}.merge")]).await?;
    let to = merge
        .map(|(m, _)| {
            String::from_utf8_lossy(&m).trim().trim_start_matches("refs/heads/").to_owned()
        })
        .filter(|b| revision(b).is_ok())
        .unwrap_or_else(|| on.clone());
    Ok((remote, url, on, to))
}

/// What a person's push or pull is told to do, for the record before it is done.
pub async fn remote_words(
    agents: &std::sync::Arc<Agents>,
    session: &SessionId,
    workspace: String,
    repository: Option<String>,
) -> Result<String, WireError> {
    let Some(git) = looking_at(agents, session, workspace, repository).await? else {
        return Err(bad("the folder is not a git repository"));
    };
    let (remote, url, on, to) = talking(&git).await?;
    Ok(format!("{on} to {to} of {remote} ({url})"))
}

/// Sends the branch that is out, as a fast-forward only.
pub async fn push(
    agents: &std::sync::Arc<Agents>,
    session: &SessionId,
    workspace: String,
    repository: Option<String>,
    env: Env,
) -> Result<String, WireError> {
    let Some(git) = looking_at(agents, session, workspace, repository).await? else {
        return Err(bad("the folder is not a git repository"));
    };
    let (remote, _, on, to) = talking(&git).await?;
    let spec = format!("refs/heads/{on}:refs/heads/{to}");
    let said = git
        .outside(&["push", "--no-recurse-submodules", "--porcelain", "--", &remote, &spec], env)
        .await?;
    if !said.ok {
        return Err(bad(format!("it was not pushed: {}", said.err)));
    }
    Ok(format!("{on} is on {remote} as {to}"))
}

/// Takes what the remote has of the branch that is followed, as a fast-forward only. What would
/// change a protected name is refused before anything changes.
pub async fn pull(
    agents: &std::sync::Arc<Agents>,
    session: &SessionId,
    workspace: String,
    repository: Option<String>,
    env: Env,
) -> Result<String, WireError> {
    let Some(git) = looking_at(agents, session, workspace, repository).await? else {
        return Err(bad("the folder is not a git repository"));
    };
    let (remote, _, on, to) = talking(&git).await?;
    let spec = format!("refs/heads/{to}:refs/remotes/{remote}/{to}");
    let said = git
        .outside(&["fetch", "--no-recurse-submodules", "--no-tags", "--", &remote, &spec], env)
        .await?;
    if !said.ok {
        return Err(bad(format!("nothing was fetched: {}", said.err)));
    }
    let theirs = format!("refs/remotes/{remote}/{to}");
    let names = git
        .run_to(&["diff", "--name-only", "-z", "--no-renames", "HEAD", &theirs, "--"], TRACKED)
        .await?;
    let names = names.map(|(n, _)| n).unwrap_or_default();
    for n in names.split(|b| *b == 0).filter(|n| !n.is_empty()) {
        let n = String::from_utf8_lossy(n);
        if n.split('/').any(|c| axiom_policy::paths::protected_name(c).is_some()) {
            return Err(bad(format!(
                "it would change {n}, which Axiom does not write: pull in a terminal"
            )));
        }
    }
    let merged =
        git.act(&["merge", "--ff-only", "--no-edit", "-q", &theirs], Writes::Folder, None).await?;
    if !merged.ok {
        return Err(bad(format!("it is not a fast-forward, or it did not apply: {}", merged.err)));
    }
    Ok(format!("{on} is where {remote}'s {to} is"))
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

    #[test]
    fn a_revision_is_never_an_option() {
        for ok in ["main", "origin/main", "HEAD~2", "v1.2", "a@{u}", "c0ffee"] {
            assert!(revision(ok).is_ok(), "{ok}");
        }
        for no in ["-p", "--output=x", "a..b", "a b", "", "x;y", "$(x)", &"a".repeat(201)] {
            assert!(revision(no).is_err(), "{no}");
        }
    }

    #[test]
    fn what_a_repository_may_configure_for_a_push() {
        for ok in [
            "remote.origin.url",
            "remote.a.b.fetch",
            "branch.main.merge",
            "core.bare",
            "user.email",
            "extensions.objectformat",
            "Remote.Origin.URL",
        ] {
            assert!(key_may(ok), "{ok}");
        }
        for no in [
            "core.sshcommand",
            "core.gitproxy",
            "credential.helper",
            "credential.https://x.helper",
            "remote.origin.uploadpack",
            "remote.origin.receivepack",
            "remote.origin.proxy",
            "url.https://x.insteadof",
            "include.path",
            "includeif.gitdir:/x.path",
            "http.proxy",
            "core.hookspath",
            "core.fsmonitor",
            "filter.lfs.clean",
            "remote.url",
            "alias.x",
            "submodule.x.update",
        ] {
            assert!(!key_may(no), "{no}");
        }
    }
}
