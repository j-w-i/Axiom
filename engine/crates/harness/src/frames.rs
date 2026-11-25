//! What was said on the wire, kept so that a failed attempt can be looked at.

use std::{collections::VecDeque, future::Future};

use serde_json::Value;

/// Kept of one frame.
pub const MOST: usize = 16 * 1024;
/// Kept of each in the last ones that end an attempt, and how many.
const LAST: usize = 2 * 1024;
const LASTS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Way {
    /// From the harness.
    In,
    /// To it.
    Out,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    pub way: Way,
    /// Milliseconds since 1970.
    pub at: u64,
    /// The whole line's length.
    pub bytes: u64,
    /// Its beginning, at most [`MOST`] bytes, without what signs anyone in.
    pub text: String,
}

/// Nothing can be put on record any more.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lost;

/// Where frames are put on record. Each is kept before anything is made of it; an attempt whose
/// frames cannot be kept ends.
pub trait Frames: Send + Sync + 'static {
    fn keep(&self, f: Frame) -> impl Future<Output = Result<(), Lost>> + Send;
}

/// The beginning of `s`, and how many bytes were left out.
pub(crate) fn cut(s: &str, most: usize) -> (&str, u64) {
    let mut end = most.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    (&s[..end], (s.len() - end) as u64)
}

pub(crate) fn frame(way: Way, line: &[u8], read: Option<&Value>) -> Frame {
    let text = match read {
        Some(v) if holds_secret(v) => shown(v),
        // Cut before it is made text: it may be very long.
        _ => String::from_utf8_lossy(&line[..line.len().min(MOST)]).into_owned(),
    };
    let bytes = line.len() as u64;
    Frame { way, at: axiom_proto::now_ms(), bytes, text: cut(&text, MOST).0.to_owned() }
}

/// As text, without what signs anyone in.
pub(crate) fn shown(v: &Value) -> String {
    let mut v = v.clone();
    hide(&mut v);
    v.to_string()
}

/// Names under which a harness may tell what signs a person in, or who they are. Adapters neither
/// ask for these nor use them.
const SECRET: &[&str] = &[
    "accesstoken",
    "refreshtoken",
    "idtoken",
    "authtoken",
    "apikey",
    "authorization",
    "password",
    "secret",
    "clientsecret",
    "cookie",
    "authurl",
    "verificationurl",
    "usercode",
    "email",
    "account",
];

fn secret(key: &str, v: &Value) -> bool {
    if v.is_null() || v.is_boolean() || v.is_number() {
        return false;
    }
    let k: String =
        key.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect();
    SECRET.contains(&k.as_str())
}

fn holds_secret(v: &Value) -> bool {
    match v {
        Value::Object(m) => m.iter().any(|(k, v)| secret(k, v) || holds_secret(v)),
        Value::Array(a) => a.iter().any(holds_secret),
        _ => false,
    }
}

fn hide(v: &mut Value) {
    match v {
        Value::Object(m) => {
            for (k, v) in m {
                if secret(k, v) {
                    *v = Value::String("(kept back)".into());
                } else {
                    hide(v);
                }
            }
        }
        Value::Array(a) => a.iter_mut().for_each(hide),
        _ => {}
    }
}

/// The last frames.
#[derive(Debug, Default)]
pub(crate) struct Tail(VecDeque<Frame>);

impl Tail {
    pub fn add(&mut self, f: &Frame) {
        if self.0.len() == LASTS {
            self.0.pop_front();
        }
        self.0.push_back(Frame { text: cut(&f.text, LAST).0.to_owned(), ..f.clone() });
    }
    pub fn take(&mut self) -> Vec<Frame> {
        self.0.drain(..).collect()
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;
    use serde_json::json;

    #[test]
    fn a_frame_is_cut_between_letters_and_says_how_long_it_was() {
        let line = "é".repeat(MOST);
        let f = frame(Way::In, line.as_bytes(), None);
        assert!(f.text.len() <= MOST && f.bytes == 2 * MOST as u64);
        assert_eq!(cut("aéb", 2), ("a", 3));
        assert_eq!(cut("aéb", 3), ("aé", 1));
        assert_eq!(cut("aéb", 9), ("aéb", 0));
        assert_eq!(cut("", 0), ("", 0));
        // Not text: kept as well as it can be.
        assert_eq!(frame(Way::Out, b"a\xffb", None).text, "a\u{fffd}b");
    }

    #[test]
    fn what_signs_anyone_in_is_not_kept() {
        let v = json!({"result": {"account": {"email": "me@x.y"},
            "list": [{"access_token": "SECRET", "n": 1}], "apiKey": "SECRET",
            "Set": {"Authorization": ["SECRET"]}, "model": "m", "password": null, "secret": true}});
        let f = frame(Way::In, v.to_string().as_bytes(), Some(&v));
        assert!(!f.text.contains("SECRET") && !f.text.contains("me@x.y"), "{}", f.text);
        assert!(f.text.contains("\"model\":\"m\"") && f.text.contains("\"n\":1"));
        assert_eq!(f.text.matches("(kept back)").count(), 4);
        assert_eq!(f.bytes, v.to_string().len() as u64);
        let v = json!({"a": [[{"password": "SECRET"}]]});
        assert!(!frame(Way::In, b"SECRET", Some(&v)).text.contains("SECRET"));
        // What holds none is kept as it came.
        let line = br#"{ "a" :1 , "tokens" : "x" }"#;
        let v: Value = serde_json::from_slice(line).unwrap();
        assert_eq!(frame(Way::In, line, Some(&v)).text.as_bytes(), line);
    }

    #[test]
    fn the_last_frames_are_few_and_short() {
        let mut t = Tail::default();
        for n in 0..100 {
            t.add(&Frame { way: Way::In, at: n, bytes: 9, text: "x".repeat(MOST) });
        }
        let last = t.take();
        assert_eq!(last.len(), LASTS);
        assert_eq!((last[0].at, last[LASTS - 1].at), (100 - LASTS as u64, 99));
        assert!(last.iter().all(|f| f.text.len() == LAST && f.bytes == 9));
    }
}
