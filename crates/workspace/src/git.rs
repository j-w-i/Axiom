//! Git state of a workspace through the `git` program (ADR-0018 §5; M4-GIT-001): status (porcelain
//! v2, parsed), a file's text at HEAD or in the index, and a watch on the repository's own files.
//! Read-only. Every run is isolated: the inherited `GIT_*` variables are removed, no pager, no
//! hooks, no fsmonitor, `GIT_OPTIONAL_LOCKS=0` (status never takes the index lock), `LC_ALL=C`.
//! The person's own configuration (ignore files, `safe.directory`) still applies.

use axiom_protocol::git::{GitFile, GitFileState, GitStatus};
use axiom_protocol::WorkspacePath;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Longest a git run may take.
pub const GIT_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("Git is not installed (no `git` on PATH).")]
    NoGit,
    #[error("{0}")]
    NotARepository(String),
    #[error("{0}")]
    NotFound(String),
    #[error("git did not finish within {0} seconds")]
    Timeout(u64),
    #[error("git failed: {0}")]
    Failed(String),
    #[error("io: {0}")]
    Io(#[from] std::io::Error),
}

impl From<GitError> for axiom_protocol::RpcError {
    fn from(e: GitError) -> Self {
        use axiom_protocol::ErrorKind;
        let kind = match &e {
            GitError::NoGit | GitError::NotARepository(_) => ErrorKind::GitUnavailable,
            GitError::NotFound(_) => ErrorKind::NotFound,
            GitError::Timeout(_) => ErrorKind::Timeout,
            GitError::Failed(_) | GitError::Io(_) => ErrorKind::Io,
        };
        axiom_protocol::RpcError::new(kind, e.to_string())
    }
}

/// The repository a workspace is in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Repo {
    pub git: PathBuf,
    /// The workspace root (where git runs).
    pub root: PathBuf,
    pub toplevel: PathBuf,
    pub git_dir: PathBuf,
    /// The workspace root relative to the top level, with a trailing `/` (empty at the top).
    pub prefix: String,
}

/// The `git` program on `PATH`.
pub fn find_git() -> Option<PathBuf> {
    let exe = if cfg!(windows) { "git.exe" } else { "git" };
    std::env::split_paths(&std::env::var_os("PATH")?)
        .map(|d| d.join(exe))
        .find(|p| p.is_file())
}

fn command(git: &Path, root: &Path) -> Command {
    let mut cmd = Command::new(git);
    for (k, _) in std::env::vars_os() {
        if k.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(&k);
        }
    }
    cmd.current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C")
        .args([
            "--no-pager",
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "core.quotePath=false",
            "-c",
            "status.relativePaths=false",
            "-c",
            "color.ui=false",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

/// Runs git, bounded by [`GIT_TIMEOUT`]; stdout on success, the stderr text on failure.
fn run(git: &Path, root: &Path, args: &[&str]) -> Result<Result<Vec<u8>, String>, GitError> {
    let mut child = command(git, root).args(args).spawn()?;
    // Read both pipes on threads (a full pipe would block the child).
    let mut out = child
        .stdout
        .take()
        .ok_or_else(|| GitError::Failed("no stdout".into()))?;
    let mut err = child
        .stderr
        .take()
        .ok_or_else(|| GitError::Failed("no stderr".into()))?;
    let out_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = std::io::Read::read_to_end(&mut out, &mut v);
        v
    });
    let err_t = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = std::io::Read::read_to_end(&mut err, &mut v);
        v
    });
    let deadline = std::time::Instant::now() + GIT_TIMEOUT;
    let status = loop {
        if let Some(s) = child.try_wait()? {
            break s;
        }
        if std::time::Instant::now() > deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(GitError::Timeout(GIT_TIMEOUT.as_secs()));
        }
        std::thread::sleep(Duration::from_millis(5));
    };
    let stdout = out_t.join().unwrap_or_default();
    let stderr = err_t.join().unwrap_or_default();
    Ok(if status.success() {
        Ok(stdout)
    } else {
        Err(String::from_utf8_lossy(&stderr).trim().to_string())
    })
}

/// The repository the workspace root is in.
pub fn discover(root: &Path) -> Result<Repo, GitError> {
    let git = find_git().ok_or(GitError::NoGit)?;
    let out = run(
        &git,
        root,
        &[
            "rev-parse",
            "--show-toplevel",
            "--absolute-git-dir",
            "--show-prefix",
        ],
    )?
    .map_err(|e| {
        if e.contains("not a git repository") {
            GitError::NotARepository("This folder is not in a Git repository.".into())
        } else {
            GitError::NotARepository(format!("Git cannot read this folder: {e}"))
        }
    })?;
    let text = String::from_utf8_lossy(&out);
    let mut lines = text.lines();
    let toplevel = PathBuf::from(lines.next().unwrap_or_default());
    let git_dir = PathBuf::from(lines.next().unwrap_or_default());
    let prefix = lines.next().unwrap_or_default().to_string();
    Ok(Repo {
        git,
        root: root.to_path_buf(),
        toplevel,
        git_dir,
        prefix,
    })
}

fn state(c: u8) -> GitFileState {
    match c {
        b'.' => GitFileState::Unmodified,
        b'M' => GitFileState::Modified,
        b'T' => GitFileState::TypeChanged,
        b'A' => GitFileState::Added,
        b'D' => GitFileState::Deleted,
        b'R' => GitFileState::Renamed,
        b'C' => GitFileState::Copied,
        b'U' => GitFileState::Conflicted,
        _ => GitFileState::Unknown,
    }
}

/// Parses `git status --porcelain=v2 -z --branch` output; paths are made relative to `prefix`
/// (entries outside it are dropped).
pub fn parse_status(out: &[u8], prefix: &str) -> GitStatus {
    let mut st = GitStatus {
        repo: true,
        reason: None,
        branch: None,
        head: None,
        upstream: None,
        ahead: 0,
        behind: 0,
        files: Vec::new(),
        polling: false,
    };
    let rel = |p: &str| -> Option<WorkspacePath> {
        p.strip_prefix(prefix).map(|r| WorkspacePath(r.to_string()))
    };
    let mut fields = out
        .split(|b| *b == 0)
        .map(|f| String::from_utf8_lossy(f).into_owned());
    while let Some(rec) = fields.next() {
        if rec.is_empty() {
            continue;
        }
        if let Some(h) = rec.strip_prefix("# ") {
            let (k, v) = h.split_once(' ').unwrap_or((h, ""));
            match k {
                "branch.oid" if v != "(initial)" => st.head = Some(v.chars().take(7).collect()),
                "branch.head" if v != "(detached)" => st.branch = Some(v.to_string()),
                "branch.upstream" => st.upstream = Some(v.to_string()),
                "branch.ab" => {
                    for part in v.split(' ') {
                        if let Some(n) = part.strip_prefix('+') {
                            st.ahead = n.parse().unwrap_or(0);
                        } else if let Some(n) = part.strip_prefix('-') {
                            st.behind = n.parse().unwrap_or(0);
                        }
                    }
                }
                _ => {}
            }
            continue;
        }
        let kind = rec.as_bytes()[0];
        match kind {
            b'1' | b'2' | b'u' => {
                // 1 XY sub mH mI mW hH hI path
                // 2 XY sub mH mI mW hH hI Xscore path<NUL>origPath
                // u XY sub m1 m2 m3 mW h1 h2 h3 path
                let n = match kind {
                    b'1' => 8,
                    b'2' => 9,
                    _ => 10,
                };
                let parts: Vec<&str> = rec.splitn(n + 1, ' ').collect();
                let Some(path) = parts.get(n) else { continue };
                let xy = parts.get(1).map(|s| s.as_bytes()).unwrap_or(b"..");
                let orig = if kind == b'2' { fields.next() } else { None };
                let Some(path) = rel(path) else { continue };
                let (index, worktree) = if kind == b'u' {
                    (GitFileState::Conflicted, GitFileState::Conflicted)
                } else {
                    (state(xy[0]), state(*xy.get(1).unwrap_or(&b'.')))
                };
                st.files.push(GitFile {
                    path,
                    index,
                    worktree,
                    orig_path: orig.and_then(|o| rel(&o)),
                });
            }
            b'?' => {
                if let Some(path) = rel(&rec[2..]) {
                    st.files.push(GitFile {
                        path,
                        index: GitFileState::Untracked,
                        worktree: GitFileState::Untracked,
                        orig_path: None,
                    });
                }
            }
            _ => {}
        }
    }
    st
}

/// The workspace's Git status; a folder outside a repository (or no `git`) is `repo: false`.
pub fn status(root: &Path) -> Result<GitStatus, GitError> {
    let repo = match discover(root) {
        Ok(r) => r,
        Err(GitError::NoGit) => return Ok(no_repo(GitError::NoGit.to_string())),
        Err(GitError::NotARepository(m)) => return Ok(no_repo(m)),
        Err(e) => return Err(e),
    };
    let out = run(
        &repo.git,
        root,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
            "--",
            ".",
        ],
    )?
    .map_err(GitError::Failed)?;
    Ok(parse_status(&out, &repo.prefix))
}

fn no_repo(reason: String) -> GitStatus {
    GitStatus {
        repo: false,
        reason: Some(reason),
        branch: None,
        head: None,
        upstream: None,
        ahead: 0,
        behind: 0,
        files: Vec::new(),
        polling: false,
    }
}

/// A file's bytes at HEAD (`head`) or in the index; `None` text when binary.
pub fn show(root: &Path, path: &str, head: bool) -> Result<(Option<String>, bool), GitError> {
    let repo = discover(root)?;
    let spec = format!("{}:{}{}", if head { "HEAD" } else { "" }, repo.prefix, path);
    let bytes = run(&repo.git, root, &["show", &spec])?.map_err(|e| {
        GitError::NotFound(if e.is_empty() {
            format!(
                "{path} is not in {}.",
                if head { "HEAD" } else { "the index" }
            )
        } else {
            format!(
                "{path} is not in {}: {e}",
                if head { "HEAD" } else { "the index" }
            )
        })
    })?;
    let binary = bytes.iter().take(8000).any(|b| *b == 0);
    if binary {
        return Ok((None, true));
    }
    Ok(match String::from_utf8(bytes) {
        Ok(t) => (Some(t), false),
        Err(_) => (None, true),
    })
}

/// Watches the repository's own state (HEAD, the index, refs, merge state): a commit, checkout,
/// stage or fetch made outside the app. Each relevant change sends `()` (coalesce on the receiver).
/// The watcher stops when the returned value is dropped.
pub fn watch_git_dir(
    git_dir: &Path,
) -> Result<(notify::RecommendedWatcher, tokio::sync::mpsc::Receiver<()>), GitError> {
    use notify::Watcher;
    let (tx, rx) = tokio::sync::mpsc::channel(16);
    let base = git_dir.to_path_buf();
    let mut w = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        let Ok(ev) = res else { return };
        let relevant = ev.paths.iter().any(|p| {
            let Ok(r) = p.strip_prefix(&base) else {
                return false;
            };
            let r = r.to_string_lossy();
            matches!(
                r.as_ref(),
                "HEAD" | "index" | "packed-refs" | "MERGE_HEAD" | "ORIG_HEAD" | "FETCH_HEAD"
            ) || r.starts_with("refs/")
                || r.starts_with("refs\\")
        });
        if relevant {
            let _ = tx.try_send(());
        }
    })
    .map_err(|e| GitError::Failed(e.to_string()))?;
    w.watch(git_dir, notify::RecursiveMode::Recursive)
        .map_err(|e| GitError::Failed(e.to_string()))?;
    Ok((w, rx))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn porcelain_v2_records() {
        let out = b"# branch.oid 5493f5ea6352b5eb6ab9f29ac563d72a0fab06c5\0# branch.head main\0# branch.upstream origin/main\0# branch.ab +2 -1\0\
1 .M N... 100644 100644 100644 aaa aaa sub/a.rs\0\
1 A. N... 000000 100644 100644 000 bbb sub/new.rs\0\
2 R. N... 100644 100644 100644 ccc ccc R100 sub/b.rs\0sub/old.rs\0\
u UU N... 100644 100644 100644 100644 d1 d2 d3 sub/c.rs\0\
? sub/untracked file.txt\0\
1 .M N... 100644 100644 100644 eee eee outside.rs\0";
        let st = parse_status(out, "sub/");
        assert_eq!(st.branch.as_deref(), Some("main"));
        assert_eq!(st.head.as_deref(), Some("5493f5e"));
        assert_eq!(st.upstream.as_deref(), Some("origin/main"));
        assert_eq!((st.ahead, st.behind), (2, 1));
        let f: Vec<_> = st
            .files
            .iter()
            .map(|f| (f.path.0.as_str(), f.index, f.worktree))
            .collect();
        assert_eq!(
            f,
            vec![
                ("a.rs", GitFileState::Unmodified, GitFileState::Modified),
                ("new.rs", GitFileState::Added, GitFileState::Unmodified),
                ("b.rs", GitFileState::Renamed, GitFileState::Unmodified),
                ("c.rs", GitFileState::Conflicted, GitFileState::Conflicted),
                (
                    "untracked file.txt",
                    GitFileState::Untracked,
                    GitFileState::Untracked
                ),
            ]
        );
        assert_eq!(
            st.files[2].orig_path.as_ref().map(|p| p.0.as_str()),
            Some("old.rs")
        );
    }

    #[test]
    fn initial_and_detached() {
        let st = parse_status(b"# branch.oid (initial)\0# branch.head (detached)\0", "");
        assert!(st.head.is_none() && st.branch.is_none());
    }
}
