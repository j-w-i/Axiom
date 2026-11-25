//! What a protocol is here: something that is told lines and commands and says what is to be sent and
//! what it means. It starts nothing, waits for nothing and holds no clock.

use std::collections::BTreeSet;

use serde_json::Value;

use crate::{
    event::{Answer, By, Done, Event, Level, Probe, Refused, Took, Why},
    frames::shown,
};

/// What comes of one thing that was heard or asked for.
#[derive(Debug, Default)]
pub(crate) struct Out {
    pub send: Vec<Value>,
    pub say: Vec<Word>,
    pub end: Option<(Why, String)>,
    pub probe: Option<Probe>,
    /// What was heard is no message of the protocol at all.
    pub bad: bool,
}

#[derive(Debug)]
pub(crate) enum Word {
    Event(Event),
    Done(Done, Keep),
}

/// What outlives a turn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Keep {
    Nothing,
    /// It was not said.
    All,
    These(BTreeSet<String>),
}

impl Out {
    pub fn event(&mut self, e: Event) {
        self.say.push(Word::Event(e));
    }
    pub fn notice(&mut self, level: Level, text: impl Into<String>) {
        self.event(Event::Notice { level, text: text.into() });
    }
    pub fn unknown(&mut self, what: &str, v: &Value) {
        self.event(Event::Unknown { what: what.to_owned(), raw: shown(v), bytes: 0 });
    }
    /// The first reason stands.
    pub fn end(&mut self, why: Why, words: impl Into<String>) {
        self.end.get_or_insert_with(|| (why, words.into()));
    }
}

pub(crate) trait Talk: Send {
    /// The first words.
    fn open(&mut self, o: &mut Out);
    fn heard(&mut self, v: Value, o: &mut Out);
    fn prompt(&mut self, text: &str, by: By, o: &mut Out) -> Result<Took, Refused>;
    fn answer(&mut self, id: &str, a: &Answer, o: &mut Out) -> Result<(), Refused>;
    /// Asks for the turn to end. Whether it could be asked.
    fn interrupt(&mut self, o: &mut Out) -> bool;
    /// The last words before its input is closed.
    fn close(&mut self, o: &mut Out);
}

pub(crate) fn text<'a>(v: &'a Value, at: &str) -> Option<&'a str> {
    v.pointer(at)?.as_str()
}

pub(crate) fn owned(v: &Value, at: &str) -> Option<String> {
    text(v, at).map(str::to_owned)
}

pub(crate) fn list<'a>(v: &'a Value, at: &str) -> &'a [Value] {
    v.pointer(at).and_then(Value::as_array).map_or(&[], Vec::as_slice)
}

/// Present and not null.
pub(crate) fn has(v: &Value, at: &str) -> bool {
    v.pointer(at).is_some_and(|x| !x.is_null())
}

const LONGEST_ID: usize = 256;

/// An id as harnesses make them. A longer one is none.
pub(crate) fn id(v: &Value, at: &str) -> Option<String> {
    let s = text(v, at)?;
    (!s.is_empty() && s.len() <= LONGEST_ID).then(|| s.to_owned())
}

/// Words of modes, options and flags with which a harness would no longer ask, or no longer be held
/// back (T34). Compared without case and punctuation. More than is needed: what is refused for
/// nothing is named, and a person chooses another.
const LOOSE: &[&str] = &[
    "yolo",
    "dangerous",
    "bypass",
    "fullaccess",
    "skippermission",
    "autoapprove",
    "fullauto",
    "alwaysapprove",
    "approveall",
    "acceptall",
    "allowall",
    "trustall",
    "noapproval",
    "nosandbox",
    "disablesandbox",
    "unrestricted",
];

pub(crate) fn loose(s: &str) -> bool {
    let s: String =
        s.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect();
    LOOSE.iter().any(|w| s.contains(w))
}

/// Whether `s` may be one argument that a program cannot take for another: a model's name, an effort,
/// a session's id.
pub(crate) fn plain(s: &str) -> bool {
    !s.is_empty()
        && s.len() <= LONGEST_ID
        && !s.starts_with('-')
        && s.chars().all(|c| {
            c.is_ascii_alphanumeric()
                || matches!(c, '-' | '_' | '.' | ':' | '/' | '@' | '[' | ']' | '+' | '=' | ',')
        })
}

/// RFC 4648. Nothing, if it is anything else.
pub(crate) fn base64(s: &str) -> Option<Vec<u8>> {
    let s = s.trim_end_matches('=').as_bytes();
    if s.len() % 4 == 1 {
        return None;
    }
    let mut out = Vec::with_capacity(s.len() / 4 * 3 + 2);
    let (mut acc, mut bits) = (0u32, 0);
    for b in s {
        let v = match b {
            b'A'..=b'Z' => b - b'A',
            b'a'..=b'z' => b - b'a' + 26,
            b'0'..=b'9' => b - b'0' + 52,
            b'+' => 62,
            b'/' => 63,
            _ => return None,
        };
        acc = acc << 6 | u32::from(v);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push((acc >> bits & 0xff) as u8);
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn what_lets_go_is_known_by_its_words() {
        for s in [
            "--yolo",
            "bypassPermissions",
            "--dangerously-skip-permissions",
            "--dangerously-bypass-approvals-and-sandbox",
            "danger-full-access",
            "dangerFullAccess",
            "--allow-all-tools",
            "--always-approve",
            "full-auto",
            "Auto Approve",
            "trust_all_tools",
            "--no-sandbox",
        ] {
            assert!(loose(s), "{s}");
        }
        for s in ["default", "acceptEdits", "plan", "manual", "read-only", "workspace-write", "ask"]
        {
            assert!(!loose(s), "{s}");
        }
    }

    #[test]
    fn a_value_is_never_a_flag() {
        for s in ["opus", "claude-opus-5-5", "a/b:c@1", "opus[1m]", "thr_1"] {
            assert!(plain(s), "{s}");
        }
        for s in ["", "-x", "--yolo", "a b", "a\nb", "a\"b", "é", "a;b", "a\0"] {
            assert!(!plain(s), "{s:?}");
        }
        assert!(plain(&"x".repeat(256)) && !plain(&"x".repeat(257)));
    }

    #[test]
    fn an_id_has_a_length() {
        let v = json!({"a": "x", "b": "", "c": 1, "d": "y".repeat(257), "e": "y".repeat(256)});
        assert_eq!(id(&v, "/a").as_deref(), Some("x"));
        assert!(["/b", "/c", "/d", "/f"].iter().all(|k| id(&v, k).is_none()));
        assert!(id(&v, "/e").is_some());
    }

    #[test]
    fn base64_is_read_whole_or_not() {
        for (s, b) in [
            ("", &b""[..]),
            ("Zg==", b"f"),
            ("Zg", b"f"),
            ("Zm8=", b"fo"),
            ("Zm9v", b"foo"),
            ("+/+/", &[0xfb, 0xff, 0xbf]),
        ] {
            assert_eq!(base64(s).unwrap(), b, "{s}");
        }
        for s in ["Z", "Zm9vZ", "Zm 9", "Zm9v\n", "-_-_", "Z=m9"] {
            assert!(base64(s).is_none(), "{s:?}");
        }
    }
}
