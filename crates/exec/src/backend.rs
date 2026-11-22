use axiom_protocol::task::Command;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;

pub struct SpawnSpec {
    pub command: Command,
    /// Already resolved by the caller.
    pub cwd: PathBuf,
    /// Added to the inherited environment. A key made by [`SpawnSpec::remove_key`] removes the
    /// inherited variable it names instead (agent-started processes do not get the daemon's
    /// secrets, AUD-010).
    pub env: BTreeMap<String, String>,
    pub label: Option<String>,
}

impl SpawnSpec {
    /// The prefix of an `env` key that removes a variable. NUL is in no variable name.
    pub const REMOVE_PREFIX: &'static str = "\0remove:";

    /// The `env` key that removes the inherited variable `name`.
    pub fn remove_key(name: &str) -> String {
        format!("{}{name}", Self::REMOVE_PREFIX)
    }
}

/// Isolation seam (ADR-0010): turns a spec into a ready-to-spawn command.
pub trait ExecBackend: Send + Sync {
    fn prepare(&self, spec: &SpawnSpec) -> tokio::process::Command;
}

/// Runs processes directly on the host.
#[derive(Debug, Default, Clone, Copy)]
pub struct HostBackend;

/// Name of the program a spec will run, for error messages.
pub(crate) fn program_name(command: &Command) -> String {
    match command {
        Command::Argv(v) => v.first().cloned().unwrap_or_default(),
        Command::Shell(_) => {
            if cfg!(windows) {
                "cmd".into()
            } else {
                "sh".into()
            }
        }
    }
}

impl ExecBackend for HostBackend {
    fn prepare(&self, spec: &SpawnSpec) -> tokio::process::Command {
        let mut cmd = match &spec.command {
            Command::Argv(argv) => {
                let mut c = tokio::process::Command::new(argv.first().map_or("", String::as_str));
                c.args(argv.iter().skip(1));
                c
            }
            Command::Shell(line) => {
                #[cfg(windows)]
                let mut c = tokio::process::Command::new("cmd");
                #[cfg(windows)]
                c.arg("/C").arg(line);
                #[cfg(not(windows))]
                let mut c = tokio::process::Command::new("sh");
                #[cfg(not(windows))]
                c.arg("-c").arg(line);
                c
            }
        };
        // Removals sort first (NUL), so a variable both removed and set is set.
        for (k, v) in &spec.env {
            match k.strip_prefix(SpawnSpec::REMOVE_PREFIX) {
                Some(name) => cmd.env_remove(name),
                None => cmd.env(k, v),
            };
        }
        cmd.current_dir(&spec.cwd)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        cmd.process_group(0);
        cmd
    }
}
