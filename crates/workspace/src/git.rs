//! Git state of a workspace through the `git` program (ADR-0018 §5; M4-GIT-001): status (porcelain
//! v2, parsed), a file's text at HEAD or in the index, and a watch on the repository's own files.
//! Read-only. Every run is isolated: the inherited `GIT_*` variables are removed, no pager, no
//! hooks, no fsmonitor, `GIT_OPTIONAL_LOCKS=0` (status never takes the index lock), `LC_ALL=C`.
//! No program the repository's configuration or attributes name runs (reviews E2, G1):
//! `core.fsmonitor` is off; every filter driver git has configured is switched off for the run
//! (its `clean`, `smudge` and `process` empty, not `required`), so a `.gitattributes` cannot
//! route a file through one; `status` does not look inside submodules (`--ignore-submodules=dirty`:
//! that would run git there, under the submodule's own configuration); `show` reads with
//! `--no-ext-diff --no-textconv`. The rest (`rev-parse`, `config`) runs no program. Nothing is
//! fetched (review H6): a partial clone's missing object stays missing, so no transport program
//! runs. The person's own configuration (ignore files, `safe.directory`) still applies.

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

/// git in `root`, isolated from the daemon's own `GIT_*` variables, with no pager and no
/// fsmonitor (a program the repository's configuration names, run on reading the index), and
/// no network (review H6): in a partial clone (`--filter=blob:none`) a `show` or a `status`
/// that needs an object the clone left out would fetch it from the promisor remote, running
/// the transport the configuration names (`remote.<name>.uploadpack`, `core.sshCommand`, a
/// credential helper). `GIT_NO_LAZY_FETCH` (git 2.44) refuses that fetch; `protocol.allow =
/// never` (passed on to the `git fetch` an older git starts for it) refuses every transport
/// as well. The object is then missing, which `show` reports and `status` does not need.
fn isolated(git: &Path, root: &Path) -> Command {
    let mut cmd = Command::new(git);
    for (k, _) in std::env::vars_os() {
        if k.to_string_lossy().starts_with("GIT_") {
            cmd.env_remove(&k);
        }
    }
    cmd.current_dir(root)
        .env("GIT_OPTIONAL_LOCKS", "0")
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_NO_LAZY_FETCH", "1")
        .env("GIT_PAGER", "cat")
        .env("LC_ALL", "C")
        .args([
            "--no-pager",
            "-c",
            "core.fsmonitor=false",
            "-c",
            "protocol.allow=never",
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    cmd
}

fn command(git: &Path, root: &Path) -> Command {
    let mut cmd = isolated(git, root);
    cmd.args([
        "-c",
        "core.hooksPath=/dev/null",
        "-c",
        "core.quotePath=false",
        "-c",
        "status.relativePaths=false",
        "-c",
        "color.ui=false",
    ]);
    cmd
}

/// Runs git, bounded by [`GIT_TIMEOUT`]; stdout on success, the stderr text on failure.
fn run(git: &Path, root: &Path, args: &[&str]) -> Result<Result<Vec<u8>, String>, GitError> {
    run_command(command(git, root), args)
}

/// The variable `--config-env` reads an empty value from (a filter name with `=` in it cannot
/// be given to `-c`).
const EMPTY_VALUE_VAR: &str = "AXIOM_GIT_EMPTY";

/// [`command`] with every filter driver git has configured for `root` switched off (review G1):
/// `git status` re-reads a changed file through the `clean` or `process` filter its attributes
/// name, and `.gitattributes` is a file in the work tree. git is asked which filters it has
/// (`config --get-regexp '^filter\.'`, the same files and conditions the run reads); each gets
/// `clean`, `smudge` and `process` empty, which git takes as no filter, and `required=false`, so
/// a file is read as it is. A configuration git cannot read is an error, not a run.
fn guarded(git: &Path, root: &Path) -> Result<Command, GitError> {
    let names = match run_command(
        isolated(git, root),
        &["config", "-z", "--name-only", "--get-regexp", r"^filter\."],
    )? {
        Ok(out) => filter_names(&out).ok_or_else(|| {
            GitError::Failed("a filter's name in git's configuration is not UTF-8".into())
        })?,
        // No such key: exit 1, nothing said.
        Err(e) if e.is_empty() => Vec::new(),
        Err(e) => return Err(GitError::Failed(e)),
    };
    let mut cmd = command(git, root);
    cmd.env(EMPTY_VALUE_VAR, "");
    for name in names {
        for var in ["clean", "smudge", "process"] {
            let key = format!("filter.{name}.{var}");
            if name.contains('=') {
                // `-c` splits at the first `=`; `--config-env` (git 2.31) at the last. An older
                // git refuses the option, so the run fails rather than runs a filter.
                cmd.arg(format!("--config-env={key}={EMPTY_VALUE_VAR}"));
            } else {
                cmd.arg("-c").arg(format!("{key}="));
            }
        }
        let key = format!("filter.{name}.required");
        if name.contains('=') {
            cmd.arg(format!("--config-env={key}={EMPTY_VALUE_VAR}"));
        } else {
            cmd.arg("-c").arg(format!("{key}=false"));
        }
    }
    Ok(cmd)
}

/// The driver names in `git config -z --name-only --get-regexp '^filter\.'`: the subsection of
/// each `filter.<name>.<variable>` key (a name may hold dots). `None` for a name that is not
/// UTF-8 (it could not be switched off exactly).
fn filter_names(out: &[u8]) -> Option<Vec<String>> {
    let mut names: Vec<String> = Vec::new();
    for key in out.split(|b| *b == 0 || *b == b'\n') {
        let key = std::str::from_utf8(key).ok()?;
        let Some(rest) = key.strip_prefix("filter.") else {
            continue;
        };
        if let Some((name, _var)) = rest.rsplit_once('.') {
            if !names.iter().any(|n| n == name) {
                names.push(name.to_string());
            }
        }
    }
    Some(names)
}

/// [`run`] with a prepared command.
fn run_command(mut cmd: Command, args: &[&str]) -> Result<Result<Vec<u8>, String>, GitError> {
    let mut child = cmd.args(args).spawn()?;
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

/// What the configuration of the repository a folder is in can make git run, as git itself
/// reads it (reviews E2, G3, G4): what the agent gate protects beside `.git`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ConfigFiles {
    /// The hooks directories: first the one git uses (`core.hooksPath` from wherever it is set,
    /// an included file or the global configuration, else the git directory's own `hooks`),
    /// then every other `core.hooksPath` a configuration file sets, live or not (an include
    /// whose condition is false today). Empty when nothing sets one (outside a repository).
    pub hooks: Vec<PathBuf>,
    /// The repository's git directory and its common directory: not `.git` for a worktree, a
    /// submodule or a repository made with `--separate-git-dir` (a `.git` file points there).
    pub git_dirs: Vec<PathBuf>,
    /// Every file git reads configuration from, and every file an `include.path` or
    /// `includeIf.<condition>.path` names, whatever its condition, whether it exists yet or not
    /// (an agent could create it), followed through the includes of those that exist. Absolute,
    /// as git names them (not resolved through symlinks).
    pub files: Vec<PathBuf>,
    /// What the configuration names as a program git runs, or as attributes it reads
    /// ([`PROGRAM_KEYS`]): each word of the value that may be a path, from the top of the work
    /// tree (the git directory for a bare repository), whether it exists yet or not.
    pub programs: Vec<PathBuf>,
    /// The program-valued keys whose value could not be placed with confidence (review G3's
    /// follow-up): shell syntax the splitter does not read (`$`, backquotes, a background `&`,
    /// a subshell, braces or a function, a glob, a here-document, a quote left open; commands
    /// joined by `;`, `&&`, `||` or `|` are read), a command that changes its folder or nests a
    /// script (`cd`, `-C`, `-c`), or a `!`-less
    /// alias that hands git options which change its configuration or folder (`-c`, `-C`,
    /// `--exec-path`, …). Its words are not guessed: the caller treats the repository as one
    /// git cannot answer for, so every write asks.
    pub unplaced: Vec<String>,
}

/// The most configuration files read for one repository (its own, the global and system ones,
/// and what they include): more is an error, so the caller protects more, not less.
pub const MAX_CONFIG_FILES: usize = 64;

/// [`ConfigFiles`] for `root`, from git's own answers (`rev-parse --git-path hooks`,
/// `config --list --show-origin`, `config --file <f> --list` for each included file git did not
/// open): no configuration file is parsed here. Outside a repository, what `<root>/.git/config`
/// sets if there is one (a `.git` directory that is not a repository yet: `git init` keeps its
/// configuration); else nothing. An error when git is missing or does not answer as expected:
/// the caller then protects more, not less.
pub fn config_files(root: &Path) -> Result<ConfigFiles, GitError> {
    // git names paths its own way (on Windows `C:/x\\.git\\../shared.gitconfig`): each is
    // normalised, so it compares with the workspace's paths (the gate resolves the rest).
    let mut c = config_files_as_named(root)?;
    for p in c
        .hooks
        .iter_mut()
        .chain(c.git_dirs.iter_mut())
        .chain(c.files.iter_mut())
        .chain(c.programs.iter_mut())
    {
        *p = normal(p);
    }
    for v in [&mut c.hooks, &mut c.files, &mut c.programs] {
        let mut seen = Vec::new();
        v.retain(|p| {
            let new = !seen.contains(p);
            seen.push(p.clone());
            new
        });
    }
    Ok(c)
}

/// `path` as the disk resolves it: the longest part of it that exists, resolved through
/// symlinks (`.git/../x` is `sub/x` when `.git` links to `sub/realgit`, as git opens it), then
/// the rest with `.` dropped and `..` folded into its parent as text ([`fold`]).
fn normal(path: &Path) -> PathBuf {
    use std::path::Component;
    let parts: Vec<Component> = path.components().collect();
    for k in (1..=parts.len()).rev() {
        let head: PathBuf = parts[..k].iter().collect();
        if !head.is_absolute() {
            break;
        }
        let Ok(mut out) = dunce::canonicalize(&head) else {
            continue;
        };
        for c in &parts[k..] {
            match c {
                Component::CurDir => {}
                Component::ParentDir => {
                    out.pop();
                }
                other => out.push(other),
            }
        }
        return out;
    }
    fold(path)
}

/// `path` with its separators the platform's own, `.` dropped and `..` folded into its parent,
/// as text (a `..` at the root stays at the root).
fn fold(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    out.push(c);
                }
            }
            other => out.push(other),
        }
    }
    out
}

fn config_files_as_named(root: &Path) -> Result<ConfigFiles, GitError> {
    let git = find_git().ok_or(GitError::NoGit)?;
    let rev = run_command(
        isolated(&git, root),
        &[
            "rev-parse",
            "--path-format=absolute",
            "--git-dir",
            "--git-common-dir",
            "--git-path",
            "hooks",
            "--is-inside-work-tree",
        ],
    )?;
    match rev {
        Ok(out) => {
            let lines: Vec<&[u8]> = out.split(|b| *b == b'\n').collect();
            let (git_dir, common, hooks, inside) = match lines.as_slice() {
                [a, b, c, d, rest @ ..] if rest.iter().all(|l| l.is_empty()) => {
                    (path_of(a), path_of(b), path_of(c), *d == b"true")
                }
                _ => return Err(GitError::Failed("unexpected `git rev-parse` output".into())),
            };
            if ![&git_dir, &common, &hooks].iter().all(|p| p.is_absolute()) {
                // An older git without `--path-format`: not answered as asked.
                return Err(GitError::Failed("unexpected `git rev-parse` output".into()));
            }
            // A relative origin is relative to where git runs: the top of the work tree, or
            // the git directory. So are a relative `core.hooksPath` and a program's path.
            let base = if inside {
                let top = run_command(
                    isolated(&git, root),
                    &["rev-parse", "--path-format=absolute", "--show-toplevel"],
                )?
                .map_err(GitError::Failed)?;
                let top = path_of(top.strip_suffix(b"\n").unwrap_or(&top));
                if !top.is_absolute() {
                    return Err(GitError::Failed("unexpected `git rev-parse` output".into()));
                }
                top
            } else {
                git_dir.clone()
            };
            let list = run_command(
                isolated(&git, root),
                &["config", "--list", "--show-origin", "-z"],
            )?
            .map_err(GitError::Failed)?;
            let mut found = Found::default();
            found.read(&list, &base);
            found.includes_not_opened(&git, root, &base)?;
            let mut all_hooks = vec![hooks];
            all_hooks.extend(found.hooks);
            Ok(ConfigFiles {
                hooks: all_hooks,
                git_dirs: vec![git_dir, common],
                files: found.files,
                programs: found.programs,
                unplaced: found.unplaced,
            })
        }
        Err(e) if e.contains("not a git repository") => {
            let config = root.join(".git").join("config");
            if std::fs::symlink_metadata(&config).is_err() {
                return Ok(ConfigFiles::default());
            }
            let mut cmd = isolated(&git, root);
            cmd.args(["config", "--file"]).arg(&config);
            let list =
                run_command(cmd, &["--list", "--show-origin", "-z"])?.map_err(GitError::Failed)?;
            // Hooks and programs run from the top of the work tree `git init` would make here.
            let mut found = Found::default();
            found.read(&list, root);
            if !found.files.contains(&config) {
                found.files.push(config.clone());
            }
            found.opened.push(config);
            found.includes_not_opened(&git, root, root)?;
            Ok(ConfigFiles {
                hooks: found.hooks,
                git_dirs: Vec::new(),
                files: found.files,
                programs: found.programs,
                unplaced: found.unplaced,
            })
        }
        Err(e) => Err(GitError::Failed(e)),
    }
}

/// What configuration listings said, gathered.
#[derive(Default)]
struct Found {
    /// The files read (`--show-origin`).
    opened: Vec<PathBuf>,
    /// Those and every include target.
    files: Vec<PathBuf>,
    includes: Vec<PathBuf>,
    hooks: Vec<PathBuf>,
    programs: Vec<PathBuf>,
    unplaced: Vec<String>,
}

fn push(v: &mut Vec<PathBuf>, p: PathBuf) {
    if !v.contains(&p) {
        v.push(p);
    }
}

impl Found {
    /// From `git config --list --show-origin -z`: the files it read (relative ones from
    /// `base`), every file an include names (relative to the file that includes it), every
    /// `core.hooksPath` and every path a program-valued key names (relative to `base`), or the
    /// key when its value cannot be placed.
    fn read(&mut self, list: &[u8], base: &Path) {
        let mut parts = list.split(|b| *b == 0);
        while let (Some(origin), Some(entry)) = (parts.next(), parts.next()) {
            let Some(file) = origin.strip_prefix(b"file:") else {
                continue; // the command line, a blob, standard input
            };
            let file = base.join(path_of(file));
            push(&mut self.opened, file.clone());
            push(&mut self.files, file.clone());
            let (key, value) = match entry.iter().position(|b| *b == b'\n') {
                Some(i) => (&entry[..i], Some(&entry[i + 1..])),
                None => (entry, None),
            };
            let key = String::from_utf8_lossy(key).to_ascii_lowercase();
            let Some(value) = value else { continue };
            let include =
                key == "include.path" || (key.starts_with("includeif.") && key.ends_with(".path"));
            if include {
                let dir = file.parent().unwrap_or(base);
                if let Some(p) = expand_path(value, dir) {
                    push(&mut self.files, p.clone());
                    push(&mut self.includes, p);
                }
            } else if key == "core.hookspath" {
                if let Some(p) = expand_path(value, base) {
                    push(&mut self.hooks, p);
                }
            } else {
                let placed = match program_key(&key, value) {
                    ProgramValue::No => Some(Vec::new()),
                    ProgramValue::Shell => program_paths(value, base, true),
                    ProgramValue::Path => program_paths(value, base, false),
                    ProgramValue::Unplaced => None,
                };
                match placed {
                    Some(paths) => {
                        for p in paths {
                            push(&mut self.programs, p);
                        }
                    }
                    None => {
                        if !self.unplaced.contains(&key) {
                            self.unplaced.push(key);
                        }
                    }
                }
            }
        }
    }

    /// Reads each include target that exists and that git did not open (an `includeIf` whose
    /// condition is false today, a file included only from such a file) as git would once it
    /// does: `git config --file <target> --list --show-origin`, each key as it is, its own
    /// includes followed the same way (review G4).
    fn includes_not_opened(
        &mut self,
        git: &Path,
        root: &Path,
        base: &Path,
    ) -> Result<(), GitError> {
        let mut i = 0;
        while i < self.includes.len() {
            let target = self.includes[i].clone();
            i += 1;
            if self.opened.iter().any(|o| fold(o) == fold(&target)) || !target.is_file() {
                continue;
            }
            if self.opened.len() >= MAX_CONFIG_FILES {
                return Err(GitError::Failed(format!(
                    "more than {MAX_CONFIG_FILES} configuration files are included"
                )));
            }
            self.opened.push(target.clone());
            let mut cmd = isolated(git, root);
            cmd.args(["config", "--file"]).arg(&target);
            // One git cannot read is protected all the same (it is in `files`); it cannot make
            // git run anything until it is valid, and making it valid is a write that asks.
            if let Ok(list) = run_command(cmd, &["--list", "--show-origin", "-z"])? {
                self.read(&list, base);
            }
        }
        Ok(())
    }
}

/// Configuration keys whose value is a program git runs (or a file that routes files through
/// one), by section and variable, `*` for any subsection or variable (reviews G3, rev6):
/// `core.fsmonitor`, `core.sshCommand`, `core.editor`, `core.pager`, `core.askPass`,
/// `core.gitProxy`, `core.alternateRefsCommand`, `core.attributesFile`, `sequence.editor`,
/// `diff.external`, `diff.*.textconv`, `diff.*.command`, `filter.*.clean`, `filter.*.smudge`,
/// `filter.*.process`, `merge.*.driver`, `credential.helper`, `credential.*.helper`,
/// `gpg.program`, `gpg.*.program`, `gpg.ssh.defaultKeyCommand`, `pager.*`, `alias.*` and every
/// key under an `[alias "<name>"]` (`alias.*.command`; a `!` alias is a shell command, one
/// without `!` is git's own arguments), `interactive.diffFilter`, `difftool.*.cmd`,
/// `difftool.*.path`, `mergetool.*.cmd`, `mergetool.*.path`, `sendemail[.*].sendmailCmd`,
/// `toCmd`, `ccCmd`, `headerCmd` and `smtpServer`, `trailer.*.command`, `trailer.*.cmd`,
/// `tar.*.command`, `browser.*.cmd`, `browser.*.path`, `man.*.cmd`, `man.*.path`,
/// `guitool.*.cmd`, `instaweb.httpd`, `imap.tunnel`, `gc.recentObjectsHook`,
/// `remote.*.uploadpack`, `remote.*.receivepack`, `submodule.*.update` (a `!` command),
/// `uploadpack.packObjectsHook`.
///
/// **This list is not a boundary.** git adds program-valued keys in new versions (hooks set
/// in configuration, for one), a protected program may run files nothing names, and the
/// person's own tools run files in the work tree too. Content that something on the person's
/// machine executes later is closed only by isolating the agent's processes (ADR-0010) and by
/// the policy that asks for writes; this list is defence in depth. Do not extend it key by key:
/// a value that cannot be placed already makes every write ask ([`ConfigFiles::unplaced`]).
pub const PROGRAM_KEYS: &[(&str, Option<&str>, &str)] = &[
    ("core", None, "fsmonitor"),
    ("core", None, "sshcommand"),
    ("core", None, "editor"),
    ("core", None, "pager"),
    ("core", None, "askpass"),
    ("core", None, "gitproxy"),
    ("core", None, "alternaterefscommand"),
    ("core", None, "attributesfile"),
    ("sequence", None, "editor"),
    ("diff", None, "external"),
    ("diff", Some("*"), "textconv"),
    ("diff", Some("*"), "command"),
    ("filter", Some("*"), "clean"),
    ("filter", Some("*"), "smudge"),
    ("filter", Some("*"), "process"),
    ("merge", Some("*"), "driver"),
    ("credential", None, "helper"),
    ("credential", Some("*"), "helper"),
    ("gpg", None, "program"),
    ("gpg", Some("*"), "program"),
    ("gpg", Some("ssh"), "defaultkeycommand"),
    ("pager", None, "*"),
    ("alias", None, "*"),
    ("alias", Some("*"), "*"),
    ("interactive", None, "difffilter"),
    ("difftool", Some("*"), "cmd"),
    ("difftool", Some("*"), "path"),
    ("mergetool", Some("*"), "cmd"),
    ("mergetool", Some("*"), "path"),
    ("sendemail", None, "sendmailcmd"),
    ("sendemail", None, "tocmd"),
    ("sendemail", None, "cccmd"),
    ("sendemail", None, "headercmd"),
    ("sendemail", None, "smtpserver"),
    ("sendemail", Some("*"), "sendmailcmd"),
    ("sendemail", Some("*"), "tocmd"),
    ("sendemail", Some("*"), "cccmd"),
    ("sendemail", Some("*"), "headercmd"),
    ("sendemail", Some("*"), "smtpserver"),
    ("trailer", Some("*"), "command"),
    ("trailer", Some("*"), "cmd"),
    ("tar", Some("*"), "command"),
    ("browser", Some("*"), "cmd"),
    ("browser", Some("*"), "path"),
    ("man", Some("*"), "cmd"),
    ("man", Some("*"), "path"),
    ("guitool", Some("*"), "cmd"),
    ("instaweb", None, "httpd"),
    ("imap", None, "tunnel"),
    ("gc", None, "recentobjectshook"),
    ("remote", Some("*"), "uploadpack"),
    ("remote", Some("*"), "receivepack"),
    ("submodule", Some("*"), "update"),
    ("uploadpack", None, "packobjectshook"),
];

/// How a configuration value names a program ([`program_key`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProgramValue {
    /// None (not a program-valued key, a boolean, a `!`-less alias with plain arguments).
    No,
    /// A command git may hand to a shell.
    Shell,
    /// One path git runs or reads as it is, without a shell (`core.attributesFile`,
    /// `gpg.program`, `sendemail.smtpServer`).
    Path,
    /// A value that cannot be placed: a `!`-less alias that gives git options changing its
    /// configuration or folder.
    Unplaced,
}

/// git's options before an alias's command that change neither its configuration nor where it
/// runs or finds programs: any other (`-c`, `--config-env`, `-C`, `--exec-path`, `--git-dir`,
/// `--work-tree`, …) makes a `!`-less alias's value unplaced.
const PLAIN_GIT_OPTIONS: &[&str] = &[
    "-p",
    "--paginate",
    "-P",
    "--no-pager",
    "--no-replace-objects",
    "--no-lazy-fetch",
    "--no-optional-locks",
    "--no-advice",
    "--literal-pathspecs",
    "--glob-pathspecs",
    "--noglob-pathspecs",
    "--icase-pathspecs",
];

/// Whether the (lower-cased) `key` names a program ([`PROGRAM_KEYS`]) with this value, and how.
/// A boolean (`core.fsmonitor = true` is git's own daemon, `pager.log = false`) names none; an
/// alias or a submodule's `update` is a shell command only with `!`. An alias without `!` is
/// git's own arguments: unplaced when an option before its command can change git's
/// configuration or folder (`-c core.fsmonitor=tools/x.sh status`), else no program.
fn program_key(key: &str, value: &[u8]) -> ProgramValue {
    let Some((section, rest)) = key.split_once('.') else {
        return ProgramValue::No;
    };
    let (sub, var) = match rest.rsplit_once('.') {
        Some((s, v)) => (Some(s), v),
        None => (None, rest),
    };
    let listed = PROGRAM_KEYS.iter().any(|(s, sb, v)| {
        *s == section
            && (*v == "*" || *v == var)
            && match (sb, sub) {
                (None, None) => true,
                (Some("*"), Some(_)) => true,
                (Some(x), Some(y)) => *x == y,
                _ => false,
            }
    });
    if !listed {
        return ProgramValue::No;
    }
    let text = String::from_utf8_lossy(value);
    let t = text.trim().to_ascii_lowercase();
    if matches!(
        t.as_str(),
        "" | "true" | "false" | "yes" | "no" | "on" | "off" | "1" | "0"
    ) {
        return ProgramValue::No;
    }
    let bang = text.trim_start().starts_with('!');
    if section == "alias" && !bang {
        // `alias.<name>` or `alias.<name>.command`: git's arguments, split by git itself.
        if sub.is_some() && var != "command" {
            return ProgramValue::No;
        }
        let Some(words) = git_words(&text) else {
            return ProgramValue::Unplaced;
        };
        let changes_git = words
            .iter()
            .take_while(|w| w.starts_with('-'))
            .any(|w| !PLAIN_GIT_OPTIONS.contains(&w.as_str()));
        return if changes_git {
            ProgramValue::Unplaced
        } else {
            ProgramValue::No
        };
    }
    if section == "submodule" && !bang {
        return ProgramValue::No;
    }
    let one_path = (section == "core" && var == "attributesfile")
        || (section == "gpg" && var == "program")
        || (section == "sendemail" && var == "smtpserver");
    if one_path {
        ProgramValue::Path
    } else {
        ProgramValue::Shell
    }
}

/// The paths a program-valued configuration value may name, from `base`; `None` when a shell
/// value cannot be placed with confidence ([`shell_words`], and a word that changes the folder
/// or hands a script to a shell: `cd`, `pushd`, `popd`, `-C`, `-c`). Its words (each that is
/// not an option or a placeholder such as `%f`): a word with a path separator, or a bare word
/// that is not a folder at `base` (`sh clean.sh` runs `base/clean.sh`; `less` is harmless to
/// protect). The part after `=` of an option or an assignment. Not `shell`: the value is one
/// path.
fn program_paths(value: &[u8], base: &Path, shell: bool) -> Option<Vec<PathBuf>> {
    let text = String::from_utf8_lossy(value);
    let text = text.trim().trim_start_matches('!');
    let words: Vec<String> = if shell {
        shell_words(text)?
    } else {
        vec![text.to_string()]
    };
    let mut out = Vec::new();
    for w in words {
        if shell && moves_or_nests(&w) {
            return None;
        }
        // `--file=x` and `VAR=x`: the value.
        let w = match w.split_once('=') {
            Some((before, after))
                if shell
                    && (before.starts_with('-')
                        || before
                            .chars()
                            .all(|c| c.is_ascii_alphanumeric() || c == '_')) =>
            {
                after.to_string()
            }
            _ => w,
        };
        if w.is_empty() || w.starts_with(['-', '%']) {
            continue;
        }
        // `~user/x`: someone's home, which this cannot place.
        if shell && w.starts_with('~') && !w.starts_with("~/") {
            return None;
        }
        let separator = w.contains('/') || (cfg!(windows) && w.contains('\\'));
        let Some(p) = expand_path(w.as_bytes(), base) else {
            continue;
        };
        if !separator && p.is_dir() {
            continue;
        }
        out.push(p);
    }
    Some(out)
}

/// A shell word after which the words that follow are not run from where they would seem to
/// be: the folder changes (`cd`, `pushd`, `popd`, `-C` as in `git -C` and `make -C`,
/// `--chdir`), or a string is a script for a shell or git's configuration (`-c` and a bundle
/// ending in it, `sh -ec '…'`; `git -c key=program`).
fn moves_or_nests(w: &str) -> bool {
    matches!(w, "cd" | "pushd" | "popd" | "-C")
        || w.starts_with("--chdir")
        || (w.len() >= 2
            && w.starts_with('-')
            && !w.starts_with("--")
            && w.ends_with('c')
            && w[1..].chars().all(|c| c.is_ascii_alphabetic()))
}

/// `text` split into words as `sh` does for simple commands, the words of every command in
/// one list (each is placed alike): blanks separate, quotes group (and are dropped), a
/// backslash takes the next character as it is (inside double quotes, only before `$`, a
/// backquote, `"` or `\`), `<` and `>` separate (a redirection's target is a word, placed as
/// a path), and so do the operators between simple commands, `;`, `&&`, `||` and `|`
/// (`diff-so-fancy | less`, `git fetch && git rebase`). `None` for what this does not read, so
/// cannot place: an expansion (`$`, a backquote, outside single quotes), a background `&` (and
/// `>&`, `|&`), a newline, a subshell, braces or a function (`(`, `)`, `{`, `}`), a glob (`*`,
/// `?`, `[`), a here-document or process substitution (`<<`, `<(`, `>(`), a comment (`#`
/// starting a word), or a quote left open.
fn shell_words(text: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = text.chars().peekable();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some('\''), '\'') => quote = None,
            (Some('\''), c) => cur.push(c),
            (Some(_), '"') => quote = None,
            (Some(_), '$' | '`') => return None,
            (Some(_), '\\') => match chars.next()? {
                n @ ('$' | '`' | '"' | '\\') => cur.push(n),
                '\n' => {}
                n => {
                    cur.push('\\');
                    cur.push(n);
                }
            },
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => quote = Some(c),
            (None, '\\') => match chars.next()? {
                '\n' => {}
                n => cur.push(n),
            },
            (None, '<' | '>') => {
                if matches!(chars.peek(), Some('<' | '(')) {
                    return None;
                }
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
            }
            (None, '#') if cur.is_empty() => return None,
            // Between simple commands: `;`, `|`, `||`, `&&` (a lone `&` is a background job).
            (None, ';' | '|' | '&') => {
                if c == '&' && chars.next_if_eq(&'&').is_none() {
                    return None;
                }
                if c == '|' {
                    chars.next_if_eq(&'|');
                }
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
            }
            (None, c) if "$`(){}*?[\n".contains(c) => return None,
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if quote.is_some() {
        return None;
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    Some(words)
}

/// A `!`-less alias's value split as git splits it (`split_cmdline`: blanks separate, quotes
/// group, a backslash takes the next character); `None` for a quote left open.
fn git_words(text: &str) -> Option<Vec<String>> {
    let mut words = Vec::new();
    let mut cur = String::new();
    let mut quote: Option<char> = None;
    let mut chars = text.chars();
    while let Some(c) = chars.next() {
        match (quote, c) {
            (Some(q), c) if c == q => quote = None,
            (_, '\\') => cur.push(chars.next()?),
            (Some(_), c) => cur.push(c),
            (None, '\'' | '"') => quote = Some(c),
            (None, c) if c.is_whitespace() => {
                if !cur.is_empty() {
                    words.push(std::mem::take(&mut cur));
                }
            }
            (None, c) => cur.push(c),
        }
    }
    if quote.is_some() {
        return None;
    }
    if !cur.is_empty() {
        words.push(cur);
    }
    Some(words)
}

/// The most folders [`repositories`] looks at, and how deep: past either, what it found is
/// incomplete and the caller protects by name as well.
pub const MAX_WALK_DIRS: usize = 100_000;
pub const MAX_WALK_DEPTH: usize = 32;
/// The most repositories [`repositories`] lists (each is asked of git).
pub const MAX_REPOSITORIES: usize = 64;

/// What [`repositories`] found.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Repositories {
    pub dirs: Vec<PathBuf>,
    /// False when a limit was reached or a folder could not be read.
    pub complete: bool,
}

/// The git repositories inside `root` (review G2): each folder that holds a `.git` (a directory,
/// or a file pointing elsewhere: a submodule, a worktree), and each folder that is itself a
/// git directory (a `HEAD` with `objects/` and `refs/`, or with a `commondir`), `root` itself
/// included. Git directories are not looked into; symlinks are not followed (what they point
/// to inside the workspace is reached by its own path). Bounded by [`MAX_WALK_DIRS`],
/// [`MAX_WALK_DEPTH`] and [`MAX_REPOSITORIES`]. Blocking.
pub fn repositories(root: &Path) -> Repositories {
    let mut out = Repositories {
        dirs: Vec::new(),
        complete: true,
    };
    let mut queue = std::collections::VecDeque::from([(root.to_path_buf(), 0usize)]);
    let mut seen = 0usize;
    while let Some((dir, depth)) = queue.pop_front() {
        seen += 1;
        if seen > MAX_WALK_DIRS {
            out.complete = false;
            break;
        }
        let Ok(entries) = std::fs::read_dir(&dir) else {
            out.complete = false;
            continue;
        };
        let mut subdirs = Vec::new();
        let (mut dot_git, mut head, mut objects, mut refs, mut commondir) =
            (false, false, false, false, false);
        for e in entries {
            let Ok(e) = e else {
                out.complete = false;
                continue;
            };
            let Ok(ft) = e.file_type() else {
                out.complete = false;
                continue;
            };
            let name = e.file_name();
            let name = name.to_string_lossy();
            // Without regard to case: a case-insensitive disk opens `HEAD` as `head`.
            let is = |n: &str| name.eq_ignore_ascii_case(n);
            if is(".git") {
                dot_git = true;
            } else if is("HEAD") {
                head = !ft.is_dir();
            } else if is("objects") {
                objects = ft.is_dir();
            } else if is("refs") {
                refs = ft.is_dir();
            } else if is("commondir") {
                commondir = !ft.is_dir();
            }
            if ft.is_dir() && !name.eq_ignore_ascii_case(".git") {
                subdirs.push(e.path());
            }
        }
        let git_dir = head && ((objects && refs) || commondir);
        if dot_git || git_dir {
            if out.dirs.len() >= MAX_REPOSITORIES {
                out.complete = false;
                break;
            }
            out.dirs.push(dir.clone());
        }
        if git_dir {
            continue;
        }
        if depth >= MAX_WALK_DEPTH {
            if !subdirs.is_empty() {
                out.complete = false;
            }
            continue;
        }
        queue.extend(subdirs.into_iter().map(|d| (d, depth + 1)));
    }
    out
}

/// A path value from git's configuration: `~/` is the home directory, a relative path is from
/// `dir`. `None` for what cannot be placed here (`~user/`, `%(prefix)/`, empty).
fn expand_path(value: &[u8], dir: &Path) -> Option<PathBuf> {
    if value.is_empty() || value.starts_with(b"%(") {
        return None;
    }
    if let Some(rest) = value.strip_prefix(b"~/") {
        let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
        return Some(PathBuf::from(home).join(path_of(rest)));
    }
    if value.starts_with(b"~") {
        return None;
    }
    Some(dir.join(path_of(value)))
}

/// A path from git's output, byte for byte where the platform allows.
fn path_of(bytes: &[u8]) -> PathBuf {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        PathBuf::from(std::ffi::OsStr::from_bytes(bytes))
    }
    #[cfg(not(unix))]
    {
        PathBuf::from(String::from_utf8_lossy(bytes).into_owned())
    }
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
    let out = run_command(
        guarded(&repo.git, root)?,
        &[
            "status",
            "--porcelain=v2",
            "-z",
            "--branch",
            "--untracked-files=all",
            // Not git in each submodule (its own configuration and filters): a submodule shows
            // as changed when its commit does, not for edits inside it.
            "--ignore-submodules=dirty",
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
    let bytes = run_command(
        guarded(&repo.git, root)?,
        &["show", "--no-ext-diff", "--no-textconv", &spec],
    )?
    .map_err(|e| {
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

    /// git's paths compare with the workspace's once normalised (Windows CI on 000d4e8: an
    /// include named `C:/…\.git\../shared.gitconfig` was not found under the workspace).
    #[test]
    fn gits_paths_are_normalised_as_text() {
        #[cfg(unix)]
        assert_eq!(
            normal(Path::new("/w/./a/.git/../shared.gitconfig")),
            PathBuf::from("/w/a/shared.gitconfig")
        );
        #[cfg(windows)]
        assert_eq!(
            normal(Path::new(r"C:/Users/x/ws\.git\../shared.gitconfig")),
            PathBuf::from(r"C:\Users\x\ws\shared.gitconfig")
        );
    }

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
