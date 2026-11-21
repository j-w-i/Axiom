//! The session journal (ADR-0015 §9, ADR-0017; `docs/protocols/agents.md` §Sessions):
//! `<dir>/<session id>.jsonl`, append-only, one versioned record per line, plus image blobs in
//! `<dir>/<session id>.blobs/<sha256>`. A record is either a wire [`AgentEvent`] (what clients
//! replay) or an internal record with the exact messages sent to the model (provider-native
//! opaque blocks included), so a session resumes with the context it had.

use crate::model::{Block, Message, Role};
use axiom_protocol::agent::{AgentEvent, EventBody, ImageData, ImageRef, SessionId};
use serde::{Deserialize, Serialize};
use std::io::Write;
use std::path::{Path, PathBuf};

pub const JOURNAL_VERSION: u32 = 1;
pub const MAX_IMAGE_BYTES: usize = 5 * 1024 * 1024;
const MEDIA_TYPES: &[&str] = &["image/png", "image/jpeg", "image/gif", "image/webp"];

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "record", rename_all = "camelCase")]
pub enum Record {
    /// A wire event (persisted kinds only; never `textDelta`).
    Event { v: u32, event: Box<AgentEvent> },
    /// The assistant message of one model round, exactly as it will be sent back.
    #[serde(rename_all = "camelCase")]
    Assistant {
        v: u32,
        turn: u32,
        /// The `seq` of the newest event when this was written.
        at_seq: u64,
        blocks: Vec<Block>,
    },
    /// The content of the files the user attached to a message (read when it was sent), added
    /// to that message in the context.
    #[serde(rename_all = "camelCase")]
    Attached {
        v: u32,
        turn: u32,
        at_seq: u64,
        text: String,
    },
    /// The tool results answering an assistant message.
    #[serde(rename_all = "camelCase")]
    ToolResults {
        v: u32,
        turn: u32,
        at_seq: u64,
        blocks: Vec<Block>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum JournalError {
    #[error("I/O error: {0}")]
    Io(String),
    #[error("journal line {line} is not valid: {reason}")]
    Corrupt { line: usize, reason: String },
    #[error("invalid image: {0}")]
    Image(String),
}

fn io(e: std::io::Error) -> JournalError {
    JournalError::Io(e.to_string())
}

/// One session's journal file.
pub struct Journal {
    path: PathBuf,
    blobs: PathBuf,
}

impl Journal {
    pub fn new(dir: &Path, id: &SessionId) -> Self {
        Self {
            path: dir.join(format!("{}.jsonl", id.0)),
            blobs: dir.join(format!("{}.blobs", id.0)),
        }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn exists(&self) -> bool {
        self.path.exists()
    }

    /// Appends records and flushes them to the OS before returning.
    pub fn append(&self, records: &[Record]) -> Result<(), JournalError> {
        if let Some(d) = self.path.parent() {
            std::fs::create_dir_all(d).map_err(io)?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)
            .map_err(io)?;
        let mut buf = String::new();
        for r in records {
            buf.push_str(&serde_json::to_string(r).map_err(|e| JournalError::Io(e.to_string()))?);
            buf.push('\n');
        }
        f.write_all(buf.as_bytes()).map_err(io)?;
        f.sync_data().map_err(io)
    }

    /// Every record. A torn last line (the daemon died mid-write) is dropped; any other bad
    /// line is an error.
    pub fn read(&self) -> Result<Vec<Record>, JournalError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(vec![]),
            Err(e) => return Err(io(e)),
        };
        let lines: Vec<&str> = text.lines().collect();
        let complete = text.ends_with('\n');
        let mut out = Vec::new();
        for (i, l) in lines.iter().enumerate() {
            if l.trim().is_empty() {
                continue;
            }
            match serde_json::from_str::<Record>(l) {
                Ok(r) => out.push(r),
                Err(_) if i + 1 == lines.len() && !complete => break,
                Err(e) => {
                    return Err(JournalError::Corrupt {
                        line: i + 1,
                        reason: e.to_string(),
                    })
                }
            }
        }
        Ok(out)
    }

    /// Removes the journal and its blobs.
    pub fn delete(&self) -> Result<(), JournalError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(io(e)),
        }
        match std::fs::remove_dir_all(&self.blobs) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(io(e)),
        }
    }

    /// Stores an image (content-addressed) after checking its type and size.
    pub fn put_image(&self, img: &ImageData) -> Result<ImageRef, JournalError> {
        use base64_lite::decode;
        use sha2::Digest;
        if !MEDIA_TYPES.contains(&img.media_type.as_str()) {
            return Err(JournalError::Image(format!(
                "`{}` is not one of PNG, JPEG, GIF, WebP",
                img.media_type
            )));
        }
        let bytes = decode(&img.data)
            .ok_or_else(|| JournalError::Image("the data is not base64".into()))?;
        if bytes.len() > MAX_IMAGE_BYTES {
            return Err(JournalError::Image("larger than 5 MiB".into()));
        }
        let sha = hex::encode(sha2::Sha256::digest(&bytes));
        std::fs::create_dir_all(&self.blobs).map_err(io)?;
        let p = self.blobs.join(&sha);
        if !p.exists() {
            std::fs::write(&p, &bytes).map_err(io)?;
        }
        Ok(ImageRef {
            media_type: img.media_type.clone(),
            sha256: sha,
            bytes: bytes.len() as u64,
        })
    }

    pub fn get_image(&self, r: &ImageRef) -> Result<ImageData, JournalError> {
        self.get_image_by_sha(&r.sha256, &r.media_type)
    }

    pub fn get_image_by_sha(&self, sha: &str, media_type: &str) -> Result<ImageData, JournalError> {
        if sha.len() != 64 || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(JournalError::Image("not a SHA-256".into()));
        }
        let bytes = std::fs::read(self.blobs.join(sha)).map_err(io)?;
        Ok(ImageData {
            media_type: media_type.into(),
            data: base64_lite::encode(&bytes),
        })
    }
}

/// The wire events of a journal, in order.
pub fn events(records: &[Record]) -> Vec<AgentEvent> {
    records
        .iter()
        .filter_map(|r| match r {
            Record::Event { event, .. } => Some((**event).clone()),
            _ => None,
        })
        .collect()
}

/// The model context a journal stands for: the messages after the latest compaction (whose
/// summary opens the conversation), without rewound turns.
///
/// `image` loads an image blob for a `user` event (`None` drops it, with a note).
pub fn context(records: &[Record], image: &dyn Fn(&ImageRef) -> Option<ImageData>) -> Vec<Message> {
    // Rewinds: a turn n is dropped if a later rewind has fromTurn <= n and was written after it.
    let mut dropped_before: Vec<(usize, u32)> = Vec::new(); // (record index, fromTurn)
    for (i, r) in records.iter().enumerate() {
        if let Record::Event { event, .. } = r {
            if let EventBody::Rewind { from_turn } = event.event {
                dropped_before.push((i, from_turn));
            }
        }
    }
    let turn_of = |r: &Record| match r {
        Record::Event { event, .. } => event.turn,
        Record::Assistant { turn, .. }
        | Record::ToolResults { turn, .. }
        | Record::Attached { turn, .. } => *turn,
    };
    let live = |i: usize, r: &Record| {
        !dropped_before
            .iter()
            .any(|(at, from)| i < *at && turn_of(r) >= *from)
    };
    // The newest live compaction.
    let mut through: Option<(u64, String)> = None;
    for (i, r) in records.iter().enumerate() {
        if let Record::Event { event, .. } = r {
            if let EventBody::Compaction {
                through_seq,
                summary,
            } = &event.event
            {
                if live(i, r) {
                    through = Some((*through_seq, summary.clone()));
                }
            }
        }
    }
    let mut msgs: Vec<Message> = Vec::new();
    if let Some((_, summary)) = &through {
        msgs.push(Message::user_text(format!(
            "[Summary of the earlier conversation]\n{summary}"
        )));
        msgs.push(Message {
            role: Role::Assistant,
            blocks: vec![Block::text(
                "Understood. I have the summary and will continue from it.",
            )],
        });
    }
    let cut = through.as_ref().map(|(s, _)| *s);
    for (i, r) in records.iter().enumerate() {
        if !live(i, r) {
            continue;
        }
        let seq = match r {
            Record::Event { event, .. } => event.seq,
            Record::Assistant { at_seq, .. }
            | Record::ToolResults { at_seq, .. }
            | Record::Attached { at_seq, .. } => *at_seq,
        };
        if cut.is_some_and(|c| seq <= c) {
            continue;
        }
        match r {
            Record::Event { event, .. } => {
                if let EventBody::User { text, images, .. } = &event.event {
                    let mut blocks = vec![Block::text(text.clone())];
                    for im in images {
                        match image(im) {
                            Some(d) => blocks.push(Block::Image {
                                media_type: d.media_type,
                                data: d.data,
                            }),
                            None => {
                                blocks.push(Block::text("[an image that is no longer available]"))
                            }
                        }
                    }
                    push(&mut msgs, Role::User, blocks);
                }
            }
            Record::Assistant { blocks, .. } => push(&mut msgs, Role::Assistant, blocks.clone()),
            Record::ToolResults { blocks, .. } => push(&mut msgs, Role::User, blocks.clone()),
            Record::Attached { text, .. } => {
                push(&mut msgs, Role::User, vec![Block::text(text.clone())])
            }
        }
    }
    msgs
}

/// Appends, merging into the previous message of the same role (model APIs want alternation).
fn push(msgs: &mut Vec<Message>, role: Role, blocks: Vec<Block>) {
    if blocks.is_empty() {
        return;
    }
    match msgs.last_mut() {
        Some(m) if m.role == role => m.blocks.extend(blocks),
        _ => msgs.push(Message { role, blocks }),
    }
}

/// Standard base64 (RFC 4648, padded), small and dependency-free.
pub mod base64_lite {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

    pub fn encode(b: &[u8]) -> String {
        let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
        for c in b.chunks(3) {
            let n = (c[0] as u32) << 16
                | (*c.get(1).unwrap_or(&0) as u32) << 8
                | *c.get(2).unwrap_or(&0) as u32;
            s.push(A[(n >> 18) as usize & 63] as char);
            s.push(A[(n >> 12) as usize & 63] as char);
            s.push(if c.len() > 1 {
                A[(n >> 6) as usize & 63] as char
            } else {
                '='
            });
            s.push(if c.len() > 2 {
                A[n as usize & 63] as char
            } else {
                '='
            });
        }
        s
    }

    pub fn decode(s: &str) -> Option<Vec<u8>> {
        let s: Vec<u8> = s.bytes().filter(|b| !b.is_ascii_whitespace()).collect();
        if !s.len().is_multiple_of(4) {
            return None;
        }
        let val = |c: u8| -> Option<u32> {
            Some(match c {
                b'A'..=b'Z' => c - b'A',
                b'a'..=b'z' => c - b'a' + 26,
                b'0'..=b'9' => c - b'0' + 52,
                b'+' => 62,
                b'/' => 63,
                _ => return None,
            } as u32)
        };
        let mut out = Vec::with_capacity(s.len() / 4 * 3);
        for (i, q) in s.chunks(4).enumerate() {
            let last = i == s.len() / 4 - 1;
            let pad = q.iter().rev().take_while(|b| **b == b'=').count();
            if pad > 2 || (pad > 0 && !last) {
                return None;
            }
            let mut n = 0u32;
            for (j, &c) in q.iter().enumerate() {
                n <<= 6;
                if j < 4 - pad {
                    n |= val(c)?;
                }
            }
            out.push((n >> 16) as u8);
            if pad < 2 {
                out.push((n >> 8) as u8);
            }
            if pad < 1 {
                out.push(n as u8);
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axiom_protocol::agent::TurnEndReason;

    fn ev(seq: u64, turn: u32, body: EventBody) -> Record {
        Record::Event {
            v: JOURNAL_VERSION,
            event: Box::new(AgentEvent {
                session_id: SessionId("s".into()),
                seq,
                ts_ms: 0,
                turn,
                event: body,
            }),
        }
    }
    fn user(seq: u64, turn: u32, t: &str) -> Record {
        ev(
            seq,
            turn,
            EventBody::User {
                text: t.into(),
                attachments: vec![],
                images: vec![],
            },
        )
    }
    fn asst(turn: u32, at: u64, t: &str) -> Record {
        Record::Assistant {
            v: 1,
            turn,
            at_seq: at,
            blocks: vec![Block::text(t)],
        }
    }
    fn texts(m: &[Message]) -> Vec<String> {
        m.iter()
            .map(|m| {
                m.blocks
                    .iter()
                    .filter_map(|b| match b {
                        Block::Text(t) => Some(t.text.clone()),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join("|")
            })
            .collect()
    }

    #[test]
    fn session_journal_round_trip_and_torn_last_line() {
        let d = tempfile::tempdir().unwrap();
        let j = Journal::new(d.path(), &SessionId("s1".into()));
        j.append(&[user(1, 1, "hi"), asst(1, 1, "hello")]).unwrap();
        assert_eq!(j.read().unwrap().len(), 2);
        // A torn last line is dropped…
        let mut f = std::fs::OpenOptions::new()
            .append(true)
            .open(j.path())
            .unwrap();
        f.write_all(b"{\"record\":\"event\",\"v\":1,\"ev").unwrap();
        assert_eq!(j.read().unwrap().len(), 2);
        // …but a bad line in the middle is corruption.
        f.write_all(b"\n").unwrap();
        j.append(&[user(2, 2, "x")]).unwrap();
        assert!(matches!(
            j.read(),
            Err(JournalError::Corrupt { line: 3, .. })
        ));
    }

    #[test]
    fn session_context_follows_rewinds_and_compaction() {
        let recs = vec![
            user(1, 1, "first"),
            asst(1, 1, "a1"),
            ev(
                2,
                1,
                EventBody::TurnEnd {
                    reason: TurnEndReason::EndTurn,
                },
            ),
            user(3, 2, "second"),
            asst(2, 3, "a2"),
            // Edit of turn 2.
            ev(4, 2, EventBody::Rewind { from_turn: 2 }),
            user(5, 2, "second, edited"),
            asst(2, 5, "a2'"),
        ];
        assert_eq!(
            texts(&context(&recs, &|_| None)),
            ["first", "a1", "second, edited", "a2'"]
        );
        let mut with_c = recs.clone();
        with_c.push(ev(
            6,
            2,
            EventBody::Compaction {
                through_seq: 2,
                summary: "S".into(),
            },
        ));
        let t = texts(&context(&with_c, &|_| None));
        assert!(t[0].contains("S"));
        assert_eq!(&t[2..], ["second, edited", "a2'"]);
    }

    #[test]
    fn session_images_are_checked_and_content_addressed() {
        let d = tempfile::tempdir().unwrap();
        let j = Journal::new(d.path(), &SessionId("s".into()));
        let png = ImageData {
            media_type: "image/png".into(),
            data: base64_lite::encode(b"\x89PNG data"),
        };
        let r = j.put_image(&png).unwrap();
        assert_eq!(r.bytes, 9);
        assert_eq!(j.get_image(&r).unwrap(), png);
        assert!(j
            .put_image(&ImageData {
                media_type: "image/svg+xml".into(),
                data: "AA==".into()
            })
            .is_err());
        assert!(j
            .put_image(&ImageData {
                media_type: "image/png".into(),
                data: "!!".into()
            })
            .is_err());
        assert!(j.get_image_by_sha("../x", "image/png").is_err());
    }

    #[test]
    fn base64_round_trip() {
        for n in 0..20 {
            let b: Vec<u8> = (0..n).map(|i| (i * 37) as u8).collect();
            assert_eq!(base64_lite::decode(&base64_lite::encode(&b)).unwrap(), b);
        }
        assert_eq!(base64_lite::encode(b"Man"), "TWFu");
        assert_eq!(base64_lite::decode("TWE="), Some(b"Ma".to_vec()));
        assert_eq!(base64_lite::decode("TW=E"), None);
    }
}
