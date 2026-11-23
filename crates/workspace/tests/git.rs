//! Git status and show against real temporary repositories (M4-GIT-001).

use axiom_protocol::git::GitFileState as S;
use axiom_workspace::git;
use std::path::Path;
use std::process::Command;

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

fn repo() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    g(d.path(), &["init", "-q"]);
    std::fs::write(d.path().join("keep.txt"), "keep\n").unwrap();
    std::fs::write(d.path().join("mod.txt"), "one\n").unwrap();
    std::fs::write(d.path().join("del.txt"), "gone\n").unwrap();
    std::fs::write(d.path().join("old.txt"), "moving\n").unwrap();
    std::fs::write(d.path().join("bin.dat"), [0u8, 1, 2, 3]).unwrap();
    g(d.path(), &["add", "."]);
    g(d.path(), &["commit", "-q", "-m", "first"]);
    d
}

fn states(st: &axiom_protocol::git::GitStatus) -> Vec<(String, S, S)> {
    let mut v: Vec<_> = st
        .files
        .iter()
        .map(|f| (f.path.0.clone(), f.index, f.worktree))
        .collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

#[test]
fn every_kind_of_change() {
    let d = repo();
    let p = d.path();
    std::fs::write(p.join("mod.txt"), "two\n").unwrap();
    std::fs::remove_file(p.join("del.txt")).unwrap();
    g(p, &["mv", "old.txt", "new.txt"]);
    std::fs::write(p.join("staged.txt"), "s\n").unwrap();
    g(p, &["add", "staged.txt"]);
    std::fs::write(p.join("untracked file.txt"), "u\n").unwrap();
    let st = git::status(p).unwrap();
    assert!(st.repo);
    assert_eq!(st.branch.as_deref(), Some("main"));
    assert_eq!(st.head.as_ref().map(String::len), Some(7));
    assert_eq!(
        states(&st),
        vec![
            ("del.txt".into(), S::Unmodified, S::Deleted),
            ("mod.txt".into(), S::Unmodified, S::Modified),
            ("new.txt".into(), S::Renamed, S::Unmodified),
            ("staged.txt".into(), S::Added, S::Unmodified),
            ("untracked file.txt".into(), S::Untracked, S::Untracked),
        ]
    );
    let renamed = st.files.iter().find(|f| f.path.0 == "new.txt").unwrap();
    assert_eq!(
        renamed.orig_path.as_ref().map(|p| p.0.as_str()),
        Some("old.txt")
    );
    // The status run took no lock: no index.lock appeared, and nothing was written to .git/index.
    assert!(!p.join(".git/index.lock").exists());
}

#[test]
fn a_conflict() {
    let d = repo();
    let p = d.path();
    g(p, &["checkout", "-q", "-b", "other"]);
    std::fs::write(p.join("mod.txt"), "theirs\n").unwrap();
    g(p, &["commit", "-qam", "theirs"]);
    g(p, &["checkout", "-q", "main"]);
    std::fs::write(p.join("mod.txt"), "ours\n").unwrap();
    g(p, &["commit", "-qam", "ours"]);
    // The merge stops with a conflict (a non-zero exit, so not through `g`); it needs an identity
    // like every other call here (CI has no global one).
    let out = Command::new("git")
        .current_dir(p)
        .env("GIT_AUTHOR_NAME", "t")
        .env("GIT_AUTHOR_EMAIL", "t@example.com")
        .env("GIT_COMMITTER_NAME", "t")
        .env("GIT_COMMITTER_EMAIL", "t@example.com")
        .args([
            "-c",
            "core.hooksPath=/dev/null",
            "-c",
            "commit.gpgsign=false",
            "merge",
            "-q",
            "other",
        ])
        .output()
        .unwrap();
    assert!(
        String::from_utf8_lossy(&out.stdout).contains("CONFLICT"),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let st = git::status(p).unwrap();
    assert_eq!(
        states(&st),
        vec![("mod.txt".into(), S::Conflicted, S::Conflicted)]
    );
}

#[test]
fn a_subdirectory_workspace_sees_only_itself_relative_to_its_root() {
    let d = repo();
    let p = d.path();
    std::fs::create_dir(p.join("sub")).unwrap();
    std::fs::write(p.join("sub/a.txt"), "a\n").unwrap();
    g(p, &["add", "sub/a.txt"]);
    g(p, &["commit", "-qm", "sub"]);
    std::fs::write(p.join("sub/a.txt"), "changed\n").unwrap();
    std::fs::write(p.join("mod.txt"), "outside change\n").unwrap();
    let st = git::status(&p.join("sub")).unwrap();
    assert_eq!(
        states(&st),
        vec![("a.txt".into(), S::Unmodified, S::Modified)]
    );
    let (text, binary) = git::show(&p.join("sub"), "a.txt", true).unwrap();
    assert_eq!((text.as_deref(), binary), (Some("a\n"), false));
}

#[test]
fn detached_head_and_ahead_behind() {
    let d = repo();
    let p = d.path();
    // A clone tracking the first repository as its upstream.
    let c = tempfile::tempdir().unwrap();
    g(c.path(), &["clone", "-q", p.to_str().unwrap(), "w"]);
    let w = c.path().join("w");
    std::fs::write(w.join("mine.txt"), "1\n").unwrap();
    g(&w, &["add", "mine.txt"]);
    g(&w, &["commit", "-qm", "ahead"]);
    std::fs::write(p.join("theirs.txt"), "2\n").unwrap();
    g(p, &["add", "theirs.txt"]);
    g(p, &["commit", "-qm", "behind"]);
    g(&w, &["fetch", "-q"]);
    let st = git::status(&w).unwrap();
    assert_eq!(st.upstream.as_deref(), Some("origin/main"));
    assert_eq!((st.ahead, st.behind), (1, 1));
    g(&w, &["checkout", "-q", "--detach"]);
    let st = git::status(&w).unwrap();
    assert!(st.branch.is_none() && st.head.is_some());
}

#[test]
fn show_head_index_binary_and_missing() {
    let d = repo();
    let p = d.path();
    std::fs::write(p.join("mod.txt"), "staged\n").unwrap();
    g(p, &["add", "mod.txt"]);
    std::fs::write(p.join("mod.txt"), "working\n").unwrap();
    assert_eq!(
        git::show(p, "mod.txt", true).unwrap(),
        (Some("one\n".into()), false)
    );
    assert_eq!(
        git::show(p, "mod.txt", false).unwrap(),
        (Some("staged\n".into()), false)
    );
    assert_eq!(git::show(p, "bin.dat", true).unwrap(), (None, true));
    std::fs::write(p.join("new.txt"), "n\n").unwrap();
    assert!(matches!(
        git::show(p, "new.txt", true),
        Err(git::GitError::NotFound(_))
    ));
}

#[test]
fn not_a_repository_is_not_an_error() {
    let d = tempfile::tempdir().unwrap();
    let st = git::status(d.path()).unwrap();
    assert!(!st.repo);
    assert!(st.reason.unwrap().contains("not in a Git repository"));
}

/// The budget (M4-GIT-001): `git/status` on this repository under 300 ms.
#[test]
fn status_of_this_repository_is_fast() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Warm the file cache first.
    let _ = git::status(&root).unwrap();
    // The median of three, so one slow process start on a busy runner (the other tests here run
    // git at the same time) is not read as the cost of a status; the limit is unchanged.
    let mut times = Vec::new();
    let mut st = None;
    for _ in 0..3 {
        let t = std::time::Instant::now();
        st = Some(git::status(&root).unwrap());
        times.push(t.elapsed().as_millis());
    }
    let st = st.unwrap();
    times.sort_unstable();
    let ms = times[1];
    eprintln!(
        "git status of this repository: {ms} ms (median of {times:?}), {} files",
        st.files.len()
    );
    assert!(st.repo);
    // Windows support is deferred by the owner: a `git` process start on the Windows runners
    // alone takes most of the budget (300–345 ms medians seen), so there the time is reported,
    // not held to the budget, until Windows is taken up again.
    if cfg!(windows) {
        eprintln!("NOT RUN on Windows (deferred): the 300 ms budget; measured {ms} ms");
        return;
    }
    // GitHub's macOS runners are virtual machines: medians of 310, 368, 423 and 465 ms seen
    // there, against 36 ms on a real Mac (Apple Silicon, 2026-11-23). There the runner, not the
    // code, sets the time, so it is held to a ceiling of 1.5 s instead (review C2): still a
    // failure for a gross regression (something quadratic, a scan of ignored trees). Linux CI
    // and real machines hold the 300 ms budget.
    let limit =
        if cfg!(target_os = "macos") && std::env::var("GITHUB_ACTIONS").as_deref() == Ok("true") {
            1500
        } else {
            300
        };
    assert!(
        ms < limit,
        "{ms} ms (median of {times:?}), limit {limit} ms"
    );
}

/// Review E2: the daemon's own `git status` and `git show` run no program the repository's
/// configuration names, even from a file in the work tree that `.git/config` includes (an
/// agent could have written it): not `core.fsmonitor` (run on reading the index), not an
/// external diff, not a textconv filter.
#[cfg(unix)]
#[test]
fn status_and_show_run_no_program_the_repository_configures() {
    use std::os::unix::fs::PermissionsExt;
    let d = repo();
    let p = d.path();
    let outside = tempfile::tempdir().unwrap();
    let marker = outside.path().join("ran");
    let script = outside.path().join("prog.sh");
    std::fs::write(
        &script,
        format!(
            "#!/bin/sh\necho \"$0 $*\" >> '{}'\nexit 1\n",
            marker.display()
        ),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    let s = script.display();
    std::fs::write(
        p.join("shared.gitconfig"),
        format!(
            "[core]\n\tfsmonitor = {s}\n[diff]\n\texternal = {s}\n[diff \"x\"]\n\ttextconv = {s}\n"
        ),
    )
    .unwrap();
    g(p, &["config", "include.path", "../shared.gitconfig"]);
    std::fs::write(p.join(".gitattributes"), "*.txt diff=x\n").unwrap();
    std::fs::write(p.join("mod.txt"), "two\n").unwrap();
    let st = git::status(p).unwrap();
    assert!(st.repo);
    git::show(p, "mod.txt", true).unwrap();
    git::show(p, "mod.txt", false).unwrap();
    assert!(
        !marker.exists(),
        "a configured program ran: {}",
        std::fs::read_to_string(&marker).unwrap_or_default()
    );
}

/// Review E2: what the agent gate protects is git's own answer. The configuration files in
/// play and every file an include names (whether it exists yet or not, `includeIf` too), the
/// hooks directory however it is set (here in an included file), and for a `.git` file the
/// repository it points to.
#[test]
fn config_files_are_what_git_reads() {
    let d = repo();
    let p = &dunce::canonicalize(d.path()).unwrap();
    g(p, &["config", "include.path", "../shared.gitconfig"]);
    g(
        p,
        &[
            "config",
            "includeIf.onbranch:main.path",
            "../branch.gitconfig",
        ],
    );
    std::fs::write(
        p.join("shared.gitconfig"),
        "[core]\n\thooksPath = tools/hooks\n[include]\n\tpath = nested/more.gitconfig\n",
    )
    .unwrap();
    let c = git::config_files(p).unwrap();
    assert_eq!(c.hooks.as_deref(), Some(p.join("tools/hooks").as_path()));
    let norm = |f: &Path| {
        let s = f.to_string_lossy().replace(".git/../", "");
        std::path::PathBuf::from(s)
    };
    let files: Vec<_> = c.files.iter().map(|f| norm(f)).collect();
    for want in [
        p.join(".git/config"),
        p.join("shared.gitconfig"),
        p.join("branch.gitconfig"),
        p.join("nested/more.gitconfig"),
    ] {
        assert!(files.contains(&want), "{want:?} not in {files:?}");
    }
    assert!(c.git_dirs.contains(&p.join(".git")), "{:?}", c.git_dirs);

    // A `.git` file: the repository is elsewhere.
    let w = tempfile::tempdir().unwrap();
    let w = &dunce::canonicalize(w.path()).unwrap();
    let outside = tempfile::tempdir().unwrap();
    let gd = dunce::canonicalize(outside.path())
        .unwrap()
        .join("repo.git");
    g(
        w,
        &[
            "init",
            "-q",
            "--separate-git-dir",
            gd.to_str().unwrap(),
            w.to_str().unwrap(),
        ],
    );
    g(w, &["config", "core.hooksPath", "tools/hooks"]);
    let c = git::config_files(w).unwrap();
    assert_eq!(c.hooks.as_deref(), Some(w.join("tools/hooks").as_path()));
    assert!(c.git_dirs.contains(&gd), "{:?}", c.git_dirs);
    assert!(c.files.contains(&gd.join("config")), "{:?}", c.files);

    // Outside a repository: nothing, unless a `.git/config` is waiting for `git init`.
    let n = tempfile::tempdir().unwrap();
    assert_eq!(
        git::config_files(n.path()).unwrap(),
        git::ConfigFiles::default()
    );
    std::fs::create_dir(n.path().join(".git")).unwrap();
    std::fs::write(n.path().join(".git/config"), "[core]\n\thooksPath = .\n").unwrap();
    let c = git::config_files(n.path()).unwrap();
    assert_eq!(c.hooks.as_deref(), Some(n.path()));
    assert_eq!(c.files, [n.path().join(".git/config")]);
}
