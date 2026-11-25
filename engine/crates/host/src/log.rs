//! Words for whoever reads stderr. Nobody may be there, or may not read: neither holds the host up.

use std::{
    io::Write,
    sync::{
        OnceLock,
        mpsc::{SyncSender, sync_channel},
    },
    thread,
};

pub fn note(words: String) {
    static TO: OnceLock<Option<SyncSender<String>>> = OnceLock::new();
    let to = TO.get_or_init(|| {
        let (tx, rx) = sync_channel::<String>(64);
        let write = move || {
            for line in rx {
                // One line each, whatever is in it.
                let line = line.replace(char::is_control, " ");
                let _ = writeln!(std::io::stderr(), "axiom-host: {line}");
            }
        };
        thread::Builder::new().name("log".into()).spawn(write).ok().map(|_| tx)
    });
    if let Some(to) = to {
        // Full, nobody reads: what is lost here is in `host.status`.
        let _ = to.try_send(words);
    }
}
