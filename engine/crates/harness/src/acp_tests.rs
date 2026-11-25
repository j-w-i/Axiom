#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::test::{Desk, site, start};

type D = Desk<Client>;

fn agent() -> Acp {
    Acp { args: vec!["acp".into()], ..Acp::default() }
}
fn desk() -> D {
    D::new(agent().talk(&site(), Some(start(Access::Ask))))
}
fn reply(id: u64, result: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": result})
}
fn fault(id: u64, code: i64, data: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": "words", "data": data}})
}
fn update(u: Value) -> Value {
    update_of("s1", u)
}
fn update_of(s: &str, u: Value) -> Value {
    json!({"jsonrpc": "2.0", "method": "session/update", "params": {"sessionId": s, "update": u}})
}
fn hello(v: u64) -> Value {
    match v {
        1 => {
            json!({"protocolVersion": 1, "agentCapabilities": {}, "agentInfo": {"name": "a", "version": "0.3"}})
        }
        _ => {
            json!({"protocolVersion": 2, "capabilities": {"session": {}}, "info": {"name": "a", "title": "An A", "version": "0.3"}})
        }
    }
}
/// In a session of version `v`, with `more` in its answer.
fn opened(v: u64, more: Value) -> D {
    let mut d = desk();
    open(&mut d, v, more);
    d
}
fn open(d: &mut D, v: u64, mut more: Value) {
    d.hear(reply(0, hello(v)));
    more["sessionId"] = json!("s1");
    d.hear(reply(1, more));
    d.sent();
}
fn ready(v: u64) -> D {
    let mut d = opened(v, json!({}));
    assert_eq!(d.told(), [format!("ready s1 acp/{v}")]);
    d
}
fn at_work(v: u64) -> D {
    let mut d = ready(v);
    d.prompt("go").unwrap();
    d.sent();
    d.told();
    d
}
fn text(t: &str) -> Value {
    json!({"type": "text", "text": t})
}

#[test]
fn version_2_is_asked_for_and_what_is_chosen_is_spoken() {
    let mut d = desk();
    let v = d.one();
    assert_eq!(v["method"], "initialize");
    assert_eq!(v["jsonrpc"], "2.0");
    let p = &v["params"];
    assert_eq!(p["protocolVersion"], 2);
    assert_eq!(p["info"]["name"], "axiom");
    // It has neither files nor a terminal to offer.
    assert_eq!(p["capabilities"], json!({}));
    d.hear(reply(0, hello(1)));
    let v = d.one();
    assert_eq!(v["method"], "session/new");
    assert_eq!(v["params"]["mcpServers"], json!([]));
    assert_eq!(v["params"]["cwd"], crate::test::place("w").as_str());
    d.hear(reply(1, json!({"sessionId": "s1"})));
    let e = d.events();
    let Event::Ready(r) = &e[0] else { panic!("{e:?}") };
    assert_eq!((r.protocol.as_str(), r.version.as_deref()), ("acp/1", Some("0.3")));
    assert_eq!(r.caps.steer, Steer::No);
    assert!(!r.caps.resume && r.caps.interrupt);
    assert_eq!(r.caps.access, [Access::Ask]);
    assert_eq!(r.caps.models, Names::None);
    let mut d = desk();
    d.hear(reply(0, hello(2)));
    d.hear(reply(1, json!({"sessionId": "s1"})));
    let e = d.events();
    let Event::Ready(r) = &e[0] else { panic!("{e:?}") };
    assert_eq!(r.protocol, "acp/2");
    assert!(r.caps.resume && r.caps.steer == Steer::AgentDecides);
}

#[test]
fn a_server_is_given_in_the_form_that_is_settled() {
    for (v, given) in [(1, true), (2, false)] {
        let tools = Some(crate::Server {
            name: "axiom".into(),
            tool: "agent".into(),
            command: crate::test::place("bin/axiom-host"),
            args: vec!["--mcp".into(), "/f".into()],
        });
        let mut d = D::new(agent().talk(&site(), Some(Start { tools, ..start(Access::Ask) })));
        d.sent();
        d.hear(reply(0, hello(v)));
        let want = match given {
            true => {
                json!([{"name": "axiom", "command": crate::test::place("bin/axiom-host").as_str(),
                "args": ["--mcp", "/f"], "env": []}])
            }
            false => json!([]),
        };
        assert_eq!(d.one()["params"]["mcpServers"], want, "{v}");
    }
}

#[test]
fn an_agent_that_does_not_know_version_2_is_asked_again() {
    // An old one that trips over the words, and a new one that only has another version.
    for (code, data) in
        [(-32602, json!(null)), (-32603, json!(null)), (-32022, json!({"supported": [1]}))]
    {
        let mut d = desk();
        d.sent();
        d.hear(fault(0, code, data));
        let v = d.one();
        assert_eq!(v["params"]["protocolVersion"], 1, "{code}");
        assert_eq!(v["params"]["clientInfo"]["name"], "axiom");
        assert_eq!(v["params"]["clientCapabilities"]["terminal"], false);
        assert_eq!(
            v["params"]["clientCapabilities"]["fs"],
            json!({"readTextFile": false, "writeTextFile": false})
        );
        d.hear(reply(1, hello(1)));
        assert_eq!(d.one()["method"], "session/new");
        // Once.
        let mut d = desk();
        d.hear(fault(0, code, json!({"supported": [1]})));
        d.hear(fault(1, -32603, json!(null)));
        assert_eq!(d.ends().0, Why::Refused);
    }
}

#[test]
fn no_version_in_common_is_the_end() {
    for data in [json!({"requested": 2, "supported": [3]}), json!({"supported": []}), json!(null)] {
        let mut d = desk();
        d.sent();
        d.hear(fault(0, -32022, data));
        assert_eq!(d.ends().0, Why::Version);
        assert!(d.sent().is_empty());
    }
    for v in [json!(3), json!(0), json!("2"), json!(null), json!(1.5)] {
        let mut d = desk();
        d.sent();
        d.hear(reply(0, json!({"protocolVersion": v})));
        assert_eq!(d.ends().0, Why::Version, "{v}");
        assert!(d.sent().is_empty(), "nothing is sent to it");
    }
    // Version 2 may come without sessions.
    let mut d = desk();
    d.hear(reply(0, json!({"protocolVersion": 2, "info": {}, "capabilities": {"session": null}})));
    assert_eq!(d.ends().0, Why::Refused);
}

#[test]
fn being_asked_what_it_can_do_starts_nothing() {
    let a = Acp { read: Some("plan".into()), ..agent() };
    let mut d = D::new(a.talk(&site(), None));
    d.sent();
    d.hear(reply(0, json!({"protocolVersion": 1, "agentCapabilities": {"loadSession": true}, "agentInfo": {"name": "a"}})));
    let p = d.probe.take().unwrap();
    assert!(d.sent().is_empty());
    assert!(p.caps.resume && p.caps.approvals);
    assert_eq!(p.caps.access, [Access::Read, Access::Ask]);
    assert_eq!((p.name.as_deref(), p.version, p.protocol.as_str()), (Some("a"), None, "acp/1"));
    let mut d = D::new(agent().talk(&site(), None));
    d.hear(reply(0, hello(2)));
    assert_eq!(d.probe.unwrap().name.as_deref(), Some("An A"));
}

#[test]
fn what_is_not_signed_in_says_so() {
    let mut d = desk();
    d.hear(reply(0, hello(1)));
    d.hear(fault(1, -32000, json!(null)));
    let (why, words) = d.ends();
    assert_eq!(why, Why::SignIn);
    assert!(words.contains("sign in"));
    let mut d = desk();
    d.hear(reply(0, hello(1)));
    d.hear(fault(1, -32603, json!(null)));
    assert_eq!(d.ends(), (Why::Refused, "it would not open the session: words".into()));
    let mut d = desk();
    d.hear(reply(0, hello(1)));
    d.hear(reply(1, json!({})));
    assert_eq!(d.ends().0, Why::Nonsense);
    // Nothing that signs in is ever sent.
    let mut d = at_work(1);
    d.hear(fault(2, -32000, json!(null)));
    assert_eq!(d.told(), [format!("done Failed error ({SIGN_IN})")]);
    assert!(d.sent().is_empty());
}

#[test]
fn going_on_with_a_session() {
    let mut s = start(Access::Ask);
    s.resume = Some("old".into());
    let caps = |c: Value| json!({"protocolVersion": 1, "agentCapabilities": c});
    let both = json!({"loadSession": true, "sessionCapabilities": {"resume": {}, "close": {}}});
    for (hello, method) in [
        (hello(2), "session/resume"),
        (caps(json!({"sessionCapabilities": {"resume": {}}})), "session/resume"),
        (caps(both), "session/resume"),
        (caps(json!({"loadSession": true})), "session/load"),
    ] {
        let mut d = D::new(agent().talk(&site(), Some(s.clone())));
        d.sent();
        d.hear(reply(0, hello));
        let v = d.one();
        assert_eq!(v["method"], method);
        assert_eq!(v["params"]["sessionId"], "old");
        assert!(v["params"].get("replayFrom").is_none());
        // The journal has what was said before.
        let m =
            json!({"sessionUpdate": "agent_message_chunk", "messageId": "m", "content": text("x")});
        d.hear(update_of("old", m.clone()));
        assert_eq!(d.told().is_empty(), method == "session/load");
        d.hear(reply(1, json!({})));
        assert!(d.told().last().unwrap().starts_with("ready old"));
        let mut m = m;
        m["messageId"] = json!("new");
        d.hear(update_of("old", m));
        assert_eq!(d.told().len(), 1);
    }
    for c in [
        json!({}),
        json!({"loadSession": false}),
        json!({"loadSession": {}, "sessionCapabilities": {"resume": null}}),
    ] {
        let mut d = D::new(agent().talk(&site(), Some(s.clone())));
        d.sent();
        d.hear(reply(0, caps(c)));
        assert_eq!(d.ends().0, Why::Refused);
        assert!(d.sent().is_empty(), "no new session in its place");
    }
}

#[test]
fn a_session_that_is_at_work_when_it_is_gone_on_with() {
    let mut s = start(Access::Ask);
    s.resume = Some("s1".into());
    let mut d = D::new(agent().talk(&site(), Some(s)));
    d.hear(reply(0, hello(2)));
    d.hear(update(json!({"sessionUpdate": "state_update", "state": "running"})));
    assert!(d.told().is_empty(), "not before it is there");
    d.hear(reply(1, json!({})));
    assert_eq!(d.told(), ["ready s1 acp/2", "began by agent"]);
}

fn select(id: &str, category: &str, current: &str, all: &[&str], v: u64) -> Value {
    let options: Vec<Value> = all.iter().map(|a| json!({"value": a, "name": a})).collect();
    let key = if v == 1 { "id" } else { "configId" };
    json!({key: id, "name": id, "category": category, "type": "select", "currentValue": current, "options": options})
}

#[test]
fn what_is_decided_is_set_before_anything_is_said() {
    for v in [1, 2] {
        let mut s = start(Access::Read);
        (s.model, s.effort) = (Some("m2".into()), Some("high".into()));
        let a = Acp { read: Some("plan".into()), ..agent() };
        let mut d = D::new(a.talk(&site(), Some(s)));
        let opts = |mode: &str, model: &str, effort: &str| {
            json!({"configOptions": [
                select("a", "mode", mode, &["ask", "plan"], v),
                select("b", "model", model, &["m1", "m2"], v),
                select("c", "thought_level", effort, &["low", "high"], v),
                {"id": "x", "configId": "x", "type": "boolean", "currentValue": true},
            ]})
        };
        open(&mut d, v, opts("ask", "m1", "low"));
        assert!(d.told().is_empty());
        assert_eq!(d.prompt("go"), Err(Refused::NotReady));
        // Sent by `open`, and seen only now.
        let set = |d: &mut D, id: u64, r: Value| {
            d.hear(reply(id, r));
            d.sent()
        };
        let all = set(&mut d, 2, opts("plan", "m1", "low"));
        assert_eq!(all[0]["method"], "session/set_config_option");
        assert_eq!(all[0]["params"]["configId"], "b");
        assert_eq!(all[0]["params"]["value"], "m2");
        assert_eq!(all[0]["params"].get("type").is_some(), v == 2);
        let all = set(&mut d, 3, opts("plan", "m2", "low"));
        assert_eq!(all[0]["params"]["configId"], "c");
        let all = set(&mut d, 4, opts("plan", "m2", "high"));
        assert!(all.is_empty());
        let e = d.events();
        let Event::Ready(r) = &e[0] else { panic!("{e:?}") };
        assert_eq!(r.model.as_deref(), Some("m2"));
        assert!(r.caps.models.has("m1") && r.caps.efforts.has("low") && !r.caps.models.has("x"));
    }
}

#[test]
fn a_mode_that_is_not_taken_is_the_end() {
    let a = Acp { read: Some("plan".into()), ..agent() };
    let make = || D::new(a.talk(&site(), Some(start(Access::Read))));
    // It has none, another, or says it took another.
    let mut d = make();
    open(&mut d, 1, json!({}));
    assert_eq!(d.ends(), (Why::Boundary, "it has no mode plan".into()));
    let mut d = make();
    open(&mut d, 2, json!({"configOptions": [select("a", "mode", "ask", &["ask"], 2)]}));
    assert_eq!(d.ends().0, Why::Boundary);
    let mut d = make();
    open(&mut d, 2, json!({"configOptions": [select("a", "mode", "ask", &["ask", "plan"], 2)]}));
    d.hear(reply(2, json!({"configOptions": [select("a", "mode", "ask", &["ask", "plan"], 2)]})));
    assert_eq!(d.ends().0, Why::Boundary);
    assert!(d.told().is_empty(), "it was never ready");
    // Without saying what it took; or not keeping it when the model is set.
    let both = |mode: &str, model: &str| json!({"configOptions": [select("a", "mode", mode, &["ask", "plan"], 2), select("b", "model", model, &["m1", "m2"], 2)]});
    let mut d = make();
    open(&mut d, 2, both("ask", "m1"));
    d.hear(reply(2, json!({})));
    assert_eq!(d.ends().0, Why::Boundary);
    let mut s = start(Access::Read);
    s.model = Some("m2".into());
    let mut d = D::new(a.talk(&site(), Some(s)));
    open(&mut d, 2, both("ask", "m1"));
    d.hear(reply(2, both("plan", "m1")));
    d.hear(reply(3, both("ask", "m2")));
    assert_eq!(d.ends(), (Why::Boundary, "it did not keep the mode plan".into()));
    assert!(d.told().is_empty());
    let mut d = make();
    open(
        &mut d,
        1,
        json!({"modes": {"currentModeId": "ask", "availableModes": [{"id": "ask"}, {"id": "plan"}]}}),
    );
    d.hear(fault(2, -32602, json!(null)));
    assert_eq!(d.ends().0, Why::Refused);
    // The older way.
    let mut d = make();
    d.hear(reply(0, hello(1)));
    d.sent();
    d.hear(reply(1, json!({"sessionId": "s1", "modes": {"currentModeId": "ask", "availableModes": [{"id": "ask"}, {"id": "plan"}]}})));
    let v = d.one();
    assert_eq!(
        (v["method"].as_str(), v["params"]["modeId"].as_str()),
        (Some("session/set_mode"), Some("plan"))
    );
    d.hear(reply(2, json!({})));
    assert_eq!(d.told(), ["ready s1 acp/1"]);
    // Nothing else may be missing either.
    for (m, e, words) in
        [(Some("m"), None, "it has no model m"), (None, Some("e"), "it has no effort e")]
    {
        let mut s = start(Access::Ask);
        (s.model, s.effort) = (m.map(String::from), e.map(String::from));
        let mut d = D::new(agent().talk(&site(), Some(s)));
        open(&mut d, 2, json!({}));
        assert_eq!(d.ends(), (Why::Refused, words.into()));
    }
}

#[test]
fn values_may_be_in_groups() {
    let mut s = start(Access::Ask);
    s.model = Some("m2".into());
    let mut d = D::new(agent().talk(&site(), Some(s)));
    d.hear(reply(0, hello(2)));
    d.sent();
    let grouped = json!({"sessionId": "s1", "configOptions": [{"configId": "b", "category": "model", "type": "select", "currentValue": "m1",
        "options": [{"groupId": "g", "name": "G", "options": [{"value": "m1", "name": "M"}, {"value": "m2", "name": "N"}]}]}]});
    d.hear(reply(1, grouped));
    assert_eq!(d.one()["params"]["value"], "m2");
}

#[test]
fn an_agent_that_asks_nobody_is_not_used() {
    for more in [
        json!({"modes": {"currentModeId": "bypassPermissions", "availableModes": []}}),
        json!({"configOptions": [select("a", "mode", "yolo", &["yolo"], 1)]}),
    ] {
        let mut d = opened(1, more);
        assert_eq!(d.ends().0, Why::Boundary);
        assert!(d.told().is_empty());
    }
    // By a switch, as the documentation's own example has one.
    let brave = |on: bool| {
        json!({"configId": "brave_mode", "name": "Brave Mode", "type": "boolean", "currentValue": on,
            "description": "Skip confirmation prompts and act autonomously"})
    };
    let mut d = opened(2, json!({"configOptions": [brave(true)]}));
    assert_eq!(d.ends(), (Why::Boundary, "it is set not to ask: Brave Mode".into()));
    assert!(d.told().is_empty());
    let mut d = opened(2, json!({"configOptions": [brave(false)]}));
    assert!(d.end.is_none());
    let mut u = json!({"sessionUpdate": "config_option_update", "configOptions": [brave(false)]});
    d.hear(update(u.clone()));
    assert!(d.end.is_none());
    u["configOptions"] = json!([brave(true)]);
    d.hear(update(u));
    assert_eq!(d.ends().0, Why::Boundary);
    // Nor one that becomes so.
    let mut d = at_work(1);
    d.hear(update(json!({"sessionUpdate": "current_mode_update", "currentModeId": "plan"})));
    assert!(d.end.is_none() && d.told().is_empty());
    d.hear(update_of(
        "child",
        json!({"sessionUpdate": "current_mode_update", "currentModeId": "yolo"}),
    ));
    assert!(d.end.is_none());
    d.hear(update(json!({"sessionUpdate": "current_mode_update", "currentModeId": "yolo"})));
    assert_eq!(d.ends().0, Why::Boundary);
    let mut d = at_work(2);
    d.hear(update(json!({"sessionUpdate": "config_option_update", "configOptions": [select("a", "mode", "auto-approve", &["auto-approve"], 2)]})));
    assert_eq!(d.ends().0, Why::Boundary);
    // One that leaves the mode it was set to is told of.
    let a = Acp { read: Some("plan".into()), ..agent() };
    let mut d = D::new(a.talk(&site(), Some(start(Access::Read))));
    open(&mut d, 1, json!({"modes": {"currentModeId": "ask", "availableModes": [{"id": "plan"}]}}));
    d.hear(reply(2, json!({})));
    d.told();
    d.hear(update(json!({"sessionUpdate": "current_mode_update", "currentModeId": "plan"})));
    assert!(d.told().is_empty());
    d.hear(update(json!({"sessionUpdate": "current_mode_update", "currentModeId": "code"})));
    assert_eq!(d.told(), ["notice Warning: The agent switched to its mode code"]);
}

#[test]
fn in_version_1_the_answer_to_the_prompt_is_the_end_of_the_turn() {
    let mut d = ready(1);
    assert_eq!(d.prompt("go"), Ok(Took::Began));
    let v = d.one();
    assert_eq!(v["method"], "session/prompt");
    assert_eq!(v["params"], json!({"sessionId": "s1", "prompt": [text("go")]}));
    assert_eq!(d.prompt("more"), Err(Refused::Busy));
    assert!(d.sent().is_empty());
    // States are of version 2.
    d.hear(update(json!({"sessionUpdate": "state_update", "state": "running"})));
    d.hear(update(
        json!({"sessionUpdate": "state_update", "state": "idle", "stopReason": "end_turn"}),
    ));
    assert_eq!(d.told(), ["began"]);
    d.hear(reply(2, json!({"stopReason": "end_turn"})));
    assert_eq!(d.told(), ["done Completed end_turn"]);
    // Told again, it is nothing.
    d.hear(reply(2, json!({"stopReason": "end_turn"})));
    assert_eq!(d.told(), ["unknown a reply to nothing"]);
    assert_eq!(d.prompt("more"), Ok(Took::Began));
}

#[test]
fn why_it_stopped() {
    for (stop, want) in [
        ("end_turn", "done Completed end_turn"),
        ("cancelled", "done Cancelled cancelled"),
        ("max_tokens", "done Failed max_tokens (it reached the limit of one answer)"),
        ("max_turn_requests", "done Failed max_turn_requests (it reached its limit of requests)"),
        ("refusal", "done Failed refusal (the agent refused to go on)"),
        ("_new", "done Unknown _new"),
    ] {
        let mut d = at_work(1);
        d.hear(reply(2, json!({"stopReason": stop})));
        assert_eq!(d.told(), [want]);
        let mut d = at_work(2);
        d.hear(update(json!({"sessionUpdate": "state_update", "state": "running"})));
        d.hear(update(
            json!({"sessionUpdate": "state_update", "state": "idle", "stopReason": stop}),
        ));
        assert_eq!(d.told(), [want]);
    }
    let mut d = at_work(1);
    d.hear(reply(2, json!({})));
    assert_eq!(d.told(), ["done Unknown "]);
    let mut d = at_work(1);
    d.hear(fault(2, -32603, json!(null)));
    assert_eq!(d.told(), ["done Failed error (words)"]);
    let mut d = at_work(1);
    d.hear(fault(2, -32800, json!(null)));
    assert_eq!(d.told(), ["done Cancelled cancelled"]);
}

#[test]
fn in_version_2_the_state_is_the_end_of_the_turn() {
    let state = |s: &str, stop: Option<&str>| {
        let mut u = json!({"sessionUpdate": "state_update", "state": s});
        if let Some(x) = stop {
            u["stopReason"] = json!(x);
        }
        update(u)
    };
    let mut d = ready(2);
    assert_eq!(d.prompt("go"), Ok(Took::Began));
    assert_eq!(d.told(), ["began"]);
    // An end of something before, told again, is not the end of this.
    d.hear(state("idle", Some("end_turn")));
    d.hear(reply(2, json!({"messageId": "u1"})));
    assert!(d.told().is_empty());
    d.hear(state("running", None));
    d.hear(state("running", None));
    d.hear(state("requires_action", None));
    assert_eq!(d.prompt("more"), Ok(Took::Left));
    assert_eq!(d.sent().len(), 2);
    assert!(d.told().is_empty());
    d.hear(reply(3, json!({"messageId": "u2"})));
    d.hear(state("unknown", None));
    d.hear(state("_dozing", None));
    assert_eq!(d.told(), ["notice Warning: The agent does not know whether it is at work"]);
    d.hear(state("idle", Some("end_turn")));
    d.hear(state("idle", Some("end_turn")));
    assert_eq!(d.told(), ["done Completed end_turn"]);
    // It goes on by itself.
    d.hear(state("requires_action", None));
    d.hear(state("idle", None));
    assert_eq!(d.told(), ["began by agent", "done Unknown "]);
    // Another session's state is not ours.
    d.hear(update_of("child", json!({"sessionUpdate": "state_update", "state": "running"})));
    assert!(d.told().is_empty());
    d.prompt("x").unwrap();
    d.hear(state("running", None));
    d.told();
    d.hear(update(json!({"sessionUpdate": "state_update", "state": "idle", "stopReason": "error", "error": {"code": -1, "message": "it broke"}})));
    assert_eq!(d.told(), ["done Failed error (it broke)"]);
    d.prompt("x").unwrap();
    d.hear(state("running", None));
    d.told();
    d.hear(state("idle", Some("error")));
    assert_eq!(d.told(), ["done Failed error (it failed and did not say how)"]);
}

#[test]
fn a_prompt_that_is_not_taken() {
    let mut d = at_work(2);
    d.hear(fault(2, -32602, json!(null)));
    assert_eq!(d.told(), ["not heard: words", "done Failed error (words)"]);
    // While something runs, only the words are lost.
    let mut d = at_work(2);
    d.hear(update(json!({"sessionUpdate": "state_update", "state": "running"})));
    d.prompt("more").unwrap();
    d.hear(fault(3, -32602, json!(null)));
    assert_eq!(d.told(), ["not heard: words"]);
    // Or while another has not been answered.
    let mut d = at_work(2);
    d.prompt("more").unwrap();
    d.hear(fault(3, -32602, json!(null)));
    assert_eq!(d.told(), ["not heard: words"]);
    d.hear(fault(2, -32800, json!(null)));
    assert_eq!(d.told(), ["not heard: words", "done Cancelled cancelled"]);
}

#[test]
fn text_belongs_to_what_it_says_it_belongs_to() {
    let chunk = |k: &str, id: Option<&str>, t: &str| {
        let mut u = json!({"sessionUpdate": k, "content": text(t)});
        if let Some(id) = id {
            u["messageId"] = json!(id);
        }
        update(u)
    };
    let mut d = at_work(1);
    for u in [
        chunk("agent_thought_chunk", None, "hm"),
        chunk("agent_thought_chunk", None, "m"),
        chunk("agent_message_chunk", None, "a"),
        chunk("agent_message_chunk", None, "b"),
        update(json!({"sessionUpdate": "tool_call", "toolCallId": "t", "title": "T"})),
        chunk("agent_message_chunk", None, "c"),
        chunk("agent_message_chunk", Some("m"), "d"),
        chunk("agent_thought_chunk", Some("m"), "e"),
        chunk("agent_message_chunk", None, "f"),
        chunk("user_message_chunk", None, "not told"),
    ] {
        d.hear(u);
    }
    assert_eq!(
        d.told(),
        [
            "think r:#1: hm",
            "think r:#1: m",
            "say p:#2: a",
            "say p:#2: b",
            "tool t  'T'",
            "say p:#3: c",
            "say p:m: d",
            "think r:m: e",
            "say p:#4: f",
        ]
    );
    let mut d = at_work(2);
    d.hear(chunk("agent_message_chunk", Some("m"), "he"));
    d.hear(update(json!({"sessionUpdate": "agent_message", "messageId": "m", "content": [text("hel"), text("lo")]})));
    d.hear(update(json!({"sessionUpdate": "agent_message", "messageId": "m"})));
    d.hear(update(
        json!({"sessionUpdate": "agent_thought", "messageId": "m", "content": [text("t")]}),
    ));
    d.hear(update(json!({"sessionUpdate": "agent_message", "messageId": "m", "content": null})));
    d.hear(update(json!({"sessionUpdate": "agent_message", "content": []})));
    d.hear(update_of(
        "child",
        json!({"sessionUpdate": "agent_message_chunk", "messageId": "m", "content": text("sub")}),
    ));
    assert_eq!(
        d.told(),
        [
            "say p:m: he",
            "say p:m: hello (whole)",
            "think r:m: t (whole)",
            "say p:m:  (whole)",
            "unknown a message without an id",
            "say child/p:m of child: sub",
        ]
    );
}

#[test]
fn other_content_is_named() {
    let mut d = at_work(1);
    let m = |c: Value| {
        update(json!({"sessionUpdate": "agent_message_chunk", "messageId": "m", "content": c}))
    };
    for c in [
        json!({"type": "image", "data": "x", "mimeType": "image/png"}),
        json!({"type": "audio"}),
        json!({"type": "resource_link", "uri": "file:///a", "name": "a"}),
        json!({"type": "resource", "resource": {"uri": "u", "text": "T"}}),
        json!({"type": "resource", "resource": {"uri": "u", "blob": "x"}}),
    ] {
        d.hear(m(c));
    }
    d.hear(m(json!({"type": "_hologram"})));
    assert_eq!(
        d.told(),
        [
            "say p:m: [image]",
            "say p:m: [audio]",
            "say p:m: [file:///a]",
            "say p:m: T",
            "say p:m: [file]"
        ]
    );
}

#[test]
fn a_tool_call_from_its_first_word_to_its_last() {
    let mut d = at_work(1);
    for u in [
        json!({"sessionUpdate": "tool_call", "toolCallId": "c", "title": "Run", "kind": "execute", "status": "pending", "rawInput": {"cmd": "ls"}}),
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "status": "in_progress", "content": [{"type": "content", "content": text("a")}]}),
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "content": null}),
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "status": "completed",
            "content": [{"type": "content", "content": text("a")}, {"type": "content", "content": text("b")}]}),
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "status": "failed"}),
    ] {
        d.hear(update(u));
    }
    assert_eq!(
        d.told(),
        [
            "tool c execute 'Run' {\"cmd\":\"ls\"}",
            "more c ",
            "output c: a (whole)",
            "more c ",
            "more c ",
            "ended c Completed \"ab\"",
        ]
    );
    for (u, want) in [
        (json!({"status": "failed", "rawOutput": "it broke"}), "ended x Failed \"it broke\""),
        (
            json!({"status": "completed", "rawOutput": {"n": 1}}),
            "ended x Completed \"{\\\"n\\\":1}\"",
        ),
        (json!({"status": "cancelled", "rawOutput": null}), "ended x Cancelled"),
        (
            json!({"status": "completed", "rawOutput": {"n": 1}, "content": [{"type": "diff", "path": "/a", "oldText": "o", "newText": "n"}]}),
            "ended x Completed [/a - Some(\"o\") Some(\"n\")]",
        ),
    ] {
        let mut d = at_work(1);
        let mut u = u;
        (u["sessionUpdate"], u["toolCallId"]) = (json!("tool_call_update"), json!("x"));
        d.hear(update(u));
        let told = d.told();
        assert!(told[0].starts_with("tool x "), "{told:?}");
        assert_eq!(told.last().unwrap(), want);
    }
    let mut d = at_work(2);
    d.hear(update(json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "name": "edit", "kind": "edit", "title": null})));
    d.hear(update(json!({"sessionUpdate": "tool_call_content_chunk", "toolCallId": "c", "content": {"type": "content", "content": text("one")}})));
    d.hear(update(json!({"sessionUpdate": "tool_call_content_chunk", "toolCallId": "c", "content": {"type": "diff",
        "changes": [{"operation": "modify", "path": "/a"}, {"operation": "move", "oldPath": "/b", "path": "/c"}], "patch": {"format": "git_patch", "text": "P"}}})));
    d.hear(update(json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "content": []})));
    d.hear(update(
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "status": "paused"}),
    ));
    d.hear(update(
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "status": "_odd"}),
    ));
    assert_eq!(
        d.told(),
        [
            "tool c edit",
            "output c: one",
            "more c  [/a Some(\"P\") - -] [/c - - -]",
            "more c ",
            "output c:  (whole)",
            "more c ",
            "more c ",
        ]
    );
    d.hear(update(json!({"sessionUpdate": "tool_call", "title": "no id"})));
    d.hear(update(json!({"sessionUpdate": "tool_call_content_chunk"})));
    assert_eq!(
        d.told(),
        ["unknown a tool call without an id", "unknown a tool call without an id"]
    );
    d.hear(update_of(
        "kid",
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "title": "x"}),
    ));
    assert_eq!(d.told(), ["tool kid/c  'x' of kid"]);
}

#[test]
fn a_terminal_belongs_to_a_tool_call() {
    let mut d = at_work(2);
    let chunk = |data: &str| {
        update(json!({"sessionUpdate": "terminal_output_chunk", "terminalId": "x", "data": data}))
    };
    let names = json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "title": "T", "content": [{"type": "terminal", "terminalId": "x"}]});
    // Before it is known whose it is: "hä", of which the last letter is cut in two.
    d.hear(chunk("aMM="));
    assert!(d.told().is_empty());
    d.hear(update(names.clone()));
    assert_eq!(d.told(), ["tool c  'T'", "output c: h"]);
    d.hear(chunk("pA=="));
    d.hear(chunk("/w=="));
    assert_eq!(d.told(), ["output c: ä", "output c: \u{fffd}"]);
    d.hear(chunk("not base64!"));
    assert_eq!(d.told(), ["unknown terminal output that cannot be read"]);
    d.hear(update(
        json!({"sessionUpdate": "terminal_update", "terminalId": "x", "output": {"data": "YWxs"}}),
    ));
    assert_eq!(d.told(), ["output c: all (whole)"]);
    // The list names only the terminal: it clears nothing.
    d.hear(update(names));
    assert_eq!(d.told(), ["more c  'T'"]);
    d.hear(update(json!({"sessionUpdate": "terminal_update", "terminalId": "x", "exitStatus": {"exitCode": 0}})));
    d.hear(update(json!({"sessionUpdate": "tool_call_update", "toolCallId": "c", "status": "completed", "content": [{"type": "terminal", "terminalId": "x"}]})));
    assert_eq!(d.told(), ["more c ", "ended c Completed"]);
    // A snapshot from before, and no end of terminals.
    let mut d = at_work(2);
    d.hear(update(
        json!({"sessionUpdate": "terminal_update", "terminalId": "x", "output": {"data": "YWxs"}}),
    ));
    d.hear(update(json!({"sessionUpdate": "tool_call_content_chunk", "toolCallId": "c", "content": {"type": "terminal", "terminalId": "x"}})));
    assert_eq!(d.told(), ["tool c ", "output c: all"]);
    for n in 0..1000 {
        d.hear(update(json!({"sessionUpdate": "terminal_output_chunk", "terminalId": format!("t{n}"), "data": "YWxs"})));
    }
    assert_eq!(d.talk.terminals.len(), MOST_TERMINALS);
    let big = "YWxs".repeat(EARLY);
    d.hear(update(
        json!({"sessionUpdate": "terminal_output_chunk", "terminalId": "t1", "data": big}),
    ));
    d.hear(update(
        json!({"sessionUpdate": "terminal_output_chunk", "terminalId": "t1", "data": big}),
    ));
    d.hear(update(
        json!({"sessionUpdate": "terminal_update", "terminalId": "t2", "output": {"data": big}}),
    ));
    assert!(d.talk.terminals.values().all(|t| t.early.len() <= EARLY));
}

#[test]
fn letters_cut_anywhere_come_whole() {
    let all = "aä€😀\u{fffd}z".as_bytes();
    for i in 0..=all.len() {
        for j in i..=all.len() {
            let mut half = Vec::new();
            let mut s = whole(&mut half, &all[..i]);
            s += &whole(&mut half, &all[i..j]);
            s += &whole(&mut half, &all[j..]);
            assert_eq!(s.as_bytes(), all);
            assert!(half.is_empty());
        }
    }
    let mut half = Vec::new();
    assert_eq!(whole(&mut half, b"a\xffb\xe2\x82"), "a\u{fffd}b");
    assert_eq!(whole(&mut half, b"\xac"), "€");
    assert_eq!(whole(&mut half, b"\xff\xff"), "\u{fffd}\u{fffd}");
}

fn ask_1(id: Value) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "method": "session/request_permission", "params": {"sessionId": "s1",
        "toolCall": {"toolCallId": "c", "title": "Run it", "rawInput": {"command": "cargo test"}},
        "options": [{"optionId": "yes", "name": "Yes", "kind": "allow_once"}, {"optionId": "ever", "name": "Ever", "kind": "allow_always"},
            {"optionId": "no", "name": "No", "kind": "reject_once"}, {"optionId": "never", "name": "Never", "kind": "reject_always"}, {"optionId": "odd", "kind": "_x"}]}})
}

#[test]
fn a_question_and_its_answer() {
    let mut d = at_work(1);
    d.hear(ask_1(json!(5)));
    let e = d.events();
    assert!(matches!(&e[0], Event::ToolStarted(t) if t.id == "c"));
    let Event::Asked(q) = &e[1] else { panic!("{e:?}") };
    assert_eq!((q.title.as_str(), q.tool.as_deref(), q.by.clone()), ("Run it", Some("c"), None));
    assert_eq!(q.subject["rawInput"]["command"], "cargo test");
    let kinds: Vec<_> = q.options.iter().map(|o| o.kind.clone()).collect();
    use ChoiceKind::*;
    assert_eq!(kinds, [AllowOnce, AllowAlways, RejectOnce, RejectAlways, Other("_x".into())]);
    assert_eq!(q.options[4].name, "odd");
    assert_eq!(d.choose("q1", "maybe"), Err(Refused::NoSuchOption));
    assert_eq!(d.choose("q2", "yes"), Err(Refused::NotAsked));
    assert!(d.sent().is_empty());
    d.choose("q1", "yes").unwrap();
    assert_eq!(
        d.one(),
        json!({"jsonrpc": "2.0", "id": 5, "result": {"outcome": {"outcome": "selected", "optionId": "yes"}}})
    );
    assert_eq!(d.choose("q1", "yes"), Err(Refused::NotAsked), "once");
    // Its ids may come again when they are free, and be words.
    for id in [json!(5), json!("a-b")] {
        d.hear(ask_1(id.clone()));
        let asked = d.told();
        let q = asked.last().unwrap().split(' ').nth(1).unwrap().to_owned();
        d.refuse(&q).unwrap();
        let v = d.one();
        assert_eq!(v["id"], id);
        assert_eq!(v["result"]["outcome"], json!({"outcome": "selected", "optionId": "no"}));
    }
}

#[test]
fn no_without_an_option_for_it() {
    let mut d = at_work(1);
    let mut a = ask_1(json!(1));
    a["params"]["options"] = json!([{"optionId": "yes", "kind": "allow_once"}, {"name": "no id"}]);
    d.hear(a);
    d.refuse("q1").unwrap();
    assert_eq!(d.one()["result"]["outcome"], json!({"outcome": "cancelled"}));
}

#[test]
fn a_question_with_an_id_that_is_taken_is_not_answered_for_the_other() {
    let mut d = at_work(1);
    d.hear(ask_1(json!(5)));
    d.told();
    d.hear(ask_1(json!(5)));
    d.hear(ask_1(json!("x".repeat(300))));
    let u = "unknown a question with an id that cannot be used";
    assert_eq!(d.told(), [u, u]);
    assert!(d.sent().is_empty());
    d.choose("q1", "no").unwrap();
    assert_eq!(d.sent().len(), 1);
}

#[test]
fn a_question_in_version_2() {
    let ask = |id: u64, subject: Value| {
        json!({"jsonrpc": "2.0", "id": id, "method": "session/request_permission", "params": {"sessionId": "s1", "title": "Run the tests?",
            "description": "They write to disk", "subject": subject, "options": [{"optionId": "y", "name": "Y", "kind": "allow_once"}, {"optionId": "n", "name": "N", "kind": "reject_once"}]}})
    };
    let mut d = at_work(2);
    d.hear(ask(
        1,
        json!({"type": "command", "command": "cargo test", "cwd": "/w", "toolCallId": "c"}),
    ));
    let e = d.events();
    let Event::Asked(q) = &e[0] else { panic!("{e:?}") };
    assert_eq!(
        (q.title.as_str(), q.detail.as_deref(), q.tool.as_deref()),
        ("Run the tests?", Some("They write to disk"), Some("c"))
    );
    assert_eq!(q.subject["command"], "cargo test");
    // Its word for no.
    d.refuse("q1").unwrap();
    assert_eq!(d.one()["result"]["outcome"], json!({"outcome": "cancelled"}));
    d.hear(ask(2, json!({"type": "tool_call", "toolCall": {"toolCallId": "t", "title": "Edit"}})));
    assert_eq!(d.told(), ["tool t  'Edit'", "asked q2 'Run the tests?' Some(\"t\") y,n"]);
    d.hear(ask(3, json!(null)));
    d.hear(ask(4, json!({"type": "_new", "x": 1})));
    assert_eq!(d.told().len(), 2);
    // A child's question says whose it is.
    let mut a = ask(5, json!(null));
    a["params"]["sessionId"] = json!("kid");
    d.hear(a);
    let e = d.events();
    assert!(matches!(&e[0], Event::Asked(q) if q.by.as_deref() == Some("kid")));
}

#[test]
fn stopping_a_turn() {
    // Version 1 wants every question answered at once.
    let mut d = at_work(1);
    d.hear(ask_1(json!(5)));
    d.told();
    assert!(d.interrupt());
    let s = d.sent();
    assert_eq!(
        s[0],
        json!({"jsonrpc": "2.0", "method": "session/cancel", "params": {"sessionId": "s1"}})
    );
    assert_eq!(s[1]["result"]["outcome"], json!({"outcome": "cancelled"}));
    assert_eq!(d.told(), ["closed q1 Withdrawn"]);
    assert_eq!(d.choose("q1", "yes"), Err(Refused::NotAsked));
    d.hear(reply(2, json!({"stopReason": "cancelled"})));
    assert_eq!(d.told(), ["ended c Cancelled", "done Cancelled cancelled"]);
    // In version 2 the agent takes back what it does not need.
    let mut d = at_work(2);
    d.hear(update(json!({"sessionUpdate": "state_update", "state": "running"})));
    d.hear(json!({"jsonrpc": "2.0", "id": 9, "method": "session/request_permission", "params": {"sessionId": "s1", "title": "t", "options": [{"optionId": "y", "kind": "allow_once"}]}}));
    d.told();
    d.interrupt();
    assert_eq!(d.sent().len(), 1);
    assert!(d.told().is_empty());
    d.hear(json!({"jsonrpc": "2.0", "method": "$/cancel_request", "params": {"requestId": 8}}));
    assert!(d.sent().is_empty());
    d.hear(json!({"jsonrpc": "2.0", "method": "$/cancel_request", "params": {"requestId": 9}}));
    assert_eq!(d.one()["result"]["outcome"], json!({"outcome": "cancelled"}));
    d.hear(update(
        json!({"sessionUpdate": "state_update", "state": "idle", "stopReason": "cancelled"}),
    ));
    assert_eq!(d.told(), ["closed q1 Withdrawn", "done Cancelled cancelled"]);
    // Before there is a session there is nothing to stop.
    assert!(!desk().interrupt());
}

#[test]
fn what_goes_on_in_the_background() {
    let mut d = at_work(2);
    d.hear(update(json!({"sessionUpdate": "state_update", "state": "running"})));
    for t in ["a", "b", "c"] {
        d.hear(update(json!({"sessionUpdate": "tool_call_update", "toolCallId": t})));
    }
    d.hear(update(json!({"sessionUpdate": "tool_call_update", "toolCallId": "b", "content": [{"type": "terminal", "terminalId": "x"}]})));
    d.told();
    d.hear(update(json!({"sessionUpdate": "state_update", "state": "idle", "stopReason": "end_turn",
        "background": [{"type": "tool_call", "toolCallId": "a"}, {"type": "terminal", "terminalId": "x"}, {"type": "_odd"}]})));
    assert_eq!(d.told(), ["ended c Unknown", "done Completed end_turn"]);
    d.hear(update(
        json!({"sessionUpdate": "tool_call_update", "toolCallId": "a", "status": "completed"}),
    ));
    assert_eq!(d.told(), ["more a ", "ended a Completed"]);
    // Not reported is not nothing.
    let mut d = at_work(2);
    d.hear(update(json!({"sessionUpdate": "state_update", "state": "running"})));
    d.hear(update(json!({"sessionUpdate": "tool_call_update", "toolCallId": "a"})));
    d.told();
    d.hear(update(
        json!({"sessionUpdate": "state_update", "state": "idle", "stopReason": "end_turn"}),
    ));
    assert_eq!(d.told(), ["done Completed end_turn"]);
}

#[test]
fn plans_usage_and_notices() {
    let mut d = at_work(2);
    for u in [
        json!({"sessionUpdate": "plan", "entries": [{"content": "a", "status": "completed"}, {"content": "b", "status": "in_progress"}, {"content": "c", "status": "pending"}, {"content": "d", "status": "cancelled"}]}),
        json!({"sessionUpdate": "plan_update", "plan": {"type": "items", "planId": "p", "entries": [{"content": "x", "status": "pending"}]}}),
        json!({"sessionUpdate": "plan_update", "plan": {"type": "markdown", "planId": "p", "content": "# P"}}),
        json!({"sessionUpdate": "plan_removed", "planId": "p"}),
        json!({"sessionUpdate": "notice", "severity": "warning", "title": "T", "description": "D"}),
        json!({"sessionUpdate": "notice", "severity": "error", "title": "E"}),
        json!({"sessionUpdate": "compaction_update", "compactionId": "x", "status": "in_progress"}),
        json!({"sessionUpdate": "compaction_update", "compactionId": "x", "status": "completed"}),
        json!({"sessionUpdate": "compaction_update", "compactionId": "x", "status": "failed"}),
    ] {
        d.hear(update(u));
    }
    assert_eq!(
        d.told(),
        [
            "plan plan [(\"a\", Done), (\"b\", Doing), (\"c\", Todo), (\"d\", Unknown)] None",
            "plan p [(\"x\", Todo)] None",
            "plan p [] Some(\"# P\")",
            "plan p [] None",
            "notice Warning: T: D",
            "notice Error: E",
            "notice Info: The agent shortened its context",
            "notice Warning: The agent could not shorten its context",
        ]
    );
    d.hear(update(json!({"sessionUpdate": "usage_update", "used": 53000, "size": 200000, "cost": {"amount": 0.045, "currency": "USD"}})));
    let e = d.events();
    let Event::Usage(s) = &e[0] else { panic!("{e:?}") };
    assert_eq!(s.context, Some((53000, 200_000)));
    assert_eq!(s.usage, Usage { context: Some(0.265), ..Usage::default() });
    assert_eq!(
        s.cost,
        Some(Cost { amount: 0.045, currency: "USD".into(), provenance: Provenance::Reported })
    );
    assert_eq!(s.provenance, Some(Provenance::Reported));
    // More than full is full; nothing of nothing is not known.
    d.hear(update(json!({"sessionUpdate": "usage_update", "used": 9, "size": 3})));
    d.hear(update(json!({"sessionUpdate": "usage_update", "used": 9, "size": 0})));
    let e = d.events();
    assert!(matches!(&e[0], Event::Usage(s) if s.usage.context == Some(1.0) && s.cost.is_none()));
    assert!(matches!(&e[1], Event::Usage(s) if s.usage.context.is_none()));
    d.hear(update(json!({"sessionUpdate": "usage_update", "used": "9"})));
    assert_eq!(d.told(), ["unknown usage"]);
    d.hear(update_of("kid", json!({"sessionUpdate": "usage_update", "used": 9, "size": 10})));
    assert!(d.told().is_empty(), "a child's context is not ours");
}

#[test]
fn what_it_does_not_know_is_kept_and_survived() {
    let mut d = at_work(2);
    for u in [
        json!({"sessionUpdate": "_holo", "x": 1}),
        json!({"sessionUpdate": 7}),
        json!({}),
        json!("x"),
    ] {
        d.hear(update(u));
    }
    d.hear(json!({"jsonrpc": "2.0", "method": "session/update", "params": null}));
    d.hear(json!({"jsonrpc": "2.0", "method": "elicitation/complete", "params": {}}));
    assert_eq!(
        d.told(),
        [
            "unknown session/update _holo",
            "unknown session/update ",
            "unknown session/update ",
            "unknown session/update ",
            "unknown session/update",
            "unknown elicitation/complete",
        ]
    );
    d.hear(json!({"jsonrpc": "2.0", "method": "new", "params": {"accessToken": "SECRET", "a": 1}}));
    let e = d.events();
    let Event::Unknown { raw, bytes, .. } = &e[0] else { panic!("{e:?}") };
    assert_eq!(raw, r#"{"a":1,"accessToken":"(kept back)"}"#);
    assert_eq!(*bytes, raw.len() as u64);
    // What was not offered is refused: files, a terminal, anything new.
    for m in [
        "fs/read_text_file",
        "fs/write_text_file",
        "terminal/create",
        "elicitation/create",
        "cursor/ask_question",
        "x",
    ] {
        d.hear(json!({"jsonrpc": "2.0", "id": 77, "method": m, "params": {"path": "/etc/passwd", "content": "x"}}));
        let v = d.one();
        assert_eq!(
            (v["id"].clone(), v["error"]["code"].as_i64()),
            (json!(77), Some(-32601)),
            "{m}"
        );
        assert_eq!(d.told(), [format!("unknown {m}")]);
    }
    for bad in [
        json!({}),
        json!({"jsonrpc": "2.0"}),
        json!({"method": 1}),
        json!({"id": null, "method": "x"}),
    ] {
        d.hear(bad);
    }
    assert_eq!(d.bad, 4);
    assert!(d.end.is_none());
    // Before it is ready, and before there is a session.
    let mut d = desk();
    d.hear(update(
        json!({"sessionUpdate": "agent_message_chunk", "messageId": "m", "content": text("x")}),
    ));
    assert!(d.told().is_empty());
}

#[test]
fn known_and_nothing_to_tell() {
    let mut d = at_work(2);
    for k in [
        "user_message",
        "available_commands_update",
        "session_info_update",
        "replay_complete",
        "compaction_summary_chunk",
        "subagent_update",
        "config_option_update",
    ] {
        d.hear(update(json!({"sessionUpdate": k, "messageId": "m", "sessionId": "kid"})));
    }
    assert!(d.told().is_empty());
}

#[test]
fn it_is_told_that_the_session_ends_where_it_wants_to_be() {
    let mut d = ready(2);
    d.close();
    assert_eq!(d.one()["method"], "session/close");
    let mut d = ready(1);
    d.close();
    assert!(d.sent().is_empty());
    let mut d = desk();
    d.hear(reply(
        0,
        json!({"protocolVersion": 1, "agentCapabilities": {"sessionCapabilities": {"close": {}}}}),
    ));
    d.hear(reply(1, json!({"sessionId": "s1"})));
    d.sent();
    d.close();
    assert_eq!(d.one()["params"], json!({"sessionId": "s1"}));
    let mut d = desk();
    d.sent();
    d.close();
    assert!(d.sent().is_empty());
}

#[test]
fn how_it_may_be_started() {
    let s = |a| start(a);
    let l = agent().launch(&s(Access::Ask)).unwrap();
    assert_eq!(l, Launch { args: vec!["acp".into()], containment: Containment::None });
    assert!(matches!(agent().launch(&s(Access::Read)), Err(Error::NoMode(Access::Read))));
    assert!(matches!(agent().launch(&s(Access::Work)), Err(Error::NoMode(Access::Work))));
    let a = Acp { read: Some("plan".into()), work: Some("yolo".into()), ..agent() };
    assert!(a.launch(&s(Access::Read)).is_ok());
    assert!(matches!(a.launch(&s(Access::Work)), Err(Error::Unsafe(m)) if m == "yolo"));
    let a = Acp { args: vec!["acp".into(), "--yolo".into()], ..Acp::default() };
    assert!(matches!(a.launch(&s(Access::Ask)), Err(Error::Unsafe(_))));
    assert!(matches!(a.checked(), Err(Error::Unsafe(_))));
    let mut u = s(Access::Ask);
    u.trust = Trust::Withheld;
    assert!(matches!(agent().launch(&u), Err(Error::Untrusted)));
    for k in KNOWN {
        assert!(k.acp().launch(&s(Access::Ask)).is_ok(), "{}", k.name);
        assert_eq!(k.person_only, k.program == "agy");
    }
}
