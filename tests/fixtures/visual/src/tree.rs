//! Process-tree control.

/// Terminates a process and all its descendants.
pub trait ProcessTree: Send {
    /// Graceful request where the platform has one.
    fn terminate(&mut self);
    /// Forced kill.
    fn kill(&mut self);
}

#[cfg(unix)]
mod unix {
    use super::ProcessTree;
    use nix::sys::signal::{killpg, Signal};
    use nix::unistd::Pid;

    /// The child was spawned with `process_group(0)`, so its pgid is its pid.
    pub struct UnixTree {
        pgid: Pid,
    }

    impl UnixTree {
        pub fn new(pid: u32) -> Self {
            Self {
                pgid: Pid::from_raw(pid as i32),
            }
        }
        fn signal(&self, sig: Signal) {
            // ESRCH just means the group is already gone.
            let _ = killpg(self.pgid, sig);
        }
    }

    impl ProcessTree for UnixTree {
        fn terminate(&mut self) {
            self.signal(Signal::SIGTERM);
        }
        fn kill(&mut self) {
            self.signal(Signal::SIGKILL);
        }
    }
}

#[cfg(unix)]
pub(crate) fn attach(child: &tokio::process::Child) -> Option<Box<dyn ProcessTree>> {
    child
        .id()
        .map(|pid| Box::new(unix::UnixTree::new(pid)) as Box<dyn ProcessTree>)
}

#[cfg(windows)]
pub(crate) fn attach(child: &tokio::process::Child) -> Option<Box<dyn ProcessTree>> {
    crate::tree_windows::JobTree::attach(child).map(|t| Box::new(t) as Box<dyn ProcessTree>)
}
