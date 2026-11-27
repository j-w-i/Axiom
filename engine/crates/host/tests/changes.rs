//! What has been changed in a session's folder, as git has it.
#![cfg(any(target_os = "macos", target_os = "linux"))]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::{path::Path, process::Command};

use axiom_host::{Config, client::Client};
use axiom_proto::*;

mod common;
use common::*;

fn git(dir: &Path, args: &[&str]) {
    let o = Command::new("git")
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com", "-c", "commit.gpgsign=false"])
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .output()
        .unwrap();
    assert!(o.status.success(), "{args:?}: {}", String::from_utf8_lossy(&o.stderr));
}

async fn start(f: &Fixture) -> (Client, SessionId) {
    let w = f.dir.path().join("w");
    for d in ["w", "home"] {
        std::fs::create_dir(f.dir.path().join(d)).unwrap();
    }
    let mut c = Config::new(f.paths.clone());
    c.home = Some(f.dir.path().join("home"));
    f.serve_with(c);
    let mut c = f.client().await;
    let w = Some(w.to_str().unwrap().to_owned());
    let v = c.call(Call::SessionCreate { title: "S".into(), workspace: w }).await.unwrap();
    (c, SessionId(v["session"].as_str().unwrap().into()))
}
async fn changes(c: &mut Client, s: &SessionId) -> Changes {
    let call = Call::Changes { session: s.clone(), repository: None, against: None, base: false };
    serde_json::from_value(c.call(call).await.unwrap()).unwrap()
}

#[tokio::test]
async fn what_has_changed_is_told() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    let w = f.dir.path().join("w");
    assert_eq!(changes(&mut c, &s).await, Changes::default(), "no repository");
    git(&w, &["init", "-q", "-b", "trunk"]);
    std::fs::write(w.join("a.txt"), "one\ntwo\n").unwrap();
    let all = changes(&mut c, &s).await;
    assert_eq!((all.repository, all.branch.as_deref()), (true, Some("trunk")));
    assert!(all.diff.contains("+one") && all.whole, "{all:?}");
    std::fs::write(w.join("gone.txt"), "x\n").unwrap();
    std::fs::write(w.join(".gitignore"), "ignored\n").unwrap();
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "first"]);
    assert_eq!(changes(&mut c, &s).await.files, []);
    std::fs::write(w.join("a.txt"), "one\n2\n").unwrap();
    std::fs::remove_file(w.join("gone.txt")).unwrap();
    std::fs::write(w.join("ignored"), "x").unwrap();
    std::fs::create_dir(w.join("new dir")).unwrap();
    std::fs::write(w.join("new dir/é.txt"), "fresh\n").unwrap();
    std::fs::write(w.join("staged.txt"), "s\n").unwrap();
    git(&w, &["add", "staged.txt"]);
    std::fs::write(w.join("bin"), b"a\0b").unwrap();
    // What a link leads to is not shown.
    std::fs::write(f.dir.path().join("secret"), "SECRET").unwrap();
    std::os::unix::fs::symlink(f.dir.path().join("secret"), w.join("link")).unwrap();
    let all = changes(&mut c, &s).await;
    let mut files: Vec<(&str, &str)> =
        all.files.iter().map(|f| (f.path.as_str(), f.what.as_str())).collect();
    files.sort();
    assert_eq!(
        files,
        [
            ("a.txt", "changed"),
            ("bin", "new"),
            ("gone.txt", "removed"),
            ("link", "new"),
            ("new dir/é.txt", "new"),
            ("staged.txt", "new"),
        ]
    );
    for part in ["-two", "+2", "-x", "+fresh", "+s", "--- a.txt", "+++ a.txt", "+++ new dir/é.txt"]
    {
        assert!(all.diff.contains(part), "{part}: {}", all.diff);
    }
    assert!(!all.diff.contains("SECRET") && all.whole);
    // Nothing of the repository was touched by looking.
    std::fs::write(w.join("big"), "y\n".repeat(200_000)).unwrap();
    let all = changes(&mut c, &s).await;
    assert!(!all.whole && all.files.iter().any(|f| f.path == "big"));
}

/// SI-6: a repository's own configuration can name programs. None of it has an effect.
#[tokio::test]
async fn nothing_of_a_repository_takes_effect() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    let w = f.dir.path().join("w");
    git(&w, &["init", "-q"]);
    std::fs::write(w.join("a.txt"), "one\n").unwrap();
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "first"]);
    let mark = f.dir.path().join("ran");
    let inside = w.join("ran");
    let evil = w.join("evil.sh");
    let script =
        format!("#!/bin/sh\ntouch '{}' '{}'\ncat \"$@\"\n", mark.display(), inside.display());
    std::fs::write(&evil, script).unwrap();
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&evil, std::fs::Permissions::from_mode(0o755)).unwrap();
    let e = evil.to_str().unwrap();
    for (k, v) in [
        ("core.fsmonitor", e),
        ("diff.external", e),
        ("core.pager", e),
        ("pager.diff", e),
        ("diff.evil.textconv", e),
        ("diff.evil.command", e),
        ("filter.evil.clean", e),
        ("filter.evil.process", e),
    ] {
        git(&w, &["config", k, v]);
    }
    std::fs::write(w.join(".git/hooks/post-index-change"), format!("#!/bin/sh\nexec {e}\n"))
        .unwrap();
    std::fs::write(w.join(".gitattributes"), "*.txt diff=evil filter=evil\n").unwrap();
    std::fs::write(w.join("a.txt"), "two\n").unwrap();
    let all = changes(&mut c, &s).await;
    assert!(all.repository && all.files.iter().any(|f| f.path == "a.txt"), "{all:?}");
    assert!(!mark.exists() && !inside.exists(), "something of it was run with an effect");
}

#[tokio::test]
async fn the_log_one_commit_and_a_comparison_are_read() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    let w = f.dir.path().join("w");
    git(&w, &["init", "-q", "-b", "trunk"]);
    std::fs::write(w.join("a.txt"), "one\n").unwrap();
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "first\n\nwhy it is"]);
    git(&w, &["switch", "-q", "-c", "side"]);
    std::fs::write(w.join("a.txt"), "two\n").unwrap();
    git(&w, &["commit", "-q", "-am", "second"]);
    std::fs::write(w.join("a.txt"), "three\n").unwrap();
    let call = |from: Option<&str>| Call::GitLog {
        session: s.clone(),
        repository: None,
        from: from.map(String::from),
        most: 10,
    };
    let log: Vec<CommitInfo> = serde_json::from_value(c.call(call(None)).await.unwrap()).unwrap();
    assert_eq!(log.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["second", "first"]);
    assert_eq!((log[1].author.as_str(), log[1].id.len()), ("t", 40));
    let trunk: Vec<CommitInfo> =
        serde_json::from_value(c.call(call(Some("trunk"))).await.unwrap()).unwrap();
    assert_eq!(trunk.len(), 1);
    let show = Call::GitShow { session: s.clone(), repository: None, commit: log[0].id.clone() };
    let one: CommitShown = serde_json::from_value(c.call(show).await.unwrap()).unwrap();
    assert_eq!(one.message, "second");
    assert!(one.diff.contains("-one\n+two") && one.whole, "{}", one.diff);
    let show = Call::GitShow { session: s.clone(), repository: None, commit: "trunk".into() };
    let first: CommitShown = serde_json::from_value(c.call(show).await.unwrap()).unwrap();
    assert_eq!(first.message, "first\n\nwhy it is");
    // Against a branch, and against where the two parted.
    let against = |base| Call::Changes {
        session: s.clone(),
        repository: None,
        against: Some("trunk".into()),
        base,
    };
    let all: Changes = serde_json::from_value(c.call(against(false)).await.unwrap()).unwrap();
    assert!(all.diff.contains("-one\n+three"), "{}", all.diff);
    assert_eq!(all.against.as_deref(), Some("trunk"));
    let all: Changes = serde_json::from_value(c.call(against(true)).await.unwrap()).unwrap();
    assert!(all.diff.contains("-one\n+three"), "{}", all.diff);
    let branches: Vec<String> = serde_json::from_value(
        c.call(Call::GitBranches { session: s.clone(), repository: None }).await.unwrap(),
    )
    .unwrap();
    assert_eq!(branches, ["side", "trunk"]);
    // Nothing a person types is taken for an option.
    for bad in ["--output=/tmp/x", "-p", "a..b", "x;y", ""] {
        let call = Call::GitShow { session: s.clone(), repository: None, commit: bad.into() };
        assert!(c.call(call).await.is_err(), "{bad}");
        let call = Call::Changes {
            session: s.clone(),
            repository: None,
            against: Some(bad.into()),
            base: false,
        };
        assert!(c.call(call).await.is_err(), "{bad}");
    }
    let call = Call::GitShow { session: s.clone(), repository: None, commit: "nothere".into() };
    assert!(c.call(call).await.is_err());
}

#[tokio::test]
async fn a_folder_of_repositories_and_how_far_one_is_ahead() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    let w = f.dir.path().join("w");
    for r in ["api", "apps/web", "node_modules/x", ".hidden"] {
        let d = w.join(r);
        std::fs::create_dir_all(&d).unwrap();
        git(&d, &["init", "-q", "-b", "main"]);
        std::fs::write(d.join("f"), "1\n").unwrap();
        git(&d, &["add", "-A"]);
        git(&d, &["commit", "-q", "-m", "one"]);
    }
    // One follows another, and is ahead of it.
    let web = w.join("apps/web");
    git(&w, &["clone", "-q", web.to_str().unwrap(), w.join("api2").to_str().unwrap()]);
    std::fs::write(w.join("api2/f"), "2\n").unwrap();
    git(&w.join("api2"), &["commit", "-q", "-am", "two"]);
    let call = |r: Option<&str>| Call::Changes {
        session: s.clone(),
        repository: r.map(String::from),
        against: None,
        base: false,
    };
    let all: Changes = serde_json::from_value(c.call(call(None)).await.unwrap()).unwrap();
    assert_eq!(all.repositories, ["api", "api2", "apps/web"]);
    assert!(all.repository, "the first is shown");
    let two: Changes = serde_json::from_value(c.call(call(Some("api2"))).await.unwrap()).unwrap();
    assert_eq!(
        (two.upstream.as_deref(), two.ahead, two.behind),
        (Some("origin/main"), Some(1), Some(0))
    );
    std::fs::write(w.join("apps/web/f"), "3\n").unwrap();
    let web: Changes =
        serde_json::from_value(c.call(call(Some("apps/web"))).await.unwrap()).unwrap();
    assert_eq!(web.files.len(), 1);
    for bad in ["node_modules/x", ".hidden", "../w", "nothere"] {
        assert!(c.call(call(Some(bad))).await.is_err(), "{bad}");
    }
}

fn digest(p: &Path) -> Option<String> {
    std::fs::read(p).ok().map(|b| axiom_tools::fs::digest(&b))
}

fn out(dir: &Path, args: &[&str]) -> String {
    let o = Command::new("git").args(args).current_dir(dir).output().unwrap();
    String::from_utf8_lossy(&o.stdout).trim().to_owned()
}

#[tokio::test]
async fn a_person_commits_what_they_saw_and_no_hook_runs() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    let w = f.dir.path().join("w");
    std::fs::write(
        f.dir.path().join("home/.gitconfig"),
        "[user]\n\tname = Ada\n\temail = \"ada@example.com\"\n[commit]\n\tgpgsign = true\n",
    )
    .unwrap();
    git(&w, &["init", "-q", "-b", "main"]);
    std::fs::write(w.join("a.txt"), "one\n").unwrap();
    std::fs::write(w.join("b.txt"), "b\n").unwrap();
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "first"]);
    let marker = f.dir.path().join("hook-ran");
    let hook = w.join(".git/hooks/pre-commit");
    std::fs::write(&hook, format!("#!/bin/sh\ntouch '{}'\n", marker.display())).unwrap();
    std::fs::set_permissions(&hook, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
    std::fs::write(w.join("a.txt"), "two\n").unwrap();
    std::fs::write(w.join("b.txt"), "not this\n").unwrap();
    std::fs::write(w.join("new.txt"), "new\n").unwrap();
    let seen =
        |p: &str| CopySeen { path: p.into(), digest: digest(&w.join(p)), run: false, there: None };
    let commit = |files: Vec<CopySeen>, branch: Option<&str>, onto_default: bool| Call::GitCommit {
        session: s.clone(),
        repository: None,
        files,
        message: "Second\n\nWhy.".into(),
        branch: branch.map(String::from),
        onto_default,
        author: None,
    };
    let all: Changes = serde_json::from_value(
        c.call(Call::Changes { session: s.clone(), repository: None, against: None, base: false })
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!((all.default_branch.as_deref(), all.hooks), (Some("main"), true));
    // The repository's own branch is asked about once more.
    let no = refused(c.call(commit(vec![seen("a.txt"), seen("new.txt")], None, false)).await);
    assert!(no.message.contains("main"), "{no:?}");
    // What was shown is what goes in; another content is refused.
    let mut stale = seen("a.txt");
    stale.digest = digest(&w.join("b.txt"));
    assert!(c.call(commit(vec![stale], None, true)).await.is_err());
    let done: Committed = serde_json::from_value(
        c.call(commit(vec![seen("a.txt"), seen("new.txt")], None, true)).await.unwrap(),
    )
    .unwrap();
    assert_eq!(done.commit.subject, "Second");
    assert!(done.hooks && done.unsigned && done.changed.is_empty(), "{done:?}");
    assert!(!marker.exists(), "no hook ran");
    assert_eq!(
        out(&w, &["log", "-n1", "--format=%an <%ae>|%B"]),
        "Ada <ada@example.com>|Second\n\nWhy."
    );
    assert_eq!(out(&w, &["show", "--name-only", "--format="]), "a.txt\nnew.txt");
    assert_eq!(out(&w, &["status", "--porcelain"]), "M b.txt", "the rest stays as it was");
    // On a new branch, without a second word.
    std::fs::write(w.join("b.txt"), "b2\n").unwrap();
    let on: Committed = serde_json::from_value(
        c.call(commit(vec![seen("b.txt")], Some("topic"), false)).await.unwrap(),
    )
    .unwrap();
    assert_eq!(on.branch.as_deref(), Some("topic"));
    assert_eq!(out(&w, &["rev-parse", "--abbrev-ref", "HEAD"]), "topic");
    for bad in ["-x", "a b", "x..y", "x@{1}"] {
        std::fs::write(w.join("b.txt"), format!("{bad}\n")).unwrap();
        assert!(c.call(commit(vec![seen("b.txt")], Some(bad), false)).await.is_err(), "{bad}");
    }
    assert!(c.call(commit(vec![seen("../x")], Some("t2"), false)).await.is_err());
    assert!(!std::fs::read_to_string(w.join(".git/config")).unwrap().contains("hooksPath"));
}

#[tokio::test]
async fn push_and_pull_are_fast_forwards_and_run_nothing_of_the_repository() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    let w = f.dir.path().join("w");
    let bare = f.dir.path().join("remote.git");
    git(f.dir.path(), &["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()]);
    git(&w, &["init", "-q", "-b", "main"]);
    std::fs::write(w.join("a.txt"), "one\n").unwrap();
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "first"]);
    git(&w, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&w, &["push", "-q", "-u", "origin", "main"]);
    std::fs::write(w.join("a.txt"), "two\n").unwrap();
    git(&w, &["commit", "-q", "-am", "second"]);
    let push = || Call::GitPush { session: s.clone(), repository: None };
    let pull = || Call::GitPull { session: s.clone(), repository: None };
    let all = changes(&mut c, &s).await;
    assert_eq!((all.remote.as_deref(), all.ahead), (Some("origin"), Some(1)));
    assert_eq!(all.remote_url.as_deref(), bare.to_str());
    c.call(push()).await.unwrap();
    assert_eq!(out(&bare, &["log", "-n1", "--format=%s", "main"]), "second");
    // Someone else moves it on; it is taken as a fast-forward.
    let other = f.dir.path().join("other");
    git(f.dir.path(), &["clone", "-q", bare.to_str().unwrap(), other.to_str().unwrap()]);
    std::fs::write(other.join("b.txt"), "theirs\n").unwrap();
    git(&other, &["add", "-A"]);
    git(&other, &["commit", "-q", "-m", "third"]);
    git(&other, &["push", "-q"]);
    c.call(pull()).await.unwrap();
    assert_eq!(std::fs::read_to_string(w.join("b.txt")).unwrap(), "theirs\n");
    assert_eq!(out(&w, &["log", "-n1", "--format=%s"]), "third");
    // What is no fast-forward is refused, both ways.
    std::fs::write(other.join("b.txt"), "theirs again\n").unwrap();
    git(&other, &["commit", "-q", "-am", "fourth"]);
    git(&other, &["push", "-q"]);
    std::fs::write(w.join("a.txt"), "mine\n").unwrap();
    git(&w, &["commit", "-q", "-am", "mine"]);
    // Git could make a merge of it here, knowing who would: it is not asked to.
    git(&w, &["config", "user.name", "t"]);
    git(&w, &["config", "user.email", "t@example.com"]);
    assert!(c.call(push()).await.is_err());
    assert!(c.call(pull()).await.is_err());
    assert_eq!(out(&bare, &["log", "-n1", "--format=%s", "main"]), "fourth", "nothing forced");
    assert_eq!(out(&w, &["log", "-n1", "--format=%s"]), "mine");
    // A configuration that would run something here is refused, before anything is run.
    let marker = f.dir.path().join("ran");
    git(&w, &["config", "core.sshCommand", &format!("touch '{}'", marker.display())]);
    let no = refused(c.call(push()).await);
    assert!(no.message.contains("core.sshcommand"), "{no:?}");
    git(&w, &["config", "--unset", "core.sshCommand"]);
    git(&w, &["config", "include.path", "/dev/null"]);
    assert!(c.call(pull()).await.is_err());
    assert!(!marker.exists());
}

#[tokio::test]
async fn a_pull_that_would_change_a_protected_name_is_refused() {
    let f = fixture();
    let (mut c, s) = start(&f).await;
    let w = f.dir.path().join("w");
    let bare = f.dir.path().join("remote.git");
    git(f.dir.path(), &["init", "-q", "--bare", "-b", "main", bare.to_str().unwrap()]);
    git(&w, &["init", "-q", "-b", "main"]);
    std::fs::write(w.join("a.txt"), "one\n").unwrap();
    git(&w, &["add", "-A"]);
    git(&w, &["commit", "-q", "-m", "first"]);
    git(&w, &["remote", "add", "origin", bare.to_str().unwrap()]);
    git(&w, &["push", "-q", "-u", "origin", "main"]);
    let other = f.dir.path().join("other");
    git(f.dir.path(), &["clone", "-q", bare.to_str().unwrap(), other.to_str().unwrap()]);
    std::fs::create_dir(other.join(".vscode")).unwrap();
    std::fs::write(other.join(".vscode/tasks.json"), "{}").unwrap();
    git(&other, &["add", "-A"]);
    git(&other, &["commit", "-q", "-m", "tasks"]);
    git(&other, &["push", "-q"]);
    let no = refused(c.call(Call::GitPull { session: s.clone(), repository: None }).await);
    assert!(no.message.contains(".vscode/tasks.json"), "{no:?}");
    assert!(!w.join(".vscode").exists());
    assert_eq!(out(&w, &["log", "-n1", "--format=%s"]), "first");
}
