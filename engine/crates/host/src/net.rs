//! The local listener: a Unix socket in a private directory, or a Windows named pipe.

use std::io;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::paths::Paths;

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
pub type Boxed = Box<dyn Stream>;

#[cfg(unix)]
pub struct Listener {
    sock: tokio::net::UnixListener,
    uid: u32,
    path: String,
}

#[cfg(unix)]
impl Listener {
    /// The caller holds the data directory's lock, so an existing socket file is stale.
    pub fn bind(paths: &Paths) -> io::Result<Self> {
        use std::os::unix::fs::{MetadataExt, PermissionsExt};
        let path = paths.endpoint()?;
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let sock = tokio::net::UnixListener::bind(&path)?;
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600))?;
        let uid = std::fs::metadata(paths.run())?.uid();
        Ok(Self { sock, uid, path })
    }

    pub async fn accept(&mut self) -> io::Result<Boxed> {
        loop {
            let (s, _) = self.sock.accept().await?;
            match s.peer_cred() {
                Ok(c) if c.uid() == self.uid => return Ok(Box::new(s)),
                _ => drop(s),
            }
        }
    }
}

#[cfg(unix)]
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(unix)]
pub async fn connect(paths: &Paths) -> io::Result<Boxed> {
    Ok(Box::new(
        tokio::net::UnixStream::connect(paths.endpoint()?).await?,
    ))
}

#[cfg(windows)]
pub struct Listener {
    next: tokio::net::windows::named_pipe::NamedPipeServer,
    name: String,
}

#[cfg(windows)]
impl Listener {
    pub fn bind(paths: &Paths) -> io::Result<Self> {
        let name = paths.endpoint()?;
        // `first_pipe_instance` fails if another process already owns the name.
        let next = tokio::net::windows::named_pipe::ServerOptions::new()
            .first_pipe_instance(true)
            .reject_remote_clients(true)
            .create(&name)?;
        Ok(Self { next, name })
    }

    pub async fn accept(&mut self) -> io::Result<Boxed> {
        self.next.connect().await?;
        let fresh = tokio::net::windows::named_pipe::ServerOptions::new()
            .reject_remote_clients(true)
            .create(&self.name)?;
        Ok(Box::new(std::mem::replace(&mut self.next, fresh)))
    }
}

#[cfg(windows)]
pub async fn connect(paths: &Paths) -> io::Result<Boxed> {
    let name = paths.endpoint()?;
    loop {
        match tokio::net::windows::named_pipe::ClientOptions::new().open(&name) {
            Ok(c) => return Ok(Box::new(c)),
            // ERROR_PIPE_BUSY: every instance is taken for the moment.
            Err(e) if e.raw_os_error() == Some(231) => {
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
            }
            Err(e) => return Err(e),
        }
    }
}
