use axiom_protocol::task::Command;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::process::Stdio;

pub struct SpawnSpec {
    pub command: Command,
    /// Already resolved by the caller.
    pub cwd: PathBuf,
    /// Added to the inherited environment.
    pub env: BTreeMap<String, String>,
    pub label: Option<String>,
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
        cmd.current_dir(&spec.cwd)
            .envs(&spec.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(unix)]
        cmd.process_group(0);
        cmd
    }
}
