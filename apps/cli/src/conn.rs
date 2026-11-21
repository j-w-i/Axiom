//! Finding a daemon: the data dir's socket if a daemon listens there, else a daemon started
//! over stdio for this command (`--no-listen`: it lives only as long as the command).

use axiom_protocol::lifecycle::{
    ClientCapabilities, Initialize, InitializeParams, PeerInfo, Shutdown,
};
use axiom_rpc::Client;
use std::path::{Path, PathBuf};

pub struct Conn {
    pub client: Client,
    child: Option<tokio::process::Child>,
}

impl Conn {
    /// Ends the connection; a daemon this command started is shut down.
    pub async fn close(mut self) {
        if let Some(mut child) = self.child.take() {
            let _ = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                self.client
                    .request::<Shutdown>(axiom_protocol::lifecycle::Empty {}),
            )
            .await;
            let _ = tokio::time::timeout(std::time::Duration::from_secs(5), child.wait()).await;
        }
    }
}

/// `--data-dir`, else `AXIOM_DATA_DIR`, else the platform's (the same rule as the daemon).
pub fn data_dir(flag: Option<PathBuf>) -> PathBuf {
    if let Some(d) = flag {
        return d;
    }
    if let Some(d) = std::env::var_os("AXIOM_DATA_DIR") {
        return PathBuf::from(d);
    }
    directories::ProjectDirs::from("org", "axiom", "Axiom")
        .map(|d| d.data_dir().to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."))
}

fn params(token: Option<String>, approvals: bool) -> InitializeParams {
    InitializeParams {
        protocol_version: axiom_protocol::PROTOCOL_VERSION.into(),
        client: PeerInfo {
            name: "axiom-cli".into(),
            version: env!("CARGO_PKG_VERSION").into(),
        },
        auth_token: token,
        capabilities: Some(ClientCapabilities { approvals }),
    }
}

/// The daemon program: `AXIOM_DAEMON`, else next to this executable, else `axiomd` on PATH.
fn daemon_program() -> PathBuf {
    if let Some(p) = std::env::var_os("AXIOM_DAEMON") {
        return PathBuf::from(p);
    }
    let name = if cfg!(windows) {
        "axiomd.exe"
    } else {
        "axiomd"
    };
    if let Ok(exe) = std::env::current_exe() {
        if let Some(dir) = exe.parent() {
            let sibling = dir.join(name);
            if sibling.exists() {
                return sibling;
            }
        }
    }
    PathBuf::from(name)
}

pub async fn connect(data_dir: &Path, approvals: bool) -> Result<Conn, String> {
    #[cfg(unix)]
    {
        let run = data_dir.join("run");
        let socket = run.join("axiomd.sock");
        if let (true, Ok(token)) = (
            socket.exists(),
            std::fs::read_to_string(run.join("axiomd.token")),
        ) {
            if let Ok(stream) = tokio::net::UnixStream::connect(&socket).await {
                let (r, w) = stream.into_split();
                let client = Client::new(r, w);
                client
                    .request::<Initialize>(params(Some(token.trim().to_string()), approvals))
                    .await
                    .map_err(|e| format!("the daemon refused the connection: {}", e.message))?;
                return Ok(Conn {
                    client,
                    child: None,
                });
            }
        }
    }
    let mut child = tokio::process::Command::new(daemon_program())
        .arg("--no-listen")
        .env("AXIOM_DATA_DIR", data_dir)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| format!("cannot start axiomd: {e}"))?;
    let (Some(stdin), Some(stdout)) = (child.stdin.take(), child.stdout.take()) else {
        return Err("cannot talk to axiomd".into());
    };
    let client = Client::new(stdout, stdin);
    client
        .request::<Initialize>(params(None, approvals))
        .await
        .map_err(|e| format!("axiomd did not start: {}", e.message))?;
    Ok(Conn {
        client,
        child: Some(child),
    })
}
