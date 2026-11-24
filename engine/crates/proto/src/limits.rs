//! How long things may be. Everything is measured as it is encoded, because that is what a line
//! holds: one NUL is six bytes of JSON. Bodies are cut before they are journalled, so a line over
//! [`crate::MAX_LINE`] cannot come about later.

use serde::Serialize;
use serde_json::{Value, json};

use crate::{Body, Execution};

/// A line from someone who has not shown the token.
pub const HELLO_LINE: usize = 4 * 1024;
/// Events travel and are read in pages of about this much. One longer event is a page of its own.
pub const PAGE: usize = 256 * 1024;
/// Text of one item. The head is kept.
pub const TEXT: usize = 1024 * 1024;
/// A tool's arguments, or its result.
pub const VALUE: usize = 256 * 1024;
/// Deeper JSON would not survive a reader that stops at 128 levels, some of which are ours.
pub const DEPTH: usize = 64;
pub const TITLE: usize = 500;
pub const WORKSPACE: usize = 4 * 1024;
const NAME: usize = 500;
const SCOPE: usize = 4 * 1024;
const LONG: usize = 64 * 1024;

/// Bytes of `v` as JSON, without building it.
pub fn encoded_len<T: Serialize + ?Sized>(v: &T) -> usize {
    struct Count(usize);
    impl std::io::Write for Count {
        fn write(&mut self, b: &[u8]) -> std::io::Result<usize> {
            self.0 += b.len();
            Ok(b.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let mut c = Count(0);
    // What cannot be encoded cannot be sent either, so it is too long.
    if serde_json::to_writer(&mut c, v).is_err() {
        return usize::MAX;
    }
    c.0
}

/// Bytes of `s` inside a JSON string, without the quotes.
pub fn text_len(s: &str) -> usize {
    s.bytes().map(escaped).sum()
}

/// The longest prefix of `s` that takes at most `max` bytes inside a JSON string.
pub fn head(s: &str, max: usize) -> &str {
    let mut left = max;
    for (i, c) in s.char_indices() {
        let n = if c.is_ascii() { escaped(c as u8) } else { c.len_utf8() };
        match left.checked_sub(n) {
            Some(l) => left = l,
            None => return &s[..i],
        }
    }
    s
}

fn escaped(b: u8) -> usize {
    match b {
        b'"' | b'\\' | b'\n' | b'\r' | b'\t' | 0x08 | 0x0c => 2,
        0..0x20 => 6,
        _ => 1,
    }
}

/// What stands where text was cut.
pub fn cut_note(bytes: usize) -> String {
    format!("\n[{bytes} more bytes were not kept]")
}
/// Room for the longest [`cut_note`].
pub const NOTE: usize = 64;

/// Cuts `s` to `max` encoded bytes, its note included.
pub fn clip(s: &mut String, max: usize) {
    // No string is longer encoded than six times its length.
    if s.len() <= max / 6 || text_len(s) <= max {
        return;
    }
    let keep = head(s, max.saturating_sub(NOTE)).len();
    let note = cut_note(s.len() - keep);
    s.truncate(keep);
    s.push_str(&note);
}

fn clip_opt(s: &mut Option<String>, max: usize) {
    if let Some(s) = s {
        clip(s, max);
    }
}

fn clip_value(v: &mut Option<Value>) {
    if let Some(v) = v
        && let n = encoded_len(v)
        && (n > VALUE || deeper(v, DEPTH))
    {
        *v = json!({ "truncated": n });
    }
}

fn deeper(v: &Value, than: usize) -> bool {
    match v {
        Value::Array(a) => than == 0 || a.iter().any(|v| deeper(v, than - 1)),
        Value::Object(o) => than == 0 || o.values().any(|v| deeper(v, than - 1)),
        _ => false,
    }
}

impl Body {
    /// Cuts what is too long, visibly.
    pub fn fit(&mut self) {
        match self {
            Body::SessionCreated { title, workspace } => {
                clip(title, TITLE);
                clip_opt(workspace, WORKSPACE);
            }
            Body::SessionTitled { title } => clip(title, TITLE),
            Body::AgentAdded { name, execution, .. } => {
                clip(name, NAME);
                let Execution { harness, provider, model, connection, effort, environment } =
                    execution;
                clip(harness, NAME);
                for s in [provider, model, connection, effort, environment] {
                    clip_opt(s, NAME);
                }
            }
            Body::RunEnded { error, .. } => clip_opt(error, LONG),
            Body::ItemStarted { name, args, .. } => {
                clip(name, NAME);
                clip_value(args);
            }
            Body::ItemDelta { text, .. } => clip(text, TEXT),
            Body::ItemEnded { text, result, .. } => {
                clip(text, TEXT);
                clip_value(result);
            }
            Body::RequestOpened { what, subject, scope, .. } => {
                clip(what, NAME);
                clip(subject, LONG);
                clip(scope, SCOPE);
            }
            Body::RequestClosed { answer, .. } => clip(answer, SCOPE),
            Body::Unknown(v) => {
                let n = encoded_len(v);
                if n > VALUE || deeper(v, DEPTH) {
                    let mut kind =
                        v.get("type").and_then(Value::as_str).unwrap_or_default().to_owned();
                    clip(&mut kind, NAME);
                    *v = json!({ "type": kind, "truncated": n });
                }
            }
            Body::AgentState { .. } | Body::RunStarted { .. } | Body::Usage { .. } => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn a_head_is_the_longest_that_fits(s in any::<String>(), max in 0usize..60) {
            let h = head(&s, max);
            let encoded = |s: &str| serde_json::to_string(s).unwrap().len() - 2;
            prop_assert_eq!(text_len(&s), encoded(&s));
            prop_assert!(encoded(h) <= max);
            if let Some(c) = s[h.len()..].chars().next() {
                let more = encoded(&format!("{h}{c}"));
                prop_assert!(more > max);
            }
        }

        #[test]
        fn cut_text_fits_and_says_how_much_is_gone(s in "[a\"\u{0}é]{0,400}", max in 100usize..400) {
            let mut c = s.clone();
            clip(&mut c, max);
            prop_assert!(text_len(&c) <= max);
            if text_len(&s) <= max {
                prop_assert_eq!(c, s);
            } else {
                let kept = c.rfind("\n[").unwrap();
                prop_assert!(s.starts_with(&c[..kept]));
                prop_assert_eq!(&c[kept..], cut_note(s.len() - kept));
            }
        }
    }

    /// Of every body, the worst that can be journalled.
    fn worst() -> Vec<Body> {
        let nul = || "\0".repeat(2 << 20);
        let some = || Some(nul());
        let wide = || Some(json!([nul()]));
        let id = |p: &str| p.to_owned();
        let (a, r, i, q) = (AgentId(id("a")), RunId(id("r")), ItemId(id("i")), RequestId(id("q")));
        vec![
            Body::SessionCreated { title: nul(), workspace: some() },
            Body::SessionTitled { title: nul() },
            Body::AgentAdded {
                agent: a.clone(),
                name: nul(),
                execution: Execution {
                    harness: nul(),
                    provider: some(),
                    model: some(),
                    connection: some(),
                    effort: some(),
                    environment: some(),
                },
                parent: None,
            },
            Body::RunEnded { run: r.clone(), outcome: Outcome::Failed, error: some() },
            Body::ItemStarted {
                item: i.clone(),
                run: r,
                agent: a.clone(),
                kind: ItemKind::Tool,
                name: nul(),
                args: wide(),
            },
            Body::ItemDelta { item: i.clone(), text: nul() },
            Body::ItemEnded { item: i, outcome: Outcome::Failed, text: nul(), result: wide() },
            Body::RequestOpened {
                request: q.clone(),
                agent: a,
                what: nul(),
                subject: nul(),
                scope: nul(),
                answerable: Answerable::No,
            },
            Body::RequestClosed { request: q, answer: nul() },
            Body::Unknown(json!({ "type": nul(), "x": nul() })),
        ]
    }

    #[test]
    fn the_longest_event_of_every_kind_fits_a_line() {
        for mut b in worst() {
            b.fit();
            let n = encoded_len(&b);
            assert!(n <= TEXT + VALUE + 1024, "{n} bytes");
            let line = serde_json::to_string(&Message::Events {
                session: SessionId("s".into()),
                events: vec![Event { seq: 1, at: 1, body: b }],
            })
            .unwrap();
            assert!(line.len() < MAX_LINE);
            serde_json::from_str::<Message>(&line).unwrap();
        }
    }

    #[test]
    fn deep_values_are_replaced_and_what_is_kept_is_readable() {
        for depth in [DEPTH, DEPTH + 1, 120, 500] {
            let mut args = json!(1);
            for _ in 0..depth {
                args = json!([args]);
            }
            let mut b = Body::ItemStarted {
                item: ItemId("i".into()),
                run: RunId("r".into()),
                agent: AgentId("a".into()),
                kind: ItemKind::Tool,
                name: "t".into(),
                args: Some(args),
            };
            b.fit();
            let Body::ItemStarted { args: Some(args), .. } = &b else { panic!() };
            assert_eq!(args.get("truncated").is_some(), depth > DEPTH, "{depth}");
            let line = serde_json::to_string(&Message::Events {
                session: SessionId("s".into()),
                events: vec![Event { seq: 1, at: 1, body: b }],
            })
            .unwrap();
            serde_json::from_str::<Message>(&line).unwrap();
        }
    }

    #[test]
    fn a_full_list_of_sessions_fits_a_line() {
        let (mut title, mut workspace) = ("\0".repeat(9000), "\0".repeat(9000));
        clip(&mut title, TITLE);
        clip(&mut workspace, WORKSPACE);
        let one = SessionInfo {
            id: SessionId::new(),
            title,
            workspace: Some(workspace),
            created: u64::MAX,
            updated: u64::MAX,
            last_seq: u64::MAX,
            running: false,
        };
        assert!(encoded_len(&vec![one; MAX_LIST as usize]) + 100 < MAX_LINE);
    }
}
