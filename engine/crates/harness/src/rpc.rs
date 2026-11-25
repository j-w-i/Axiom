//! JSON-RPC, as two of the protocols have it: with its version on every message (ACP) or without
//! (Codex).

use std::collections::BTreeMap;

use serde_json::{Map, Value, json};

use crate::{event::Why, talk::Out};

pub(crate) const NO_METHOD: i64 = -32601;

#[derive(Debug, PartialEq)]
pub(crate) enum Msg {
    Call { id: Value, method: String, params: Value },
    Note { method: String, params: Value },
    Reply { id: Value, result: Result<Value, Fault> },
    Bad,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Fault {
    pub code: i64,
    pub message: String,
    pub data: Value,
}

pub(crate) fn read(v: Value) -> Msg {
    let Value::Object(mut m) = v else { return Msg::Bad };
    let method = m.get("method").map(|x| x.as_str().map(str::to_owned));
    let id = m.remove("id");
    match (method, id) {
        (Some(Some(method)), None) => Msg::Note { method, params: params(&mut m) },
        (Some(Some(method)), Some(id)) if id.is_string() || id.is_u64() || id.is_i64() => {
            Msg::Call { id, method, params: params(&mut m) }
        }
        (None, Some(id)) => {
            if let Some(e) = m.remove("error") {
                let code = e.get("code").and_then(Value::as_i64).unwrap_or(0);
                let message = e.get("message").and_then(Value::as_str).unwrap_or("no words");
                let message = crate::frames::cut(message, 8 * 1024).0.to_owned();
                let data = e.get("data").cloned().unwrap_or_default();
                return Msg::Reply { id, result: Err(Fault { code, message, data }) };
            }
            match m.remove("result") {
                Some(r) => Msg::Reply { id, result: Ok(r) },
                None => Msg::Bad,
            }
        }
        _ => Msg::Bad,
    }
}

fn params(m: &mut Map<String, Value>) -> Value {
    m.remove("params").unwrap_or(Value::Null)
}

/// Calls that have no reply yet, each with what it was for.
#[derive(Debug)]
pub(crate) struct Calls<W> {
    version: bool,
    next: u64,
    open: BTreeMap<u64, W>,
}

/// More would mean that nobody answers.
const MOST: usize = 256;

impl<W> Calls<W> {
    pub fn new(version: bool) -> Self {
        Self { version, next: 0, open: BTreeMap::new() }
    }

    fn send(&self, mut m: Value, o: &mut Out) {
        if self.version {
            m["jsonrpc"] = json!("2.0");
        }
        o.send.push(m);
    }

    pub fn call(&mut self, method: &str, params: Value, what: W, o: &mut Out) -> u64 {
        let id = self.next;
        self.next += 1;
        if self.open.len() >= MOST {
            o.end(Why::Deaf, "it does not answer what it is asked");
            return id;
        }
        self.open.insert(id, what);
        self.send(json!({"id": id, "method": method, "params": params}), o);
        id
    }

    pub fn note(&self, method: &str, params: Value, o: &mut Out) {
        self.send(json!({"method": method, "params": params}), o);
    }

    pub fn reply(&self, id: &Value, result: Value, o: &mut Out) {
        self.send(json!({"id": id, "result": result}), o);
    }

    pub fn refuse(&self, id: &Value, code: i64, message: &str, o: &mut Out) {
        self.send(json!({"id": id, "error": {"code": code, "message": message}}), o);
    }

    /// What the call was for, if it waits for this reply.
    pub fn answered(&mut self, id: &Value) -> Option<W> {
        self.open.remove(&id.as_u64()?)
    }

    pub fn waits(&self, f: impl Fn(&W) -> bool) -> bool {
        self.open.values().any(f)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn what_a_message_is() {
        assert_eq!(
            read(json!({"id": 1, "method": "a", "params": [2]})),
            Msg::Call { id: json!(1), method: "a".into(), params: json!([2]) }
        );
        assert_eq!(
            read(json!({"jsonrpc": "2.0", "method": "a"})),
            Msg::Note { method: "a".into(), params: Value::Null }
        );
        assert_eq!(
            read(json!({"id": "x", "result": null})),
            Msg::Reply { id: json!("x"), result: Ok(Value::Null) }
        );
        let e = Fault { code: -1, message: "m".into(), data: json!({"a": 1}) };
        assert_eq!(
            read(json!({"id": 1, "error": {"code": -1, "message": "m", "data": {"a": 1}}})),
            Msg::Reply { id: json!(1), result: Err(e) }
        );
        // An error without a shape is still an error.
        assert!(matches!(
            read(json!({"id": 1, "error": "no", "result": 1})),
            Msg::Reply { result: Err(_), .. }
        ));
        for bad in [
            json!([]),
            json!([{"id": 1, "method": "a"}]),
            json!({}),
            json!({"id": 1}),
            json!({"id": null, "method": "a"}),
            json!({"id": 1.5, "method": "a"}),
            json!({"id": {}, "method": "a"}),
            json!({"id": 1, "method": 2}),
            json!({"result": 1}),
            json!("a"),
        ] {
            assert_eq!(read(bad.clone()), Msg::Bad, "{bad}");
        }
    }

    #[test]
    fn a_reply_is_for_one_call_once() {
        let mut o = Out::default();
        let mut c = Calls::new(true);
        c.call("a", json!({}), 'a', &mut o);
        c.call("b", json!({}), 'b', &mut o);
        assert_eq!(o.send[1], json!({"jsonrpc": "2.0", "id": 1, "method": "b", "params": {}}));
        assert!(c.waits(|w| *w == 'a') && !c.waits(|w| *w == 'x'));
        // In any order; not twice; not for another.
        assert_eq!(c.answered(&json!(1)), Some('b'));
        assert_eq!(c.answered(&json!(1)), None);
        assert_eq!(c.answered(&json!(7)), None);
        assert_eq!(c.answered(&json!("0")), None);
        assert_eq!(c.answered(&json!(null)), None);
        assert_eq!(c.answered(&json!(0)), Some('a'));
        let mut o = Out::default();
        Calls::<()>::new(false).reply(&json!("z"), json!(1), &mut o);
        Calls::<()>::new(false).refuse(&json!(3), -5, "no", &mut o);
        Calls::<()>::new(true).note("n", json!(1), &mut o);
        assert_eq!(o.send[0], json!({"id": "z", "result": 1}));
        assert_eq!(o.send[1], json!({"id": 3, "error": {"code": -5, "message": "no"}}));
        assert_eq!(o.send[2], json!({"jsonrpc": "2.0", "method": "n", "params": 1}));
    }

    #[test]
    fn calls_nobody_answers_end_it() {
        let mut o = Out::default();
        let mut c = Calls::new(false);
        for _ in 0..MOST {
            c.call("a", json!({}), (), &mut o);
        }
        assert!(o.end.is_none() && o.send.len() == MOST);
        c.call("a", json!({}), (), &mut o);
        assert_eq!(o.end.map(|e| e.0), Some(Why::Deaf));
        assert_eq!(o.send.len(), MOST);
    }
}
