//! `git/*` through the daemon (M4-GIT-001): status and show, `git/didChange` after a worktree
//! edit and after a commit made outside the app, and a folder that is not a repository.

mod common;

use axiom_protocol::git::{GitFileState, GitRev, Show, ShowParams, Status};
use axiom_protocol::workspace::WorkspaceRef;
use axiom_protocol::{ErrorKind, WorkspacePath};
use common::{kind, Daemon};
use std::path::Path;
use std::process::Command;
use std::time::Duration;

fn g(dir: &Path, args: &[&str]) {
    let out = Command::new("git")
        .current_dir(dir)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .args([
            "-c",
            "init.defaultBranch=main",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
        ])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[tokio::test]
async fn status_show_and_change_notifications() {
    let d = Daemon::ready().await;
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().to_path_buf();
    g(&p, &["init", "-q"]);
    std::fs::write(p.join("a.txt"), "one\n").unwrap();
    g(&p, &["add", "."]);
    g(&p, &["commit", "-qm", "first"]);
    let f = d.open_dir(dir).await;
    let st = d
        .client
        .request::<Status>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    assert!(st.repo && st.files.is_empty() && st.branch.as_deref() == Some("main"));

    // An edit in the worktree: git/didChange, then the status shows it.
    std::fs::write(p.join("a.txt"), "two\n").unwrap();
    d.next_notification("git/didChange", Duration::from_secs(10))
        .await;
    let st = d
        .client
        .request::<Status>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(st.files.len(), 1);
    assert_eq!(
        (st.files[0].path.0.as_str(), st.files[0].worktree),
        ("a.txt", GitFileState::Modified)
    );
    let show = d
        .client
        .request::<Show>(ShowParams {
            workspace_id: f.id.clone(),
            path: WorkspacePath("a.txt".into()),
            rev: GitRev::Head,
        })
        .await
        .unwrap();
    assert_eq!(show.text.as_deref(), Some("one\n"));

    // A commit made outside the app (only .git changes): git/didChange, and the status is clean.
    d.drain_until_quiet(Duration::from_millis(600)).await;
    g(&p, &["commit", "-qam", "second"]);
    d.next_notification("git/didChange", Duration::from_secs(10))
        .await;
    let st = d
        .client
        .request::<Status>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    assert!(st.files.is_empty(), "{:?}", st.files);

    // A path that escapes the workspace is refused.
    let e = d
        .client
        .request::<Show>(ShowParams {
            workspace_id: f.id.clone(),
            path: WorkspacePath("../x".into()),
            rev: GitRev::Head,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
}

#[tokio::test]
async fn a_folder_outside_a_repository() {
    let d = Daemon::ready().await;
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("x.txt"), "x").unwrap();
    let f = d.open_dir(dir).await;
    let st = d
        .client
        .request::<Status>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    assert!(!st.repo && st.reason.is_some());
    let e = d
        .client
        .request::<Show>(ShowParams {
            workspace_id: f.id.clone(),
            path: WorkspacePath("x.txt".into()),
            rev: GitRev::Head,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::GitUnavailable);
}

/// AUD-012, review N14: a repository whose change watch cannot be made is polled, and the very
/// first `git/status` already says so. The `git` here is a stand-in whose repository directory
/// does not exist, so the watch of it fails while `status` works.
#[cfg(unix)]
#[tokio::test]
async fn the_first_status_says_the_repository_is_polled() {
    use std::os::unix::fs::PermissionsExt;
    let bin = tempfile::tempdir().unwrap();
    let missing = bin.path().join("no-such-git-dir");
    let git = bin.path().join("git");
    std::fs::write(
        &git,
        format!(
            "#!/bin/sh\n\
             case \"$*\" in\n\
             *rev-parse*) printf '%s\\n%s\\n\\n' \"$(pwd -P)\" '{}' ;;\n\
             *status*) printf '# branch.oid (initial)\\000# branch.head main\\000' ;;\n\
             *) exit 1 ;;\n\
             esac\n",
            missing.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&git, std::fs::Permissions::from_mode(0o755)).unwrap();
    let data = tempfile::tempdir().unwrap();
    let d = Daemon::spawn_configured(data.path(), |c| {
        c.env("PATH", bin.path());
    })
    .await;
    d.initialize().await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let st = d
        .client
        .request::<Status>(WorkspaceRef {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    assert!(st.repo, "{st:?}");
    assert!(st.polling, "the first status: {st:?}");
}
