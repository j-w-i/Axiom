//! The local listener: a Unix socket in a private directory, on every system.

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
    Ok(Box::new(tokio::net::UnixStream::connect(paths.endpoint()?).await?))
}

#[cfg(windows)]
pub struct Listener {
    rx: tokio::sync::mpsc::Receiver<Boxed>,
    path: String,
}

/// Windows has Unix sockets but no way to wait for them with tokio's sockets, so each connection
/// gets two threads that carry its bytes. There are a few clients, not thousands. The socket lies
/// in the user's own application data, which other users cannot enter; the token does the rest.
#[cfg(windows)]
impl Listener {
    pub fn bind(paths: &Paths) -> io::Result<Self> {
        let path = paths.endpoint()?;
        match std::fs::remove_file(&path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => return Err(e),
            _ => {}
        }
        let sock = uds_windows::UnixListener::bind(&path)?;
        let (tx, rx) = tokio::sync::mpsc::channel(16);
        let rt = tokio::runtime::Handle::current();
        std::thread::Builder::new().name("accept".into()).spawn(move || {
            while let Ok((s, _)) = sock.accept() {
                if let Ok(b) = carry(s, &rt)
                    && tx.blocking_send(b).is_err()
                {
                    break;
                }
            }
        })?;
        Ok(Self { rx, path })
    }

    pub async fn accept(&mut self) -> io::Result<Boxed> {
        self.rx.recv().await.ok_or_else(|| io::Error::other("the listener has stopped"))
    }
}

#[cfg(windows)]
impl Drop for Listener {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

#[cfg(windows)]
fn carry(s: uds_windows::UnixStream, rt: &tokio::runtime::Handle) -> io::Result<Boxed> {
    use std::io::{Read, Write};
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let (ours, theirs) = tokio::io::duplex(64 * 1024);
    let (mut from_us, mut to_us) = tokio::io::split(theirs);
    let (mut rd, mut wr) = (s.try_clone()?, s);
    let h = rt.clone();
    std::thread::Builder::new().name("socket in".into()).spawn(move || {
        let mut buf = vec![0u8; 64 * 1024];
        while let Ok(n) = rd.read(&mut buf) {
            if n == 0 || h.block_on(to_us.write_all(&buf[..n])).is_err() {
                break;
            }
        }
        let _ = h.block_on(to_us.shutdown());
    })?;
    let h = rt.clone();
    std::thread::Builder::new().name("socket out".into()).spawn(move || {
        let mut buf = vec![0u8; 64 * 1024];
        while let Ok(n) = h.block_on(from_us.read(&mut buf)) {
            if n == 0 || wr.write_all(&buf[..n]).is_err() {
                break;
            }
        }
        // Also wakes the other thread.
        let _ = wr.shutdown(std::net::Shutdown::Both);
    })?;
    Ok(Box::new(ours))
}

#[cfg(windows)]
pub async fn connect(paths: &Paths) -> io::Result<Boxed> {
    let path = paths.endpoint()?;
    let s = tokio::task::spawn_blocking(move || uds_windows::UnixStream::connect(path))
        .await
        .map_err(io::Error::other)??;
    carry(s, &tokio::runtime::Handle::current())
}
