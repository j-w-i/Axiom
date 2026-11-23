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
//!   directory, nested repositories and what a snapshot could not capture (then or now) are
//!   never restored; they are reported ([`RestoreOutcome::not_restored`]).

use axiom_protocol::agent::{CheckpointInfo, ConflictReason, RestoreConflict, RestoreResult};
use axiom_protocol::WorkspacePath;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
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

/// Why a path that differs from the checkpoint was left as it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NotRestored {
    /// Inside a git directory (`.git`, or a `<name>.git` that is a repository): hooks run from
    /// there, so it changes only through the gate, with the person's approval.
    GitDirectory,
    /// A nested repository (a git link): its content was never captured, and a restore never
    /// touches it.
    NestedRepository,
    /// The checkpoint, or the snapshot of the workspace now, could not capture it (over
    /// 16 MiB, unreadable, inside a nested repository with no commit): what it held is unknown.
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
                " Not captured by the checkpoint or now (too large, unreadable, or in a \
                 repository with no commit), so their content is unknown: {s}."
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
            .env("GIT_INDEX_FILE", self.git_dir.join("axiom-index"))
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
        let _g = self.lock.lock().await;
        self.create_locked(session, label, turn).await
    }

    async fn create_locked(
        &self,
        session: &str,
        label: &str,
        turn: u32,
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
            left_out: snap.left_out.clone(),
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
        let _g = self.lock.lock().await;
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
        let _g = self.lock.lock().await;
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
        let mut paths: Vec<&String> = target.keys().chain(now.keys()).collect();
        paths.sort();
        paths.dedup();
        let mut restore = Vec::new();
        let mut conflicts = Vec::new();
        let mut not_restored = Vec::new();
        let gitlink = |e: Option<&(String, String)>| e.is_some_and(|(mode, _)| mode == "160000");
        for p in paths {
            let (t, n, a) = (target.get(p), now.get(p), agent.get(p));
            if t == n {
                continue;
            }
            if gitlink(t) || gitlink(n) {
                // A nested repository then or now: its content was never captured, and removing
                // or replacing its directory would lose it.
                not_restored.push((WorkspacePath(p.clone()), NotRestored::NestedRepository));
            } else if is_uncaptured(p) {
                not_restored.push((WorkspacePath(p.clone()), NotRestored::NotCaptured));
            } else if n == a {
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
        if !conflicts.is_empty() && !skip_conflicts {
            return Err(CheckpointError::Conflicts(conflicts));
        }
        // Every path is checked before anything changes, off the async thread (review E3). A
        // path inside a git directory (`.git`, or a `<name>.git` that is a bare repository or is
        // being made one: `gate::git_dir_of`) is skipped, not restored: hooks run from there, so
        // they change only through the gate, with the person's approval each time; it is
        // reported in `not_restored`, and the rest is restored. A restore that would write through a symlinked directory changes nothing
        // (review D4). A symlink the restore itself removes or replaces first (it sorts before
        // what is under it) is no obstacle; the write checks again once it is gone.
        let root = self.root.clone();
        let (restore, skipped) = tokio::task::spawn_blocking(move || {
            let (skipped, kept): (Vec<_>, Vec<_>) = restore
                .into_iter()
                .partition(|(p, _)| crate::gate::git_dir_of(&root, p).is_some());
            for (p, _) in &skipped {
                tracing::warn!("restore: `{p}` is inside a git directory; not restored");
            }
            let replaced: std::collections::BTreeSet<&str> =
                kept.iter().map(|(p, _)| p.as_str()).collect();
            for (p, _) in &kept {
                writable(&root, p, |dir| replaced.contains(dir))?;
            }
            Ok::<_, CheckpointError>((kept, skipped))
        })
        .await
        .map_err(|e| CheckpointError::Io(e.to_string()))??;
        not_restored.extend(
            skipped
                .into_iter()
                .map(|(p, _)| (WorkspacePath(p), NotRestored::GitDirectory)),
        );
        not_restored.sort_by(|a, b| a.0 .0.cmp(&b.0 .0));
        let undo = self
            .create_locked(session, &format!("before restore to {number}"), turn)
            .await?;
        let mut restored = Vec::new();
        for (p, entry) in restore {
            self.write_entry(&p, entry.as_ref()).await?;
            restored.push(WorkspacePath(p));
        }
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
                if std::fs::symlink_metadata(&path).is_ok_and(|m| m.file_type().is_symlink()) {
                    std::fs::remove_file(&path).map_err(io)?;
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
