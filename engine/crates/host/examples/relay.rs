//! `relay SOCKET PORT`: a Unix socket whose every connection is carried to a TCP port of this
//! machine. For the app's tests on Windows: a Unix socket that Dart itself listens on can bring the
//! whole machine to a stop there when it is closed (K31). The product has no such thing; its sockets
//! are the host's. Ends with its standard input.
// What it prints says that it is ready.
#![allow(clippy::print_stdout)]

use std::{
    io::{self, ErrorKind, Read, Write},
    net::{Shutdown, TcpStream},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering::Relaxed},
    },
    thread,
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::net::{UnixListener, UnixStream};
#[cfg(windows)]
use uds_windows::{UnixListener, UnixStream};

const PAUSE: Duration = Duration::from_millis(1);

/// Until it does not have to wait.
fn looking<T>(over: &AtomicBool, mut f: impl FnMut() -> io::Result<T>) -> io::Result<T> {
    loop {
        match f() {
            Err(e) if e.kind() == ErrorKind::WouldBlock && !over.load(Relaxed) => {
                thread::sleep(PAUSE);
            }
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            r => return r,
        }
    }
}

fn carry(mut from: impl Read, mut to: impl Write, over: &AtomicBool) {
    let mut buf = vec![0u8; 64 * 1024];
    while let Ok(n @ 1..) = looking(over, || from.read(&mut buf)) {
        let mut left = &buf[..n];
        while !left.is_empty() {
            match looking(over, || to.write(left)) {
                Ok(0) | Err(_) => return,
                Ok(n) => left = &left[n..],
            }
        }
    }
}

fn connection(a: UnixStream, port: u16) -> io::Result<()> {
    let b = TcpStream::connect(("127.0.0.1", port))?;
    b.set_nodelay(true)?;
    a.set_nonblocking(true)?;
    b.set_nonblocking(true)?;
    let over = Arc::new(AtomicBool::new(false));
    let (a2, b2, o2) = (a.try_clone()?, b.try_clone()?, over.clone());
    // Either end's end is the other's: that is how a host that lets go looks to a client.
    thread::spawn(move || {
        carry(&b2, &a2, &o2);
        let _ = a2.shutdown(Shutdown::Both);
        let _ = b2.shutdown(Shutdown::Both);
        o2.store(true, Relaxed);
    });
    thread::spawn(move || {
        carry(&a, &b, &over);
        let _ = a.shutdown(Shutdown::Both);
        let _ = b.shutdown(Shutdown::Both);
        over.store(true, Relaxed);
    });
    Ok(())
}

fn main() -> io::Result<()> {
    let mut args = std::env::args().skip(1);
    let (Some(path), Some(port)) = (args.next(), args.next().and_then(|p| p.parse().ok())) else {
        return Err(io::Error::other("usage: relay SOCKET PORT"));
    };
    let l = UnixListener::bind(path)?;
    l.set_nonblocking(true)?;
    thread::spawn(|| {
        let _ = io::stdin().read_to_end(&mut Vec::new());
        std::process::exit(0);
    });
    println!("ready");
    io::stdout().flush()?;
    let over = AtomicBool::new(false);
    loop {
        let (s, _) = looking(&over, || l.accept())?;
        if let Err(e) = connection(s, port) {
            println!("{e}");
        }
    }
}
