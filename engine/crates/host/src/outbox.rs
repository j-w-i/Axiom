//! What waits to be written to one client, bounded in bytes.

use std::{
    collections::VecDeque,
    sync::{Mutex, MutexGuard, PoisonError},
};

use axiom_proto::Message;
use tokio::sync::Notify;

pub struct Outbox {
    q: Mutex<Q>,
    ready: Notify,
    room: Notify,
    limit: usize,
}
#[derive(Default)]
struct Q {
    lines: VecDeque<Vec<u8>>,
    bytes: usize,
    closed: bool,
}

impl Outbox {
    pub fn new(limit: usize) -> Self {
        Self { q: Mutex::default(), ready: Notify::new(), room: Notify::new(), limit }
    }
    fn q(&self) -> MutexGuard<'_, Q> {
        self.q.lock().unwrap_or_else(PoisonError::into_inner)
    }
    fn put(&self, m: &Message, force: bool) -> bool {
        let Ok(mut line) = serde_json::to_vec(m) else {
            return true;
        };
        line.push(b'\n');
        let mut q = self.q();
        if q.closed {
            return true;
        }
        if !force && q.bytes > 0 && q.bytes + line.len() > self.limit {
            return false;
        }
        q.bytes += line.len();
        q.lines.push_back(line);
        drop(q);
        self.ready.notify_one();
        true
    }
    /// False if there is no room. Never waits.
    pub fn offer(&self, m: &Message) -> bool {
        self.put(m, false)
    }
    /// For answers and resync notices: few, small, and needed to make progress.
    pub fn force(&self, m: &Message) {
        self.put(m, true);
    }
    /// Waits for room. Used for history, which a client asked for. False if the client has gone.
    pub async fn send(&self, m: &Message) -> bool {
        loop {
            let room = self.room.notified();
            tokio::pin!(room);
            room.as_mut().enable();
            if self.q().closed {
                return false;
            }
            if self.put(m, false) {
                return true;
            }
            room.await;
        }
    }
    /// Everything waiting, or `None` once closed and drained.
    pub async fn take(&self) -> Option<Vec<u8>> {
        loop {
            let ready = self.ready.notified();
            {
                let mut q = self.q();
                if !q.lines.is_empty() {
                    let mut all = Vec::with_capacity(q.bytes);
                    for l in q.lines.drain(..) {
                        all.extend(l);
                    }
                    q.bytes = 0;
                    drop(q);
                    self.room.notify_waiters();
                    return Some(all);
                }
                if q.closed {
                    return None;
                }
            }
            ready.await;
        }
    }
    pub fn close(&self) {
        self.q().closed = true;
        self.ready.notify_one();
        self.room.notify_waiters();
    }
}
