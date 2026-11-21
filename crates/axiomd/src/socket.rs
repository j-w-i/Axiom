//! The local socket for more clients (ADR-0015 §6; M3-DAEMON-001): `<data dir>/run/axiomd.sock`
//! (mode 0600) in `<data dir>/run/` (mode 0700), with the token in `run/axiomd.token` (0600),
//! regenerated at every start. A connection must send the token in `initialize.authToken`.
//! Only one daemon per data dir listens: a live socket is left to its owner. Unix only; on
//! Windows (deferred) every client starts its own daemon over stdio.

use std::path::{Path, PathBuf};

pub const SOCKET: &str = "axiomd.sock";
pub const TOKEN: &str = "axiomd.token";

pub fn run_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("run")
}

#[cfg(unix)]
pub use imp::Listening;

#[cfg(unix)]
mod imp {
    use super::*;
    use axiom_rpc::{serve_with, Router, ServeOptions, ServerInfo};
    use std::io::Read;
    use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
    use std::sync::Arc;

    /// The listener; dropping it removes the socket and token files.
    pub struct Listening {
        socket: PathBuf,
        token: PathBuf,
        task: tokio::task::JoinHandle<()>,
    }

    impl Drop for Listening {
        fn drop(&mut self) {
            self.task.abort();
            let _ = std::fs::remove_file(&self.socket);
            let _ = std::fs::remove_file(&self.token);
        }
    }

    fn random_token() -> std::io::Result<String> {
        let mut b = [0u8; 32];
        std::fs::File::open("/dev/urandom")?.read_exact(&mut b)?;
        Ok(b.iter().map(|x| format!("{x:02x}")).collect())
    }

    /// Starts listening unless another live daemon owns the socket (`Ok(None)`).
    pub async fn listen(
        data_dir: &Path,
        router: Arc<Router>,
        info: ServerInfo,
    ) -> std::io::Result<Option<Listening>> {
        let dir = run_dir(data_dir);
        std::fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&dir)?;
        std::fs::set_permissions(&dir, std::fs::Permissions::from_mode(0o700))?;
        let socket = dir.join(SOCKET);
        if socket.exists() {
            if tokio::net::UnixStream::connect(&socket).await.is_ok() {
                return Ok(None);
            }
            std::fs::remove_file(&socket)?;
        }
        let token = random_token()?;
        let token_path = dir.join(TOKEN);
        let _ = std::fs::remove_file(&token_path);
        {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&token_path)?;
            f.write_all(token.as_bytes())?;
        }
        let listener = tokio::net::UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        let task = tokio::spawn(async move {
            loop {
                let Ok((stream, _)) = listener.accept().await else {
                    continue;
                };
                let (r, w) = stream.into_split();
                let router = router.clone();
                let info = info.clone();
                let opts = ServeOptions {
                    token: Some(token.clone()),
                    run_hooks: false,
                };
                tokio::spawn(async move {
                    if let Err(e) = serve_with(r, w, router, info, opts).await {
                        tracing::info!("a socket client ended with an error: {e}");
                    }
                });
            }
        });
        Ok(Some(Listening {
            socket,
            token: token_path,
            task,
        }))
    }
}

#[cfg(unix)]
pub use imp::listen;
