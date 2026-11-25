//! How long things may be. Everything is measured as it is encoded, because that is what a line
//! holds: one NUL is six bytes of JSON. Bodies are cut before they are journalled, so a line over
//! [`crate::MAX_LINE`] cannot come about later.

use serde::Serialize;
use serde_json::{Value, json};

use crate::{Answerable, Body, Execution, UsedOn};

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
/// Of an agent's list of things to do: how many, and each.
pub const STEPS: usize = 100;
const STEP: usize = 500;
/// Of an agent's plan in words.
const PLAN: usize = 16 * 1024;
/// Models of one report.
pub const TOTALS: usize = 64;
/// What a request is about, and why a run failed.
pub const LONG: usize = 64 * 1024;

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

/// Takes away what is too long, and says how long it was: 0 if it stays.
fn cut_value(v: &mut Option<Value>, cut: &mut u64) {
    if let Some(v) = v
        && let n = encoded_len(v)
        && (n > VALUE || deeper(v, DEPTH))
    {
        *cut = n as u64;
    }
    if *cut > 0 {
        *v = None;
    }
}

fn deeper(v: &Value, than: usize) -> bool {
    match v {
        Value::Array(a) => than == 0 || a.iter().any(|v| deeper(v, than - 1)),
        Value::Object(o) => than == 0 || o.values().any(|v| deeper(v, than - 1)),
        _ => false,
    }
}

/// A person cannot decide on what they are not shown (SI-15).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("a request's {what} has {bytes} bytes; with more than {max} it cannot be asked")]
pub struct TooLong {
    pub what: &'static str,
    pub bytes: usize,
    pub max: usize,
}

impl Execution {
    fn fit(&mut self) {
        let Execution {
            harness,
            provider,
            model,
            connection,
            effort,
            environment,
            thinking: _,
            speed,
        } = self;
        clip(harness, NAME);
        for s in [provider, model, connection, effort, environment, speed] {
            clip_opt(s, NAME);
        }
    }
}

impl Body {
    /// Refuses what must not be cut: the text of a request that can be answered.
    pub fn check(&self) -> Result<(), TooLong> {
        let Body::RequestOpened { what, subject, scope, answerable, .. } = self else {
            return Ok(());
        };
        if *answerable == Answerable::No {
            return Ok(());
        }
        for (what, s, max) in
            [("action", what, NAME), ("subject", subject, LONG), ("scope", scope, SCOPE)]
        {
            let bytes = text_len(s);
            if bytes > max {
                return Err(TooLong { what, bytes, max });
            }
        }
        Ok(())
    }

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
                execution.fit();
            }
            Body::AgentChanged { execution, .. } => execution.fit(),
            Body::RunEnded { error, .. } => clip_opt(error, LONG),
            Body::ItemStarted { name, args, args_cut, .. } => {
                clip(name, NAME);
                cut_value(args, args_cut);
            }
            Body::ItemDelta { text, .. } => clip(text, TEXT),
            Body::ItemEnded { text, result, result_cut, .. } => {
                clip(text, TEXT);
                cut_value(result, result_cut);
            }
            // One that can be answered is whole, or refused: see [`Body::check`].
            Body::RequestOpened { what, subject, scope, answerable: Answerable::No, .. } => {
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
            Body::Usage { on, cost, totals, .. } => {
                if let Some(on) = on {
                    let UsedOn { harness, lane: _, provider, connection, model, answered, speed } =
                        on.as_mut();
                    clip(harness, NAME);
                    for s in [provider, connection, model, answered, speed] {
                        clip_opt(s, NAME);
                    }
                }
                if let Some(c) = cost {
                    clip(&mut c.currency, NAME);
                }
                totals.truncate(TOTALS);
                for t in totals {
                    clip_opt(&mut t.model, NAME);
                    if let Some(c) = &mut t.cost {
                        clip(&mut c.currency, NAME);
                    }
                }
            }
            Body::Todo { steps, text, .. } => {
                steps.truncate(STEPS);
                for s in steps {
                    clip(&mut s.text, STEP);
                }
                clip_opt(text, PLAN);
            }
            Body::RequestOpened { .. } | Body::AgentState { .. } | Body::RunStarted { .. } => {}
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
        // In lists. What is made here is held whole: 5000 of `nul` are more than a computer has.
        let long = || "\0".repeat(8 << 10);
        let wide = || Some(json!([nul()]));
        let id = |p: &str| p.to_owned();
        let cost = |currency| Cost {
            amount: f64::MAX,
            currency,
            provenance: Provenance::Estimated,
            of: Covers::Session,
        };
        let (a, r, i, q) = (AgentId(id("a")), RunId(id("r")), ItemId(id("i")), RequestId(id("q")));
        vec![
            Body::SessionCreated { title: nul(), workspace: some() },
            Body::SessionTitled { title: nul() },
            Body::AgentAdded {
                agent: a.clone(),
                name: nul(),
                execution: Box::new(Execution {
                    harness: nul(),
                    provider: some(),
                    model: some(),
                    connection: some(),
                    effort: some(),
                    environment: some(),
                    ..Execution::default()
                }),
                parent: None,
                access: None,
                apart: false,
                from: None,
                plan: false,
            },
            Body::AgentChanged {
                agent: a.clone(),
                execution: Box::new(Execution {
                    harness: nul(),
                    provider: some(),
                    model: some(),
                    connection: some(),
                    effort: some(),
                    environment: some(),
                    ..Execution::default()
                }),
                access: None,
                plan: false,
            },
            Body::RunEnded { run: r.clone(), outcome: Outcome::Failed, error: some() },
            Body::ItemStarted {
                item: i.clone(),
                run: r,
                agent: a.clone(),
                kind: ItemKind::Tool,
                name: nul(),
                args: wide(),
                args_cut: 0,
            },
            Body::ItemDelta { item: i.clone(), text: nul() },
            Body::ItemEnded {
                item: i,
                outcome: Outcome::Failed,
                text: nul(),
                result: wide(),
                result_cut: 0,
            },
            Body::RequestOpened {
                request: q.clone(),
                agent: a,
                what: nul(),
                subject: nul(),
                scope: nul(),
                answerable: Answerable::No,
                digest: String::new(),
                item: None,
                session: false,
            },
            Body::RequestClosed { request: q, answer: nul() },
            Body::Usage {
                agent: AgentId(id("a")),
                usage: Usage::default(),
                provenance: Provenance::Reported,
                of: Covers::Turn,
                run: None,
                on: Some(Box::new(UsedOn {
                    harness: nul(),
                    lane: Lane::Program,
                    provider: some(),
                    connection: some(),
                    model: some(),
                    answered: some(),
                    speed: some(),
                })),
                cost: Some(Box::new(cost(nul()))),
                totals: vec![
                    ModelTotal {
                        model: Some(long()),
                        usage: Usage::default(),
                        cost: Some(cost(long()))
                    };
                    5000
                ],
                cut: true,
                aside: true,
            },
            Body::Todo {
                agent: AgentId(id("a")),
                run: None,
                steps: vec![Todo { text: long(), state: TodoState::Doing }; 5000],
                text: some(),
            },
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
                args_cut: 0,
            };
            b.fit();
            let Body::ItemStarted { args, args_cut, .. } = &b else { panic!() };
            assert_eq!(args.is_none(), depth > DEPTH, "{depth}");
            assert_eq!(*args_cut, if depth > DEPTH { 2 * depth as u64 + 1 } else { 0 });
            let line = serde_json::to_string(&Message::Events {
                session: SessionId("s".into()),
                events: vec![Event { seq: 1, at: 1, body: b }],
            })
            .unwrap();
            serde_json::from_str::<Message>(&line).unwrap();
        }
    }

    #[test]
    fn a_value_is_whole_or_gone_and_then_its_length_is_kept() {
        let end = |text: usize, result: Option<Value>| {
            let mut b = Body::ItemEnded {
                item: ItemId("i".into()),
                outcome: Outcome::Completed,
                text: "x".repeat(text),
                result,
                result_cut: 0,
            };
            b.fit();
            // Fitting again changes nothing.
            let once = b.clone();
            b.fit();
            assert_eq!(b, once);
            serde_json::to_value(b).unwrap()
        };
        // What a tool may really have said is not taken for a mark of ours.
        let own = json!({ "truncated": 5 });
        assert_eq!(end(1, Some(own.clone()))["result"], own);
        assert!(end(1, Some(own)).get("result_cut").is_none());
        let fits = json!("x".repeat(VALUE - 2));
        assert_eq!(end(1, Some(fits.clone()))["result"], fits);
        let cut = end(1, Some(json!("x".repeat(VALUE - 1))));
        assert!(cut.get("result").is_none());
        assert_eq!(cut["result_cut"], VALUE + 1);
    }

    #[test]
    fn what_can_be_answered_is_never_cut() {
        let ask = |what: usize, subject: usize, scope: usize, answerable| Body::RequestOpened {
            request: RequestId("q".into()),
            agent: AgentId("a".into()),
            what: "w".repeat(what),
            subject: "s".repeat(subject),
            scope: "c".repeat(scope),
            answerable,
            digest: String::new(),
            item: None,
            session: false,
        };
        use Answerable::{Live, Message, No, Unknown};
        for answerable in [Live, Message, Unknown] {
            assert_eq!(ask(NAME, LONG, SCOPE, answerable).check(), Ok(()));
            for (b, what) in [
                (ask(NAME + 1, 1, 1, answerable), "action"),
                (ask(1, LONG + 1, 1, answerable), "subject"),
                (ask(1, 1, SCOPE + 1, answerable), "scope"),
            ] {
                assert_eq!(b.check().unwrap_err().what, what);
                let mut same = b.clone();
                same.fit();
                assert_eq!(same, b, "refused, so there is nothing to cut");
            }
        }
        let mut told = ask(NAME + 1, LONG + 1, SCOPE + 1, No);
        assert_eq!(told.check(), Ok(()));
        told.fit();
        let Body::RequestOpened { subject, .. } = told else { panic!() };
        assert!(subject.ends_with("more bytes were not kept]"));
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
