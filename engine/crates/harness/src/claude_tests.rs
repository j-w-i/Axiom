#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::{
    Harness,
    test::{Desk, place, start},
};

type D = Desk<Claude>;

fn desk_with(p: Prompts, s: Start) -> D {
    let l = launch(p, &s).unwrap();
    D::new(Claude::new(p, s, &l))
}
fn desk() -> D {
    let mut s = start(Access::Ask);
    s.resume = Some("s1".into());
    desk_with(Prompts::Stdio, s)
}
fn begun() -> Value {
    json!({"type": "control_response", "response": {"subtype": "success", "request_id": "axiom-1", "response": {"models": []}, "pending_permission_requests": []}})
}
fn ready() -> D {
    let mut d = desk();
    d.hear(begun());
    assert_eq!(d.told(), ["ready s1 claude stream-json"]);
    d.sent();
    d
}
fn at_work() -> D {
    let mut d = ready();
    d.prompt("go").unwrap();
    d.told();
    d
}
fn init(mode: &str) -> Value {
    json!({"type": "system", "subtype": "init", "session_id": "s1", "permissionMode": mode, "capabilities": ["interrupt_cancel_queued_v1", "x"], "tools": []})
}
fn result(more: Value) -> Value {
    let mut r = json!({"type": "result", "subtype": "success", "is_error": false, "session_id": "s1", "permission_denials": []});
    for (k, v) in more.as_object().unwrap() {
        r[k] = v.clone();
    }
    r
}
fn stream(e: Value) -> Value {
    json!({"type": "stream_event", "parent_tool_use_id": null, "event": e})
}
fn assistant(id: &str, content: Value) -> Value {
    json!({"type": "assistant", "parent_tool_use_id": null, "message": {"id": id, "content": content}})
}
fn args(p: Prompts, s: &Start) -> Vec<String> {
    launch(p, s).unwrap().args
}

#[test]
fn how_it_is_started() {
    use Prompts::*;
    let both = |p, a, words: &[&str]| {
        let all = args(p, &start(a)).join(" ");
        let want = format!(
            "-p --input-format stream-json --output-format stream-json --verbose --include-partial-messages {} --session-id ",
            words.join(" ")
        );
        assert!(all.starts_with(&want), "{all}\n{want}");
    };
    both(
        Refused,
        Access::Read,
        &[
            "--permission-mode dontAsk --permission-prompts none --tools Read,Glob,Grep --disallowedTools AskUserQuestion,mcp__*",
        ],
    );
    both(
        Stdio,
        Access::Ask,
        &[
            "--permission-mode default --permission-prompt-tool stdio --disallowedTools AskUserQuestion",
        ],
    );
    both(
        Refused,
        Access::Work,
        &[
            "--permission-mode acceptEdits --permission-prompts none --disallowedTools AskUserQuestion --settings",
            SANDBOX,
        ],
    );
    assert!(matches!(launch(Refused, &start(Access::Ask)), Err(Error::NoMode(Access::Ask))));
    let s: Value = serde_json::from_str(SANDBOX).unwrap();
    assert_eq!(
        s,
        json!({"sandbox": {"enabled": true, "failIfUnavailable": true, "allowUnsandboxedCommands": false}})
    );
    for (a, c) in [
        (Access::Read, Containment::None),
        (Access::Ask, Containment::None),
        (Access::Work, Containment::Harness),
    ] {
        assert_eq!(launch(Stdio, &start(a)).unwrap().containment, c);
    }
    let mut s = start(Access::Work);
    (s.model, s.effort, s.resume) = (Some("opus".into()), Some("high".into()), Some("old".into()));
    let all = args(Refused, &s);
    assert_eq!(all[all.len() - 6..], ["--model", "opus", "--effort", "high", "--resume", "old"]);
    assert!(
        !all.contains(&"--session-id".to_owned()) && !all.contains(&"--setting-sources".to_owned())
    );
    s.trust = Trust::Withheld;
    let all = args(Refused, &s).join(" ");
    assert!(all.contains("--setting-sources user --model"), "{all}");
}

#[test]
fn a_session_has_an_id_of_its_own() {
    let ids: std::collections::BTreeSet<_> =
        (0..50).map(|_| args(Prompts::Stdio, &start(Access::Ask)).pop().unwrap()).collect();
    assert_eq!(ids.len(), 50);
    for id in ids {
        let p: Vec<_> = id.split('-').map(str::len).collect();
        assert_eq!(p, [8, 4, 4, 4, 12]);
        assert!(id.bytes().all(|b| b == b'-' || b.is_ascii_hexdigit()));
        assert_eq!(id.as_bytes()[14], b'4');
        assert!(matches!(id.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
    }
    let s = start(Access::Ask);
    let l = launch(Prompts::Stdio, &s).unwrap();
    let mut d = D::new(Claude::new(Prompts::Stdio, s, &l));
    d.hear(begun());
    assert_eq!(d.told(), [format!("ready {} claude stream-json", l.args.last().unwrap())]);
}

#[test]
fn which_version_it_says() {
    assert_eq!(version("2.1.325 (Claude Code)\n").as_deref(), Some("2.1.325"));
    for s in [
        "",
        "Claude Code",
        "2.1",
        "2.1.x",
        "v2.1.3",
        "2.1.3.4",
        "2..3",
        "1111111111111111111.1111111111111111111111.1",
    ] {
        assert_eq!(version(s), None, "{s}");
    }
}

#[test]
fn what_it_can_do() {
    let c = caps(Prompts::Refused);
    assert!(!c.approvals && c.resume && c.interrupt && c.usage && c.quota && !c.plan);
    assert_eq!(c.access, [Access::Read, Access::Work]);
    assert_eq!((c.steer, c.models), (Steer::Yes, Names::Any));
    let c = caps(Prompts::Stdio);
    assert!(c.approvals);
    assert_eq!(c.access, [Access::Read, Access::Ask, Access::Work]);
}

#[test]
fn its_first_words() {
    let mut d = desk();
    assert_eq!(
        d.one(),
        json!({"type": "control_request", "request_id": "axiom-1", "request": {"subtype": "initialize", "hooks": null}})
    );
    assert_eq!(d.prompt("go"), Err(Refused::NotReady));
    let mut r = begun();
    r["response"]["response"] = json!({"claude_code_version": "2.1.330", "account": {"email": "x"}, "models": [
        {"value": "opus", "displayName": "Opus", "supportedEffortLevels": ["low", "high"]}, {"value": "haiku"}, {"x": 1}, {"value": "fable", "supportedEffortLevels": ["low", "max"]}]});
    d.hear(r);
    let e = d.events();
    let Event::Ready(r) = &e[0] else { panic!("{e:?}") };
    assert_eq!((r.session.as_str(), r.version.as_deref()), ("s1", Some("2.1.330")));
    let Names::Of(m) = &r.caps.models else { panic!() };
    assert_eq!((m.len(), m[0].label.as_deref(), m[0].efforts.len()), (3, Some("Opus"), 2));
    let Names::Of(e) = &r.caps.efforts else { panic!() };
    assert_eq!(e.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["low", "high", "max"]);
    let mut d = desk();
    d.hear(json!({"type": "control_response", "response": {"subtype": "error", "request_id": "axiom-1", "error": "no"}}));
    assert_eq!(d.ends(), (Why::Refused, "it would not begin: no".into()));
    let mut d = ready();
    d.hear(begun());
    assert_eq!(d.told(), ["unknown a reply to nothing"]);
    let mut s = start(Access::Work);
    s.model = Some("opus".into());
    let mut d = desk_with(Prompts::Refused, s);
    d.hear(begun());
    let e = d.events();
    let Event::Ready(r) = &e[0] else { panic!() };
    assert_eq!((r.model.as_deref(), &r.caps.models), (Some("opus"), &Names::Any));
}

#[test]
fn a_prompt_says_whose_words_it_holds() {
    let mut d = ready();
    assert_eq!(d.prompt("/go @x"), Ok(Took::Began));
    let v = d.one();
    assert_eq!(v["type"], "user");
    assert_eq!(v["message"], json!({"role": "user", "content": "/go @x"}));
    assert_eq!(v["session_id"], "s1");
    assert_eq!(v["parent_tool_use_id"], Value::Null);
    assert_eq!((&v["client_composed"], &v["origin"]), (&json!(true), &json!({"kind": "human"})));
    assert!(v.get("isSynthetic").is_none() && v.get("priority").is_none());
    assert_eq!(v["uuid"].as_str().unwrap().len(), 36);
    assert_eq!(d.prompt_by("more", By::Agent), Ok(Took::Steered));
    let v = d.one();
    assert_eq!((&v["isSynthetic"], &v["priority"]), (&json!(true), &json!("next")));
    assert!(v.get("origin").is_none());
    assert_eq!(v["client_composed"], true);
    assert_eq!(d.told(), ["began"]);
}

#[test]
fn it_runs_in_the_mode_it_was_told() {
    for (m, ok) in [
        ("default", true),
        ("manual", true),
        ("acceptEdits", false),
        ("bypassPermissions", false),
        ("auto", false),
        ("plan", false),
    ] {
        let mut d = at_work();
        d.hear(init(m));
        assert_eq!(d.end.is_none(), ok, "{m}");
        if !ok {
            assert_eq!(d.ends().0, Why::Boundary);
        }
    }
    let mut d = at_work();
    d.hear(json!({"type": "system", "subtype": "init", "session_id": "s1"}));
    assert_eq!(d.ends().0, Why::Boundary);
    let mut d = desk_with(Prompts::Refused, start(Access::Work));
    d.hear(init("manual"));
    assert_eq!(d.ends().0, Why::Boundary);
    let mut d = desk_with(Prompts::Refused, start(Access::Read));
    d.hear(init("dontAsk"));
    assert!(d.end.is_none());
    // Later it may change, and is told of, or ended.
    let status = |m: Value| json!({"type": "system", "subtype": "status", "status": null, "permissionMode": m});
    let mut d = at_work();
    d.hear(status(json!("default")));
    d.hear(status(json!(null)));
    assert!(d.told().is_empty());
    d.hear(status(json!("acceptEdits")));
    assert_eq!(d.told(), ["notice Warning: Claude Code switched to its mode acceptEdits"]);
    d.hear(status(json!("bypassPermissions")));
    assert_eq!(d.ends().0, Why::Boundary);
}

#[test]
fn a_turn_and_its_end() {
    let mut d = at_work();
    d.hear(result(json!({})));
    assert_eq!(d.told(), ["done Completed success"]);
    d.hear(result(json!({})));
    assert!(d.told().is_empty());
    for (r, want) in [
        (json!({"terminal_reason": "completed"}), "done Completed completed"),
        (
            json!({"subtype": "error_max_turns", "is_error": true, "errors": ["a", "b"]}),
            "done Failed error_max_turns (a; b)",
        ),
        (
            json!({"subtype": "error_during_execution"}),
            "done Failed error_during_execution (it failed: error_during_execution)",
        ),
        (
            json!({"is_error": true, "result": "Not logged in"}),
            "done Failed success (Not logged in)",
        ),
    ] {
        let mut d = at_work();
        d.hear(result(r));
        assert_eq!(d.told(), [want]);
    }
    let mut d = at_work();
    d.hear(json!({"type": "assistant", "parent_tool_use_id": null, "error": "rate_limit", "message": {"id": "m", "content": []}}));
    d.hear(result(json!({"is_error": true, "result": ""})));
    assert_eq!(
        d.told(),
        ["notice Error: The model's answer failed: rate_limit", "done Failed success (rate_limit)"]
    );
    // It is forgotten with the next.
    d.prompt("x").unwrap();
    d.told();
    d.hear(result(json!({"is_error": true})));
    assert_eq!(d.told(), ["done Failed success (it failed: success)"]);
    // A sub-agent's is not the turn's.
    let mut d = at_work();
    d.hear(json!({"type": "assistant", "parent_tool_use_id": "t", "error": "overloaded", "message": {"id": "m", "content": []}}));
    d.told();
    d.hear(result(json!({"is_error": true})));
    assert_eq!(d.told(), ["done Failed success (it failed: success)"]);
}

#[test]
fn a_result_is_for_the_prompts_it_names() {
    let mut d = ready();
    d.prompt("one").unwrap();
    d.prompt("two").unwrap();
    let ids: Vec<Value> = d.sent().iter().map(|v| v["uuid"].clone()).collect();
    d.told();
    // Of a task in the background; of nobody's prompt.
    d.hear(result(json!({"origin": {"kind": "task-notification"}})));
    d.hear(result(json!({"user_message_uuids": ["other"]})));
    assert!(d.told().is_empty());
    d.hear(result(json!({"user_message_uuids": [ids[0]], "subtype": "error_during_execution", "errors": ["half"]})));
    assert_eq!(d.told(), ["notice Error: half"], "the other goes on");
    assert_eq!(d.prompt("three"), Ok(Took::Steered));
    let third = d.one()["uuid"].clone();
    d.hear(result(json!({"user_message_uuids": [ids[1], third]})));
    assert_eq!(d.told(), ["done Completed success"]);
    d.prompt("four").unwrap();
    let fourth = d.one()["uuid"].clone();
    d.hear(result(json!({"user_message_uuid": fourth})));
    assert_eq!(d.told(), ["began", "done Completed success"]);
    // One that names none answers all.
    d.prompt("a").unwrap();
    d.prompt("b").unwrap();
    d.hear(result(json!({"origin": {"kind": "human"}})));
    assert_eq!(d.told(), ["began", "done Completed success"]);
}

#[test]
fn what_it_does_by_itself_is_a_turn() {
    let mut d = ready();
    d.hear(json!({"type": "assistant", "parent_tool_use_id": "t", "message": {"id": "m", "content": []}}));
    assert!(d.told().is_empty(), "a sub-agent's work is not");
    d.hear(assistant("m", json!([{"type": "text", "text": "done"}])));
    d.hear(result(json!({"origin": {"kind": "task-notification"}})));
    assert_eq!(
        d.told(),
        ["began by agent", "say m:0: done (whole)", "said m:0", "done Completed success"]
    );
    d.hear(stream(json!({"type": "message_start", "message": {"id": "n"}})));
    assert_eq!(d.told(), ["began by agent"]);
}

#[test]
fn stopping_a_turn() {
    let mut d = at_work();
    d.sent();
    assert!(d.interrupt());
    assert_eq!(d.one()["request"], json!({"subtype": "interrupt"}));
    d.hear(json!({"type": "control_response", "response": {"subtype": "success", "request_id": "axiom-2", "response": {"still_queued": []}}}));
    d.hear(result(json!({"subtype": "error_during_execution", "errors": ["aborted"], "terminal_reason": "aborted_tools", "user_message_uuids": ["x"]})));
    assert_eq!(d.told(), ["done Cancelled aborted_tools"]);
    // The next is not stopped.
    d.prompt("x").unwrap();
    d.hear(result(json!({})));
    assert_eq!(d.told(), ["began", "done Completed success"]);
    // What waits is stopped with it, where it can do that.
    let mut d = at_work();
    d.hear(init("default"));
    d.prompt("more").unwrap();
    d.sent();
    d.interrupt();
    assert_eq!(d.one()["request"], json!({"subtype": "interrupt", "cancel_queued": true}));
    d.hear(result(json!({"user_message_uuids": ["x"]})));
    assert_eq!(d.told(), ["done Cancelled success"]);
}

#[test]
fn texts() {
    let delta =
        |i: u64, d: Value| stream(json!({"type": "content_block_delta", "index": i, "delta": d}));
    let mut d = at_work();
    for v in [
        stream(json!({"type": "message_start", "message": {"id": "m"}})),
        stream(
            json!({"type": "content_block_start", "index": 0, "content_block": {"type": "thinking"}}),
        ),
        delta(0, json!({"type": "thinking_delta", "thinking": "h"})),
        delta(0, json!({"type": "thinking_delta", "thinking": "m"})),
        delta(0, json!({"type": "signature_delta", "signature": "x"})),
        assistant("m", json!([{"type": "thinking", "thinking": "hm", "signature": "x"}])),
        delta(1, json!({"type": "text_delta", "text": "Lo"})),
        delta(1, json!({"type": "text_delta", "text": "ok"})),
        assistant("m", json!([{"type": "text", "text": "Look"}])),
        delta(2, json!({"type": "input_json_delta", "partial_json": "{"})),
        assistant(
            "m",
            json!([{"type": "tool_use", "id": "t", "name": "Bash", "input": {"command": "ls"}}]),
        ),
        stream(json!({"type": "message_stop"})),
        delta(3, json!({"type": "text_delta", "text": "of nothing"})),
        stream(json!({"type": "ping"})),
    ] {
        d.hear(v);
    }
    assert_eq!(
        d.told(),
        [
            "think m:0: h",
            "think m:0: m",
            "said m:0",
            "say m:1: Lo",
            "say m:1: ok",
            "said m:1",
            "tool t Bash {\"command\":\"ls\"}",
        ]
    );
    // Those of sub-agents come whole, and say whose they are.
    let mut d = at_work();
    let mut v = assistant(
        "k",
        json!([{"type": "text", "text": "sub"}, {"type": "redacted_thinking", "data": "x"}, {"type": "text", "text": "two"}, {"type": "odd"}]),
    );
    v["parent_tool_use_id"] = json!("t");
    d.hear(v);
    let mut v = delta(0, json!({"type": "text_delta", "text": "x"}));
    v["parent_tool_use_id"] = json!("t");
    d.hear(v);
    d.hear(json!({"type": "assistant", "message": {}}));
    assert_eq!(
        d.told(),
        [
            "say k:0 of t: sub (whole)",
            "said k:0",
            "say k:2 of t: two (whole)",
            "said k:2",
            "unknown block odd",
            "unknown a message without an id"
        ]
    );
    for n in 0..1000 {
        d.hear(assistant(&format!("m{n}"), json!([{"type": "text", "text": "x"}])));
    }
    assert!(d.talk.blocks.len() <= MESSAGES);
}

#[test]
fn tools() {
    let mut d = at_work();
    let user = |content: Value, more: Value| {
        let mut u = json!({"type": "user", "parent_tool_use_id": null, "message": {"role": "user", "content": content}});
        for (k, v) in more.as_object().unwrap() {
            u[k] = v.clone();
        }
        u
    };
    let patch = json!({"filePath": "/a", "structuredPatch": [{"oldStart": 1, "oldLines": 2, "newStart": 3, "newLines": 4, "lines": ["-x", "+y"]}]});
    for v in [
        assistant(
            "m",
            json!([{"type": "tool_use", "id": "t", "name": "Edit", "input": {}}, {"type": "tool_use", "name": "no id"}]),
        ),
        user(
            json!([{"type": "tool_result", "tool_use_id": "t", "content": "done"}]),
            json!({"tool_use_result": patch}),
        ),
        user(
            json!([{"type": "tool_result", "tool_use_id": "u", "content": [{"type": "text", "text": "a"}, {"type": "image"}, {"type": "text", "text": "b"}], "is_error": true}]),
            json!({}),
        ),
        user(
            json!([{"type": "tool_result", "tool_use_id": "w"}]),
            json!({"tool_use_result": {"filePath": "/n", "type": "create", "content": "new", "structuredPatch": []}}),
        ),
        user(json!([{"type": "tool_result", "tool_use_id": "r"}]), json!({"isReplay": true})),
        user(json!("only text"), json!({})),
        user(json!([{"type": "text", "text": "x"}, {"type": "tool_result"}]), json!({})),
        assistant(
            "n",
            json!([{"type": "server_tool_use", "id": "s", "name": "web_search", "input": {"q": 1}}, {"type": "web_search_tool_result", "tool_use_id": "s", "content": []}]),
        ),
    ] {
        d.hear(v);
    }
    assert_eq!(
        d.told(),
        [
            "tool t Edit {}",
            "unknown a tool call without an id",
            "ended t Completed \"done\" [/a Some(\"@@ -1,2 +3,4 @@\\n-x\\n+y\\n\") - -]",
            "tool u ",
            "ended u Failed \"ab\"",
            "tool w ",
            "ended w Completed [/n - - Some(\"new\")]",
            "unknown a result without its call",
            "tool s web_search {\"q\":1}",
            "ended s Completed \"\"",
        ]
    );
    let mut v =
        assistant("k", json!([{"type": "tool_use", "id": "z", "name": "Read", "input": {}}]));
    v["parent_tool_use_id"] = json!("t");
    d.hear(v);
    assert_eq!(d.told(), ["tool z Read {} of t"]);
}

fn ask(id: &str) -> Value {
    json!({"type": "control_request", "request_id": id, "request": {"subtype": "can_use_tool", "tool_name": "Bash", "input": {"command": "cargo test"},
        "tool_use_id": "toolu_1", "permission_suggestions": [{"x": 1}], "blocked_path": "/etc", "decision_reason": "why"}})
}

#[test]
fn a_question_and_its_answer() {
    let mut d = at_work();
    d.sent();
    d.hear(ask("c7"));
    d.hear(ask("c7"));
    let e = d.events();
    assert_eq!(e.len(), 1, "told again, it is the same");
    let Event::Asked(q) = &e[0] else { panic!("{e:?}") };
    assert_eq!(
        (q.title.as_str(), q.detail.as_deref(), q.tool.as_deref()),
        ("Bash", Some("why"), Some("toolu_1"))
    );
    assert_eq!(
        q.subject,
        json!({"tool": "Bash", "input": {"command": "cargo test"}, "blocked": "/etc"})
    );
    let ids: Vec<_> = q.options.iter().map(|o| (o.id.as_str(), o.kind.clone())).collect();
    assert_eq!(ids, [("allow", ChoiceKind::AllowOnce), ("deny", ChoiceKind::RejectOnce)]);
    assert!(d.sent().is_empty());
    let id = q.id.clone();
    assert_eq!(d.choose(&id, "always"), Err(Refused::NoSuchOption));
    assert_eq!(d.choose("q0", "allow"), Err(Refused::NotAsked));
    d.choose(&id, "allow").unwrap();
    assert_eq!(
        d.one(),
        json!({"type": "control_response", "response": {"subtype": "success", "request_id": "c7", "response": {"behavior": "allow", "updatedInput": {"command": "cargo test"}}}})
    );
    assert_eq!(d.choose(&id, "allow"), Err(Refused::NotAsked));
    for refuse in [true, false] {
        d.hear(ask("c8"));
        let id = format!("q{}", d.talk.made);
        if refuse {
            d.refuse(&id).unwrap()
        } else {
            d.choose(&id, "deny").unwrap()
        }
        let v = d.one();
        assert_eq!(v["response"]["response"]["behavior"], "deny");
        assert!(v["response"]["response"].get("updatedInput").is_none());
    }
    d.told();
    // A sub-agent's, with words of its own.
    let mut a = ask("c9");
    (a["request"]["agent_id"], a["request"]["title"]) = (json!("sub"), json!("Run tests"));
    d.hear(a);
    let e = d.events();
    let Event::Asked(q) = &e[0] else { panic!() };
    assert_eq!((q.by.as_deref(), q.title.as_str()), (Some("sub"), "Run tests"));
    // Taken back: nothing is sent.
    d.hear(json!({"type": "control_cancel_request", "request_id": "c0"}));
    assert!(d.told().is_empty());
    d.hear(json!({"type": "control_cancel_request", "request_id": "c9"}));
    assert_eq!(d.told(), [format!("closed {} Withdrawn", q.id)]);
    assert!(d.sent().is_empty());
}

#[test]
fn nobody_to_ask_is_no() {
    let mut d = desk_with(Prompts::Refused, start(Access::Work));
    d.hear(begun());
    d.prompt("go").unwrap();
    d.told();
    d.sent();
    d.hear(ask("c7"));
    let v = d.one();
    assert_eq!(
        (v["response"]["request_id"].as_str(), v["response"]["response"]["behavior"].as_str()),
        (Some("c7"), Some("deny"))
    );
    assert_eq!(d.told(), ["notice Warning: Not allowed, nobody can be asked: Bash"]);
}

#[test]
fn what_else_it_asks_for_is_refused() {
    let mut d = at_work();
    d.sent();
    for s in ["hook_callback", "mcp_message", "elicitation", "new"] {
        d.hear(json!({"type": "control_request", "request_id": "r", "request": {"subtype": s}}));
        let v = d.one();
        assert_eq!(
            v["response"],
            json!({"subtype": "error", "request_id": "r", "error": "not offered"}),
            "{s}"
        );
        assert_eq!(d.told(), ["unknown control_request"]);
    }
    d.hear(json!({"type": "control_request", "request": {"subtype": "can_use_tool"}}));
    assert_eq!(d.told(), ["unknown a question without an id"]);
    assert!(d.sent().is_empty());
}

#[test]
fn usage_and_limits() {
    let mut d = at_work();
    d.hear(result(json!({"total_cost_usd": 0.12, "usage": {"input_tokens": 3, "cache_read_input_tokens": 40, "cache_creation_input_tokens": 5, "output_tokens": 6,
        "output_tokens_details": {"thinking_tokens": 2}}})));
    let e = d.events();
    let Event::Usage(s) = &e[0] else { panic!("{e:?}") };
    let want = Usage {
        input: Some(3),
        cache_read: Some(40),
        cache_write: Some(5),
        output: Some(6),
        reasoning: Some(2),
        context: None,
    };
    assert_eq!((&s.usage, s.provenance), (&want, Some(Provenance::Reported)));
    assert_eq!(
        s.cost,
        Some(Cost { amount: 0.12, currency: "USD".into(), provenance: Provenance::Estimated })
    );
    d.hear(json!({"type": "rate_limit_event", "rate_limit_info": {"status": "allowed_warning", "resetsAt": 1_795_600_000, "utilization": 0.83}}));
    let e = d.events();
    let Event::Limit(l) = &e[0] else { panic!("{e:?}") };
    let want = Limit {
        name: "plan".into(),
        used: Some(0.83),
        resets_at: Some(1_795_600_000),
        minutes: None,
        status: Some("allowed_warning".into()),
    };
    assert_eq!(*l, want);
    d.hear(json!({"type": "rate_limit_event"}));
    assert_eq!(d.told(), ["unknown limit"]);
}

#[test]
fn what_is_told_beside() {
    let sys = |s: &str, more: Value| {
        let mut v = json!({"type": "system", "subtype": s});
        for (k, x) in more.as_object().unwrap() {
            v[k] = x.clone();
        }
        v
    };
    let mut d = at_work();
    for v in [
        sys("compact_boundary", json!({})),
        sys("informational", json!({"level": "warning", "content": "w"})),
        sys("informational", json!({"level": "notice", "content": "n"})),
        sys("api_retry", json!({"attempt": 2, "error": "overloaded"})),
        sys("permission_denied", json!({"tool_name": "Bash", "decision_reason": "a rule"})),
        sys("permission_denied", json!({"tool_name": "Bash", "message": "no"})),
        sys("model_refusal_fallback", json!({"fallback_model": "x"})),
        sys("new_thing", json!({})),
        json!({"type": "system"}),
        json!({"type": "brand_new", "a": 1}),
        json!({"type": "keep_alive"}),
        json!({"type": "tool_progress"}),
    ] {
        d.hear(v);
    }
    for q in QUIET {
        d.hear(sys(q, json!({})));
    }
    assert_eq!(
        d.told(),
        [
            "notice Info: The agent shortened its context",
            "notice Warning: w",
            "notice Info: n",
            "notice Warning: It tries again (attempt 2): overloaded",
            "notice Warning: Claude Code did not allow Bash: a rule",
            "notice Warning: Claude Code did not allow Bash: no",
            "notice Warning: Another model answers: x",
            "unknown system new_thing",
            "unknown system ",
            "unknown brand_new",
        ]
    );
    d.hear(json!({"a": 1}));
    d.hear(json!({"type": 3}));
    assert_eq!(d.bad, 2);
    let mut v = init("default");
    v["session_id"] = json!("s2");
    d.hear(v.clone());
    d.hear(v);
    assert_eq!(d.told(), ["session s2"]);
    d.hear(result(json!({"session_id": "s3", "permission_denials": [{}, {}]})));
    assert_eq!(
        d.told(),
        [
            "session s3",
            "notice Warning: Claude Code did not allow 2 tool calls of this turn",
            "done Completed success"
        ]
    );
}

#[test]
fn control_requests_that_nobody_answers_end_it() {
    let mut d = ready();
    for _ in 0..64 {
        d.interrupt();
    }
    assert!(d.end.is_none());
    d.interrupt();
    assert_eq!(d.ends().0, Why::Deaf);
}

#[test]
fn it_is_given_a_server_where_it_may_act() {
    let tools = Some(crate::Server {
        name: "axiom".into(),
        tool: "agent".into(),
        command: place("bin/axiom-host"),
        args: vec!["--mcp".into(), "/tmp/a file".into()],
    });
    for (access, given) in [(Access::Work, true), (Access::Read, false)] {
        let s = Start { tools: tools.clone(), ..start(access) };
        let l = Harness::Claude(Prompts::Refused).launch(&s).unwrap();
        let at = l.args.iter().position(|a| a == "--mcp-config");
        assert_eq!(at.is_some(), given, "{access:?}");
        let Some(at) = at else { continue };
        let v: Value = serde_json::from_str(&l.args[at + 1]).unwrap();
        let server = &v["mcpServers"]["axiom"];
        assert_eq!(server["args"], json!(["--mcp", "/tmp/a file"]));
        assert_eq!(server["command"], place("bin/axiom-host").as_str());
        assert!(server.get("env").is_none());
        // Its one tool, and no other.
        let allowed = l.args.iter().position(|a| a == "--allowedTools").unwrap();
        assert_eq!(l.args[allowed + 1], "mcp__axiom__agent");
    }
    // None of the folder's own comes with it.
    let s = Start { tools, trust: Trust::Withheld, ..start(Access::Work) };
    let l = Harness::Claude(Prompts::Refused).launch(&s).unwrap();
    let at = l.args.iter().position(|a| a == "--setting-sources").unwrap();
    assert_eq!(l.args[at + 1], "user");
}
