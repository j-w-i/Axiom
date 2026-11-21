//! Checkpoints: a shadow git repository per workspace, driven through the `git` program
//! (ADR-0015 §5; `docs/protocols/agents.md` §Checkpoints; M3-CKPT-001).
//!
//! - **Isolation.** The repository is `<data dir>/checkpoints/<workspace key>/`, with its own
//!   index and `GIT_WORK_TREE` = the workspace root. Every `GIT_*` variable is removed and
//!   there is no system/global config, no hooks, no fsmonitor and no autocrlf. The user's
//!   `.git` (index, refs, stash, config) is never read or written.
//! - **What a snapshot captures.** Everything `git add -A` would, under the workspace's own
//!   ignore files, minus files over 16 MiB (listed as skipped). Nested repositories are
//!   recorded as git links, not their content.
//! - **Refs.** A checkpoint is a commit at `refs/axiom/<session>/<nnnn>`; its subject is a JSON
//!   line with its metadata. `refs/axiom/<session>/post` is the newest state the agent left
//!   (taken after each writing tool).
//! - **Restore** to checkpoint C restores each file that differs between C and the workspace now
//!   only if its content is what the agent last left; anything else is a conflict and is left
//!   alone. A restore is preceded by a checkpoint, so it can be undone.

use axiom_protocol::agent::{CheckpointInfo, ConflictReason, RestoreConflict, RestoreResult};
use axiom_protocol::WorkspacePath;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::AsyncWriteExt;

pub const MAX_FILE_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CheckpointError {
    #[error("git is not installed (checkpoints need it)")]
    GitMissing,
    #[error("git failed: {0}")]
    Git(String),
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
}

/// One workspace's shadow repository. Calls are serialised (one private index).
pub struct Shadow {
    git: PathBuf,
    git_dir: PathBuf,
    root: PathBuf,
    lock: tokio::sync::Mutex<()>,
}

struct Snapshot {
    tree: String,
    skipped: Vec<String>,
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
            lock: tokio::sync::Mutex::new(()),
        }
    }

    async fn git(&self, args: &[&str], stdin: Option<&[u8]>) -> Result<Vec<u8>, CheckpointError> {
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
        if let (Some(data), Some(mut si)) = (stdin, child.stdin.take()) {
            si.write_all(data)
                .await
                .map_err(|e| CheckpointError::Io(e.to_string()))?;
        }
        let out = child
            .wait_with_output()
            .await
            .map_err(|e| CheckpointError::Git(e.to_string()))?;
        if !out.status.success() {
            return Err(CheckpointError::Git(format!(
                "git {}: {}",
                args.first().copied().unwrap_or(""),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        Ok(out.stdout)
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
        self.git(&["add", "-A", "--ignore-errors", "--", "."], None)
            .await
            .or_else(|e| match e {
                // `--ignore-errors` still exits 1 after skipping unreadable files.
                CheckpointError::Git(m) if m.contains("adding files failed") => Ok(vec![]),
                other => Err(other),
            })?;
        if !skipped.is_empty() {
            let mut args = vec!["rm", "-q", "--cached", "--ignore-unmatch", "--"];
            args.extend(skipped.iter().map(String::as_str));
            self.git(&args, None).await?;
        }
        let tree = self.text(&["write-tree"]).await?;
        Ok(Snapshot { tree, skipped })
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
    pub async fn restore(
        &self,
        session: &str,
        number: u32,
        turn: u32,
        skip_conflicts: bool,
    ) -> Result<RestoreResult, CheckpointError> {
        let _g = self.lock.lock().await;
        let refs = self.refs(session).await?;
        let (_, target_id, _) = refs
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
        let mut paths: Vec<&String> = target.keys().chain(now.keys()).collect();
        paths.sort();
        paths.dedup();
        let mut restore = Vec::new();
        let mut conflicts = Vec::new();
        for p in paths {
            let (t, n, a) = (target.get(p), now.get(p), agent.get(p));
            if t == n {
                continue;
            }
            if n == a {
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
        Ok(RestoreResult {
            checkpoint: number,
            undo_checkpoint: undo.number,
            restored,
            conflicts,
        })
    }

    /// Writes one tree entry into the workspace (`None` deletes the file).
    async fn write_entry(
        &self,
        rel: &str,
        entry: Option<&(String, String)>,
    ) -> Result<(), CheckpointError> {
        let path = contained(&self.root, rel)?;
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
                std::fs::write(&path, &bytes).map_err(io)?;
                #[cfg(unix)]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let exec = mode == "100755";
                    let md = std::fs::metadata(&path).map_err(io)?;
                    let mut perm = md.permissions();
                    let m = perm.mode();
                    perm.set_mode(if exec { m | 0o111 } else { m & !0o111 });
                    std::fs::set_permissions(&path, perm).map_err(io)?;
                }
                Ok(())
            }
        }
    }
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
