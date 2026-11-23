//! Checkpoints: a shadow git repository per workspace, driven through the `git` program
//! (ADR-0015 §5; `docs/protocols/agents.md` §Checkpoints; M3-CKPT-001).
//!
//! - **Isolation.** The repository is `<data dir>/checkpoints/<workspace key>/`, with its own
//!   index and `GIT_WORK_TREE` = the workspace root. Every `GIT_*` variable is removed and
//!   there is no system/global config, no hooks, no fsmonitor and no autocrlf. The user's
//!   `.git` (index, refs, stash, config) is never read or written, so no filter driver, hook or
//!   fsmonitor the workspace's repositories configure ever runs (a `.gitattributes` naming one
//!   finds no driver in the only configuration read).
//! - **Bounded.** Every `git` is stopped (killed) when it runs past its bound:
//!   [`TREE_TIMEOUT`] for the commands that walk the workspace, [`OBJECT_TIMEOUT`] for the
//!   others; the checkpoint then fails with [`CheckpointError::TimedOut`].
//! - **What a snapshot captures.** Everything `git add -A` would, under the workspace's own
//!   ignore files, minus files over 16 MiB (listed as skipped). Nested repositories are
//!   recorded as git links, not their content; one with no commit yet (nothing to link) and an
//!   unreadable file are left out, and the checkpoint is taken without them.
//! - **Refs.** A checkpoint is a commit at `refs/axiom/<session>/<nnnn>`; its subject is a JSON
//!   line with its metadata. `refs/axiom/<session>/post` is the newest state the agent left
//!   (taken after each writing tool).
//! - **Restore** to checkpoint C restores each file that differs between C and the workspace now
//!   only if its content is what the agent last left; anything else is a conflict and is left
//!   alone. A restore is preceded by a checkpoint, so it can be undone. Paths inside a git
//!   directory (or that would make one), anything at or under a nested repository, and what a
//!   snapshot could not capture (then or now, an ignored file on disk included) are never
//!   restored; they are reported ([`RestoreOutcome::not_restored`]).
//! - **Locked.** An operation that changes the store holds its lock: within the process, and a
//!   lock file in the shadow directory across processes. Held, a lock of git's found there (the
//!   index's, `HEAD`'s, `packed-refs`', or a ref's under `refs/axiom/`) is stale (a git killed at
//!   its bound, or with its daemon) and is removed.

use axiom_protocol::agent::{CheckpointInfo, ConflictReason, RestoreConflict, RestoreResult};
use axiom_protocol::WorkspacePath;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

/// The bound on one `git` that walks the workspace (`ls-files`, `add`, `rm`, `write-tree`,
/// `ls-tree`). This repository's first checkpoint takes about 1.5 s (`add` hashes every file),
/// later ones under 0.1 s; ten minutes leaves room for a workspace hundreds of times larger.
pub const TREE_TIMEOUT: Duration = Duration::from_secs(600);
/// The bound on any other `git` (one object or ref: milliseconds normally).
pub const OBJECT_TIMEOUT: Duration = Duration::from_secs(60);

/// The store's private index, in the shadow directory.
const INDEX: &str = "axiom-index";
/// The file the store's operations lock, across processes (two daemons on one data dir), in the
/// shadow directory.
const STORE_LOCK: &str = "axiom.lock";

/// What makes a folder a git directory (the gate's own list, `gate::makes_git_dir`): a restore
/// that writes two of them into one folder could make it one (review H8).
const GIT_DIR_MARKERS: &[&str] = &["HEAD", "commondir", "objects", "refs"];

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CheckpointError {
    #[error("git is not installed (checkpoints need it)")]
    GitMissing,
    #[error("git failed: {0}")]
    Git(String),
    /// A `git` ran past its bound (a lock, a stuck file system, a huge workspace) and was
    /// killed.
    #[error("git {command} did not finish within {} s and was stopped", .after.as_secs())]
    TimedOut { command: String, after: Duration },
    #[error("no checkpoint {0} in this session")]
    NotFound(u32),
    #[error("{} files changed since the agent last wrote them", .0.len())]
    Conflicts(Vec<RestoreConflict>),
    #[error("I/O error: {0}")]
    Io(String),
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Meta {
    label: String,
    turn: u32,
    created_ms: u64,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    skipped: Vec<String>,
    /// What `git add` could not capture (unreadable, a nested repository with no commit).
    /// Not on the wire: `CheckpointInfo::skipped` is the files over 16 MiB.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    left_out: Vec<String>,
}

impl Meta {
    /// Everything this checkpoint did not capture.
    fn uncaptured(&self) -> impl Iterator<Item = &String> {
        self.skipped.iter().chain(&self.left_out)
    }
}

/// One workspace's shadow repository. Calls are serialised (one private index).
pub struct Shadow {
    git: PathBuf,
    git_dir: PathBuf,
    root: PathBuf,
    tree_timeout: Duration,
    object_timeout: Duration,
    lock: tokio::sync::Mutex<()>,
}

/// The store's lock, held for one operation ([`Shadow::hold`]); released on drop.
struct Held<'a> {
    _mutex: tokio::sync::MutexGuard<'a, ()>,
    _file: Option<std::fs::File>,
}

/// Why a path that differs from the checkpoint was left as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotRestored {
    /// Inside a git directory (`.git`, or a `<name>.git` that is a repository), or one of git's
    /// own entries written into a folder that holds or would get what makes a git directory
    /// (the gate's `makes_git_dir` rule): hooks run from there, so it changes only through the
    /// gate, with the person's approval.
    GitDirectory,
    /// A nested repository (a git link in the checkpoint, now, or as the agent left the
    /// workspace), reported by its path: its content was never captured, and a restore never
    /// touches it or anything under it.
    NestedRepository,
    /// The checkpoint, or the snapshot of the workspace now, could not capture it (over
    /// 16 MiB, unreadable, inside a nested repository with no commit, or on disk but ignored
    /// now): what it held is unknown.
    NotCaptured,
}

/// A restore's result, with what it left out: the wire result ([`RestoreResult`]) has no field
/// for those, so the caller tells the person through [`RestoreOutcome::notice`].
#[derive(Debug, Clone, PartialEq)]
pub struct RestoreOutcome {
    pub result: RestoreResult,
    /// Paths that differ from the checkpoint and were not restored (and are not conflicts).
    pub not_restored: Vec<(WorkspacePath, NotRestored)>,
}

impl RestoreOutcome {
    /// A sentence for the person naming what was left as it is, or `None` when nothing was.
    pub fn notice(&self) -> Option<String> {
        if self.not_restored.is_empty() {
            return None;
        }
        let list = |why: NotRestored| -> Option<String> {
            let paths: Vec<&str> = self
                .not_restored
                .iter()
                .filter(|(_, w)| *w == why)
                .map(|(p, _)| p.0.as_str())
                .collect();
            if paths.is_empty() {
                return None;
            }
            const SHOWN: usize = 20;
            let mut s = paths[..paths.len().min(SHOWN)].join(", ");
            if paths.len() > SHOWN {
                s.push_str(&format!(" and {} more", paths.len() - SHOWN));
            }
            Some(s)
        };
        let n = self.not_restored.len();
        let mut text = format!(
            "Restoring checkpoint {} left {n} path{} as {} {}.",
            self.result.checkpoint,
            if n == 1 { "" } else { "s" },
            if n == 1 { "it" } else { "they" },
            if n == 1 { "is" } else { "are" },
        );
        if let Some(s) = list(NotRestored::GitDirectory) {
            text.push_str(&format!(
                " Inside a git directory, which changes only with your approval: {s}."
            ));
        }
        if let Some(s) = list(NotRestored::NestedRepository) {
            text.push_str(&format!(
                " Nested repositories, which a restore never changes: {s}."
            ));
        }
        if let Some(s) = list(NotRestored::NotCaptured) {
            text.push_str(&format!(
                " Not captured by the checkpoint or now (too large, unreadable, ignored, or in \
                 a repository with no commit), so their content is unknown: {s}."
            ));
        }
        Some(text)
    }
}

struct Snapshot {
    tree: String,
    skipped: Vec<String>,
    left_out: Vec<String>,
}

/// `path → (mode, blob id)` of a tree.
type Listing = BTreeMap<String, (String, String)>;

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

fn valid_session(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= 64
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl Shadow {
    /// `data_dir/checkpoints/<key>/` for `root`, using `git` from `PATH`.
    pub fn new(data_dir: &Path, root: &Path) -> Self {
        Self::with_git(data_dir, root, PathBuf::from("git"))
    }

    /// With an explicit git program (tests).
    pub fn with_git(data_dir: &Path, root: &Path, git: PathBuf) -> Self {
        Self {
            git,
            git_dir: data_dir
                .join("checkpoints")
                .join(crate::workspace_key(root)),
            root: root.to_path_buf(),
            tree_timeout: TREE_TIMEOUT,
            object_timeout: OBJECT_TIMEOUT,
            lock: tokio::sync::Mutex::new(()),
        }
    }

    /// Other bounds than [`TREE_TIMEOUT`] and [`OBJECT_TIMEOUT`] (tests).
    pub fn with_timeouts(mut self, tree: Duration, object: Duration) -> Self {
        self.tree_timeout = tree;
        self.object_timeout = object;
        self
    }

    /// The store's lock, for an operation that changes it: this `Shadow`'s, then the shadow
    /// directory's lock file, which a second daemon on the same data dir (one not listening
    /// serves its own client over stdio) takes too. Held, no other git of ours runs in the store,
    /// so a lock of git's found there (the index's, review H3, or a ref's, review I3) was left
    /// by a git that was killed (at its bound, or with its daemon), and is removed
    /// ([`remove_stale_locks`]). Where the file system has no file locks, the store is
    /// serialised within this process only, and those locks are left.
    async fn hold(&self) -> Result<Held<'_>, CheckpointError> {
        let mutex = self.lock.lock().await;
        let dir = self.git_dir.clone();
        let file = tokio::task::spawn_blocking(move || {
            let io = |e: std::io::Error| CheckpointError::Io(format!("{}: {e}", dir.display()));
            std::fs::create_dir_all(&dir).map_err(io)?;
            let f = std::fs::OpenOptions::new()
                .create(true)
                .truncate(false)
                .write(true)
                .open(dir.join(STORE_LOCK))
                .map_err(io)?;
            if let Err(e) = f.lock() {
                tracing::warn!("checkpoint: cannot lock {}: {e}", dir.display());
                return Ok(None);
            }
            remove_stale_locks(&dir);
            Ok::<_, CheckpointError>(Some(f))
        })
        .await
        .map_err(|e| CheckpointError::Io(e.to_string()))??;
        Ok(Held {
            _mutex: mutex,
            _file: file,
        })
    }

    async fn git(&self, args: &[&str], stdin: Option<&[u8]>) -> Result<Vec<u8>, CheckpointError> {
        let out = self.run(args, stdin).await?;
        if !out.status.success() {
            return Err(failed(args, &out.stderr));
        }
        Ok(out.stdout)
    }

    /// Runs one `git` in the isolated environment, killed if it runs past its bound.
    async fn run(
        &self,
        args: &[&str],
        stdin: Option<&[u8]>,
    ) -> Result<std::process::Output, CheckpointError> {
        let command = args.first().copied().unwrap_or("");
        let limit = match command {
            "ls-files" | "add" | "rm" | "write-tree" | "ls-tree" => self.tree_timeout,
            _ => self.object_timeout,
        };
        let mut cmd = tokio::process::Command::new(&self.git);
        for (k, _) in std::env::vars_os() {
            if k.to_string_lossy().starts_with("GIT_") {
                cmd.env_remove(&k);
            }
        }
        cmd.env("GIT_DIR", &self.git_dir)
            .env("GIT_WORK_TREE", &self.root)
            .env("GIT_INDEX_FILE", self.git_dir.join(INDEX))
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env(
                "GIT_CONFIG_GLOBAL",
                if cfg!(windows) { "NUL" } else { "/dev/null" },
            )
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_OPTIONAL_LOCKS", "0")
            .env("HOME", &self.git_dir)
            .env("LC_ALL", "C")
            .args([
                "-c",
                "core.hooksPath=/dev/null",
                "-c",
                "core.autocrlf=false",
                "-c",
                "core.safecrlf=false",
                "-c",
                "core.fsmonitor=false",
                "-c",
                "core.excludesFile=",
                "-c",
                "core.quotePath=false",
                "-c",
                "core.symlinks=true",
                "-c",
                "gc.auto=0",
                "-c",
                "user.name=Axiom",
                "-c",
                "user.email=checkpoints@axiom.invalid",
                "-c",
                "commit.gpgSign=false",
            ])
            .args(args)
            .current_dir(&self.root)
            .stdin(if stdin.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        let mut child = cmd.spawn().map_err(|e| match e.kind() {
            std::io::ErrorKind::NotFound => CheckpointError::GitMissing,
            _ => CheckpointError::Git(e.to_string()),
        })?;
        let input = child.stdin.take();
        let mut stdout = child.stdout.take();
        let mut stderr = child.stderr.take();
        let work = async {
            let feed = async {
                if let (Some(data), Some(mut si)) = (stdin, input) {
                    si.write_all(data).await?;
                }
                Ok::<_, std::io::Error>(())
            };
            let read_out = async {
                let mut buf = Vec::new();
                if let Some(p) = stdout.as_mut() {
                    p.read_to_end(&mut buf).await?;
                }
                Ok::<_, std::io::Error>(buf)
            };
            let read_err = async {
                let mut buf = Vec::new();
                if let Some(p) = stderr.as_mut() {
                    p.read_to_end(&mut buf).await?;
                }
                Ok::<_, std::io::Error>(buf)
            };
            let ((), out, err) = tokio::try_join!(feed, read_out, read_err)?;
            let status = child.wait().await?;
            Ok::<_, std::io::Error>(std::process::Output {
                status,
                stdout: out,
                stderr: err,
            })
        };
        let done = tokio::time::timeout(limit, work).await;
        match done {
            Ok(r) => r.map_err(|e| CheckpointError::Git(format!("git {command}: {e}"))),
            Err(_) => {
                // Killed, and reaped if it goes within a few seconds (a process stuck in the
                // kernel may not; `kill_on_drop` covers it then).
                let _ = child.start_kill();
                let _ = tokio::time::timeout(Duration::from_secs(5), child.wait()).await;
                tracing::warn!("checkpoint: git {command} ran past {limit:?}; stopped");
                // A killed git leaves its lock behind, and then every later checkpoint (and so
                // every agent write) fails, even after a restart (review H3). The lock is this
                // git's: the shadow directory is private, and the store's lock lets no other git
                // run in it meanwhile.
                let mut locks = vec![self.git_dir.join(format!("{INDEX}.lock"))];
                if let ("update-ref", Some(name)) = (command, args.get(1)) {
                    if name.starts_with("refs/axiom/") && !name.split('/').any(|s| s == "..") {
                        let mut p = self.git_dir.clone();
                        p.extend(format!("{name}.lock").split('/'));
                        locks.push(p);
                    }
                }
                for l in locks {
                    if std::fs::remove_file(&l).is_ok() {
                        tracing::warn!("checkpoint: removed {} left by it", l.display());
                    }
                }
                Err(CheckpointError::TimedOut {
                    command: command.to_string(),
                    after: limit,
                })
            }
        }
    }

    async fn text(&self, args: &[&str]) -> Result<String, CheckpointError> {
        Ok(String::from_utf8_lossy(&self.git(args, None).await?)
            .trim()
            .to_string())
    }

    async fn ensure_repo(&self) -> Result<(), CheckpointError> {
        if self.git_dir.join("HEAD").exists() {
            return Ok(());
        }
        std::fs::create_dir_all(&self.git_dir).map_err(|e| CheckpointError::Io(e.to_string()))?;
        self.git(&["init", "-q"], None).await?;
        Ok(())
    }

    async fn snapshot(&self) -> Result<Snapshot, CheckpointError> {
        self.ensure_repo().await?;
        // Candidates under the workspace's ignore rules; anything too large is excluded.
        let listed = self
            .git(
                &[
                    "ls-files",
                    "-z",
                    "--cached",
                    "--others",
                    "--exclude-standard",
                ],
                None,
            )
            .await?;
        let mut skipped = Vec::new();
        for p in listed.split(|b| *b == 0).filter(|p| !p.is_empty()) {
            let rel = String::from_utf8_lossy(p).into_owned();
            if let Ok(md) = std::fs::symlink_metadata(self.root.join(&rel)) {
                if md.is_file() && md.len() > MAX_FILE_BYTES {
                    skipped.push(rel);
                }
            }
        }
        skipped.sort();
        skipped.dedup();
        let exclude: String = skipped
            .iter()
            .map(|p| format!("/{}\n", escape_pattern(p)))
            .collect();
        let info = self.git_dir.join("info");
        std::fs::create_dir_all(&info).map_err(|e| CheckpointError::Io(e.to_string()))?;
        std::fs::write(info.join("exclude"), exclude)
            .map_err(|e| CheckpointError::Io(e.to_string()))?;
        let add = ["add", "-A", "--ignore-errors", "--", "."];
        let out = self.run(&add, None).await?;
        let mut left_out = Vec::new();
        if !out.status.success() {
            // With `--ignore-errors`, git indexes everything it can and exits 1 for what it
            // could not: an unreadable file, or a nested repository with no commit yet (no
            // HEAD to record as a git link; review E round). Those are left out of the
            // checkpoint (an entry an earlier snapshot indexed too: its content is unknown
            // now), recorded, and never restored. Anything fatal fails.
            let err = String::from_utf8_lossy(&out.stderr);
            if out.status.code() != Some(1) || err.lines().any(|l| l.starts_with("fatal:")) {
                return Err(failed(&add, &out.stderr));
            }
            tracing::warn!("checkpoint: left out of the snapshot: {}", err.trim());
            left_out = err
                .lines()
                .filter_map(|l| l.strip_prefix("error: unable to index file '"))
                .filter_map(|l| l.strip_suffix('\''))
                .map(|p| p.trim_end_matches('/').to_string())
                .filter(|p| !p.is_empty())
                .collect();
            left_out.sort();
            left_out.dedup();
        }
        if !skipped.is_empty() || !left_out.is_empty() {
            let mut args = vec!["rm", "-q", "-r", "--cached", "--ignore-unmatch", "--"];
            let literal: Vec<String> = skipped
                .iter()
                .chain(&left_out)
                .map(|p| format!(":(literal){p}"))
                .collect();
            args.extend(literal.iter().map(String::as_str));
            self.git(&args, None).await?;
        }
        let tree = self.text(&["write-tree"]).await?;
        Ok(Snapshot {
            tree,
            skipped,
            left_out,
        })
    }

    async fn commit(
        &self,
        tree: &str,
        parent: Option<&str>,
        subject: &str,
    ) -> Result<String, CheckpointError> {
        let mut args = vec!["commit-tree", tree];
        if let Some(p) = parent {
            args.push("-p");
            args.push(p);
        }
        args.push("-m");
        args.push(subject);
        self.text(&args).await
    }

    async fn refs(&self, session: &str) -> Result<Vec<(u32, String, String)>, CheckpointError> {
        let prefix = format!("refs/axiom/{session}/");
        let out = self
            .text(&[
                "for-each-ref",
                "--format=%(refname) %(objectname) %(contents:subject)",
                &prefix,
            ])
            .await?;
        let mut v = Vec::new();
        for line in out.lines() {
            let mut it = line.splitn(3, ' ');
            let (Some(name), Some(id), Some(subject)) = (it.next(), it.next(), it.next()) else {
                continue;
            };
            if let Some(n) = name
                .strip_prefix(&prefix)
                .and_then(|n| n.parse::<u32>().ok())
            {
                v.push((n, id.to_string(), subject.to_string()));
            }
        }
        v.sort_by_key(|(n, _, _)| *n);
        Ok(v)
    }

    /// A new checkpoint of the workspace as it is now.
    pub async fn create(
        &self,
        session: &str,
        label: &str,
        turn: u32,
    ) -> Result<CheckpointInfo, CheckpointError> {
        let _g = self.hold().await?;
        self.create_locked(session, label, turn, &[]).await
    }

    /// `not_captured`: paths on disk the snapshot has no entry for besides what `git add` reports
    /// (a restore's files that are ignored now, review H1), recorded with its `left_out` so that
    /// restoring this checkpoint leaves them alone too.
    async fn create_locked(
        &self,
        session: &str,
        label: &str,
        turn: u32,
        not_captured: &[String],
    ) -> Result<CheckpointInfo, CheckpointError> {
        if !valid_session(session) {
            return Err(CheckpointError::Git(format!(
                "invalid session id `{session}`"
            )));
        }
        let snap = self.snapshot().await?;
        let existing = self.refs(session).await?;
        let number = existing.last().map_or(1, |(n, _, _)| n + 1);
        let meta = Meta {
            label: label.into(),
            turn,
            created_ms: now_ms(),
            skipped: snap.skipped.clone(),
            left_out: {
                let mut v: Vec<String> =
                    snap.left_out.iter().chain(not_captured).cloned().collect();
                v.sort();
                v.dedup();
                v
            },
        };
        let subject =
            serde_json::to_string(&meta).map_err(|e| CheckpointError::Io(e.to_string()))?;
        let parent = existing.last().map(|(_, id, _)| id.as_str());
        let id = self.commit(&snap.tree, parent, &subject).await?;
        self.git(
            &[
                "update-ref",
                &format!("refs/axiom/{session}/{number:04}"),
                &id,
            ],
            None,
        )
        .await?;
        Ok(info(number, &meta))
    }

    /// Records the workspace as the agent left it (after a writing tool).
    pub async fn record_post(&self, session: &str) -> Result<(), CheckpointError> {
        let _g = self.hold().await?;
        self.record_post_locked(session).await
    }

    async fn record_post_locked(&self, session: &str) -> Result<(), CheckpointError> {
        let snap = self.snapshot().await?;
        let id = self.commit(&snap.tree, None, "post").await?;
        self.git(
            &["update-ref", &format!("refs/axiom/{session}/post"), &id],
            None,
        )
        .await?;
        Ok(())
    }

    pub async fn list(&self, session: &str) -> Result<Vec<CheckpointInfo>, CheckpointError> {
        let _g = self.lock.lock().await;
        if !self.git_dir.join("HEAD").exists() {
            return Ok(vec![]);
        }
        Ok(self
            .refs(session)
            .await?
            .into_iter()
            .filter_map(|(n, _, s)| serde_json::from_str::<Meta>(&s).ok().map(|m| info(n, &m)))
            .collect())
    }

    async fn listing(&self, tree: &str) -> Result<Listing, CheckpointError> {
        let out = self
            .git(&["ls-tree", "-r", "-z", "--full-tree", tree], None)
            .await?;
        let mut m = BTreeMap::new();
        for rec in out.split(|b| *b == 0).filter(|r| !r.is_empty()) {
            let rec = String::from_utf8_lossy(rec);
            // "<mode> <type> <id>\t<path>"
            let Some((head, path)) = rec.split_once('\t') else {
                continue;
            };
            let mut h = head.split(' ');
            let (Some(mode), Some(_), Some(id)) = (h.next(), h.next(), h.next()) else {
                continue;
            };
            m.insert(path.to_string(), (mode.to_string(), id.to_string()));
        }
        Ok(m)
    }

    /// Restores checkpoint `number`. With conflicts and not `skip_conflicts`, nothing changes.
    /// What it leaves as it is besides conflicts is in [`RestoreOutcome::not_restored`].
    pub async fn restore(
        &self,
        session: &str,
        number: u32,
        turn: u32,
        skip_conflicts: bool,
    ) -> Result<RestoreOutcome, CheckpointError> {
        let _g = self.hold().await?;
        let refs = self.refs(session).await?;
        let (_, target_id, target_subject) = refs
            .iter()
            .find(|(n, _, _)| *n == number)
            .cloned()
            .ok_or(CheckpointError::NotFound(number))?;
        let target = self.listing(&format!("{target_id}^{{tree}}")).await?;
        let now_snap = self.snapshot().await?;
        let now = self.listing(&now_snap.tree).await?;
        let post_ref = format!("refs/axiom/{session}/post");
        let agent = match self.text(&["rev-parse", "--verify", "-q", &post_ref]).await {
            Ok(id) if !id.is_empty() => self.listing(&format!("{id}^{{tree}}")).await?,
            _ => target.clone(),
        };
        // What the checkpoint or the workspace now could not capture: its content then or now is
        // unknown, so it is never restored (removed, or written over).
        let mut uncaptured: std::collections::BTreeSet<String> = now_snap
            .skipped
            .iter()
            .chain(&now_snap.left_out)
            .cloned()
            .collect();
        if let Ok(m) = serde_json::from_str::<Meta>(&target_subject) {
            uncaptured.extend(m.uncaptured().cloned());
        }
        let is_uncaptured = |p: &str| {
            uncaptured.contains(p)
                || p.match_indices('/')
                    .any(|(i, _)| uncaptured.contains(&p[..i]))
        };
        // Nested repositories (git links) in the checkpoint, now, or as the agent left the
        // workspace: their content was never captured, and a restore never writes, removes or
        // replaces anything at or under one (a person's uncommitted edit in a clone the agent made
        // is in no checkpoint; review H2). Reported by the repository's path.
        let nested: std::collections::BTreeSet<&str> = [&target, &now, &agent]
            .into_iter()
            .flat_map(|l| l.iter())
            .filter(|(_, (mode, _))| mode == "160000")
            .map(|(p, _)| p.as_str())
            .collect();
        let nested_repository = |p: &str| -> Option<String> {
            p.match_indices('/')
                .map(|(i, _)| &p[..i])
                .chain([p])
                .find(|d| nested.contains(d))
                .map(str::to_string)
        };
        // The snapshot's paths by case, to tell a name on disk that is a captured file under
        // another case (a case-insensitive file system) from one no snapshot captured.
        let mut by_case: std::collections::HashMap<String, Vec<&String>> = Default::default();
        let mut paths: Vec<&String> = target.keys().chain(now.keys()).collect();
        paths.sort();
        paths.dedup();
        let mut restore = Vec::new();
        let mut conflicts = Vec::new();
        let mut not_restored = Vec::new();
        // Paths the restore would create (absent from the snapshot now), with the snapshot's
        // paths of the same name in another case.
        let mut absent: std::collections::BTreeMap<String, Vec<String>> = Default::default();
        for p in paths {
            let (t, n, a) = (target.get(p), now.get(p), agent.get(p));
            if t == n {
                continue;
            }
            if let Some(repo) = nested_repository(p) {
                not_restored.push((WorkspacePath(repo), NotRestored::NestedRepository));
            } else if is_uncaptured(p) {
                not_restored.push((WorkspacePath(p.clone()), NotRestored::NotCaptured));
            } else if n == a {
                // Absent now, and no captured file under it either (a folder the agent made of a
                // file is not unknown).
                let below = format!("{p}/");
                let captured_below = now
                    .range(below.clone()..)
                    .next()
                    .is_some_and(|(q, _)| q.starts_with(&below));
                if n.is_none() && !captured_below {
                    if by_case.is_empty() {
                        for q in now.keys() {
                            by_case.entry(q.to_lowercase()).or_default().push(q);
                        }
                    }
                    let same = by_case.get(&p.to_lowercase()).into_iter().flatten();
                    absent.insert(p.clone(), same.map(|q| (*q).clone()).collect());
                }
                restore.push((p.clone(), t.cloned()));
            } else {
                let reason = match (n, a) {
                    (None, _) => ConflictReason::DeletedSinceAgent,
                    (Some(_), None) => ConflictReason::CreatedSinceAgent,
                    _ => ConflictReason::ModifiedSinceAgent,
                };
                conflicts.push(RestoreConflict {
                    path: WorkspacePath(p.clone()),
                    reason,
                });
            }
        }
        let conflicted: BTreeMap<String, ConflictReason> = conflicts
            .iter()
            .map(|c| (c.path.0.clone(), c.reason))
            .collect();
        // Every path is checked before anything changes, off the async thread (review E3).
        // - A path inside a git directory (`.git`, or a `<name>.git` that is a bare repository or
        //   is being made one: `gate::git_dir_of`), or a write of one of git's own entries into
        //   a folder that holds, or that the restore also writes, what makes a git directory (the
        //   gate's `makes_git_dir` rule, review H8), is skipped, not restored: hooks run from
        //   there, so they change only through the gate, with the person's approval each time.
        // - A path the snapshot now has no entry for, where something is on disk, was not
        //   captured (ignored now, review H1): what it holds is unknown, so it is never written
        //   over. A name that is a captured file under another case (on a case-insensitive file
        //   system, the same file) is not such a path when this restore removes or replaces that
        //   file too (review I1); otherwise the file there is the person's, and the path is a
        //   conflict (review I2).
        // - A file the agent made a folder of is written back only when that folder holds nothing
        //   but files this restore removes (what the agent made, unchanged since); otherwise the
        //   path is a conflict, and the rest goes ahead (review I4).
        // - Both are reported in `not_restored`, and the rest is restored. A restore that would
        //   write through a symlinked directory changes nothing (review D4). A symlink the
        //   restore itself removes first (removals go before writes) is no obstacle; the write
        //   checks again once it is gone.
        let root = self.root.clone();
        let (restore, left, on_disk, late, refused) = tokio::task::spawn_blocking(move || {
            let mut markers: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
            for (p, _) in restore.iter().filter(|(_, e)| e.is_some()) {
                for (d, e) in git_entries(p) {
                    if GIT_DIR_MARKERS.iter().any(|m| m.eq_ignore_ascii_case(&e)) {
                        markers.entry(d.to_ascii_lowercase()).or_default().insert(e);
                    }
                }
            }
            let (mut planned, mut left) = (Vec::new(), Vec::new());
            for (p, e) in restore {
                if crate::gate::git_dir_of(&root, &p).is_some()
                    || (e.is_some() && makes_git_dir(&root, &p, &markers))
                {
                    tracing::warn!("restore: `{p}` is inside a git directory; not restored");
                    left.push((p, NotRestored::GitDirectory, None));
                } else if let Some(repo) = repository_on_disk(&root, &p) {
                    // A folder on the way is a repository the snapshots hold as plain files (the
                    // store's index had them before it became one): it is left all the same.
                    tracing::warn!("restore: `{p}` is in the repository `{repo}`; not restored");
                    left.push((p, NotRestored::NestedRepository, Some(repo)));
                } else {
                    planned.push((p, e));
                }
            }
            // What this restore changes (removes or replaces), and what it removes. A path absent
            // from the snapshot now never depends on another such path (a case variant of it is
            // in the snapshot now), so these are final.
            let changes: BTreeSet<String> = planned.iter().map(|(p, _)| p.clone()).collect();
            let removes: BTreeSet<String> = planned
                .iter()
                .filter(|(_, e)| e.is_none())
                .map(|(p, _)| p.clone())
                .collect();
            let (mut kept, mut on_disk, mut late) = (Vec::new(), Vec::new(), Vec::new());
            for (p, e) in planned {
                match absent.get(&p).map(|same| on_disk_at(&root, &p, same)) {
                    None | Some(OnDisk::Nothing) => {}
                    Some(OnDisk::Uncaptured) => {
                        tracing::warn!(
                            "restore: `{p}` is on disk but was not captured; not restored"
                        );
                        on_disk.push(p.clone());
                        left.push((p, NotRestored::NotCaptured, None));
                        continue;
                    }
                    Some(OnDisk::Captured(variants)) => {
                        if !variants.iter().any(|q| changes.contains(q)) {
                            // The file there is one the restore leaves (a person's, a conflict):
                            // writing this name would write over it.
                            let reason = variants
                                .iter()
                                .find_map(|q| conflicted.get(q))
                                .copied()
                                .unwrap_or(ConflictReason::CreatedSinceAgent);
                            tracing::warn!(
                                "restore: `{p}` is `{}` on disk, which it leaves; a conflict",
                                variants.join("`, `")
                            );
                            late.push(RestoreConflict {
                                path: WorkspacePath(p),
                                reason,
                            });
                            continue;
                        }
                    }
                }
                if e.is_some() && !holds_only(&root, &p, &removes) {
                    tracing::warn!("restore: `{p}` is a folder holding more than the agent made");
                    late.push(RestoreConflict {
                        path: WorkspacePath(p),
                        reason: ConflictReason::ModifiedSinceAgent,
                    });
                    continue;
                }
                kept.push((p, e));
            }
            let replaced: std::collections::BTreeSet<&str> =
                kept.iter().map(|(p, _)| p.as_str()).collect();
            // What it leaves for what is on disk (not in a git directory) is checked too: a
            // symlinked directory on the way refuses the whole restore all the same.
            let on_the_way = left
                .iter()
                .filter(|(_, why, _)| *why != NotRestored::GitDirectory)
                .map(|(p, _, _)| p);
            let refused = kept
                .iter()
                .map(|(p, _)| p)
                .chain(on_the_way)
                .find_map(|p| writable(&root, p, |dir| replaced.contains(dir)).err());
            (kept, left, on_disk, late, refused)
        })
        .await
        .map_err(|e| CheckpointError::Io(e.to_string()))?;
        // With conflicts (from the snapshots, or found on disk) nothing changes unless the person
        // chose to restore the rest; they are reported first, as the whole list.
        conflicts.extend(late);
        conflicts.sort_by(|a, b| a.path.0.cmp(&b.path.0));
        if !conflicts.is_empty() && !skip_conflicts {
            return Err(CheckpointError::Conflicts(conflicts));
        }
        if let Some(e) = refused {
            return Err(e);
        }
        not_restored.extend(
            left.into_iter()
                .map(|(p, why, repo)| (WorkspacePath(repo.unwrap_or(p)), why)),
        );
        not_restored.sort_by(|a, b| a.0 .0.cmp(&b.0 .0));
        not_restored.dedup();
        // The undo checkpoint does not capture those either: restoring it leaves them too.
        let undo = self
            .create_locked(
                session,
                &format!("before restore to {number}"),
                turn,
                &on_disk,
            )
            .await?;
        // Removals first: on a case-insensitive file system a name the restore writes can be the
        // same file as one it removes, which removed afterwards would take the restored file with
        // it (review I1); and a file it writes can replace a folder its removals empty (review
        // I4).
        let (removals, writes): (Vec<_>, Vec<_>) =
            restore.into_iter().partition(|(_, e)| e.is_none());
        let mut restored = Vec::new();
        for (p, entry) in removals.into_iter().chain(writes) {
            self.write_entry(&p, entry.as_ref()).await?;
            restored.push(WorkspacePath(p));
        }
        restored.sort_by(|a, b| a.0.cmp(&b.0));
        // The restored state is now what the agent left.
        self.record_post_locked(session).await?;
        Ok(RestoreOutcome {
            result: RestoreResult {
                checkpoint: number,
                undo_checkpoint: undo.number,
                restored,
                conflicts,
            },
            not_restored,
        })
    }

    /// Writes one tree entry into the workspace (`None` deletes the file).
    async fn write_entry(
        &self,
        rel: &str,
        entry: Option<&(String, String)>,
    ) -> Result<(), CheckpointError> {
        let path = writable(&self.root, rel, |_| false)?;
        let io = |e: std::io::Error| CheckpointError::Io(format!("{rel}: {e}"));
        match entry {
            None => match std::fs::remove_file(&path) {
                Ok(()) => Ok(()),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(e) => Err(io(e)),
            },
            Some((mode, id)) => {
                if mode == "160000" {
                    return Ok(()); // a nested repository: never touched
                }
                let bytes = self.git(&["cat-file", "blob", id], None).await?;
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir).map_err(io)?;
                }
                match std::fs::symlink_metadata(&path) {
                    Ok(m) if m.file_type().is_symlink() => {
                        std::fs::remove_file(&path).map_err(io)?
                    }
                    // A folder the agent made of this file, which the removals before emptied (it
                    // held only those, checked before anything changed; review I4). Only empty
                    // folders are removed: anything that appeared since stops the restore here.
                    Ok(m) if m.is_dir() => remove_empty_folders(&path).map_err(io)?,
                    _ => {}
                }
                if mode == "120000" {
                    #[cfg(unix)]
                    {
                        let _ = std::fs::remove_file(&path);
                        let target = String::from_utf8_lossy(&bytes).into_owned();
                        return std::os::unix::fs::symlink(target, &path).map_err(io);
                    }
                }
                // A new file beside it, renamed over the entry: a hard link (to a git hook, or a
                // file outside the workspace) is replaced, never written through, and nothing
                // is followed (review N14/R5). The exclusive create refuses an existing name, a
                // symlink included.
                let dir = path
                    .parent()
                    .ok_or_else(|| io(std::io::ErrorKind::NotFound.into()))?;
                let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
                let name = name.unwrap_or_default();
                let mut tmp = None;
                for n in 0..100u32 {
                    let t = dir.join(format!(".{name}.axiom-restore-{}-{n}", std::process::id()));
                    match std::fs::OpenOptions::new()
                        .write(true)
                        .create_new(true)
                        .open(&t)
                    {
                        Ok(mut f) => {
                            use std::io::Write;
                            if let Err(e) = f.write_all(&bytes).and_then(|()| f.sync_all()) {
                                let _ = std::fs::remove_file(&t);
                                return Err(io(e));
                            }
                            tmp = Some(t);
                            break;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                        Err(e) => return Err(io(e)),
                    }
                }
                let tmp = tmp.ok_or_else(|| io(std::io::ErrorKind::AlreadyExists.into()))?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let exec = mode == "100755";
                    // The permission bits of the file it replaces (never through a symlink), or
                    // the default for new files. Never setuid, setgid or sticky (review D4).
                    let m = std::fs::symlink_metadata(&path)
                        .ok()
                        .filter(|m| m.is_file())
                        .map_or(0o644, |m| m.permissions().mode() & 0o777);
                    let m = if exec { m | 0o111 } else { m & !0o111 };
                    if let Err(e) =
                        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(m))
                    {
                        let _ = std::fs::remove_file(&tmp);
                        return Err(io(e));
                    }
                }
                // Still inside the workspace, through no symlink, right before it lands (a parent
                // swapped for a symlink since the first check is refused, not followed).
                if let Err(e) = writable(&self.root, rel, |_| false) {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(e);
                }
                if let Err(e) = std::fs::rename(&tmp, &path) {
                    let _ = std::fs::remove_file(&tmp);
                    return Err(io(e));
                }
                Ok(())
            }
        }
    }
}

fn failed(args: &[&str], stderr: &[u8]) -> CheckpointError {
    CheckpointError::Git(format!(
        "git {}: {}",
        args.first().copied().unwrap_or(""),
        String::from_utf8_lossy(stderr).trim()
    ))
}

fn info(number: u32, m: &Meta) -> CheckpointInfo {
    CheckpointInfo {
        number,
        label: m.label.clone(),
        created_ms: m.created_ms,
        turn: m.turn,
        skipped: m.skipped.iter().map(|s| WorkspacePath(s.clone())).collect(),
    }
}

/// Removes the locks a git killed inside the store (with its daemon: SIGKILL, power loss) leaves
/// behind, which make every later checkpoint, and so every agent write, fail even after a restart:
/// the store's index lock (review H3), `HEAD.lock`, `packed-refs.lock`, and any `*.lock` under
/// `refs/axiom/` (an `update-ref` killed mid-way, review I3). Called only with the store's lock
/// held, when no git of ours runs in it, so each one found is stale. Nothing is followed through
/// a symlink. Blocking.
fn remove_stale_locks(dir: &Path) {
    let mut stale = vec![
        dir.join(format!("{INDEX}.lock")),
        dir.join("HEAD.lock"),
        dir.join("packed-refs.lock"),
    ];
    // A ref's name never ends in `.lock` (git refuses one), so every such file is a lock.
    let mut folders = vec![dir.join("refs").join("axiom")];
    while let Some(d) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&d) else {
            continue;
        };
        for e in entries.flatten() {
            let Ok(t) = e.file_type() else {
                continue;
            };
            if t.is_dir() {
                folders.push(e.path());
            } else if t.is_file() && e.file_name().to_string_lossy().ends_with(".lock") {
                stale.push(e.path());
            }
        }
    }
    for l in stale {
        if std::fs::remove_file(&l).is_ok() {
            tracing::warn!("checkpoint: removed a stale {}", l.display());
        }
    }
}

/// Where `rel` puts one of git's own entries (`gate::GIT_DIR_ENTRIES`) directly into a folder:
/// (the folder, the root as `""`; the entry, lowercased).
fn git_entries(rel: &str) -> Vec<(String, String)> {
    let parts: Vec<&str> = rel.split('/').collect();
    (0..parts.len())
        .filter(|&i| {
            crate::gate::GIT_DIR_ENTRIES
                .iter()
                .any(|e| e.eq_ignore_ascii_case(parts[i]))
        })
        .map(|i| (parts[..i].join("/"), parts[i].to_ascii_lowercase()))
        .collect()
}

/// The gate's rule for writing one of git's own entries (`gate::makes_git_dir`, review G5),
/// applied to a restore's write of `rel` (review H8): it puts one into a folder that holds on
/// disk, or into which another of the restore's writes puts (`markers`: by folder, lowercased),
/// one of what makes a git directory ([`GIT_DIR_MARKERS`]), as `HEAD` and `commondir` together
/// would. The entry `rel` itself puts there does not count: one alone makes no git directory
/// (so `src/refs/a.ts` is restored beside `src/refs/b.ts`). Blocking.
fn makes_git_dir(root: &Path, rel: &str, markers: &BTreeMap<String, BTreeSet<String>>) -> bool {
    git_entries(rel).iter().any(|(d, e)| {
        let mut dir = root.to_path_buf();
        if !d.is_empty() {
            dir.extend(d.split('/'));
        }
        let other = |m: &str| !m.eq_ignore_ascii_case(e);
        GIT_DIR_MARKERS
            .iter()
            .any(|m| other(m) && std::fs::symlink_metadata(dir.join(m)).is_ok())
            || markers
                .get(&d.to_ascii_lowercase())
                .is_some_and(|written| written.iter().any(|m| other(m)))
    })
}

/// The outermost folder on the way to `rel` (not the workspace root) that is a repository on
/// disk: it has its own `.git`. Blocking.
fn repository_on_disk(root: &Path, rel: &str) -> Option<String> {
    rel.match_indices('/')
        .map(|(i, _)| &rel[..i])
        .find_map(|d| {
            let mut p = root.to_path_buf();
            p.extend(d.split('/'));
            std::fs::symlink_metadata(p.join(".git"))
                .is_ok()
                .then(|| d.to_string())
        })
}

/// What is on disk at a path the snapshot now has no entry for (review H1).
enum OnDisk {
    /// Nothing, or only through a symlinked directory on the way: it is elsewhere, and
    /// [`writable`] decides (the restore removes that symlink, or refuses).
    Nothing,
    /// Something no snapshot captured (ignored now): its content is unknown.
    Uncaptured,
    /// The same file as these of the snapshot's paths, named in another case (a case-insensitive
    /// file system).
    Captured(Vec<String>),
}

fn host_path(root: &Path, rel: &str) -> PathBuf {
    let mut p = root.to_path_buf();
    p.extend(rel.split('/'));
    p
}

/// Whether a folder on the way to `rel` is a symlink, or missing (then so is `rel`). Blocking.
fn through_symlink(root: &Path, rel: &str) -> bool {
    rel.match_indices('/').any(|(i, _)| {
        std::fs::symlink_metadata(host_path(root, &rel[..i]))
            .map_or(true, |m| m.file_type().is_symlink())
    })
}

/// What is on disk at `rel`, a path the snapshot now has no entry for, given the snapshot's paths
/// of the same name in another case (`same`). Only what is at `rel` itself counts. Blocking.
fn on_disk_at(root: &Path, rel: &str, same: &[String]) -> OnDisk {
    if through_symlink(root, rel) {
        return OnDisk::Nothing;
    }
    let Ok(here) = std::fs::symlink_metadata(host_path(root, rel)) else {
        return OnDisk::Nothing;
    };
    let variants: Vec<String> = same
        .iter()
        .filter(|q| {
            std::fs::symlink_metadata(host_path(root, q)).is_ok_and(|m| same_file(&here, &m))
        })
        .cloned()
        .collect();
    if variants.is_empty() {
        OnDisk::Uncaptured
    } else {
        OnDisk::Captured(variants)
    }
}

/// Whether the file a restore writes at `rel` may replace what is there (review I4): it is no
/// folder, or a folder (not a symlink) holding, at any depth, only files this restore removes
/// (`removes`: what the agent made, unchanged since) and folders. Anything else in it (a person's
/// file, an ignored file, a repository's `.git`) keeps it. Through a symlinked folder on the way
/// it is elsewhere, and [`writable`] decides. Blocking.
fn holds_only(root: &Path, rel: &str, removes: &BTreeSet<String>) -> bool {
    if through_symlink(root, rel) {
        return true;
    }
    let path = host_path(root, rel);
    if !std::fs::symlink_metadata(&path).is_ok_and(|m| m.is_dir()) {
        return true;
    }
    let mut folders = vec![(path, rel.to_string())];
    while let Some((dir, under)) = folders.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else {
            return false;
        };
        for e in entries {
            let Ok(e) = e else {
                return false;
            };
            let (Ok(t), Some(name)) = (e.file_type(), e.file_name().to_str().map(str::to_string))
            else {
                return false;
            };
            let q = format!("{under}/{name}");
            if t.is_dir() {
                folders.push((e.path(), q));
            } else if !removes.contains(&q) {
                return false;
            }
        }
    }
    true
}

/// Removes the folder `path` and the folders in it, all of which must be empty of anything else
/// (a file found fails it, and is left). Nothing is followed through a symlink. Blocking.
fn remove_empty_folders(path: &Path) -> std::io::Result<()> {
    for e in std::fs::read_dir(path)? {
        let e = e?;
        if e.file_type()?.is_dir() {
            remove_empty_folders(&e.path())?;
        }
    }
    std::fs::remove_dir(path)
}

#[cfg(unix)]
fn same_file(a: &std::fs::Metadata, b: &std::fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    a.dev() == b.dev() && a.ino() == b.ino()
}

/// No file identity to compare (stable std has none on Windows): names that differ only by case
/// are the same file on Windows's case-insensitive file systems.
#[cfg(not(unix))]
fn same_file(_: &std::fs::Metadata, _: &std::fs::Metadata) -> bool {
    true
}

/// A gitignore pattern matching exactly `path`.
fn escape_pattern(path: &str) -> String {
    let mut out = String::new();
    for c in path.chars() {
        if matches!(c, '*' | '?' | '[' | ']' | '\\' | '!' | '#' | ' ') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

/// The host path a restore may write or remove for a tree path (review D4): [`contained`], no
/// directory on the way a symlink (a symlinked directory inside the workspace, such as one into
/// `.git/hooks`, would take the write somewhere the checkpoint never named), and not inside a
/// git directory (`.git`, or a `<name>.git` that is a bare repository: hooks run from there, so
/// they change only through the gate, with the person's approval each time; `restore` skips
/// those paths before it starts, this refuses one that became so since). The workspace's own
/// configuration (`.axiom/`) and hooks kept in the tree (`.husky/`) are restored like any file:
/// a restore is the person's own undo, and touches only what the agent last left.
/// `replaced`: directories on the way that the restore removes before it gets to `rel`.
fn writable(
    root: &Path,
    rel: &str,
    replaced: impl Fn(&str) -> bool,
) -> Result<PathBuf, CheckpointError> {
    let path = contained(root, rel)?;
    let parts: Vec<&str> = rel.split('/').collect();
    if let Some(dir) = crate::gate::git_dir_of(root, rel) {
        return Err(CheckpointError::Io(format!(
            "`{rel}` is inside a git directory (`{dir}`): a restore never writes there"
        )));
    }
    let mut dir = root.to_path_buf();
    for (i, name) in parts[..parts.len() - 1].iter().enumerate() {
        dir.push(name);
        match std::fs::symlink_metadata(&dir) {
            Ok(m) if m.file_type().is_symlink() && !replaced(&parts[..=i].join("/")) => {
                return Err(CheckpointError::Io(format!(
                    "`{rel}` is under a symlink (`{}`): a restore does not write through one",
                    parts[..=i].join("/")
                )))
            }
            Ok(_) => {}
            // Missing from here on: the restore creates real directories.
            Err(_) => break,
        }
    }
    Ok(path)
}

/// The host path for a tree path, refusing anything that leaves the root (a `..`, an absolute
/// path, or a parent directory that is a symlink pointing out).
fn contained(root: &Path, rel: &str) -> Result<PathBuf, CheckpointError> {
    let bad = || CheckpointError::Io(format!("`{rel}` is outside the workspace"));
    if rel.is_empty()
        || rel.starts_with('/')
        || rel
            .split('/')
            .any(|s| s.is_empty() || s == "." || s == "..")
    {
        return Err(bad());
    }
    let path = root.join(rel);
    let root_c = std::fs::canonicalize(root).map_err(|e| CheckpointError::Io(e.to_string()))?;
    let mut dir = path.parent().map(Path::to_path_buf);
    // The nearest existing ancestor must be inside the root.
    while let Some(d) = dir {
        if d.exists() {
            let dc = std::fs::canonicalize(&d).map_err(|e| CheckpointError::Io(e.to_string()))?;
            if !dc.starts_with(&root_c) {
                return Err(bad());
            }
            break;
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    Ok(path)
}
