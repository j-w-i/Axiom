//! One task per open session. It alone gives events their numbers.

use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
    time::Duration,
};

use axiom_journal::Durability;
use axiom_proto::{Body, Event, ItemId, Message, Outcome, SessionId, now_ms};
use serde_json::Value;
use tokio::{
    sync::{mpsc, oneshot},
    time::{Instant, sleep_until},
};

use crate::{outbox::Outbox, store::Store};

/// Deltas wait at most this long to be joined with others of their item.
pub const COALESCE: Duration = Duration::from_millis(33);
/// Text kept for one item. What comes after is counted, not kept.
pub const ITEM_LIMIT: usize = 4 * 1024 * 1024;
/// Deltas of one item are committed early when this much has gathered.
const FLUSH_BYTES: usize = 64 * 1024;
const RECENT: usize = 1024;

pub enum Say {
    Event(Body),
    Text(ItemId, String),
    End(ItemId, Outcome, Option<Value>),
}

pub enum Cmd {
    Say(Vec<Say>),
    /// Registers if `after` is within the recent events; otherwise the caller reads more history.
    Follow {
        client: u64,
        out: Arc<Outbox>,
        after: u64,
        joined: oneshot::Sender<bool>,
    },
    Unfollow {
        client: u64,
    },
    /// Resolves once everything said before it is in the journal.
    Sync(oneshot::Sender<u64>),
}

#[derive(Clone)]
pub struct Session {
    pub id: SessionId,
    tx: mpsc::Sender<Cmd>,
}

impl Session {
    pub fn open(id: SessionId, last_seq: u64, store: Store) -> Self {
        let (tx, rx) = mpsc::channel(256);
        let a = Actor {
            id: id.clone(),
            seq: last_seq,
            store,
            items: HashMap::new(),
            order: Vec::new(),
            recent: VecDeque::new(),
            floor: last_seq,
            followers: HashMap::new(),
            due: None,
            broken: false,
        };
        tokio::spawn(a.run(rx));
        Self { id, tx }
    }
    /// Waits while the session is behind: this is the producer's backpressure.
    pub async fn say(&self, s: Vec<Say>) {
        let _ = self.tx.send(Cmd::Say(s)).await;
    }
    pub async fn cmd(&self, c: Cmd) {
        let _ = self.tx.send(c).await;
    }
    pub async fn sync(&self) -> Option<u64> {
        let (tx, rx) = oneshot::channel();
        self.tx.send(Cmd::Sync(tx)).await.ok()?;
        rx.await.ok()
    }
}

#[derive(Default)]
struct Item {
    text: String,
    pending: String,
    dropped: usize,
}

struct Actor {
    id: SessionId,
    seq: u64,
    store: Store,
    items: HashMap<ItemId, Item>,
    /// Items with pending text, oldest first.
    order: Vec<ItemId>,
    recent: VecDeque<Event>,
    /// `recent` holds every event after this one.
    floor: u64,
    followers: HashMap<u64, Arc<Outbox>>,
    due: Option<Instant>,
    broken: bool,
}

impl Actor {
    async fn run(mut self, mut rx: mpsc::Receiver<Cmd>) {
        loop {
            let due = self.due;
            let timer = async move {
                match due {
                    Some(t) => sleep_until(t).await,
                    None => std::future::pending().await,
                }
            };
            tokio::select! {
                biased;
                () = timer => self.flush(Vec::new()).await,
                c = rx.recv() => match c {
                    Some(c) => self.handle(c).await,
                    None => break,
                },
            }
        }
        self.flush(Vec::new()).await;
    }

    async fn handle(&mut self, c: Cmd) {
        match c {
            Cmd::Say(all) => {
                let mut bodies = Vec::new();
                for s in all {
                    self.say(s, &mut bodies);
                }
                let heavy = self.items.values().any(|i| i.pending.len() > FLUSH_BYTES);
                if !bodies.is_empty() || heavy {
                    self.flush(bodies).await;
                }
            }
            Cmd::Follow {
                client,
                out,
                after,
                joined,
            } => {
                let ok = after >= self.floor && after <= self.seq;
                if ok {
                    let gap: Vec<_> = self
                        .recent
                        .iter()
                        .filter(|e| e.seq > after)
                        .cloned()
                        .collect();
                    let fits = gap.is_empty()
                        || out.offer(&Message::Events {
                            session: self.id.clone(),
                            events: gap,
                        });
                    if fits {
                        self.followers.insert(client, out);
                    }
                    let _ = joined.send(fits);
                } else {
                    let _ = joined.send(false);
                }
            }
            Cmd::Unfollow { client } => {
                self.followers.remove(&client);
            }
            Cmd::Sync(tx) => {
                self.flush(Vec::new()).await;
                let _ = tx.send(self.seq);
            }
        }
    }

    /// Text waits; anything else goes out after the text that came before it.
    fn say(&mut self, s: Say, bodies: &mut Vec<Body>) {
        match s {
            Say::Text(item, text) => {
                if text.is_empty() {
                    return;
                }
                let Some(i) = self.items.get_mut(&item) else {
                    return;
                };
                let room = ITEM_LIMIT.saturating_sub(i.text.len());
                let cut = floor_char(&text, room);
                i.dropped += text.len() - cut;
                if cut == 0 {
                    return;
                }
                i.text.push_str(&text[..cut]);
                if i.pending.is_empty() {
                    self.order.push(item.clone());
                }
                i.pending.push_str(&text[..cut]);
                self.due.get_or_insert_with(|| Instant::now() + COALESCE);
            }
            Say::End(item, outcome, result) => {
                let Some(mut i) = self.items.remove(&item) else {
                    return;
                };
                self.order.retain(|o| *o != item);
                if i.dropped > 0 {
                    i.text
                        .push_str(&format!("\n[{} more bytes were not kept]", i.dropped));
                }
                bodies.push(Body::ItemEnded {
                    item,
                    outcome,
                    text: i.text,
                    result,
                });
            }
            Say::Event(b) => {
                self.drain(bodies);
                if let Body::ItemStarted { item, .. } = &b {
                    self.items.insert(item.clone(), Item::default());
                }
                bodies.push(b);
            }
        }
    }

    fn drain(&mut self, bodies: &mut Vec<Body>) {
        for item in self.order.drain(..) {
            if let Some(i) = self.items.get_mut(&item) {
                let text = std::mem::take(&mut i.pending);
                bodies.push(Body::ItemDelta { item, text });
            }
        }
        self.due = None;
    }

    async fn flush(&mut self, mut bodies: Vec<Body>) {
        if self.due.is_some_and(|d| d <= Instant::now()) || bodies.is_empty() {
            self.drain(&mut bodies);
        }
        if bodies.is_empty() || self.broken {
            return;
        }
        let how = if bodies.iter().any(intent) {
            Durability::Intent
        } else {
            Durability::Stream
        };
        let at = now_ms();
        let first = self.seq + 1;
        let events: Vec<_> = bodies
            .into_iter()
            .zip(first..)
            .map(|(body, seq)| Event { seq, at, body })
            .collect();
        match self.store.append(self.id.clone(), events, how).await {
            Ok(events) => {
                self.seq += events.len() as u64;
                let m = Message::Events {
                    session: self.id.clone(),
                    events,
                };
                let (id, seq) = (&self.id, self.seq);
                self.followers.retain(|_, out| {
                    let ok = out.offer(&m);
                    if !ok {
                        out.force(&Message::Resync {
                            session: id.clone(),
                            resync: seq,
                        });
                    }
                    ok
                });
                if let Message::Events { events, .. } = m {
                    self.recent.extend(events);
                }
                while self.recent.len() > RECENT {
                    if let Some(e) = self.recent.pop_front() {
                        self.floor = e.seq;
                    }
                }
            }
            Err(e) => {
                // A journal that cannot be written ends the session here rather than showing what
                // would not survive. Followers learn by resync, and `host.status` says why.
                eprintln!("axiom-host: {}: {e}", self.id);
                self.broken = true;
                for out in self.followers.values() {
                    out.force(&Message::Resync {
                        session: self.id.clone(),
                        resync: self.seq,
                    });
                }
                self.followers.clear();
            }
        }
    }
}

fn intent(b: &Body) -> bool {
    matches!(
        b,
        Body::RequestOpened { .. }
            | Body::RequestClosed { .. }
            | Body::AgentAdded { .. }
            | Body::RunStarted { .. }
    )
}

/// The longest prefix of `s` that is at most `n` bytes and ends between characters.
fn floor_char(s: &str, n: usize) -> usize {
    if n >= s.len() {
        return s.len();
    }
    (0..=n).rev().find(|i| s.is_char_boundary(*i)).unwrap_or(0)
}
