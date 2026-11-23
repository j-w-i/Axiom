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
    assert_eq!(c.hooks.first(), Some(&p.join("tools/hooks")));
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
    assert_eq!(c.hooks.first(), Some(&w.join("tools/hooks")));
    assert!(c.git_dirs.contains(&gd), "{:?}", c.git_dirs);
    assert!(c.files.contains(&gd.join("config")), "{:?}", c.files);

    // Outside a repository: nothing, unless a `.git/config` is waiting for `git init`. (Paths
    // that exist come back resolved through symlinks, review G-normal: the temporary folder's
    // own path is one on macOS.)
    let n = tempfile::tempdir().unwrap();
    let n = &dunce::canonicalize(n.path()).unwrap();
    assert_eq!(git::config_files(n).unwrap(), git::ConfigFiles::default());
    std::fs::create_dir(n.join(".git")).unwrap();
    std::fs::write(n.join(".git/config"), "[core]\n\thooksPath = .\n").unwrap();
    let c = git::config_files(n).unwrap();
    assert_eq!(c.hooks, std::slice::from_ref(n));
    assert_eq!(c.files, [n.join(".git/config")]);
}

/// A script outside the repository that records each run in `marker` (and passes its input
/// through, as a filter would), for the configuration to name.
#[cfg(unix)]
fn marker_script(dir: &Path, name: &str, marker: &Path) -> std::path::PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = dir.join(name);
    std::fs::write(
        &script,
        format!("#!/bin/sh\necho \"$0 $*\" >> '{}'\ncat\n", marker.display()),
    )
    .unwrap();
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).unwrap();
    script
}

/// Review G1, the reviewer's `probe_e`: the daemon's own `git status` runs no filter driver.
/// The configuration defines filters (a `clean` script in the work tree, a `process` one, one
/// that is `required`, one whose name has `=` and dots in it), an agent-written
/// `.gitattributes` routes every file through them, and a same-size edit makes `status` read
/// the file again. A submodule with its own filter and attributes, edited too, is not looked
/// into. Every other program-valued key is set as well: none runs on `status`, `show`,
/// `config_files` or `repositories`. The status is still right.
#[cfg(unix)]
#[test]
fn status_runs_no_filter_driver_and_nothing_else_the_configuration_names() {
    let d = repo();
    let p = d.path();
    let outside = tempfile::tempdir().unwrap();
    let marker = outside.path().join("ran");
    let prog = marker_script(outside.path(), "prog.sh", &marker);
    let prog = prog.display();
    // A submodule (set up first: git's own setup here would run the filters below): its own
    // filter, its own attributes, an edit inside it.
    let src = repo();
    g(
        p,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            src.path().to_str().unwrap(),
            "sm",
        ],
    );
    g(p, &["commit", "-qm", "sm"]);
    let sm = p.join("sm");
    g(&sm, &["config", "filter.sub.clean", &prog.to_string()]);
    std::fs::write(sm.join(".gitattributes"), "*.txt filter=sub\n").unwrap();
    // The in-tree filter script, as the review's probe has it.
    std::fs::create_dir(p.join("tools")).unwrap();
    marker_script(&p.join("tools"), "clean.sh", &marker);
    let config = format!(
        "[filter \"strip\"]\n\tclean = tools/clean.sh %f\n\tsmudge = tools/clean.sh %f\n\
         [filter \"proc\"]\n\tprocess = {prog}\n\trequired = true\n\
         [filter \"odd=name.x\"]\n\tclean = {prog}\n\trequired = true\n\
         [core]\n\tsshCommand = {prog}\n\teditor = {prog}\n\tpager = {prog}\n\taskPass = {prog}\n\
         [credential]\n\thelper = !{prog}\n[gpg]\n\tprogram = {prog}\n\
         [sequence]\n\teditor = {prog}\n[alias]\n\tst = !{prog}\n\
         [diff \"x\"]\n\ttextconv = {prog}\n\tcommand = {prog}\n[merge \"x\"]\n\tdriver = {prog}\n"
    );
    let mut c = std::fs::read_to_string(p.join(".git/config")).unwrap();
    c.push_str(&config);
    std::fs::write(p.join(".git/config"), c).unwrap();
    std::fs::write(
        p.join(".gitattributes"),
        "*.txt filter=strip diff=x merge=x\nmod.txt filter=proc\ndel.txt filter=odd=name.x\n",
    )
    .unwrap();
    // Same-size edits: git must read the files again to know.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    std::fs::write(p.join("keep.txt"), "kepp\n").unwrap();
    std::fs::write(p.join("mod.txt"), "two\n").unwrap();
    std::fs::write(p.join("del.txt"), "gonE\n").unwrap();
    std::fs::write(sm.join("keep.txt"), "kepp\n").unwrap();
    // Checked before any result is: a filter that ran may also have failed the run.
    let st = git::status(p);
    let head = git::show(p, "keep.txt", true);
    let index = git::show(p, "keep.txt", false);
    let config = git::config_files(p);
    git::repositories(p);
    assert!(
        !marker.exists(),
        "a configured program ran: {}",
        std::fs::read_to_string(&marker).unwrap_or_default()
    );
    let st = st.unwrap();
    assert_eq!(head.unwrap(), (Some("keep\n".into()), false));
    assert_eq!(index.unwrap(), (Some("keep\n".into()), false));
    config.unwrap();
    let mut got = states(&st);
    got.retain(|(path, _, _)| !path.starts_with(".gitattributes") && path != "tools/clean.sh");
    assert_eq!(
        got,
        vec![
            ("del.txt".into(), S::Unmodified, S::Modified),
            ("keep.txt".into(), S::Unmodified, S::Modified),
            ("mod.txt".into(), S::Unmodified, S::Modified),
        ]
    );
    // The probe itself: the same `status` without the switch runs the script (the setup is
    // live, so the assertion above means something).
    Command::new("git")
        .current_dir(p)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .args(["-c", "core.fsmonitor=false", "status", "--porcelain=v2"])
        .output()
        .unwrap();
    assert!(
        marker.exists(),
        "the probe's filter did not run without the switch"
    );
}

/// Reviews G3, G4: what git's configuration names as a program, in the work tree, is listed
/// (`core.fsmonitor`, a filter, `core.sshCommand`, a `!` alias's paths, `core.attributesFile`;
/// not a boolean, not a placeholder or an option), and so is what an include git does not open
/// today (`includeIf.onbranch:release`) would set, and what that file includes in turn.
/// (Review rev6: the alias was `!cd sub && ./run.sh`, listed as `run.sh` where git runs
/// `sub/run.sh`; such a value is now unplaced, in
/// `program_values_that_cannot_be_placed_are_unplaced_not_guessed`.)
#[test]
fn config_files_list_programs_and_what_a_false_include_would_set() {
    let d = repo();
    let p = &dunce::canonicalize(d.path()).unwrap();
    let mut c = std::fs::read_to_string(p.join(".git/config")).unwrap();
    c.push_str(
        "[core]\n\tfsmonitor = tools/fsmon.sh\n\tsshCommand = \"sh scripts/ssh.sh\" -v\n\
         \tattributesFile = conf/attrs\n\tpager = false\n\
         [filter \"strip\"]\n\tclean = tools/clean.sh %f --flag\n\
         [alias]\n\tx = !sh scripts/run.sh --out=build/log sub\n\tlg = log --oneline\n\
         [includeIf \"onbranch:release\"]\n\tpath = ../release.cfg\n",
    );
    std::fs::write(p.join(".git/config"), c).unwrap();
    std::fs::write(
        p.join("release.cfg"),
        "[core]\n\thooksPath = tools/relhooks\n\teditor = tools/ed.sh\n\
         [include]\n\tpath = nested.cfg\n",
    )
    .unwrap();
    std::fs::write(p.join("nested.cfg"), "[core]\n\taskPass = tools/ask.sh\n").unwrap();
    // A folder named alone is not a program (`cd sub`).
    std::fs::create_dir(p.join("sub")).unwrap();
    let c = git::config_files(p).unwrap();
    for want in [
        "tools/fsmon.sh",
        "scripts/ssh.sh",
        "conf/attrs",
        "tools/clean.sh",
        "scripts/run.sh",
        "build/log",
        "tools/ed.sh",
        "tools/ask.sh",
    ] {
        assert!(
            c.programs.contains(&p.join(want)),
            "{want}: {:?}",
            c.programs
        );
    }
    for not in ["false", "%f", "--flag", "log", "--oneline", "sub"] {
        assert!(
            !c.programs.contains(&p.join(not)),
            "{not}: {:?}",
            c.programs
        );
    }
    assert!(c.unplaced.is_empty(), "{:?}", c.unplaced);
    assert!(c.hooks.contains(&p.join("tools/relhooks")), "{:?}", c.hooks);
    // The live hooks directory is still git's own.
    assert_eq!(c.hooks.first(), Some(&p.join(".git/hooks")));
    for f in ["release.cfg", "nested.cfg"] {
        assert!(c.files.contains(&p.join(f)), "{f}: {:?}", c.files);
    }
}

/// Review G2: every repository inside a folder is found, the folder itself not being one: a
/// clone, a clone nested in it, a submodule's work tree (a `.git` file), a bare repository,
/// none inside a git directory, none through a symlink.
#[test]
fn repositories_inside_a_folder_are_found() {
    let w = tempfile::tempdir().unwrap();
    let w = &dunce::canonicalize(w.path()).unwrap();
    for r in ["app", "app/vendor/x"] {
        std::fs::create_dir_all(w.join(r)).unwrap();
        g(&w.join(r), &["init", "-q"]);
    }
    std::fs::create_dir_all(w.join("libs/wt")).unwrap();
    std::fs::write(w.join("libs/wt/.git"), "gitdir: ../../app/.git\n").unwrap();
    g(w, &["init", "-q", "--bare", "mirror.git"]);
    std::fs::create_dir_all(w.join("plain/deep")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(w.join("app"), w.join("plain/link")).unwrap();
    let found = git::repositories(w);
    assert!(found.complete);
    let mut dirs = found.dirs.clone();
    dirs.sort();
    let mut want = vec![
        w.join("app"),
        w.join("app/vendor/x"),
        w.join("libs/wt"),
        w.join("mirror.git"),
    ];
    want.sort();
    assert_eq!(dirs, want);
}

/// The reviewer's follow-up on `normal()`: where `.git` is a symlink (to `sub/realgit`), git
/// opens an include `../x.gitconfig` as `sub/x.gitconfig`, so that is the file listed, not the
/// `x.gitconfig` that `..` folded as text would give.
#[cfg(unix)]
#[test]
fn an_include_through_a_symlinked_git_directory_is_the_file_git_opens() {
    let d = repo();
    let p = &dunce::canonicalize(d.path()).unwrap();
    std::fs::create_dir(p.join("sub")).unwrap();
    std::fs::rename(p.join(".git"), p.join("sub/realgit")).unwrap();
    std::os::unix::fs::symlink("sub/realgit", p.join(".git")).unwrap();
    g(p, &["config", "include.path", "../x.gitconfig"]);
    std::fs::write(p.join("sub/x.gitconfig"), "[core]\n\thooksPath = h\n").unwrap();
    let c = git::config_files(p).unwrap();
    assert!(
        c.files.contains(&p.join("sub/x.gitconfig")),
        "{:?}",
        c.files
    );
    assert!(!c.files.contains(&p.join("x.gitconfig")), "{:?}", c.files);
}

/// Review rev6 (H6): the daemon's own git fetches nothing. In a partial clone
/// (`--filter=blob:none`, so no file's content is in it) whose remote's `uploadpack` names a
/// program, `show` of a file at HEAD needs a blob the clone left out: git would fetch it, and
/// run that program. It is not fetched (the file is reported missing), and `status` and
/// `config_files` run nothing either.
#[cfg(unix)]
#[test]
fn a_partial_clone_fetches_nothing_and_runs_no_transport() {
    let src = repo();
    g(src.path(), &["config", "uploadpack.allowFilter", "true"]);
    let outside = tempfile::tempdir().unwrap();
    let marker = outside.path().join("ran");
    // Records its run and fails (a transport that waited for input would hang the probe).
    let prog = outside.path().join("upload-pack.sh");
    std::fs::write(
        &prog,
        format!(
            "#!/bin/sh\necho \"$0 $*\" >> '{}'\nexit 1\n",
            marker.display()
        ),
    )
    .unwrap();
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&prog, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let w = tempfile::tempdir().unwrap();
    let dst = w.path().join("dst");
    let url = format!("file://{}", src.path().display());
    g(
        w.path(),
        &[
            "clone",
            "-q",
            "--filter=blob:none",
            "--no-checkout",
            &url,
            dst.to_str().unwrap(),
        ],
    );
    g(
        &dst,
        &[
            "config",
            "remote.origin.uploadpack",
            &prog.to_string_lossy(),
        ],
    );
    // The probe is live: plain git, asked for the same blob, runs the program.
    let plain = Command::new("git")
        .current_dir(&dst)
        .args(["cat-file", "-p", "HEAD:mod.txt"])
        .output()
        .unwrap();
    assert!(!plain.status.success());
    assert!(marker.exists(), "the probe's lazy fetch did not run");
    std::fs::remove_file(&marker).unwrap();

    let shown = git::show(&dst, "keep.txt", true);
    let st = git::status(&dst);
    let c = git::config_files(&dst);
    assert!(
        !marker.exists(),
        "the daemon's git fetched: {}",
        std::fs::read_to_string(&marker).unwrap_or_default()
    );
    assert!(
        matches!(shown, Err(git::GitError::NotFound(_))),
        "{shown:?}"
    );
    assert!(st.unwrap().repo);
    c.unwrap();
}

/// Review rev6 (G3's follow-up): a program value the splitter cannot place with confidence is
/// not guessed into words: its key is listed as unplaced (the gate then asks for every write).
/// Shell syntax (an open quote, `$`, `$(…)`, a backquote, a background `&`, a here-document,
/// a glob, braces, a function, a subshell), a command that changes its folder or nests a
/// script (`cd`, `git -C`, `sh -c '…'`, `git -c`), also when joined to others by `&&` or `;`,
/// and a `!`-less alias that gives git `-c` or `-C` (also under the `[alias "<name>"]` form). A plain value beside them is still placed
/// as its program alone, and a `!`-less alias with harmless options names nothing.
#[test]
fn program_values_that_cannot_be_placed_are_unplaced_not_guessed() {
    let d = repo();
    let p = &dunce::canonicalize(d.path()).unwrap();
    let unplaced = [
        ("core.fsmonitor", "tools/fs.sh \"open"),
        ("alias.dollar", "!$PWD/tools/c.sh"),
        (
            "alias.subst",
            "!tools/x.sh $(git rev-parse --show-toplevel)",
        ),
        ("alias.tick", "!tools/x.sh `pwd`"),
        ("alias.and", "!cd sub && ./run.sh"),
        ("alias.gitchdir", "!git -C sub status"),
        ("alias.chained", "!tools/a.sh; git -C sub status"),
        ("alias.background", "!tools/x.sh & tools/y.sh"),
        ("alias.both", "!tools/x.sh |& less"),
        ("alias.heredoc", "!cat <<EOF"),
        ("alias.glob", "!sh tools/*.sh"),
        ("alias.func", "!f() { git x; }; f"),
        ("alias.subshell", "!(tools/x.sh)"),
        ("alias.shc", "!sh -c 'cd tools && ./k.sh'"),
        ("alias.gitc", "!git -c core.fsmonitor=tools/b.sh status"),
        ("alias.config", "-c core.fsmonitor=tools/b.sh status"),
        ("alias.chdir", "-C sub status"),
        ("alias.x y.command", "--exec-path=tools status"),
    ];
    for (k, v) in unplaced {
        g(p, &["config", k, v]);
    }
    g(p, &["config", "core.pager", "tools/pager.sh -R"]);
    g(p, &["config", "alias.lg", "--no-pager log -p --oneline"]);
    let c = git::config_files(p).unwrap();
    let mut got = c.unplaced.clone();
    got.sort();
    let mut want: Vec<String> = unplaced.iter().map(|(k, _)| k.to_lowercase()).collect();
    want.sort();
    assert_eq!(got, want);
    assert!(
        c.programs.contains(&p.join("tools/pager.sh")),
        "{:?}",
        c.programs
    );
    // Nothing from an unplaced value is guessed.
    for not in [
        "run.sh",
        "tools/c.sh",
        "tools/b.sh",
        "tools/k.sh",
        "tools/a.sh",
        "tools/y.sh",
        "-R",
        "log",
    ] {
        assert!(
            !c.programs.contains(&p.join(not)),
            "{not}: {:?}",
            c.programs
        );
    }
}

/// Review rev6 (G3): the keys the review found that name programs are read: every key under
/// `[alias "<name>"]` with `!`, `gc.recentObjectsHook`, `imap.tunnel`, `sendemail.headerCmd`
/// and `sendemail.smtpServer`, `guitool.<name>.cmd`, `instaweb.httpd`. `gpg.program` is run
/// without a shell, so a space is part of its one path.
#[test]
fn the_program_keys_the_review_found_are_read() {
    let d = repo();
    let p = &dunce::canonicalize(d.path()).unwrap();
    for (k, v) in [
        ("alias.x y.command", "!tools/a.sh"),
        ("gc.recentObjectsHook", "tools/d.sh"),
        ("imap.tunnel", "tools/tunnel.sh --host x"),
        ("sendemail.headerCmd", "tools/hdr.sh"),
        ("sendemail.smtpServer", "tools/sendmail"),
        ("guitool.lint.cmd", "tools/lint.sh"),
        ("instaweb.httpd", "tools/httpd.sh"),
        ("gpg.program", "tools/my gpg"),
    ] {
        g(p, &["config", k, v]);
    }
    let c = git::config_files(p).unwrap();
    assert!(c.unplaced.is_empty(), "{:?}", c.unplaced);
    for want in [
        "tools/a.sh",
        "tools/d.sh",
        "tools/tunnel.sh",
        "tools/hdr.sh",
        "tools/sendmail",
        "tools/lint.sh",
        "tools/httpd.sh",
        "tools/my gpg",
    ] {
        assert!(
            c.programs.contains(&p.join(want)),
            "{want}: {:?}",
            c.programs
        );
    }
    assert!(
        !c.programs.contains(&p.join("tools/my")),
        "{:?}",
        c.programs
    );
}

/// Review rev6: simple commands joined by `;`, `&&`, `||` or `|` are read one by one, each
/// placed as a single command is: its program and every word that may be a path
/// (`diff-so-fancy | less`, `!git fetch && git rebase`). Nothing is unplaced.
#[test]
fn commands_joined_by_separators_are_placed_word_by_word() {
    let d = repo();
    let p = &dunce::canonicalize(d.path()).unwrap();
    std::fs::create_dir(p.join("sub")).unwrap();
    for (k, v) in [
        ("core.pager", "diff-so-fancy | less --tabs=4 -RFX"),
        ("alias.up", "!git fetch && git rebase"),
        ("alias.both", "!tools/a.sh; tools/b.sh --out=build/log"),
        ("alias.either", "!tools/c.sh||tools/d.sh sub"),
        ("alias.redirect", "!tools/e.sh > logs/e.txt 2>/dev/null"),
    ] {
        g(p, &["config", k, v]);
    }
    let c = git::config_files(p).unwrap();
    assert!(c.unplaced.is_empty(), "{:?}", c.unplaced);
    for want in [
        "diff-so-fancy",
        "less",
        "4",
        "git",
        "fetch",
        "rebase",
        "tools/a.sh",
        "tools/b.sh",
        "build/log",
        "tools/c.sh",
        "tools/d.sh",
        "tools/e.sh",
        "logs/e.txt",
    ] {
        assert!(
            c.programs.contains(&p.join(want)),
            "{want}: {:?}",
            c.programs
        );
    }
    for not in ["|", "&&", ";", "||", ">", "-RFX", "--tabs", "sub"] {
        assert!(
            !c.programs.contains(&p.join(not)),
            "{not}: {:?}",
            c.programs
        );
    }
}
