#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
use super::*;
use crate::test::{Desk, site, start};

type D = Desk<Codex>;

fn desk(a: Access) -> D {
    D::new(Codex::new(&site(), Some(start(a))))
}
fn note(method: &str, mut params: Value) -> Value {
    if params.get("threadId").is_none() {
        params["threadId"] = json!("th");
    }
    json!({"method": method, "params": params})
}
fn reply(id: u64, result: Value) -> Value {
    json!({"id": id, "result": result})
}
fn fault(id: u64) -> Value {
    json!({"id": id, "error": {"code": -32600, "message": "words"}})
}
fn thread(sandbox: &str) -> Value {
    json!({"thread": {"id": "th", "cliVersion": "1.2.3"}, "model": "m1", "sandbox": {"type": sandbox}, "approvalPolicy": "on-request", "approvalsReviewer": "user"})
}
fn until_thread(d: &mut D) -> Value {
    d.hear(reply(0, json!({"userAgent": "axiom/0.1 (macOS) codex/1.2.3 x", "codexHome": "/h"})));
    d.hear(reply(1, json!({"data": []})));
    d.sent().pop().unwrap()
}
fn ready() -> D {
    let mut d = desk(Access::Work);
    until_thread(&mut d);
    d.hear(reply(2, thread("workspaceWrite")));
    assert_eq!(d.told(), ["ready th codex app-server"]);
    d
}
fn at_work() -> D {
    let mut d = ready();
    d.prompt("go").unwrap();
    d.sent();
    d.hear(reply(
        3,
        json!({"turn": {"id": "t1", "status": "inProgress", "items": [], "error": null}}),
    ));
    d.told();
    d
}
fn started(item: Value) -> Value {
    note("item/started", json!({"turnId": "t1", "startedAtMs": 1, "item": item}))
}
fn completed(item: Value) -> Value {
    note("item/completed", json!({"turnId": "t1", "completedAtMs": 2, "item": item}))
}

#[test]
fn its_first_words() {
    let mut d = desk(Access::Work);
    let v = d.one();
    assert_eq!(v["method"], "initialize");
    assert!(v.get("jsonrpc").is_none());
    assert_eq!(v["params"]["clientInfo"]["name"], "axiom");
    assert_eq!(v["params"]["capabilities"], json!({"experimentalApi": false}));
    d.hear(reply(0, json!({"userAgent": "x"})));
    let all = d.sent();
    assert_eq!(all[0], json!({"method": "initialized", "params": {}}));
    assert_eq!(all[1]["method"], "model/list");
    d.hear(reply(1, json!({"data": [
        {"id": "i", "model": "m1", "displayName": "M", "defaultReasoningEffort": "high",
            "supportedReasoningEfforts": [{"reasoningEffort": "low"}, {"reasoningEffort": "high"}]},
        {"id": "m2", "defaultReasoningEffort": "max", "supportedReasoningEfforts": [{"reasoningEffort": "low"}]},
        {"x": 1}]})));
    let v = d.one();
    assert_eq!(v["method"], "thread/start");
    d.hear(reply(2, thread("workspaceWrite")));
    let e = d.events();
    let Event::Ready(r) = &e[0] else { panic!("{e:?}") };
    assert_eq!(
        (r.session.as_str(), r.model.as_deref(), r.version.as_deref()),
        ("th", Some("m1"), Some("1.2.3"))
    );
    let Names::Of(m) = &r.caps.models else { panic!() };
    assert_eq!((m[0].id.as_str(), m[0].label.as_deref(), m[0].efforts.len()), ("m1", Some("M"), 2));
    assert_eq!((m.len(), m[1].id.as_str()), (2, "m2"));
    // The one it begins with, if it is one it takes.
    assert_eq!(
        (m[0].default_effort.as_deref(), m[1].default_effort.as_deref()),
        (Some("high"), None)
    );
    let Names::Of(e) = &r.caps.efforts else { panic!() };
    assert_eq!(e.iter().map(|e| e.id.as_str()).collect::<Vec<_>>(), ["low", "high"]);
}

#[test]
fn what_it_may_do_is_said_with_every_thread() {
    for (a, sandbox, runs) in [
        (Access::Read, "read-only", "readOnly"),
        (Access::Ask, "read-only", "readOnly"),
        (Access::Work, "workspace-write", "workspaceWrite"),
    ] {
        for resume in [None, Some("old")] {
            let mut s = start(a);
            (s.model, s.resume) = (Some("m".into()), resume.map(String::from));
            let mut d = D::new(Codex::new(&site(), Some(s)));
            let v = until_thread(&mut d);
            let p = &v["params"];
            assert_eq!(
                v["method"],
                if resume.is_some() { "thread/resume" } else { "thread/start" }
            );
            assert_eq!(p["sandbox"], sandbox);
            assert_eq!(p["approvalPolicy"], "on-request");
            assert_eq!(p["approvalsReviewer"], "user");
            assert_eq!(p["model"], "m");
            assert_eq!(p["cwd"], crate::test::place("w").as_str());
            assert_eq!(p.get("threadId").and_then(Value::as_str), resume);
            assert_eq!(p.get("excludeTurns").is_some(), resume.is_some());
            d.hear(reply(2, thread(runs)));
            assert_eq!(d.told(), ["ready th codex app-server"]);
        }
    }
}

#[test]
fn what_it_runs_with_is_compared_with_what_it_was_told() {
    let change = |k: &str, v: Value| {
        let mut t = thread("workspaceWrite");
        t[k] = v;
        t
    };
    for t in [
        thread("dangerFullAccess"),
        thread("externalSandbox"),
        thread("readOnly"),
        change("sandbox", json!(null)),
        change("approvalPolicy", json!("never")),
        change("approvalPolicy", json!("untrusted")),
        change("approvalPolicy", json!(null)),
        change("approvalPolicy", json!({"granular": {}})),
        change("approvalsReviewer", json!("auto_review")),
        change("approvalsReviewer", json!("guardian_subagent")),
    ] {
        let mut d = desk(Access::Work);
        until_thread(&mut d);
        d.hear(reply(2, t.clone()));
        assert_eq!(d.ends().0, Why::Boundary, "{t}");
        assert!(d.told().is_empty(), "it was never ready");
        assert_eq!(d.prompt("go"), Err(Refused::NotReady));
    }
    // An older one does not say who reviews.
    let mut d = desk(Access::Work);
    until_thread(&mut d);
    d.hear(reply(2, change("approvalsReviewer", json!(null))));
    assert!(d.end.is_none());
    let mut d = desk(Access::Read);
    until_thread(&mut d);
    d.hear(reply(2, thread("workspaceWrite")));
    assert_eq!(d.ends().0, Why::Boundary);
}

#[test]
fn what_it_refuses_at_the_start() {
    let mut d = desk(Access::Work);
    d.hear(fault(0));
    assert_eq!(d.ends(), (Why::Refused, "it would not begin: words".into()));
    let mut d = desk(Access::Work);
    until_thread(&mut d);
    d.hear(fault(2));
    assert_eq!(d.ends(), (Why::Refused, "it would not open the thread: words".into()));
    let mut d = desk(Access::Work);
    until_thread(&mut d);
    d.hear(reply(2, json!({"thread": {}, "sandbox": {"type": "workspaceWrite"}})));
    assert_eq!(d.ends().0, Why::Nonsense);
    // Its list of models is of use, and not needed.
    let mut d = desk(Access::Work);
    d.hear(reply(0, json!({})));
    d.sent();
    d.hear(fault(1));
    assert_eq!(d.one()["method"], "thread/start");
}

#[test]
fn being_asked_what_it_can_do_starts_nothing() {
    let mut d = D::new(Codex::new(&site(), None));
    d.hear(reply(0, json!({"userAgent": "axiom/0.1 (macOS) codex/1.2.3 x"})));
    d.sent();
    d.hear(reply(1, json!({"data": [{"model": "m"}]})));
    let p = d.probe.take().unwrap();
    assert!(d.sent().is_empty());
    assert!(p.caps.models.has("m") && !p.caps.models.has("n"));
    assert!(
        p.caps.resume
            && p.caps.interrupt
            && p.caps.approvals
            && p.caps.quota
            && p.caps.usage
            && p.caps.plan
    );
    assert_eq!(p.caps.steer, Steer::Yes);
    assert_eq!(p.caps.access.len(), 3);
    assert_eq!((p.name.as_deref(), p.version.as_deref()), (Some("Codex"), Some("0.1")));
    assert_eq!(version("codex 0.12"), Some("0.12".into()));
    assert_eq!(version("codex"), None);
    assert_eq!(version("1 a.b"), None);
}

#[test]
fn a_turn() {
    let mut s = start(Access::Work);
    s.effort = Some("high".into());
    let mut d = D::new(Codex::new(&site(), Some(s)));
    until_thread(&mut d);
    d.hear(reply(2, thread("workspaceWrite")));
    d.told();
    assert_eq!(d.prompt("go"), Ok(Took::Began));
    let v = d.one();
    assert_eq!(v["method"], "turn/start");
    assert_eq!(
        v["params"],
        json!({"threadId": "th", "input": [{"type": "text", "text": "go"}], "effort": "high"})
    );
    // Which turn it is, is not known yet.
    assert_eq!(d.prompt("more"), Err(Refused::NotReady));
    d.hear(note("turn/started", json!({"turn": {"id": "t1"}})));
    assert_eq!(d.prompt("more"), Ok(Took::Steered));
    let v = d.one();
    assert_eq!(v["method"], "turn/steer");
    assert_eq!(v["params"]["expectedTurnId"], "t1");
    assert_eq!(v["params"]["input"][0]["text"], "more");
    d.hear(reply(3, json!({"turn": {"id": "t1"}})));
    d.hear(fault(4));
    assert_eq!(d.told(), ["began", "not heard: words"]);
    // Another's end is not its.
    d.hear(note("turn/completed", json!({"turn": {"id": "t0", "status": "completed"}})));
    d.hear(note(
        "turn/completed",
        json!({"threadId": "kid", "turn": {"id": "t1", "status": "completed"}}),
    ));
    assert!(d.told().is_empty());
    d.hear(note("turn/completed", json!({"turn": {"id": "t1", "status": "completed"}})));
    d.hear(note("turn/completed", json!({"turn": {"id": "t1", "status": "completed"}})));
    assert_eq!(d.told(), ["done Completed completed"]);
    assert_eq!(d.prompt("again"), Ok(Took::Began));
    // Without the answer to its start, its end still ends it.
    d.hear(note("turn/completed", json!({"turn": {"id": "t2", "status": "completed"}})));
    assert_eq!(d.told(), ["began", "done Completed completed"]);
    // One that it begins by itself; a late answer begins none.
    d.hear(reply(5, json!({"turn": {"id": "t2"}})));
    d.hear(note("turn/started", json!({"turn": {"id": "t3"}})));
    assert_eq!(d.told(), ["began by agent"]);
    assert_eq!(d.prompt("more"), Ok(Took::Steered));
}

#[test]
fn how_a_turn_ends() {
    for (turn, want) in [
        (json!({"status": "interrupted"}), "done Cancelled interrupted"),
        (json!({"status": "failed"}), "done Failed failed (it failed and did not say how)"),
        (
            json!({"status": "failed", "error": {"message": "no more", "codexErrorInfo": "usageLimitExceeded"}}),
            "done Failed failed (no more (usageLimitExceeded))",
        ),
        (
            json!({"status": "failed", "error": {"message": "cut", "codexErrorInfo": {"httpConnectionFailed": {"httpStatusCode": 503}}}}),
            "done Failed failed (cut (httpConnectionFailed))",
        ),
        (json!({"status": "dozing"}), "done Unknown dozing"),
        (json!({}), "done Unknown "),
    ] {
        let mut d = at_work();
        let mut turn = turn;
        turn["id"] = json!("t1");
        d.hear(note("turn/completed", json!({"turn": turn})));
        assert_eq!(d.told(), [want]);
    }
    // What was told of it before.
    let mut d = at_work();
    d.hear(note("error", json!({"turnId": "t1", "willRetry": true, "error": {"message": "busy"}})));
    d.hear(note(
        "error",
        json!({"turnId": "t1", "willRetry": false, "error": {"message": "over"}}),
    ));
    d.hear(note("turn/completed", json!({"turn": {"id": "t1", "status": "failed"}})));
    assert_eq!(d.told(), ["notice Warning: It tries again: busy", "done Failed failed (over)"]);
    // It is forgotten with the turn.
    d.prompt("x").unwrap();
    d.hear(reply(4, json!({"turn": {"id": "t2"}})));
    d.told();
    d.hear(note("turn/completed", json!({"turn": {"id": "t2", "status": "failed"}})));
    assert_eq!(d.told(), ["done Failed failed (it failed and did not say how)"]);
    let mut d = ready();
    d.prompt("go").unwrap();
    d.hear(fault(3));
    assert_eq!(d.told(), ["began", "done Failed error (words)"]);
    assert_eq!(d.prompt("go"), Ok(Took::Began));
}

#[test]
fn stopping_a_turn() {
    let mut d = at_work();
    assert!(d.interrupt());
    let v = d.one();
    assert_eq!(
        (v["method"].as_str(), &v["params"]),
        (Some("turn/interrupt"), &json!({"threadId": "th", "turnId": "t1"}))
    );
    // Asked for before it is known which: it is sent when it is.
    let mut d = ready();
    d.prompt("go").unwrap();
    d.sent();
    assert!(d.interrupt());
    assert!(d.sent().is_empty());
    d.hear(reply(3, json!({"turn": {"id": "t1"}})));
    assert_eq!(d.one()["params"]["turnId"], "t1");
    d.hear(note("turn/started", json!({"turn": {"id": "t1"}})));
    assert!(d.sent().is_empty(), "once");
    d.hear(note("turn/completed", json!({"turn": {"id": "t1", "status": "interrupted"}})));
    // Not the next.
    d.prompt("go").unwrap();
    d.sent();
    d.hear(reply(5, json!({"turn": {"id": "t2"}})));
    assert!(d.sent().is_empty());
    let mut d = ready();
    d.prompt("go").unwrap();
    d.interrupt();
    d.hear(fault(3));
    d.prompt("go").unwrap();
    d.sent();
    d.hear(reply(4, json!({"turn": {"id": "t2"}})));
    assert!(d.sent().is_empty(), "nor after one that did not begin");
}

#[test]
fn texts() {
    let mut d = at_work();
    let delta = |m: &str, p: Value| {
        let mut p = p;
        p["itemId"] = json!("i");
        note(m, p)
    };
    for v in [
        started(json!({"type": "agentMessage", "id": "i", "text": ""})),
        delta("item/agentMessage/delta", json!({"delta": "he"})),
        delta("item/agentMessage/delta", json!({"delta": "llo"})),
        completed(json!({"type": "agentMessage", "id": "i", "text": "hello"})),
        delta("item/agentMessage/delta", json!({"delta": "late"})),
        completed(json!({"type": "agentMessage", "id": "j", "text": "whole"})),
        note("item/agentMessage/delta", json!({"threadId": "kid", "itemId": "k", "delta": "sub"})),
        note("item/agentMessage/delta", json!({"delta": "of nothing"})),
    ] {
        d.hear(v);
    }
    assert_eq!(
        d.told(),
        [
            "say i: he",
            "say i: llo",
            "said i",
            "say j: whole (whole)",
            "said j",
            "say kid/k of kid: sub"
        ]
    );
    let mut d = at_work();
    for v in [
        delta("item/reasoning/summaryPartAdded", json!({"summaryIndex": 0})),
        delta("item/reasoning/summaryTextDelta", json!({"summaryIndex": 0, "delta": "a"})),
        delta("item/reasoning/summaryPartAdded", json!({"summaryIndex": 1})),
        delta("item/reasoning/summaryTextDelta", json!({"summaryIndex": 1, "delta": "b"})),
        delta("item/reasoning/textDelta", json!({"contentIndex": 0, "delta": "raw"})),
        completed(
            json!({"type": "reasoning", "id": "i", "summary": ["a", "b"], "content": ["ra", "w"]}),
        ),
        completed(json!({"type": "reasoning", "id": "k", "summary": ["x"], "content": []})),
    ] {
        d.hear(v);
    }
    assert_eq!(
        d.told(),
        [
            "think i: a",
            "think i: \n\n",
            "think i: b",
            "think i#raw: raw",
            "said i",
            "said i#raw",
            "think k: x (whole)",
            "said k"
        ]
    );
}

#[test]
fn tools() {
    let mut d = at_work();
    for v in [
        started(
            json!({"type": "commandExecution", "id": "c", "command": "ls", "cwd": "/w", "status": "inProgress", "commandActions": []}),
        ),
        note("item/commandExecution/outputDelta", json!({"itemId": "c", "delta": "a\n"})),
        completed(
            json!({"type": "commandExecution", "id": "c", "command": "ls", "cwd": "/w", "status": "failed", "aggregatedOutput": "a\n", "exitCode": 2}),
        ),
        started(
            json!({"type": "fileChange", "id": "f", "status": "inProgress", "changes": [{"path": "/a", "kind": {"type": "add"}, "diff": "+x"}]}),
        ),
        note(
            "item/fileChange/patchUpdated",
            json!({"itemId": "f", "changes": [{"path": "/a", "diff": "+y"}]}),
        ),
        completed(
            json!({"type": "fileChange", "id": "f", "status": "declined", "changes": [{"path": "/a", "diff": "+y"}]}),
        ),
        started(
            json!({"type": "mcpToolCall", "id": "m", "server": "s", "tool": "t", "status": "inProgress", "arguments": {"a": 1}}),
        ),
        note("item/mcpToolCall/progress", json!({"itemId": "m", "message": "half"})),
        completed(
            json!({"type": "mcpToolCall", "id": "m", "server": "s", "tool": "t", "status": "completed", "arguments": {"a": 1}, "result": {"content": [{"type": "text", "text": "r"}]}}),
        ),
        completed(
            json!({"type": "mcpToolCall", "id": "n", "server": "s", "tool": "t", "status": "failed", "arguments": null, "error": {"message": "no"}}),
        ),
        completed(json!({"type": "webSearch", "id": "w", "query": "q"})),
        completed(
            json!({"type": "dynamicToolCall", "id": "d", "tool": "x", "arguments": {}, "status": "odd"}),
        ),
        started(json!({"type": "sleep", "id": "z", "durationMs": 5})),
    ] {
        d.hear(v);
    }
    let told = d.told();
    let want = [
        "tool c command 'ls' {\"command\":\"ls\",\"cwd\":\"/w\"}",
        "output c: a\n",
        "more c command 'ls' {\"command\":\"ls\",\"cwd\":\"/w\"}",
        "ended c Failed \"a\\n\" (exit 2)",
        "tool f file change [/a Some(\"+x\") - -]",
        "more f  [/a Some(\"+y\") - -]",
        "more f file change [/a Some(\"+y\") - -]",
        "ended f Cancelled (it was not allowed) [/a Some(\"+y\") - -]",
        "tool m s/t {\"a\":1}",
        "output m: half\n",
        "more m s/t {\"a\":1}",
        "ended m Completed \"r\"",
        "tool n s/t",
        "ended n Failed (no)",
        "tool w web search 'q' {\"action\":null,\"query\":\"q\"}",
        "ended w Completed",
        "tool d x {}",
        "ended d Unknown",
    ];
    assert_eq!(told[..want.len()], want);
    assert!(told[want.len()].starts_with("tool z sleep"));
    // A child's are its own.
    d.hear(note("item/started", json!({"threadId": "kid", "item": {"type": "commandExecution", "id": "c", "command": "ls"}})));
    assert!(d.told()[0].starts_with("tool kid/c command"));
    d.hear(completed(json!({"type": "commandExecution", "id": "ok", "command": "ls", "status": "completed", "exitCode": 0})));
    assert_eq!(d.told()[1], "ended ok Completed");
}

#[test]
fn other_items() {
    let mut d = at_work();
    for t in
        ["userMessage", "hookPrompt", "functionCallOutput", "enteredReviewMode", "exitedReviewMode"]
    {
        d.hear(started(json!({"type": t, "id": "x"})));
        d.hear(completed(json!({"type": t, "id": "x"})));
    }
    for t in ["agentMessage", "reasoning", "plan", "contextCompaction"] {
        d.hear(started(json!({"type": t, "id": "x", "text": "t"})));
    }
    assert!(d.told().is_empty());
    d.hear(completed(json!({"type": "plan", "id": "p", "text": "# Plan"})));
    d.hear(completed(json!({"type": "contextCompaction", "id": "y"})));
    d.hear(started(json!({"type": "hologram", "id": "h", "x": 1})));
    d.hear(started(json!({"type": "agentMessage"})));
    d.hear(note("item/started", json!({})));
    assert_eq!(
        d.told(),
        [
            "plan p [] Some(\"# Plan\")",
            "notice Info: The agent shortened its context",
            "unknown item hologram",
            "unknown an item without an id",
            "unknown item",
        ]
    );
}

#[test]
fn plans_usage_and_limits() {
    let mut d = at_work();
    d.hear(note("turn/plan/updated", json!({"turnId": "t1", "explanation": "why", "plan": [{"step": "a", "status": "completed"}, {"step": "b", "status": "inProgress"}, {"step": "c", "status": "pending"}, {"step": "d"}]})));
    assert_eq!(
        d.told(),
        [
            "plan plan [(\"a\", Done), (\"b\", Doing), (\"c\", Todo), (\"d\", Unknown)] Some(\"why\")"
        ]
    );
    d.hear(note("thread/tokenUsage/updated", json!({"turnId": "t1", "tokenUsage": {"modelContextWindow": 400_000, "total": {"inputTokens": 99},
        "last": {"inputTokens": 10, "cachedInputTokens": 4, "cacheWriteInputTokens": 1, "outputTokens": 5, "reasoningOutputTokens": 2, "totalTokens": 15}}})));
    let e = d.events();
    let Event::Usage(s) = &e[0] else { panic!("{e:?}") };
    let want = Usage {
        input: Some(10),
        cache_read: Some(4),
        cache_write: Some(1),
        output: Some(5),
        reasoning: Some(2),
        ..Usage::default()
    };
    assert_eq!((&s.usage, s.provenance, &s.cost), (&want, Some(Provenance::Reported), &None));
    assert_eq!(s.of, axiom_proto::Covers::Unknown);
    assert_eq!(s.totals[0].usage.input, Some(99));
    d.hear(note(
        "thread/tokenUsage/updated",
        json!({"tokenUsage": {"last": {"outputTokens": 1}, "total": {"outputTokens": 6}}}),
    ));
    let e = d.events();
    let Event::Usage(s) = &e[0] else { panic!("{e:?}") };
    let usage = Usage { output: Some(6), ..Usage::default() };
    assert_eq!(s.totals, [ModelTotal { model: None, usage, cost: None }]);
    d.hear(note("thread/tokenUsage/updated", json!({"tokenUsage": {"last": {}}})));
    assert!(matches!(&d.events()[0], Event::Usage(s) if s.totals.is_empty()));
    d.hear(note("thread/tokenUsage/updated", json!({"tokenUsage": {}})));
    assert_eq!(d.told(), ["unknown usage"]);
    d.hear(note(
        "thread/tokenUsage/updated",
        json!({"threadId": "kid", "tokenUsage": {"last": {}}}),
    ));
    assert!(d.told().is_empty());
    d.hear(json!({"method": "account/rateLimits/updated", "params": {"rateLimits": {"limitId": "codex",
        "primary": {"usedPercent": 25, "windowDurationMins": 300, "resetsAt": 1_795_510_000}, "secondary": null}}}));
    let e = d.events();
    assert_eq!(e.len(), 1);
    let Event::Limit(l) = &e[0] else { panic!("{e:?}") };
    let want = Limit {
        name: "codex primary".into(),
        used: Some(0.25),
        resets_at: Some(1_795_510_000),
        minutes: Some(300),
        status: None,
    };
    assert_eq!(*l, want);
}

#[test]
fn what_is_told_beside() {
    let mut d = at_work();
    for v in [
        note("warning", json!({"message": "w"})),
        note("guardianWarning", json!({"message": "g"})),
        note("configWarning", json!({"summary": "s", "details": "d"})),
        note("deprecationNotice", json!({"summary": "old"})),
        note("model/rerouted", json!({"toModel": "m9", "fromModel": "m1"})),
        note("thread/closed", json!({"threadId": "kid"})),
        note("turn/diff/updated", json!({"diff": "x"})),
        note("thread/name/updated", json!({})),
        note("new/thing", json!({"a": 1})),
    ] {
        d.hear(v);
    }
    assert_eq!(
        d.told(),
        [
            "notice Warning: w",
            "notice Warning: g",
            "notice Warning: s: d",
            "notice Info: old",
            "notice Warning: Another model answers: m9",
            "unknown new/thing"
        ]
    );
    for m in QUIET.iter().filter(|m| {
        **m != "error"
            && !m.starts_with("turn/")
            && **m != "thread/closed"
            && **m != "thread/tokenUsage/updated"
            && **m != "model/rerouted"
    }) {
        d.hear(note(m, json!({})));
    }
    assert!(d.told().is_empty());
    assert!(d.end.is_none());
    d.hear(note("thread/closed", json!({})));
    assert_eq!(d.ends().0, Why::Died);
    let mut d = at_work();
    for bad in [json!({}), json!({"id": null, "method": "x"}), json!({"id": 99, "result": 1})] {
        d.hear(bad);
    }
    assert_eq!(d.bad, 2);
    assert_eq!(d.told(), ["unknown a reply to nothing"]);
}

fn ask(method: &str, id: Value, mut p: Value) -> Value {
    (p["threadId"], p["turnId"], p["itemId"]) = (json!("th"), json!("t1"), json!("c"));
    json!({"id": id, "method": method, "params": p})
}

#[test]
fn a_command_that_needs_a_yes() {
    let m = "item/commandExecution/requestApproval";
    let mut d = at_work();
    d.hear(ask(
        m,
        json!(90),
        json!({"command": "cargo test", "cwd": "/w", "reason": "needs network"}),
    ));
    let e = d.events();
    let Event::Asked(q) = &e[0] else { panic!("{e:?}") };
    assert_eq!(
        (q.title.as_str(), q.detail.as_deref(), q.tool.as_deref()),
        ("cargo test", Some("needs network"), Some("c"))
    );
    assert_eq!(q.subject["cwd"], "/w");
    let ids: Vec<_> = q.options.iter().map(|o| (o.id.as_str(), o.kind.clone())).collect();
    use ChoiceKind::*;
    assert_eq!(
        ids,
        [
            ("accept", AllowOnce),
            ("acceptForSession", AllowAlways),
            ("decline", RejectOnce),
            ("cancel", RejectOnce)
        ]
    );
    assert_eq!(d.choose("q1", "yes"), Err(Refused::NoSuchOption));
    assert_eq!(d.choose("q9", "accept"), Err(Refused::NotAsked));
    d.choose("q1", "accept").unwrap();
    assert_eq!(d.one(), json!({"id": 90, "result": {"decision": "accept"}}));
    assert_eq!(d.choose("q1", "accept"), Err(Refused::NotAsked));
    for c in ["acceptForSession", "decline", "cancel"] {
        d.hear(ask(m, json!("r"), json!({"command": "x"})));
        let q = format!("q{}", d.talk.made);
        d.choose(&q, c).unwrap();
        assert_eq!(d.one(), json!({"id": "r", "result": {"decision": c}}));
    }
    d.hear(ask(m, json!(1), json!({"command": "x"})));
    let q = format!("q{}", d.talk.made);
    d.refuse(&q).unwrap();
    assert_eq!(d.one()["result"]["decision"], "decline");
    d.told();
    // A network is not a command; another kind is not remembered.
    d.hear(ask(
        m,
        json!(2),
        json!({"command": "x", "networkApprovalContext": {"host": "a.io", "protocol": "https"}}),
    ));
    d.hear(ask(m, json!(3), json!({"kind": "humanCommand"})));
    d.hear(ask(m, json!(4), json!({"kind": "command"})));
    let told = d.told();
    assert_eq!(
        told[0],
        "asked q6 'Connect to a.io' Some(\"c\") accept,acceptForSession,decline,cancel"
    );
    assert_eq!(told[1], "asked q7 'Run a command' Some(\"c\") accept,decline,cancel");
    assert_eq!(
        told[2],
        "asked q8 'Run a command' Some(\"c\") accept,acceptForSession,decline,cancel"
    );
}

#[test]
fn files_and_permissions_that_need_a_yes() {
    let mut d = at_work();
    d.hear(ask("item/fileChange/requestApproval", json!(1), json!({"grantRoot": "/x"})));
    assert_eq!(
        d.told(),
        ["asked q1 'Change files' Some(\"c\") accept,acceptForSession,decline,cancel"]
    );
    d.choose("q1", "accept").unwrap();
    assert_eq!(d.one()["result"], json!({"decision": "accept"}));
    let want = json!({"network": {"enabled": true}, "fileSystem": null});
    let m = "item/permissions/requestApproval";
    d.hear(ask(m, json!(2), json!({"cwd": "/w", "permissions": want})));
    assert_eq!(d.told(), ["asked q2 'More permissions, for this turn' Some(\"c\") accept,decline"]);
    d.choose("q2", "accept").unwrap();
    assert_eq!(d.one()["result"], json!({"permissions": want, "scope": "turn"}));
    for no in [true, false] {
        d.hear(ask(m, json!(3), json!({"cwd": "/w", "permissions": want})));
        let q = format!("q{}", d.talk.made);
        if no {
            d.refuse(&q).unwrap()
        } else {
            d.choose(&q, "decline").unwrap()
        }
        assert_eq!(d.one()["result"], json!({"permissions": {}, "scope": "turn"}));
    }
}

#[test]
fn questions_that_are_not_put_to_anyone() {
    let mut d = at_work();
    d.hear(ask(
        "mcpServer/elicitation/request",
        json!(1),
        json!({"serverName": "s", "mode": "form", "message": "Your key?"}),
    ));
    assert_eq!(d.one()["result"], json!({"action": "decline", "content": null}));
    assert_eq!(d.told(), ["notice Warning: Not asked: an MCP server wants to know: Your key?"]);
    for m in [
        "item/tool/requestUserInput",
        "item/tool/call",
        "account/chatgptAuthTokens/refresh",
        "attestation/generate",
        "applyPatchApproval",
        "execCommandApproval",
        "new",
    ] {
        d.hear(ask(m, json!(5), json!({})));
        let v = d.one();
        assert_eq!((v["id"].clone(), v["error"]["code"].as_i64()), (json!(5), Some(-32601)), "{m}");
        assert!(v.get("result").is_none());
        assert_eq!(d.told(), [format!("unknown {m}")]);
    }
}

#[test]
fn a_question_that_is_settled_elsewhere_or_asked_twice() {
    let m = "item/fileChange/requestApproval";
    let mut d = at_work();
    d.hear(ask(m, json!(1), json!({})));
    d.hear(ask(m, json!(1), json!({})));
    d.hear(ask(m, json!("x".repeat(257)), json!({})));
    let u = "unknown a question with an id that cannot be used";
    assert_eq!(d.told()[1..], [u, u]);
    d.hear(note("serverRequest/resolved", json!({"requestId": 2})));
    d.hear(note("serverRequest/resolved", json!({"requestId": "1"})));
    assert!(d.told().is_empty());
    d.hear(note("serverRequest/resolved", json!({"requestId": 1})));
    assert_eq!(d.told(), ["closed q1 Withdrawn"]);
    assert_eq!(d.choose("q1", "accept"), Err(Refused::NotAsked));
    assert!(d.sent().is_empty());
    // Of a child's.
    let mut a = ask(m, json!(3), json!({}));
    a["params"]["threadId"] = json!("kid");
    d.hear(a);
    let e = d.events();
    let Event::Asked(q) = &e[0] else { panic!("{e:?}") };
    assert_eq!((q.by.as_deref(), q.tool.as_deref()), (Some("kid"), Some("kid/c")));
}

#[test]
fn how_it_may_be_started() {
    for a in [Access::Read, Access::Ask, Access::Work] {
        let l = launch(&start(a)).unwrap();
        assert_eq!(
            l,
            Launch { args: vec!["app-server".into()], containment: Containment::Harness }
        );
        let mut s = start(a);
        s.trust = Trust::Withheld;
        assert!(matches!(launch(&s), Err(Error::Untrusted)));
    }
}
