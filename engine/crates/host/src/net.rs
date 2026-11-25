//! The local listener: a Unix socket in a private directory, on every system.

use std::io;

use tokio::io::{AsyncRead, AsyncWrite};

use crate::paths::{Paths, Place};

pub trait Stream: AsyncRead + AsyncWrite + Unpin + Send {}
impl<T: AsyncRead + AsyncWrite + Unpin + Send> Stream for T {}
pub type Boxed = Box<dyn Stream>;

#[cfg(unix)]
pub use unix::{Listener, Raw, carry, connect};
#[cfg(windows)]
pub use windows::{Listener, Raw, carry, connect};

#[cfg(unix)]
mod unix {
    use super::{Boxed, Paths, Place, io};

    /// A connection before the host has taken it.
    pub type Raw = tokio::net::UnixStream;

    pub struct Listener {
        sock: tokio::net::UnixListener,
        uid: u32,
        place: Place,
    }

    impl Listener {
        pub fn bind(paths: &Paths) -> io::Result<Self> {
            use std::os::unix::fs::{MetadataExt, PermissionsExt};
            let mut place = Place::new(paths)?;
            let sock = tokio::net::UnixListener::bind(&place.path)?;
            place.made()?;
            std::fs::set_permissions(&place.path, std::fs::Permissions::from_mode(0o600))?;
            // The socket was made by this process, so its owner is who we are.
            let uid = std::fs::symlink_metadata(&place.path)?.uid();
            owned(uid, &paths.data)?;
            owned(uid, &paths.run())?;
            Ok(Self { sock, uid, place })
        }

        pub fn path(&self) -> &str {
            &self.place.path
        }
        /// Whether clients that look for it find this one.
        pub fn still(&self) -> bool {
            self.place.still()
        }

        /// The next peer of this user. Others are dropped without a word.
        pub async fn accept(&mut self) -> io::Result<Raw> {
            loop {
                let (s, _) = self.sock.accept().await?;
                match s.peer_cred() {
                    Ok(c) if c.uid() == self.uid => return Ok(s),
                    _ => drop(s),
                }
            }
        }
    }

    fn owned(uid: u32, dir: &std::path::Path) -> io::Result<()> {
        use std::os::unix::fs::MetadataExt;
        match std::fs::metadata(dir)?.uid() == uid {
            true => Ok(()),
            false => Err(io::Error::other(format!("{} belongs to another user", dir.display()))),
        }
    }

    pub fn carry(s: Raw) -> io::Result<Boxed> {
        Ok(Box::new(s))
    }

    pub async fn connect(paths: &Paths) -> io::Result<Boxed> {
        carry(tokio::net::UnixStream::connect(paths.endpoint()?).await?)
    }

    #[cfg(test)]
    mod tests {
        use tokio::io::AsyncReadExt;

        use super::*;

        #[tokio::test]
        async fn another_user_is_dropped() {
            let d = tempfile::tempdir().unwrap();
            let p = Paths::new(d.path().join("d"));
            crate::paths::private_dir(&p.run()).unwrap();
            let mut l = Listener::bind(&p).unwrap();
            let ours = l.uid;
            l.uid = ours + 1;
            let mut peer = tokio::net::UnixStream::connect(l.path()).await.unwrap();
            // The peer is closed while the listener goes on waiting for one it may take.
            let mut b = [0; 1];
            let none = tokio::select! {
                _ = l.accept() => panic!("taken"),
                n = peer.read(&mut b) => n.unwrap_or(0),
            };
            assert_eq!(none, 0);
            l.uid = ours;
            let _peer = tokio::net::UnixStream::connect(l.path()).await.unwrap();
            l.accept().await.unwrap();
        }

        #[test]
        fn a_directory_of_another_user_is_refused() {
            use std::os::unix::fs::MetadataExt;
            let d = tempfile::tempdir().unwrap();
            let ours = std::fs::metadata(d.path()).unwrap().uid();
            owned(ours, d.path()).unwrap();
            assert!(owned(ours + 1, d.path()).is_err());
        }

        #[tokio::test]
        async fn a_socket_that_is_not_ours_any_more_is_seen_and_left_alone() {
            let d = tempfile::tempdir().unwrap();
            let p = Paths::new(d.path().join("d"));
            crate::paths::private_dir(&p.run()).unwrap();
            let l = Listener::bind(&p).unwrap();
            assert!(l.still());
            let path = std::path::PathBuf::from(l.path());
            std::fs::remove_file(&path).unwrap();
            assert!(!l.still());
            let other = std::os::unix::net::UnixListener::bind(&path).unwrap();
            assert!(!l.still());
            drop(l);
            assert!(path.exists());
            drop(other);
        }

        #[tokio::test]
        async fn a_long_name_gets_a_short_place_that_is_cleaned_up() {
            let d = tempfile::tempdir().unwrap();
            let p = Paths::new(d.path().join("n".repeat(120)));
            crate::paths::private_dir(&p.run()).unwrap();
            let l = Listener::bind(&p).unwrap();
            let first = std::path::PathBuf::from(l.path());
            crate::paths::private_file(&p.endpoint_file(), l.path()).unwrap();
            assert!(first.exists());
            // Killed: nothing is cleaned up. The next host does it.
            std::mem::forget(l);
            let l = Listener::bind(&p).unwrap();
            let second = std::path::PathBuf::from(l.path());
            assert_ne!(first, second);
            assert!(!first.parent().unwrap().exists());
            drop(l);
            assert!(!second.parent().unwrap().exists());
        }
    }
}

/// Windows has Unix sockets but no way to wait for them with tokio's sockets, so each connection
/// gets two threads that carry its bytes. There are a few clients, not thousands. The socket lies
/// in the user's own application data, which other users cannot enter; the token does the rest.
///
/// The threads never wait inside the system: they look, and look again a little later. When one end
/// shuts a socket while the other begins to wait for it, Windows can lose the wake-up, and then both
/// wait for good (seen in CI; K31).
#[cfg(windows)]
mod windows {
    use std::{
        io::{ErrorKind, Read, Write},
        net::Shutdown,
        pin::Pin,
        sync::{
            Arc,
            atomic::{AtomicBool, Ordering::Relaxed},
        },
        task::{Context, Poll},
        time::Duration,
    };

    use tokio::{
        io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, DuplexStream, ReadBuf},
        runtime::Handle,
        sync::mpsc,
    };
    use uds_windows::{UnixListener, UnixStream};

    use super::{Boxed, Paths, Place, io};

    /// A connection before the host has taken it, and spent its threads on it.
    pub type Raw = UnixStream;

    pub struct Listener {
        rx: mpsc::Receiver<io::Result<UnixStream>>,
        place: Place,
    }

    /// Between two looks: short while something happens, longer while nothing does.
    #[derive(Default)]
    struct Pause(u32);
    impl Pause {
        const LONGEST: Duration = Duration::from_millis(10);
        fn wait(&mut self) {
            // At first only lets others run: what is asked for is mostly about to come.
            if self.0 < 50 {
                std::thread::yield_now();
            } else {
                let ms = Duration::from_millis(1 << ((self.0 - 50).min(10) / 3));
                std::thread::sleep(ms.min(Self::LONGEST));
            }
            self.0 = self.0.saturating_add(1);
        }
    }

    impl Listener {
        pub fn bind(paths: &Paths) -> io::Result<Self> {
            let mut place = Place::new(paths)?;
            let sock = UnixListener::bind(&place.path)?;
            sock.set_nonblocking(true)?;
            place.made()?;
            // One at a time: the rest wait in the system's queue until the host wants them.
            let (tx, rx) = mpsc::channel(1);
            std::thread::Builder::new().name("accept".into()).spawn(move || {
                // The host decides what an error means; it waits before it asks again.
                while !tx.is_closed() {
                    let r = match sock.accept() {
                        Err(e) if e.kind() == ErrorKind::WouldBlock => {
                            std::thread::sleep(Pause::LONGEST);
                            continue;
                        }
                        r => r.map(|(s, _)| s),
                    };
                    if tx.blocking_send(r).is_err() {
                        break;
                    }
                }
            })?;
            Ok(Self { rx, place })
        }

        pub fn path(&self) -> &str {
            &self.place.path
        }
        /// Whether clients that look for it find one.
        pub fn still(&self) -> bool {
            self.place.still()
        }

        pub async fn accept(&mut self) -> io::Result<Raw> {
            match self.rx.recv().await {
                Some(r) => r,
                None => Err(io::Error::other("the listener has stopped")),
            }
        }
    }

    /// How long bytes already written may take to reach a peer after the host let go of it.
    const LINGER: Duration = Duration::from_secs(2);

    /// Our end of the threads' pipe. Letting go of it releases them.
    struct Carried {
        io: DuplexStream,
        over: Arc<AtomicBool>,
        rt: Handle,
    }
    impl Drop for Carried {
        fn drop(&mut self) {
            let over = self.over.clone();
            self.rt.spawn(async move {
                tokio::time::sleep(LINGER).await;
                over.store(true, Relaxed);
            });
        }
    }
    impl AsyncRead for Carried {
        fn poll_read(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &mut ReadBuf<'_>,
        ) -> Poll<io::Result<()>> {
            Pin::new(&mut self.io).poll_read(cx, buf)
        }
    }
    impl AsyncWrite for Carried {
        fn poll_write(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
            buf: &[u8],
        ) -> Poll<io::Result<usize>> {
            Pin::new(&mut self.io).poll_write(cx, buf)
        }
        fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.io).poll_flush(cx)
        }
        fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
            Pin::new(&mut self.io).poll_shutdown(cx)
        }
    }

    /// Does `f` until it does not have to wait, or it is over.
    fn looking<T>(over: &AtomicBool, mut f: impl FnMut() -> io::Result<T>) -> io::Result<T> {
        let mut pause = Pause::default();
        loop {
            match f() {
                Err(e) if e.kind() == ErrorKind::WouldBlock && !over.load(Relaxed) => pause.wait(),
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                r => return r,
            }
        }
    }

    pub fn carry(s: Raw) -> io::Result<Boxed> {
        let rt = Handle::current();
        let (ours, theirs) = tokio::io::duplex(64 * 1024);
        let (mut from_us, mut to_us) = tokio::io::split(theirs);
        let (mut rd, mut wr) = (s.try_clone()?, s);
        rd.set_nonblocking(true)?;
        wr.set_nonblocking(true)?;
        let over = Arc::new(AtomicBool::new(false));
        let (h, o) = (rt.clone(), over.clone());
        std::thread::Builder::new().name("socket in".into()).spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            while let Ok(n) = looking(&o, || rd.read(&mut buf)) {
                if n == 0 || h.block_on(to_us.write_all(&buf[..n])).is_err() {
                    break;
                }
            }
            let _ = h.block_on(to_us.shutdown());
        })?;
        let (h, o) = (rt.clone(), over.clone());
        let out = std::thread::Builder::new().name("socket out".into()).spawn(move || {
            let mut buf = vec![0u8; 64 * 1024];
            'all: while let Ok(n) = h.block_on(from_us.read(&mut buf)) {
                let mut left = &buf[..n];
                if n == 0 {
                    break;
                }
                while !left.is_empty() {
                    match looking(&o, || wr.write(left)) {
                        Ok(0) | Err(_) => break 'all,
                        Ok(n) => left = &left[n..],
                    }
                }
            }
            // The other thread sees it when it next looks, and so does the peer.
            let _ = wr.shutdown(Shutdown::Both);
            o.store(true, Relaxed);
        });
        if let Err(e) = out {
            over.store(true, Relaxed);
            return Err(e);
        }
        Ok(Box::new(Carried { io: ours, over, rt }))
    }

    pub async fn connect(paths: &Paths) -> io::Result<Boxed> {
        let path = paths.endpoint()?;
        let s = tokio::task::spawn_blocking(move || UnixStream::connect(path))
            .await
            .map_err(io::Error::other)??;
        carry(s)
    }
}
