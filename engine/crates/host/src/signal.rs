//! Being asked to stop by the system: a closed terminal, an interrupt, a logout.

use std::io;

/// Listens from now on; resolves with the signal's number.
#[cfg(unix)]
pub fn stop() -> io::Result<impl Future<Output = i32>> {
    use tokio::signal::unix::{SignalKind, signal};
    let mut hangup = signal(SignalKind::hangup())?;
    let mut interrupt = signal(SignalKind::interrupt())?;
    let mut terminate = signal(SignalKind::terminate())?;
    Ok(async move {
        tokio::select! {
            _ = hangup.recv() => SignalKind::hangup(),
            _ = interrupt.recv() => SignalKind::interrupt(),
            _ = terminate.recv() => SignalKind::terminate(),
        }
        .as_raw_value()
    })
}

/// As on Unix, with the number an interrupt has there.
#[cfg(windows)]
pub fn stop() -> io::Result<impl Future<Output = i32>> {
    use tokio::signal::windows;
    let mut c = windows::ctrl_c()?;
    let mut brk = windows::ctrl_break()?;
    let mut close = windows::ctrl_close()?;
    let mut logoff = windows::ctrl_logoff()?;
    let mut shutdown = windows::ctrl_shutdown()?;
    Ok(async move {
        tokio::select! {
            _ = c.recv() => {}
            _ = brk.recv() => {}
            _ = close.recv() => {}
            _ = logoff.recv() => {}
            _ = shutdown.recv() => {}
        }
        2
    })
}
