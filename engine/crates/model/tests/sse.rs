//! The SSE decoder against a plain reading of the standard, whatever way the bytes are cut.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_model::sse::{Decoder, Event, SseError};
use proptest::prelude::*;

fn run(limit: usize, pieces: &[&[u8]]) -> (Vec<Event>, Option<SseError>) {
    let (mut d, mut out) = (Decoder::new(limit), Vec::new());
    for p in pieces {
        if let Err(e) = d.push(p, &mut out) {
            return (out, Some(e));
        }
    }
    (out, None)
}
fn all(text: &str) -> Vec<(String, String)> {
    let (events, error) = run(1 << 16, &[text.as_bytes()]);
    assert_eq!(error, None);
    events.into_iter().map(|e| (e.name, e.data)).collect()
}
fn one(name: &str, data: &str) -> Vec<(String, String)> {
    vec![(name.into(), data.into())]
}

#[test]
fn the_standard() {
    assert_eq!(all("data: a\n\n"), one("", "a"));
    assert_eq!(all("data:a\r\n\r\n"), one("", "a"));
    assert_eq!(all("data: a\r\r"), one("", "a"));
    assert_eq!(all("data:  a \n\n"), one("", " a "));
    assert_eq!(all("event: e\ndata: a\ndata: b\n\n"), one("e", "a\nb"));
    assert_eq!(all("data: a\nevent: e\nevent: f\n\n"), one("f", "a"));
    assert_eq!(all("data\n\n"), one("", ""));
    assert_eq!(all("data:\ndata:\n\n"), one("", "\n"));
    assert_eq!(all("data: a:b: c\n\n"), one("", "a:b: c"));
    assert_eq!(all(": a comment\n:\ndata: a\n\n"), one("", "a"));
    assert_eq!(all("id: 1\nretry: 5\nfoo: bar\nfoo\ndata: a\n\n"), one("", "a"));
    assert_eq!(all("\n\n\r\n"), vec![]);
    // Neither without data, nor carried over to the next.
    assert_eq!(all("event: e\n\ndata: a\n\n"), one("", "a"));
    assert_eq!(all("event: e\ndata: a\n\ndata: b\n\n")[1], ("".into(), "b".into()));
    assert_eq!(all("Data: a\n data: a\n\n"), vec![]);
}

#[test]
fn what_has_not_ended_is_not_an_event() {
    assert_eq!(all("data: a"), vec![]);
    assert_eq!(all("data: a\n"), vec![]);
    assert_eq!(all("data: a\n\ndata: b\n"), one("", "a"));
}

#[test]
fn one_byte_order_mark_at_the_start_is_not_text() {
    assert_eq!(all("\u{feff}data: a\n\n"), one("", "a"));
    assert_eq!(all("\u{feff}\u{feff}data: a\n\n"), vec![]);
    assert_eq!(all("data: \u{feff}\n\n"), one("", "\u{feff}"));
    assert_eq!(all("\n\u{feff}data: a\n\n"), vec![]);
}

#[test]
fn invalid_utf8_is_an_error_and_stays_one() {
    let (events, e) = run(99, &[b"data: a\n\ndata: \xff\n\ndata: b\n\n"]);
    assert_eq!((events.len(), e), (1, Some(SseError::NotUtf8)));
    let (mut d, mut out) = (Decoder::new(99), Vec::new());
    assert!(d.push(b"data: \xc3\n\n", &mut out).is_err());
    assert_eq!(d.push(b"data: a\n\n", &mut out), Err(SseError::NotUtf8));
    assert!(out.is_empty());
    // A character cut in two by a chunk is none.
    assert_eq!(run(99, &[b"data: \xc3", b"\xa9\n\n"]).0[0].data, "é");
}

#[test]
fn an_event_is_bounded_in_all_its_parts() {
    let full = |n: usize, text: &str| run(n, &[text.as_bytes()]).1;
    // The line counts with its field name: "data: abc" is 9.
    assert_eq!(full(9, "data: abc\n\n"), None);
    assert_eq!(full(8, "data: abc\n\n"), Some(SseError::TooLarge(8)));
    assert_eq!(full(8, "data: abc"), Some(SseError::TooLarge(8)));
    // Two lines: what is kept of the first (3) and the second (9).
    assert_eq!(full(12, "data: abc\ndata: abc\n\n"), None);
    assert_eq!(full(11, "data: abc\ndata: abc\n\n"), Some(SseError::TooLarge(11)));
    assert_eq!(full(11, "event: abc\ndata: abc\n\n"), Some(SseError::TooLarge(11)));
    assert_eq!(full(8, ": a comment without an end"), Some(SseError::TooLarge(8)));
    // Not across events.
    assert_eq!(full(9, &"data: abc\n\n".repeat(100)), None);
    assert_eq!(full(9, &": comment\n".repeat(100)), None);
}

/// What the standard says of the whole, written for reading.
fn plain(bytes: &[u8]) -> Vec<Event> {
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let Some((ended, _)) = text.rsplit_once('\n') else { return Vec::new() };
    let (mut out, mut name, mut data) = (Vec::new(), String::new(), Vec::new());
    for line in ended.split('\n') {
        if line.is_empty() {
            if !data.is_empty() {
                out.push(Event { name: name.clone(), data: data.join("\n") });
            }
            (name, data) = (String::new(), Vec::new());
            continue;
        }
        let (field, value) = match line.find(':') {
            Some(at) => (&line[..at], line[at + 1..].strip_prefix(' ').unwrap_or(&line[at + 1..])),
            None => (line, ""),
        };
        match field {
            "event" => name = value.to_owned(),
            "data" => data.push(value.to_owned()),
            _ => {}
        }
    }
    out
}

fn stream() -> impl Strategy<Value = Vec<u8>> {
    let token = prop_oneof![
        4 => Just("\n"), 3 => Just("\r\n"), 2 => Just("\r"), 3 => Just("data: "), 2 => Just("data:"),
        1 => Just("data"), 2 => Just("event: "), 1 => Just("event"), 1 => Just("id: 1"),
        1 => Just(":"), 1 => Just(" "), 2 => Just("x"), 1 => Just("é😀"), 1 => Just("\u{feff}"),
        1 => Just("\0"),
    ];
    proptest::collection::vec(token, 0..60).prop_map(|t| t.concat().into_bytes())
}
fn cut<'a>(bytes: &'a [u8], cuts: &[prop::sample::Index]) -> Vec<&'a [u8]> {
    let mut at: Vec<usize> = cuts.iter().map(|c| c.index(bytes.len() + 1)).collect();
    at.sort_unstable();
    at.push(bytes.len());
    let mut from = 0;
    at.into_iter().map(|to| &bytes[std::mem::replace(&mut from, to)..to]).collect()
}

proptest! {
    #[test]
    fn every_cut_gives_what_the_standard_says(
        bytes in stream(),
        cuts in proptest::collection::vec(any::<prop::sample::Index>(), 0..20),
    ) {
        let (events, error) = run(1 << 16, &cut(&bytes, &cuts));
        prop_assert_eq!(error, None);
        prop_assert_eq!(&events, &plain(&bytes));
        let singles: Vec<&[u8]> = bytes.chunks(1).collect();
        prop_assert_eq!(run(1 << 16, &singles).0, events);
    }

    #[test]
    fn every_cut_gives_the_same_error_at_the_same_place(
        mut bytes in stream(),
        bad in proptest::option::of(any::<prop::sample::Index>()),
        limit in 4usize..40,
        cuts in proptest::collection::vec(any::<prop::sample::Index>(), 0..20),
    ) {
        if let Some(at) = bad {
            bytes.insert(at.index(bytes.len() + 1), 0xff);
        }
        prop_assert_eq!(run(limit, &cut(&bytes, &cuts)), run(limit, &[&bytes]));
    }
}
