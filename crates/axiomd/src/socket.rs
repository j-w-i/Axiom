//! The local socket for more clients (ADR-0015 §6; M3-DAEMON-001): `<data dir>/run/axiomd.sock`
//! (mode 0600) in `<data dir>/run/` (mode 0700), with the token in `run/axiomd.token` (0600),
//! regenerated at every start. A connection must send the token in `initialize.authToken`.
//! Only one daemon per data dir listens: the one holding the exclusive lock on `run/axiomd.lock`
//! for its lifetime (two daemons starting at once cannot both get past it, AUD-095); a live
//! socket is also left to its owner. The token is written after the bind, to a temporary file
//! renamed into place, so it always matches the listener. Unix only; on Windows (deferred)
//! every client starts its own daemon over stdio.

use std::path::{Path, PathBuf};

pub const SOCKET: &str = "axiomd.sock";
pub const TOKEN: &str = "axiomd.token";
pub const LOCK: &str = "axiomd.lock";

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

    /// The listener; dropping it removes the socket and token files and releases the lock.
    pub struct Listening {
        socket: PathBuf,
        token: PathBuf,
        task: tokio::task::JoinHandle<()>,
        /// Holds the data dir's listener lock while open.
        _lock: std::fs::File,
    }

    /// Backoff after a failed `accept` (EMFILE, ENFILE: no descriptor until one is freed), so
    /// the loop does not spin (AUD-094).
    pub(crate) fn accept_backoff(failures: u32) -> std::time::Duration {
        std::time::Duration::from_millis((50u64 << failures.min(5)).min(1000))
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
        // One listener per data dir: whoever holds the lock (released when the process ends,
        // however it ends).
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .mode(0o600)
            .open(dir.join(LOCK))?;
        match rustix::fs::flock(&lock, rustix::fs::FlockOperation::NonBlockingLockExclusive) {
            Ok(()) => {}
            Err(rustix::io::Errno::WOULDBLOCK) => return Ok(None),
            Err(e) => return Err(e.into()),
        }
        let socket = dir.join(SOCKET);
        if socket.exists() {
            // A daemon from before the lock may still own it.
            if tokio::net::UnixStream::connect(&socket).await.is_ok() {
                return Ok(None);
            }
            std::fs::remove_file(&socket)?;
        }
        let listener = tokio::net::UnixListener::bind(&socket)?;
        std::fs::set_permissions(&socket, std::fs::Permissions::from_mode(0o600))?;
        let token = random_token()?;
        let token_path = dir.join(TOKEN);
        let written = (|| -> std::io::Result<()> {
            use std::io::Write;
            let tmp = dir.join(format!("{TOKEN}.{}", std::process::id()));
            let _ = std::fs::remove_file(&tmp);
            let mut f = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)?;
            f.write_all(token.as_bytes())?;
            f.sync_all()?;
            std::fs::rename(&tmp, &token_path)
        })();
        if let Err(e) = written {
            let _ = std::fs::remove_file(&socket);
            return Err(e);
        }
        let task = tokio::spawn(async move {
            let mut failures = 0u32;
            loop {
                let stream = match listener.accept().await {
                    Ok((stream, _)) => {
                        failures = 0;
                        stream
                    }
                    Err(e) => {
                        if failures == 0 {
                            tracing::warn!("the local socket cannot accept: {e}");
                        }
                        tokio::time::sleep(accept_backoff(failures)).await;
                        failures = failures.saturating_add(1);
                        continue;
                    }
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
            _lock: lock,
        }))
    }
}

#[cfg(unix)]
pub use imp::listen;

#[cfg(all(test, unix))]
mod tests {
    use super::imp::{accept_backoff, listen};
    use axiom_rpc::{Router, ServerInfo};
    use std::sync::Arc;

    #[test]
    fn accept_failures_back_off_up_to_a_second() {
        assert_eq!(accept_backoff(0).as_millis(), 50);
        assert_eq!(accept_backoff(1).as_millis(), 100);
        assert_eq!(accept_backoff(4).as_millis(), 800);
        assert_eq!(accept_backoff(5).as_millis(), 1000);
        assert_eq!(accept_backoff(u32::MAX).as_millis(), 1000);
    }

    /// AUD-095: two daemons starting at once on one data dir: exactly one listens, and the
    /// token file is the listener's.
    #[tokio::test]
    async fn concurrent_starts_make_one_listener_with_its_token() {
        let d = tempfile::tempdir().unwrap();
        let info = ServerInfo {
            name: "t".into(),
            version: "0".into(),
        };
        let r = Arc::new(Router::new());
        let (a, b) = tokio::join!(
            listen(d.path(), r.clone(), info.clone()),
            listen(d.path(), r.clone(), info.clone())
        );
        let (a, b) = (a.unwrap(), b.unwrap());
        assert!(a.is_some() != b.is_some(), "exactly one listens");
        let token = std::fs::read_to_string(super::run_dir(d.path()).join(super::TOKEN)).unwrap();
        assert_eq!(token.len(), 64);
        let stream = tokio::net::UnixStream::connect(super::run_dir(d.path()).join(super::SOCKET))
            .await
            .unwrap();
        let (rd, wr) = stream.into_split();
        let c = axiom_rpc::Client::new(rd, wr);
        let init = axiom_protocol::lifecycle::InitializeParams {
            protocol_version: "0.6".into(),
            client: axiom_protocol::lifecycle::PeerInfo {
                name: "t".into(),
                version: "0".into(),
            },
            auth_token: Some(token),
            capabilities: None,
        };
        c.request::<axiom_protocol::lifecycle::Initialize>(init)
            .await
            .expect("the token file matches the listener");
        // Once the listener is gone, the next start listens.
        drop((a, b));
        let again = listen(d.path(), r, info).await.unwrap();
        assert!(again.is_some());
    }
}
