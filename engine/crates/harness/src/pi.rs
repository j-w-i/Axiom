//! Pi, by its RPC mode (`pi --mode rpc`): lines of JSON, commands with a `type` and answered by a
//! `response`, events without an id (`docs/rpc.md`, `docs/json.md` at v1.6.1, read 2026-11-27).
//!
//! It never asks before it acts and has no sandbox of its own (`docs/security.md`): it is offered only
//! to work, and said to run outside any sandbox. Left out: images, its own sessions (it is started
//! without one), forks, compaction by command, and what it uses (its figures have no documented sum).

use std::collections::BTreeSet;

use axiom_policy::{Access, Place, audit::Containment};
use axiom_proto::Outcome;
use serde_json::{Value, json};

use crate::{
    Site, Start, Trust,
    event::*,
    talk::{Keep, Out, Talk, Word, id, list, owned, plain, text},
};

pub(crate) const ARGS: [&str; 2] = ["--mode", "rpc"];
pub(crate) const PROTOCOL: &str = "pi rpc";

/// What may follow its own arguments: what Axiom gives it for an account (`given.rs`), and nothing
/// that would let it read more of the folder or anything else.
pub(crate) fn passes(extra: &[String]) -> Result<(), Error> {
    let bare = ["--no-session", "--offline", "--no-approve"];
    let valued = ["--provider", "--model", "--thinking"];
    let mut all = extra.iter();
    while let Some(a) = all.next() {
        if bare.contains(&a.as_str()) {
            continue;
        }
        if !valued.contains(&a.as_str()) {
            return Err(Error::NotPassed(a.clone()));
        }
        match all.next() {
            Some(v) if plain(v) => {}
            _ => return Err(Error::NotPassed(a.clone())),
        }
    }
    Ok(())
}

pub(crate) fn launch(s: &Start) -> Result<Launch, Error> {
    // A project's own `.pi` is read unless it is told `--no-approve`, which Axiom always is where it
    // configures it (`given.rs`).
    if s.trust != Trust::Given && !s.configured {
        return Err(Error::Untrusted);
    }
    // It cannot be told to ask, or only to look.
    if s.access != Access::Work {
        return Err(Error::NoMode(s.access));
    }
    Ok(Launch { args: ARGS.map(String::from).into(), containment: Containment::None })
}

#[derive(Debug)]
pub(crate) struct Pi {
    program: Place,
    start: Option<Start>,
    caps: Caps,
    ready: bool,
    busy: bool,
    /// Each assistant message is counted, for its items' ids.
    message: u64,
    /// Its parts that were spoken of, to be ended.
    open: BTreeSet<String>,
    error: Option<String>,
    made: u64,
}

impl Pi {
    pub(crate) fn new(site: &Site, start: Option<Start>) -> Self {
        Self {
            program: site.program.place().clone(),
            start,
            caps: Caps {
                resume: false,
                steer: Steer::Yes,
                interrupt: true,
                approvals: false,
                usage: false,
                quota: false,
                plan: false,
                access: vec![Access::Work],
                models: Names::Any,
                efforts: Names::None,
            },
            ready: false,
            busy: false,
            message: 0,
            open: BTreeSet::new(),
            error: None,
            made: 0,
        }
    }

    fn command(&mut self, mut c: Value, o: &mut Out) {
        self.made += 1;
        c["id"] = json!(format!("ax{}", self.made));
        o.send.push(c);
    }

    fn part(&self, v: &Value) -> String {
        let at = v.pointer("/assistantMessageEvent/contentIndex").and_then(Value::as_u64);
        format!("m{}.{}", self.message, at.unwrap_or(0))
    }

    fn said_all(&mut self, o: &mut Out) {
        for id in std::mem::take(&mut self.open) {
            o.event(Event::Said { id });
        }
    }

    fn settled(&mut self, v: &Value, o: &mut Out) {
        if !self.busy {
            return;
        }
        self.busy = false;
        self.said_all(o);
        let (outcome, stop) = match (v.pointer("/aborted") == Some(&json!(true)), &self.error) {
            (true, _) => (Outcome::Cancelled, "aborted"),
            (false, Some(_)) => (Outcome::Failed, "error"),
            (false, None) => (Outcome::Completed, "settled"),
        };
        let error = self.error.take().filter(|_| outcome == Outcome::Failed);
        o.say.push(Word::Done(Done { outcome, stop: stop.into(), error }, Keep::Nothing));
    }

    fn response(&mut self, v: &Value, o: &mut Out) {
        let ok = v.pointer("/success") == Some(&json!(true));
        let error = owned(v, "/error").unwrap_or_else(|| "it did not say why".into());
        match text(v, "/command") {
            Some("get_state") if !self.ready => {
                if !ok {
                    return o.end(Why::Refused, format!("it would not begin: {error}"));
                }
                self.ready = true;
                let model = owned(v, "/data/model/id");
                let Some(s) = &self.start else {
                    o.probe = Some(Probe {
                        program: self.program.clone(),
                        name: Some("Pi".into()),
                        version: None,
                        protocol: PROTOCOL.into(),
                        caps: self.caps.clone(),
                    });
                    return;
                };
                let _ = s;
                let session = id(v, "/data/sessionId").unwrap_or_else(|| "pi".into());
                o.event(Event::Ready(Box::new(Ready {
                    session,
                    protocol: PROTOCOL.into(),
                    version: None,
                    model,
                    caps: self.caps.clone(),
                })));
            }
            Some("prompt") if !ok && self.busy => {
                self.error = Some(error);
                self.settled(&json!({}), o);
            }
            Some("prompt" | "abort" | "get_state") => {}
            _ if !ok => o.notice(Level::Warning, format!("It refused: {error}")),
            _ => {}
        }
    }

    fn update(&mut self, v: &Value, o: &mut Out) {
        let e = v.pointer("/assistantMessageEvent").cloned().unwrap_or_default();
        let kind = match text(&e, "/type") {
            Some("text_delta") => Kind::Prose,
            Some("thinking_delta") => Kind::Reasoning,
            Some("error") => {
                self.error = owned(&e, "/error/errorMessage").or(owned(&e, "/errorMessage"));
                return;
            }
            _ => return,
        };
        let id = self.part(v);
        self.open.insert(id.clone());
        let text = owned(&e, "/delta").unwrap_or_default();
        o.event(Event::Say(Say { id, kind, text, whole: false, parent: None }));
    }
}

impl Talk for Pi {
    fn open(&mut self, o: &mut Out) {
        self.command(json!({"type": "get_state"}), o);
    }

    fn heard(&mut self, v: Value, o: &mut Out) {
        let Some(t) = text(&v, "/type").map(str::to_owned) else {
            o.bad = true;
            return;
        };
        match t.as_str() {
            "response" => self.response(&v, o),
            "agent_start" if !self.busy => {
                (self.busy, self.error) = (true, None);
                o.event(Event::Began { by_agent: true });
            }
            "message_start" if text(&v, "/message/role") == Some("assistant") => self.message += 1,
            "message_update" => self.update(&v, o),
            "message_end" if text(&v, "/message/role") == Some("assistant") => {
                if matches!(text(&v, "/message/stopReason"), Some("error")) {
                    let why = owned(&v, "/message/errorMessage");
                    self.error =
                        Some(why.unwrap_or_else(|| "it failed and did not say how".into()));
                }
                self.said_all(o);
            }
            "tool_execution_start" => {
                let Some(id) = id(&v, "/toolCallId") else { return o.unknown(&t, &v) };
                let name = owned(&v, "/toolName").unwrap_or_default();
                let args = v.pointer("/args").filter(|a| !a.is_null()).cloned();
                let tool = Tool { id, name, args, ..Tool::default() };
                o.event(Event::ToolStarted(Box::new(tool)));
            }
            "tool_execution_end" => {
                let Some(id) = id(&v, "/toolCallId") else { return o.unknown(&t, &v) };
                let failed = v.pointer("/isError") == Some(&json!(true));
                let output: String = list(&v, "/result/content")
                    .iter()
                    .filter_map(|c| text(c, "/text"))
                    .collect::<Vec<_>>()
                    .join("\n");
                let (outcome, error) = match failed {
                    true => (Outcome::Failed, Some("it failed".to_owned())),
                    false => (Outcome::Completed, None),
                };
                let output = Some(output).filter(|s| !s.is_empty());
                let end = ToolEnd { id, outcome, output, output_cut: 0, diffs: vec![], error };
                o.event(Event::ToolEnded(Box::new(end)));
            }
            "agent_settled" => self.settled(&v, o),
            "auto_retry_start" => o.notice(Level::Warning, "It tries again"),
            "compaction_end" => o.notice(Level::Info, "The agent shortened its context"),
            "extension_error" => {
                o.notice(Level::Warning, text(&v, "/error").unwrap_or("an extension failed"));
            }
            m if QUIET.contains(&m) => {}
            m => o.unknown(m, &v),
        }
    }

    fn prompt(&mut self, text: &str, _: By, o: &mut Out) -> Result<Took, Refused> {
        if !self.ready || self.start.is_none() {
            return Err(Refused::NotReady);
        }
        if self.busy {
            self.command(
                json!({"type": "prompt", "message": text, "streamingBehavior": "steer"}),
                o,
            );
            return Ok(Took::Steered);
        }
        (self.busy, self.error) = (true, None);
        o.event(Event::Began { by_agent: false });
        self.command(json!({"type": "prompt", "message": text}), o);
        Ok(Took::Began)
    }

    fn answer(&mut self, _: &str, _: &Answer, _: &mut Out) -> Result<(), Refused> {
        // It never asks.
        Err(Refused::NotAsked)
    }

    fn interrupt(&mut self, o: &mut Out) -> bool {
        if self.busy {
            // What waits to be said after is not said either.
            self.command(json!({"type": "clear_queue"}), o);
            self.command(json!({"type": "abort"}), o);
        }
        true
    }

    fn close(&mut self, _: &mut Out) {}
}

/// Of its events, nothing for a transcript.
const QUIET: &[&str] = &[
    "agent_start",
    "agent_end",
    "turn_start",
    "turn_end",
    "message_start",
    "message_end",
    "tool_execution_update",
    "queue_update",
    "compaction_start",
    "auto_retry_end",
    "session_info_changed",
    "thinking_level_changed",
    "bash_execution_update",
];

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::test::{site, start};

    fn pi(s: Option<Start>) -> (Pi, Out) {
        let mut p = Pi::new(&site(), s);
        let mut o = Out::default();
        p.open(&mut o);
        (p, o)
    }
    fn hear(p: &mut Pi, v: Value) -> Out {
        let mut o = Out::default();
        p.heard(v, &mut o);
        o
    }
    fn events(o: &Out) -> Vec<&Event> {
        o.say
            .iter()
            .filter_map(|w| match w {
                Word::Event(e) => Some(e),
                Word::Done(..) => None,
            })
            .collect()
    }
    fn done(o: &Out) -> Option<&Done> {
        o.say.iter().find_map(|w| match w {
            Word::Done(d, _) => Some(d),
            Word::Event(_) => None,
        })
    }
    fn ready() -> (Pi, Out) {
        let (mut p, o) = pi(Some(start(Access::Work)));
        assert_eq!(o.send, [json!({"type": "get_state", "id": "ax1"})]);
        let o = hear(
            &mut p,
            json!({"type": "response", "id": "ax1", "command": "get_state", "success": true, "data": {"sessionId": "s9", "model": {"id": "m-1"}}}),
        );
        (p, o)
    }

    #[test]
    fn it_is_asked_how_it_stands_then_given_work_and_says_what_it_does() {
        let (mut p, o) = ready();
        let Some(Event::Ready(r)) = events(&o).first().copied() else { panic!("{o:?}") };
        assert_eq!((r.session.as_str(), r.model.as_deref()), ("s9", Some("m-1")));
        let mut o = Out::default();
        assert_eq!(p.prompt("go", By::Person, &mut o), Ok(Took::Began));
        assert_eq!(o.send, [json!({"type": "prompt", "message": "go", "id": "ax2"})]);
        hear(&mut p, json!({"type": "agent_start"}));
        hear(&mut p, json!({"type": "message_start", "message": {"role": "assistant"}}));
        let o = hear(
            &mut p,
            json!({"type": "message_update", "assistantMessageEvent": {"type": "text_delta", "contentIndex": 0, "delta": "Hel"}}),
        );
        assert_eq!(
            events(&o),
            [&Event::Say(Say {
                id: "m1.0".into(),
                kind: Kind::Prose,
                text: "Hel".into(),
                whole: false,
                parent: None
            })]
        );
        let o = hear(
            &mut p,
            json!({"type": "tool_execution_start", "toolCallId": "t1", "toolName": "bash", "args": {"command": "ls"}}),
        );
        let Event::ToolStarted(t) = events(&o)[0] else { panic!() };
        assert_eq!((t.name.as_str(), t.args.clone()), ("bash", Some(json!({"command": "ls"}))));
        let o = hear(
            &mut p,
            json!({"type": "tool_execution_end", "toolCallId": "t1", "toolName": "bash", "result": {"content": [{"type": "text", "text": "a\nb"}]}, "isError": false}),
        );
        let Event::ToolEnded(e) = events(&o)[0] else { panic!() };
        assert_eq!((e.outcome, e.output.as_deref()), (Outcome::Completed, Some("a\nb")));
        let o = hear(
            &mut p,
            json!({"type": "message_end", "message": {"role": "assistant", "stopReason": "stop"}}),
        );
        assert_eq!(events(&o), [&Event::Said { id: "m1.0".into() }]);
        let o = hear(&mut p, json!({"type": "agent_settled", "aborted": false}));
        assert_eq!(done(&o).map(|d| d.outcome), Some(Outcome::Completed));
        // Said again while it works: it is heard in the turn.
        let mut o = Out::default();
        p.prompt("one", By::Person, &mut o).unwrap();
        hear(&mut p, json!({"type": "agent_start"}));
        let mut o2 = Out::default();
        assert_eq!(p.prompt("more", By::Person, &mut o2), Ok(Took::Steered));
        assert_eq!(o2.send[0]["streamingBehavior"], "steer");
        drop(o);
    }

    #[test]
    fn what_fails_is_said_and_a_stop_is_a_stop() {
        let (mut p, _) = ready();
        p.prompt("go", By::Person, &mut Out::default()).unwrap();
        hear(&mut p, json!({"type": "message_start", "message": {"role": "assistant"}}));
        hear(
            &mut p,
            json!({"type": "message_end", "message": {"role": "assistant", "stopReason": "error", "errorMessage": "no credit"}}),
        );
        let o = hear(&mut p, json!({"type": "agent_settled", "aborted": false}));
        let d = done(&o).unwrap();
        assert_eq!((d.outcome, d.error.as_deref()), (Outcome::Failed, Some("no credit")));
        p.prompt("again", By::Person, &mut Out::default()).unwrap();
        let mut o = Out::default();
        assert!(p.interrupt(&mut o));
        let types: Vec<_> = o.send.iter().map(|c| c["type"].as_str().unwrap()).collect();
        assert_eq!(types, ["clear_queue", "abort"]);
        let o = hear(&mut p, json!({"type": "agent_settled", "aborted": true}));
        assert_eq!(done(&o).map(|d| d.outcome), Some(Outcome::Cancelled));
        // A prompt it would not take.
        p.prompt("x", By::Person, &mut Out::default()).unwrap();
        let o = hear(
            &mut p,
            json!({"type": "response", "command": "prompt", "success": false, "error": "no model"}),
        );
        assert_eq!(done(&o).and_then(|d| d.error.clone()).as_deref(), Some("no model"));
        assert_eq!(p.answer("q", &Answer::Refuse, &mut Out::default()), Err(Refused::NotAsked));
    }

    #[test]
    fn it_only_works_and_reads_a_project_only_where_that_is_allowed() {
        for a in [Access::Read, Access::Ask] {
            assert!(matches!(launch(&start(a)), Err(Error::NoMode(x)) if x == a));
        }
        let untrusted = Start { trust: Trust::Withheld, ..start(Access::Work) };
        assert!(matches!(launch(&untrusted), Err(Error::Untrusted)));
        let told = Start { configured: true, ..untrusted };
        let l = launch(&told).unwrap();
        assert_eq!(
            (l.args, l.containment),
            (vec!["--mode".to_owned(), "rpc".into()], Containment::None)
        );
        let given = [
            "--no-session",
            "--offline",
            "--no-approve",
            "--provider",
            "anthropic",
            "--model",
            "m-1",
        ];
        assert!(passes(&given.map(String::from)).is_ok());
        for bad in
            [&["--api-key", "x"][..], &["--provider"], &["--model", "-x"], &["--extension", "a.js"]]
        {
            assert!(
                passes(&bad.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>()).is_err(),
                "{bad:?}"
            );
        }
    }

    #[test]
    fn without_work_it_is_only_looked_at() {
        let (mut p, _) = pi(None);
        let o = hear(
            &mut p,
            json!({"type": "response", "command": "get_state", "success": true, "data": {}}),
        );
        assert_eq!(o.probe.map(|p| p.protocol), Some(PROTOCOL.into()));
        assert_eq!(p.prompt("go", By::Person, &mut Out::default()), Err(Refused::NotReady));
        let o = hear(&mut p, json!({"no": "type"}));
        assert!(o.bad);
    }
}
