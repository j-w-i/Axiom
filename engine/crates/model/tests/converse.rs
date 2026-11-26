//! The Converse codec, on what Amazon's documentation prints (fixtures/README.md) and on what may come
//! besides.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axiom_model::{
    Args, Block, Cache, Class, Codec, Content, Delta, EncodeError, Error, Event, Json, Loss, Lost,
    Message, Profile, Quirks, Reply, Request, Retry, Role, Settings, Start, Stop, Tool, ToolCall,
    Usage,
    codec::{Frames, MAX_ANSWER, MAX_BLOCKS, encode},
    profile::{CachePoints, Effort, Replay},
};
use common::*;
use serde_json::{Value, json};

const CLAUDE: &str = "us.anthropic.claude-sonnet-5-5";

fn read(bytes: &[u8]) -> (Reply, Option<Error>) {
    reply(Codec::Converse, &Quirks::default(), bytes)
}
fn documented(name: &str) -> Reply {
    let text = fixture(&format!("bedrock-from-documentation/{name}.jsonl"));
    let (reply, error) = read(&amazon::stream(&String::from_utf8(text).unwrap()));
    assert_eq!(error, None);
    assert!(reply.complete);
    reply
}
fn native(block: &Block) -> &str {
    block.native.as_ref().unwrap().json.get()
}
fn calls(reply: &Reply) -> Vec<(String, String, Value)> {
    let calls = reply.message.calls();
    calls.map(|c| (c.id().into(), c.name().into(), Value::Object(c.args().clone()))).collect()
}

#[test]
fn text() {
    let r = documented("text");
    assert_eq!((r.id, r.model, r.stop), (None, None, Some(Stop::MaxOutput)));
    let [b] = r.message.blocks.as_slice() else { panic!() };
    // The page does not end it; its message does.
    assert_eq!((&b.content, b.cut), (&Content::Text { text: " Title: The".into() }, false));
    assert_eq!(native(b), r#"{"text":" Title: The"}"#);
    let want = Usage { prompt: Some(47), output: Some(20), ..Usage::default() };
    assert_eq!(r.usage, want);
}

#[test]
fn a_call() {
    let r = documented("tool");
    assert_eq!(r.stop, Some(Stop::ToolUse));
    let id = "tooluse_20Z9zl0BQWSXjFuLKdTJcA";
    assert_eq!(calls(&r), [(id.into(), "top_song".into(), json!({"sign":"WZPZ"}))]);
    assert_eq!(
        native(&r.message.blocks[0]),
        format!(
            r#"{{"toolUse":{{"toolUseId":"{id}","name":"top_song","input":{{"sign": "WZPZ"}}}}}}"#
        )
    );
}

#[test]
fn reasoning() {
    let r = documented("reasoning");
    let [signed, redacted, text] = r.message.blocks.as_slice() else { panic!() };
    let words = "An even number is 2k. 2j + 2k = 2(j + k).";
    assert_eq!(signed.content, Content::Reasoning { text: words.into() });
    assert_eq!(
        native(signed),
        format!(
            r#"{{"reasoningContent":{{"reasoningText":{{"text":"{words}","signature":"<Signature_of_the_reasoning>"}}}}}}"#
        )
    );
    // Nothing to show, and whole.
    assert_eq!(redacted.content, Content::Reasoning { text: String::new() });
    assert_eq!(native(redacted), r#"{"reasoningContent":{"redactedContent":"PFJlZGFjdGVkPg=="}}"#);
    assert_eq!(native(text), r#"{"text":"The sum of two even numbers is even."}"#);
    let want = Usage {
        prompt: Some(125),
        uncached: Some(5),
        cache_read: Some(100),
        cache_write: Some(20),
        cache_write_long: Some(12),
        output: Some(60),
        ..Usage::default()
    };
    assert_eq!(r.usage, want);
}

/// One event a line, with what every stream begins and ends with.
fn stream(lines: &[&str]) -> Vec<u8> {
    let end = r#"{"messageStop":{"stopReason":"end_turn"}}"#;
    amazon::stream(&[&[START], lines, &[end, METADATA]].concat().join("\n"))
}
const START: &str = r#"{"messageStart":{"role":"assistant"}}"#;
const METADATA: &str = r#"{"metadata":{"metrics":{"latencyMs":1}}}"#;
fn delta(index: u64, delta: Value) -> String {
    json!({"contentBlockDelta":{"contentBlockIndex":index,"delta":delta}}).to_string()
}
fn starts(index: u64, start: Value) -> String {
    json!({"contentBlockStart":{"contentBlockIndex":index,"start":start}}).to_string()
}
fn stops(index: u64) -> String {
    json!({"contentBlockStop":{"contentBlockIndex":index}}).to_string()
}
fn use_of(id: &str, name: &str) -> Value {
    json!({"toolUse":{"toolUseId":id,"name":name}})
}
fn ended(reason: &str) -> String {
    json!({"messageStop":{"stopReason":reason}}).to_string()
}
fn blocks(lines: &[String]) -> Vec<Block> {
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (r, e) = read(&stream(&lines));
    assert_eq!((e, r.complete), (None, true));
    r.message.blocks
}

#[test]
fn reasoning_goes_back_only_if_something_proves_it() {
    let think = |d: Value| delta(0, json!({ "reasoningContent": d }));
    let of = |pieces: &[Value]| {
        let mut lines: Vec<String> = pieces.iter().cloned().map(think).collect();
        lines.push(stops(0));
        let [b] = &blocks(&lines)[..] else { panic!() };
        let Content::Reasoning { text } = &b.content else { panic!() };
        (text.clone(), b.native.as_ref().map(|n| n.json.get().to_owned()), b.extra.len())
    };
    // Empty, as Claude 5 sends it: the signature is all of it.
    assert_eq!(
        of(&[json!({"text":""}), json!({"signature":"S"})]),
        (
            String::new(),
            Some(r#"{"reasoningContent":{"reasoningText":{"text":"","signature":"S"}}}"#.into()),
            0
        )
    );
    assert_eq!(
        of(&[json!({"signature":"S"}), json!({"signature":"T"}), json!({"text":"t"})]).1.unwrap(),
        r#"{"reasoningContent":{"reasoningText":{"text":"t","signature":"ST"}}}"#
    );
    assert_eq!(of(&[json!({"text":"why"})]), ("why".into(), None, 0));
    // Base64 can be joined where none of it has ended.
    let redacted = |pieces: &[&str]| {
        let pieces: Vec<Value> = pieces.iter().map(|p| json!({"redactedContent":p})).collect();
        of(&pieces).1
    };
    let whole = |s: &str| Some(format!(r#"{{"reasoningContent":{{"redactedContent":"{s}"}}}}"#));
    assert_eq!(redacted(&["AAAA"]), whole("AAAA"));
    assert_eq!(redacted(&["AAAA", "BBB="]), whole("AAAABBB="));
    assert_eq!(redacted(&["AAA=", "BBBB"]), None);
    assert_eq!(redacted(&["AA", "BBBB"]), None);
    // Both at once is not a block this API has.
    let both = [json!({"redactedContent":"AAAA"}), json!({"signature":"S"}), json!({"text":"t"})];
    assert_eq!(of(&both).1, None);
    assert_eq!(of(&[json!({"redactedContent":"AAAA"}), json!({"text":"t"})]).1, None);
    // What is newer than this build is kept beside it, and what is known still goes back.
    let (text, native, extra) =
        of(&[json!({"text":"t"}), json!({"novel":1}), json!({"signature":"S"})]);
    assert_eq!((text.as_str(), native.is_some(), extra), ("t", true, 1));
}

#[test]
fn a_call_is_run_only_if_everything_about_it_is_whole() {
    let call = |args: &[&str], reason: &str| {
        let mut lines = vec![START.to_owned(), starts(0, use_of("t1", "f"))];
        lines.extend(args.iter().map(|a| delta(0, json!({"toolUse":{"input":a}}))));
        lines.extend([stops(0), ended(reason), METADATA.to_owned()]);
        let (r, e) = read(&amazon::stream(&lines.join("\n")));
        assert_eq!((e, r.complete), (None, true));
        let native = r.message.blocks[0].native.is_some();
        let Content::ToolCall(c) = &r.message.blocks[0].content else { panic!() };
        (c.args.clone(), native)
    };
    assert!(matches!(call(&[r#"{"a":"#, "1}"], "tool_use"), (Args::Ready(_), true)));
    assert!(matches!(call(&["{}"], "end_turn"), (Args::Ready(_), true)));
    // Whole, and nothing was said: no arguments.
    for none in [&[][..], &[""], &["", ""]] {
        assert!(matches!(call(none, "tool_use"), (Args::Ready(a), true) if a.is_empty()));
    }
    assert!(matches!(call(&["{"], "tool_use"), (Args::Broken(_), false)));
    assert!(matches!(call(&["[]"], "tool_use"), (Args::Broken(_), true)));
    for reason in [
        "max_tokens",
        "malformed_tool_use",
        "malformed_model_output",
        "guardrail_intervened",
        "content_filtered",
        "stop_sequence",
        "model_context_window_exceeded",
        "novel",
    ] {
        assert!(matches!(call(&["{}"], reason).0, Args::Cut), "{reason}");
    }
    // Without an end of its own, whatever its message says.
    let open = [START, &starts(0, use_of("t1", "f")), &ended("tool_use"), METADATA].join("\n");
    let (r, e) = read(&amazon::stream(&open));
    assert_eq!((e, calls(&r).len(), r.message.blocks[0].cut), (None, 0, true));
    assert!(r.message.blocks[0].native.is_none());
    // A piece that is none.
    let lines = [
        START.to_owned(),
        starts(0, use_of("t1", "f")),
        delta(0, json!({"toolUse":{"input":"{}"}})),
        delta(0, json!({"text":"x"})),
        stops(0),
        ended("tool_use"),
    ];
    let (r, _) = read(&amazon::stream(&lines.join("\n")));
    let Content::ToolCall(c) = &r.message.blocks[0].content else { panic!() };
    assert!(matches!(c.args, Args::Broken(_)));
    assert_eq!(r.message.blocks[0].extra.len(), 1);
    // Two with one id, and one without.
    let got = blocks(&[
        starts(0, use_of("t1", "f")),
        stops(0),
        starts(1, use_of("t1", "f")),
        stops(1),
        starts(2, json!({"toolUse":{"name":"f"}})),
        stops(2),
    ]);
    let args = |b: &Block| match &b.content {
        Content::ToolCall(c) => c.ready().is_some(),
        _ => panic!(),
    };
    assert_eq!(got.iter().map(args).collect::<Vec<_>>(), [true, false, false]);
}

#[test]
fn what_the_provider_runs_itself_is_not_a_call() {
    let got = blocks(&[
        starts(0, json!({"toolUse":{"toolUseId":"t","name":"f","type":"server_tool_use"}})),
        delta(0, json!({"toolUse":{"input":"{}"}})),
        stops(0),
        starts(1, json!({"toolResult":{"toolUseId":"t"}})),
        delta(1, json!({"toolResult":[{"text":"r"}]})),
        stops(1),
        delta(2, json!({"image":{"source":{"bytes":"AA=="}}})),
        stops(2),
        delta(3, json!({})),
        stops(3),
    ]);
    let names: Vec<_> = got
        .iter()
        .map(|b| match &b.content {
            Content::Unknown { name } => (name.as_str(), b.native.is_none(), b.extra.len()),
            _ => panic!(),
        })
        .collect();
    // All that came is kept; as no more than that, so it is not sent back.
    let want = [("toolUse", true, 2), ("toolResult", true, 2), ("image", true, 1), ("", true, 1)];
    assert_eq!(names, want);
    assert_eq!(got[0].extra[1].get(), r#"{"toolUse":{"input":"{}"}}"#);
}

#[test]
fn what_is_newer_than_this_build_is_no_error() {
    let mut bytes = amazon::event("novelEvent", "not even JSON");
    bytes.extend(stream(&[
        r#"{"messageStart":{"role":"assistant","p":"abcd"}}"#,
        r#"{"contentBlockDelta":{"contentBlockIndex":0,"delta":{"text":"a"},"p":"abcdefgh"}}"#,
        &delta(0, json!({"citation":{"title":"t"}})),
        &delta(0, json!({"text":"b"})),
        &stops(0),
        &stops(7),
    ]));
    let (r, e) = read(&bytes);
    assert_eq!((e, r.complete), (None, true));
    let [b] = r.message.blocks.as_slice() else { panic!() };
    assert_eq!((native(b), b.extra.len()), (r#"{"text":"ab"}"#, 1));
    // After its end nothing counts, not even damage.
    let mut more = stream(&[&delta(0, json!({"text":"a"})), &stops(0)]);
    more.extend([0; 40]);
    assert_eq!(read(&more).1, None);
}

#[test]
fn a_text_of_nothing_is_not_sent_back() {
    let got = blocks(&[
        delta(0, json!({"text":""})),
        stops(0),
        delta(1, json!({"text":" \n"})),
        stops(1),
        delta(2, json!({"text":"a"})),
        stops(2),
    ]);
    let [blank, a] = &got[..] else { panic!("{got:?}") };
    assert!(blank.native.is_none() && a.native.is_some());
    let mut r = request();
    r.messages = vec![Message { role: Role::Assistant, blocks: got }, Message::user(" ")];
    assert!(body(&r).ends_with(r#""messages":[{"role":"assistant","content":[{"text":"a"}]}]}"#));
}

#[test]
fn usage_is_what_was_told() {
    let of = |usage: Value| {
        let told = json!({"metadata":{"usage":usage,"metrics":{}}}).to_string();
        let lines = [START, &ended("end_turn"), &told].join("\n");
        read(&amazon::stream(&lines)).0.usage
    };
    let plain = json!({"inputTokens":125,"outputTokens":60,"totalTokens":185});
    let want = Usage { prompt: Some(125), output: Some(60), ..Usage::default() };
    assert_eq!(of(plain), want);
    let cached = json!({"inputTokens":1,"outputTokens":2,"totalTokens":6,"cacheReadInputTokens":3,"cacheWriteInputTokens":0,"cacheDetails":[]});
    let want = Usage {
        prompt: Some(4),
        uncached: Some(1),
        cache_read: Some(3),
        cache_write: Some(0),
        cache_write_long: Some(0),
        output: Some(2),
        ..Usage::default()
    };
    assert_eq!(of(cached), want);
    // What does not follow is not said.
    let half = of(json!({"inputTokens":1,"cacheReadInputTokens":3}));
    assert_eq!((half.prompt, half.uncached, half.cache_read), (None, Some(1), Some(3)));
    let half = of(json!({"cacheReadInputTokens":3,"cacheWriteInputTokens":4}));
    assert_eq!((half.prompt, half.uncached), (None, None));
    let huge = of(
        json!({"inputTokens":u64::MAX,"cacheReadInputTokens":1,"cacheWriteInputTokens":1,"cacheDetails":[{"ttl":"1h","inputTokens":u64::MAX},{"ttl":"1h","inputTokens":1}]}),
    );
    assert_eq!((huge.prompt, huge.cache_write_long), (None, None));
    for none in [Value::Null, json!(7), json!({"inputTokens":-1,"outputTokens":"9"})] {
        assert_eq!(of(none), Usage::default());
    }
}

#[test]
fn stops_are_read() {
    let of = |reason: &str| {
        let lines = [START, &ended(reason), METADATA].join("\n");
        read(&amazon::stream(&lines)).0.stop.unwrap()
    };
    for (reason, want) in [
        ("end_turn", Stop::EndTurn),
        ("tool_use", Stop::ToolUse),
        ("max_tokens", Stop::MaxOutput),
        ("stop_sequence", Stop::Sequence(None)),
        ("model_context_window_exceeded", Stop::ContextFull),
        ("guardrail_intervened", Stop::Refusal(Some("guardrail_intervened".into()))),
        ("content_filtered", Stop::Refusal(Some("content_filtered".into()))),
        ("malformed_tool_use", Stop::Other("malformed_tool_use".into())),
        ("malformed_model_output", Stop::Other("malformed_model_output".into())),
        ("novel", Stop::Other("novel".into())),
    ] {
        assert_eq!(of(reason), want);
    }
}

#[test]
fn the_end_is_the_stop_and_what_was_used() {
    let text = [START, &delta(0, json!({"text":"a"})), &stops(0)].join("\n");
    let stop = ended("end_turn");
    let used = r#"{"metadata":{"usage":{"inputTokens":1}}}"#;
    let of = |lines: &[&str]| {
        let (r, e) = read(&amazon::stream(&lines.join("\n")));
        (r.complete, r.usage.prompt, e.map(|e| (e.class, e.retry)))
    };
    assert_eq!(of(&[&text, &stop, used]), (true, Some(1), None));
    // Without the count it is whole, and there is no count.
    assert_eq!(of(&[&text, &stop]), (true, None, None));
    // Without the stop it is not.
    let cut = Some((Class::CutShort, Retry::Restart));
    assert_eq!(of(&[&text]), (false, None, cut));
    assert_eq!(of(&[&text, used]), (false, Some(1), cut));
    assert_eq!(of(&[]), (false, None, cut));
    // An event that has not all come is not one, the last one included.
    let bytes = amazon::stream(&[&text, &stop].map(|s| s.as_str()).join("\n"));
    let (r, e) = read(&bytes[..bytes.len() - 1]);
    assert_eq!((r.complete, e.unwrap().class), (false, Class::CutShort));
    // A service that fails after it stopped has failed.
    let mut bytes = amazon::stream(&[&text, &stop].map(|s| s.as_str()).join("\n"));
    bytes.extend(amazon::exception("internalServerException", "{}"));
    let (r, e) = read(&bytes);
    assert_eq!((r.complete, e.unwrap().class), (false, Class::Server));
}

#[test]
fn an_event_is_one_or_an_account_of_what_went_wrong() {
    let read = |bytes: &[u8]| {
        let (mut frames, mut out) = (Frames::new(Codec::Converse, 1 << 16), Vec::new());
        let done = frames.push(bytes, &mut out);
        if done.is_err() {
            let seen = out.len();
            // It is the end, however often it is asked.
            assert_eq!(frames.push(&amazon::event("a", "{}"), &mut out), done);
            assert_eq!(out.len(), seen);
        }
        (out, done.err())
    };
    let (got, e) = read(&amazon::event("novel", r#"{"é":1}"#));
    assert_eq!((got[0].name.as_str(), got[0].data.as_str(), e), ("novel", r#"{"é":1}"#, None));
    for (bytes, words) in [
        (amazon::framed(&[], b"{}"), "neither an event nor an error"),
        (amazon::message(&[(":message-type", "novel")], b"{}"), "neither an event nor an error"),
        (amazon::message(&[(":message-type", "event")], b"{}"), "has no name"),
        (
            amazon::message(&[(":message-type", "event"), (":event-type", "a")], b"\xff"),
            "not UTF-8",
        ),
    ] {
        let (got, e) = read(&[amazon::event("a", "{}"), bytes].concat());
        assert_eq!(got.len(), 1, "what came before is there");
        assert!(e.unwrap().message.contains(words), "{words}");
    }
    // A header that says it is text and is not.
    let mut head = amazon::text(":event-type", "a");
    head.extend([13]);
    head.extend(b":message-type");
    head.push(1);
    assert!(read(&amazon::framed(&head, b"{}")).1.is_some());
}

#[test]
fn what_goes_wrong_after_it_began_is_said() {
    use {Class::*, Retry::*};
    let begun = amazon::stream(&[START, &delta(0, json!({"text":"a"}))].join("\n"));
    let of = |message: Vec<u8>| {
        let (r, e) = read(&[begun.clone(), message].concat());
        // What had come is kept, and not taken for all.
        assert_eq!((r.complete, r.message.blocks[0].cut), (false, true));
        e.unwrap()
    };
    for (name, body, class, retry) in [
        ("throttlingException", r#"{"message":"m"}"#, RateLimit, Restart),
        ("serviceUnavailableException", r#"{"message":"m"}"#, Overloaded, Restart),
        ("internalServerException", r#"{"message":"m"}"#, Server, Restart),
        ("modelStreamErrorException", r#"{"message":"m"}"#, Server, Restart),
        (
            "modelStreamErrorException",
            r#"{"message":"m","originalStatusCode":400,"originalMessage":"o"}"#,
            BadRequest,
            Never,
        ),
        (
            "modelStreamErrorException",
            r#"{"message":"m","originalStatusCode":529}"#,
            Overloaded,
            Restart,
        ),
        (
            "modelStreamErrorException",
            r#"{"message":"m: prompt is too long","originalStatusCode":400}"#,
            ContextTooLong,
            Never,
        ),
        ("validationException", r#"{"message":"m"}"#, BadRequest, Never),
        (
            "validationException",
            r#"{"message":"m: Input is too long for requested model."}"#,
            ContextTooLong,
            Never,
        ),
        ("accessDeniedException", r#"{"message":"m"}"#, Auth, Never),
        ("expiredTokenException", r#"{"message":"m"}"#, Auth, Never),
        ("modelNotReadyException", r#"{"message":"m"}"#, Overloaded, Restart),
        ("resourceNotFoundException", r#"{"message":"m"}"#, BadRequest, Never),
        ("novelException", r#"{"message":"m"}"#, Server, Restart),
        ("novelException", "", Server, Restart),
        ("", r#"{"message":"m"}"#, Server, Restart),
    ] {
        let e = of(amazon::exception(name, body));
        assert_eq!((e.class, e.retry, e.status), (class, retry, None), "{name} {body}");
        assert!(e.message.starts_with('m') || body.is_empty(), "{e:?}");
        assert_eq!(e.code.as_deref(), (!name.is_empty()).then_some(name));
    }
    let e = of(amazon::error("InternalError", "An internal server error occurred."));
    assert_eq!((e.class, e.code.as_deref()), (Server, Some("InternalError")));
    assert_eq!(e.message, "An internal server error occurred.");
    let e = of(amazon::exception("throttlingException", "\u{fffd}"));
    assert_eq!(e.class, RateLimit);
    // What cannot be read.
    for bad in [
        r#"{"contentBlockDelta":{"delta":{"text":"a"}}}"#,
        r#"{"contentBlockDelta":{"contentBlockIndex":0}}"#,
        r#"{"contentBlockDelta":{"contentBlockIndex":-1,"delta":{}}}"#,
        r#"{"contentBlockStart":{"contentBlockIndex":1}}"#,
        r#"{"contentBlockStart":{"start":{"toolUse":{}}}}"#,
        r#"{"contentBlockStart":{"contentBlockIndex":0,"start":{"toolUse":{}}}}"#,
        r#"{"contentBlockStop":{}}"#,
        r#"{"messageStop":{}}"#,
        r#"{"messageStop":{"stopReason":7}}"#,
        r#"{"metadata":[]}"#,
    ] {
        let e = of(amazon::stream(bad));
        assert_eq!((e.class, e.retry), (Protocol, Never), "{bad}");
    }
    let e = of(amazon::event("messageStart", "{"));
    assert_eq!(e.class, Protocol);
    let mut damaged = amazon::event("messageStop", r#"{"stopReason":"end_turn"}"#);
    *damaged.last_mut().unwrap() ^= 1;
    assert_eq!(of(damaged).class, Protocol);
}

#[test]
fn blocks_begin_and_end_once() {
    let fails = |lines: &[String]| {
        let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
        read(&stream(&lines)).1.unwrap()
    };
    let text = |i| delta(i, json!({"text":"a"}));
    for bad in [
        vec![text(0), stops(0), stops(0)],
        vec![text(0), stops(0), text(0)],
        vec![text(0), stops(0), starts(0, use_of("t", "f"))],
        vec![text(0), starts(0, use_of("t", "f"))],
        vec![starts(0, use_of("t", "f")), starts(0, use_of("u", "f"))],
        vec![ended("end_turn"), text(0)],
        vec![ended("end_turn"), starts(0, use_of("t", "f"))],
        vec![ended("end_turn"), stops(0)],
    ] {
        assert_eq!(fails(&bad).class, Class::Protocol, "{bad:?}");
    }
    // Several may be open at once.
    let got = blocks(&[
        starts(4, use_of("t", "f")),
        text(1),
        delta(4, json!({"toolUse":{"input":"{"}})),
        text(1),
        delta(4, json!({"toolUse":{"input":"}"}})),
        stops(1),
        stops(4),
    ]);
    assert_eq!(native(&got[1]), r#"{"text":"aa"}"#);
    assert!(native(&got[0]).ends_with(r#""input":{}}}"#));
    // Texts that nothing ended end in their order.
    let got = blocks(&[text(3), text(1), text(2)]);
    assert!(got.iter().all(|b| !b.cut && b.native.is_some()));
    let lines: Vec<String> = (0..9).rev().map(text).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (got, _) = events(Codec::Converse, &Quirks::default(), &[&stream(&lines)]);
    let order = got.iter().filter_map(|e| match e {
        Event::BlockStop { block, .. } => Some(*block),
        _ => None,
    });
    assert_eq!(order.collect::<Vec<_>>(), (0..9).collect::<Vec<_>>());
    // Reasoning that nothing ended has not ended: its signature may be missing.
    let got = blocks(&[delta(0, json!({"reasoningContent":{"signature":"S"}}))]);
    assert!(got[0].cut && got[0].native.is_none());
}

#[test]
fn an_answer_is_bounded() {
    let most: Vec<String> =
        (0..MAX_BLOCKS as u64).flat_map(|i| [delta(i, json!({"text":"a"})), stops(i)]).collect();
    assert_eq!(blocks(&most).len(), MAX_BLOCKS);
    let mut more = most;
    more.push(delta(MAX_BLOCKS as u64, json!({"text":"a"})));
    let more: Vec<&str> = more.iter().map(String::as_str).collect();
    assert_eq!(read(&stream(&more)).1.unwrap().class, Class::Protocol);
    let piece = amazon::stream(&delta(0, json!({"text":"x".repeat(1 << 19)})));
    let mut bytes = Vec::new();
    while bytes.len() <= MAX_ANSWER {
        bytes.extend(&piece);
    }
    let e = read(&bytes).1.unwrap();
    assert!(e.class == Class::Protocol && e.message.contains("larger"), "{e}");
}

#[test]
fn events_as_they_come() {
    let lines = [
        delta(0, json!({"reasoningContent":{"text":"w"}})),
        delta(0, json!({"reasoningContent":{"signature":"S"}})),
        stops(0),
        starts(1, use_of("t", "f")),
        delta(1, json!({"toolUse":{"input":"{}"}})),
        stops(1),
    ];
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    let (got, e) = events(Codec::Converse, &Quirks::default(), &[&stream(&lines)]);
    assert_eq!(e, None);
    let stop = |block, json: &str| Event::BlockStop {
        block,
        native: Some(Json::parse(json).unwrap()),
        broken: None,
    };
    let want = [
        Event::Started { id: None, model: None },
        Event::BlockStart { block: 0, start: Start::Reasoning },
        Event::Delta { block: 0, delta: Delta::Reasoning("w".into()) },
        stop(0, r#"{"reasoningContent":{"reasoningText":{"text":"w","signature":"S"}}}"#),
        Event::BlockStart { block: 1, start: Start::ToolCall { id: "t".into(), name: "f".into() } },
        Event::Delta { block: 1, delta: Delta::Args("{}".into()) },
        stop(1, r#"{"toolUse":{"toolUseId":"t","name":"f","input":{}}}"#),
        Event::Stop(Stop::EndTurn),
        Event::End,
    ];
    assert_eq!(got, want);
}

fn tool(name: &str) -> Tool {
    Tool {
        name: name.into(),
        description: "does".into(),
        schema: Json::parse(r#"{"type":"object","properties":{"z":{},"a":{}}}"#).unwrap(),
    }
}
fn request() -> Request {
    Request {
        model: "m".into(),
        system: "Be brief.".into(),
        tools: vec![tool("f")],
        messages: vec![Message::user("hi")],
        settings: Settings { max_output: Some(9), ..Settings::default() },
    }
}
fn assistant(blocks: Vec<Block>) -> Message {
    Message { role: Role::Assistant, blocks }
}
fn user(blocks: Vec<Block>) -> Message {
    Message { role: Role::User, blocks }
}
fn results(ids: &[&str]) -> Message {
    user(ids.iter().map(|id| Block::result(*id, "ok", false)).collect())
}
fn ready() -> Args {
    Args::Ready(serde_json::Map::new())
}
fn call(id: &str, name: &str, args: Args) -> Block {
    let (id, name) = (id.into(), name.into());
    Block::new(Content::ToolCall(ToolCall { id, name, raw: "{}".into(), args }))
}
fn text_of(bytes: &[u8]) -> String {
    String::from_utf8(bytes.to_vec()).unwrap()
}
fn body(r: &Request) -> String {
    text_of(&encode(r, &Profile::bedrock()).unwrap().body)
}

const SPEC: &str = r#"{"toolSpec":{"name":"f","description":"does","inputSchema":{"json":{"type":"object","properties":{"z":{},"a":{}}}}}}"#;

#[test]
fn a_request_to_the_byte() {
    let mut r = request();
    r.settings.stop = vec!["END".into()];
    r.messages = vec![
        Message::user("hi"),
        assistant(vec![Block::text("Look."), call("t1", "f", ready())]),
        user(vec![Block::result("t1", "out", true)]),
    ];
    let messages = r#""messages":[{"role":"user","content":[{"text":"hi"}]},{"role":"assistant","content":[{"text":"Look."},{"toolUse":{"toolUseId":"t1","name":"f","input":{}}}]},{"role":"user","content":[{"toolResult":{"toolUseId":"t1","content":[{"text":"out"}]}}]}]}"#;
    assert_eq!(
        body(&r),
        format!(
            r#"{{"inferenceConfig":{{"maxTokens":9,"stopSequences":["END"]}},"toolConfig":{{"tools":[{SPEC}]}},"system":[{{"text":"Be brief."}}],{messages}"#
        )
    );
    // Nothing that was not asked for; the model's own limit is the default.
    let least =
        Request { model: "m".into(), messages: vec![Message::user("hi")], ..Request::default() };
    assert_eq!(body(&least), r#"{"messages":[{"role":"user","content":[{"text":"hi"}]}]}"#);
    // A description is at least one character long.
    r.tools[0].description.clear();
    assert!(body(&r).contains(r#"{"name":"f","inputSchema""#));
}

#[test]
fn a_model_is_known_by_each_of_its_names() {
    let p = Profile::bedrock();
    let arn = "arn:aws:bedrock:us-east-1";
    for (names, effort, points) in [
        (
            vec![
                "anthropic.claude-sonnet-5-5".to_owned(),
                "us.anthropic.claude-sonnet-5-5".into(),
                "global.anthropic.claude-sonnet-5-5".into(),
                format!("{arn}::foundation-model/anthropic.claude-sonnet-5-5"),
                format!("{arn}:111122223333:inference-profile/eu.anthropic.claude-sonnet-5-5"),
                "anthropic.claude-opus-4-6-v1".into(),
                "jp.anthropic.claude-sonnet-4-6".into(),
            ],
            Some(Effort::AdaptiveThinking),
            Some(CachePoints::All),
        ),
        (
            vec![
                "anthropic.claude-haiku-4-5-20251001-v1:0".into(),
                "au.anthropic.claude-opus-4-5-20251101-v1:0".into(),
            ],
            None,
            Some(CachePoints::All),
        ),
        (
            vec!["amazon.nova-pro-v1:0".into(), "us.amazon.nova-2-lite-v1:0".into()],
            None,
            Some(CachePoints::NotTools),
        ),
        (
            vec![
                "anthropic.claude-sonnet-5-50".into(),
                "anthropic.claude-sonnet-5-5-v1:0".into(),
                "xanthropic.claude-sonnet-5-5".into(),
                "us.eu.anthropic.claude-sonnet-5-5".into(),
                "us.anthropic.claude-3-haiku-20240307-v1:0".into(),
                "openai.gpt-6.1-sol".into(),
                "us.deepseek.r1-v1:0".into(),
                format!("{arn}:111122223333:application-inference-profile/abcdefgh1234"),
                "m".into(),
                String::new(),
                "claude-sonnet-5-5".into(),
            ],
            None,
            None,
        ),
    ] {
        for name in names {
            let q = p.quirks(&name);
            assert_eq!((q.effort, q.cache_points), (effort, points), "{name}");
            assert_eq!(q.result_status, points.map(|_| true), "{name}");
            assert_eq!((q.context, q.max_output), (None, None));
        }
    }
    // Elsewhere a model has one name.
    let mut other = Profile::openai_compatible("p", "x");
    other.models = p.models;
    assert_eq!(other.quirks("us.anthropic.claude-sonnet-5-5"), Quirks::default());
    assert!(other.quirks("anthropic.claude-sonnet-5-5").effort.is_some());
}

#[test]
fn effort_goes_with_the_thinking_it_is_of() {
    let mut r = request();
    r.settings.effort = Some("low".into());
    assert_eq!(
        encode(&r, &Profile::bedrock()).unwrap_err(),
        EncodeError::Unsupported("effort"),
        "of a model nothing is known of"
    );
    r.model = CLAUDE.into();
    assert!(body(&r).starts_with(
        r#"{"inferenceConfig":{"maxTokens":9},"additionalModelRequestFields":{"thinking":{"type":"adaptive"},"output_config":{"effort":"low"}},"toolConfig""#
    ));
    // Without one it thinks all the same, at its own.
    r.settings.effort = None;
    assert!(body(&r).contains(
        r#""additionalModelRequestFields":{"thinking":{"type":"adaptive"}},"toolConfig""#
    ));
    r.model = "amazon.nova-pro-v1:0".into();
    r.settings.effort = Some("low".into());
    let mut p = Profile::bedrock();
    assert!(encode(&r, &p).is_err());
    p.models.clear();
    for other in [Effort::OutputConfig, Effort::Reasoning, Effort::ThinkingLevel] {
        p.quirks.effort = Some(other);
        assert_eq!(encode(&r, &p).unwrap_err(), EncodeError::Unsupported("effort"));
    }
    // And this one is nobody else's.
    let mut q = Profile::anthropic();
    q.quirks.effort = Some(Effort::AdaptiveThinking);
    assert_eq!(encode(&r, &q).unwrap_err(), EncodeError::Unsupported("effort"));
}

#[test]
fn what_cannot_be_asked_for_is_an_error() {
    let mut p = Profile::bedrock();
    p.quirks.body.insert("performanceConfig".into(), json!({"latency":"optimized"}));
    assert!(
        text_of(&encode(&request(), &p).unwrap().body)
            .contains(r#","performanceConfig":{"latency":"optimized"},"toolConfig""#)
    );
    for own in
        ["inferenceConfig", "additionalModelRequestFields", "toolConfig", "system", "messages"]
    {
        p.quirks.body = [(own.to_owned(), json!(1))].into();
        assert_eq!(encode(&request(), &p).unwrap_err(), EncodeError::Reserved(own.into()));
    }
    let mut r = request();
    let long = "x".repeat(65);
    for bad in ["", "a.b", "a b", "é", &long] {
        r.tools = vec![tool("ok"), tool(bad)];
        let e = encode(&r, &Profile::bedrock()).unwrap_err();
        assert_eq!(e, EncodeError::ToolName(bad.into()));
    }
    r.tools = vec![tool(&"x".repeat(64)), tool("A-z_09")];
    assert!(encode(&r, &Profile::bedrock()).is_ok());
}

#[test]
fn what_is_to_be_kept_is_marked_where_it_can_be() {
    let point = r#"{"cachePoint":{"type":"default"}}"#;
    let hour = r#"{"cachePoint":{"ttl":"1h","type":"default"}}"#;
    let mut r = request();
    r.messages = vec![
        Message::user("one"),
        assistant(vec![Block::text("a")]),
        Message::user("two"),
        assistant(vec![Block::text("b")]),
        results(&["t"]),
        Message::user("three"),
    ];
    for (model, cache, want) in [
        (CLAUDE, Cache::Off, None),
        (CLAUDE, Cache::Short, Some(point)),
        (CLAUDE, Cache::Long, Some(hour)),
        ("amazon.nova-pro-v1:0", Cache::Short, Some(point)),
        ("amazon.nova-pro-v1:0", Cache::Long, Some(point)),
        ("openai.gpt-6-sol", Cache::Short, None),
        ("m", Cache::Long, None),
    ] {
        r.model = model.into();
        r.settings.cache = cache;
        let text = body(&r);
        let Some(p) = want else {
            assert!(!text.contains("cachePoint"), "{model} {cache:?}");
            continue;
        };
        // After the system, and the last two of the person's turns, the last thing in each.
        assert_eq!(text.matches("cachePoint").count(), 3, "{text}");
        assert!(text.contains(&format!(r#""system":[{{"text":"Be brief."}},{p}]"#)));
        assert!(text.contains(&format!(r#"{{"text":"two"}},{p}]"#)));
        assert!(text.ends_with(&format!(r#"{{"text":"three"}},{p}]}}]}}"#)));
        // Without a system the tools are what does not change, where they can be marked.
        let mut bare = r.clone();
        bare.system.clear();
        let text = body(&bare);
        assert_eq!(text.contains(&format!("{SPEC},{p}]")), model == CLAUDE);
        assert!(!text.contains("\"system\""));
        bare.tools.clear();
        assert!(!body(&bare).contains("toolConfig"));
    }
}

#[test]
fn turns_alternate() {
    let mut r = request();
    r.messages = vec![
        assistant(vec![Block::text("a")]),
        assistant(vec![Block::text("b"), call("t", "f", ready())]),
        results(&["t"]),
        // Nothing of this can be told: the two beside it are one turn.
        assistant(vec![call("cut", "f", Args::Cut)]),
        Message::user("more"),
        results(&["cut"]),
    ];
    let e = encode(&r, &Profile::bedrock()).unwrap();
    assert!(text_of(&e.body).ends_with(
        r#""messages":[{"role":"assistant","content":[{"text":"a"},{"text":"b"},{"toolUse":{"toolUseId":"t","name":"f","input":{}}}]},{"role":"user","content":[{"toolResult":{"toolUseId":"t","content":[{"text":"ok"}]}},{"text":"more"}]}]}"#
    ));
    let lost: Vec<_> = e.lost.iter().map(|l| (l.message, l.block, l.what)).collect();
    assert_eq!(lost, [(3, 0, Loss::UnfinishedCall), (5, 0, Loss::ResultOfUnfinishedCall)]);
}

#[test]
fn results_are_as_the_provider_takes_them() {
    let of = |model: &str, text: &str, failed| {
        let mut r = request();
        r.model = model.into();
        r.messages = vec![user(vec![Block::result("t", text, failed)])];
        let all = body(&r);
        let at = all.find(r#""messages":[{"role":"user","content":["#).unwrap();
        all[at + 38..all.len() - 4].to_owned()
    };
    let plain =
        |t: &str| format!(r#"{{"toolResult":{{"toolUseId":"t","content":[{{"text":"{t}"}}]}}}}"#);
    assert_eq!(of("m", "out", false), plain("out"));
    assert_eq!(of("m", "out", true), plain("out"));
    assert_eq!(of(CLAUDE, "out", false), plain("out"));
    assert_eq!(
        of(CLAUDE, "out", true),
        r#"{"toolResult":{"toolUseId":"t","content":[{"text":"out"}],"status":"error"}}"#
    );
    assert!(of("amazon.nova-lite-v1:0", "out", true).contains("status"));
    for blank in ["", " \n"] {
        assert_eq!(of("m", blank, false), plain("(empty)"));
    }
}

#[test]
fn other_providers_names_are_made_to_fit() {
    let mut r = request();
    let long = "functions.".to_owned() + &"x".repeat(60);
    let longer = long.clone() + "y";
    let ids = ["call_1", "a.b:c-d", "functions.bash:0", "a b", "é", "", &long, &longer];
    r.messages =
        vec![assistant(ids.iter().map(|id| call(id, "f", ready())).collect()), results(&ids)];
    let v: Value = serde_json::from_str(&body(&r)).unwrap();
    let of = |message: usize, kind: &str| -> Vec<String> {
        let blocks = v["messages"][message]["content"].as_array().unwrap();
        blocks.iter().map(|b| b[kind]["toolUseId"].as_str().unwrap().to_owned()).collect()
    };
    // Calls without an id are not among those that are sent, as `Args::Ready` has them all here.
    let calls = of(0, "toolUse");
    assert_eq!(calls, of(1, "toolResult"));
    assert_eq!(calls[..5], ["call_1", "a.b:c-d", "functions.bash:0", "a_20b", "_e9"]);
    for id in &calls {
        let fits = |b: u8| b.is_ascii_alphanumeric() || b"_.:-".contains(&b);
        assert!((1..=64).contains(&id.len()) && id.bytes().all(fits), "{id}");
    }
    let mut each = calls.clone();
    each.sort();
    each.dedup();
    assert_eq!(each.len(), ids.len(), "{calls:?}");
    // The same each time, also in another process.
    assert!(calls[6].starts_with("functions.xxxxxxxxxxxxxxxxxxxxxx") && calls[6].len() == 64);
    assert_eq!(body(&r), body(&r));

    // A name a model made up, which no tool here could have.
    let name = "n".repeat(65);
    r.messages = vec![assistant(vec![
        call("1", "functions.bash", ready()),
        call("2", "a:b", ready()),
        call("3", &name, ready()),
    ])];
    let text = body(&r);
    assert!(text.contains(r#""name":"functions_2ebash""#) && text.contains(r#""name":"a_3ab""#));
    assert!(!text.contains(&name));
}

/// A turn with everything in it, in the provider's own words.
fn turn() -> Message {
    let lines = [
        delta(0, json!({"reasoningContent":{"text":"why"}})),
        delta(0, json!({"reasoningContent":{"signature":"S0"}})),
        stops(0),
        delta(1, json!({"reasoningContent":{"redactedContent":"UkVE"}})),
        stops(1),
        delta(2, json!({"text":"Look"})),
        delta(2, json!({"text":"ing."})),
        stops(2),
        starts(3, json!({"toolResult":{"toolUseId":"x"}})),
        stops(3),
        starts(4, use_of("t1", "f")),
        delta(4, json!({"toolUse":{"input":"{\"n\": 1.0, \"a\":\"\\u0041\"}"}})),
        stops(4),
        starts(5, use_of("t2", "g")),
        stops(5),
    ];
    let got = blocks(&lines);
    assert_eq!(got.len(), 6);
    assistant(got)
}
fn conversation() -> Vec<Message> {
    vec![
        Message::user("Is it even?"),
        documented("reasoning").message,
        Message::user("and the song?"),
        turn(),
        results(&["t1", "t2"]),
        assistant(vec![call("cut", "f", Args::Cut)]),
        results(&["cut"]),
        documented("text").message,
        Message::user("thanks"),
        Message::user("and"),
    ]
}

#[test]
fn appending_changes_nothing_before() {
    for cache in [Cache::Off, Cache::Short, Cache::Long] {
        let mut r = request();
        r.settings.cache = cache;
        r.model = CLAUDE.into();
        r.messages.clear();
        let mut before: Option<String> = None;
        for message in conversation() {
            r.messages.push(message);
            // Points move, as they are meant to.
            let now = body(&r)
                .replace(r#",{"cachePoint":{"type":"default"}}"#, "")
                .replace(r#",{"cachePoint":{"ttl":"1h","type":"default"}}"#, "");
            assert!(!now.contains("cachePoint"));
            if let Some(before) = before {
                // But for the end of the last message, which another may go on in.
                assert_eq!(now[..before.len() - 4], before[..before.len() - 4]);
            }
            before = Some(now);
        }
        assert!(!before.unwrap().contains("S0"), "blocks of another model");
    }
}

#[test]
fn native_blocks_go_back_to_the_byte_and_only_where_they_belong() {
    let p = Profile::bedrock();
    let mut r = request();
    r.messages = conversation();
    for m in &mut r.messages {
        for n in m.blocks.iter_mut().filter_map(|b| b.native.as_mut()) {
            n.scope =
                axiom_model::codec::scope(&Request { model: "m".into(), ..Request::default() }, &p);
        }
    }
    let encoded = encode(&r, &p).unwrap();
    let text = text_of(&encoded.body);
    // All of them, in their order, with nothing between.
    let parts: Vec<&str> =
        r.messages[3].blocks.iter().filter(|b| b.native.is_some()).map(native).collect();
    assert_eq!(parts.len(), 5);
    assert!(
        text.contains(&format!(r#"{{"role":"assistant","content":[{}]}}"#, parts.join(","))),
        "{text}"
    );
    assert!(text.contains(r#""input":{"n": 1.0, "a":"\u0041"}"#));
    for n in [1, 7] {
        for block in &r.messages[n].blocks {
            assert!(text.contains(native(block)));
        }
    }
    let unfinished = [
        Lost { message: 3, block: 3, what: Loss::Unknown },
        Lost { message: 5, block: 0, what: Loss::UnfinishedCall },
        Lost { message: 6, block: 0, what: Loss::ResultOfUnfinishedCall },
    ];
    assert_eq!(encoded.lost, unfinished);
    let stored = serde_json::to_value(&r.messages).unwrap();
    let mut again = r.clone();
    again.messages = serde_json::from_value(stored).unwrap();
    assert_eq!((again == r, body(&again)), (true, text));

    // Elsewhere: what can be told, and an account of the rest.
    let elsewhere = |p: &Profile, r: &Request| {
        let e = encode(r, p).unwrap();
        let text = text_of(&e.body);
        for gone in ["S0", "UkVE", "<Signature", "why", "PFJl"] {
            assert!(!text.contains(gone), "{gone}: {text}");
        }
        assert!(text.contains(r#"{"text":"Looking."},{"toolUse":{"toolUseId":"t1""#));
        let lost: Vec<_> = e.lost.iter().map(|l| (l.message, l.block, l.what)).collect();
        assert_eq!(
            lost,
            [
                (1, 0, Loss::Reasoning),
                (1, 1, Loss::Reasoning),
                (3, 0, Loss::Reasoning),
                (3, 1, Loss::Reasoning),
                (3, 3, Loss::Unknown),
                (5, 0, Loss::UnfinishedCall),
                (6, 0, Loss::ResultOfUnfinishedCall),
            ]
        );
    };
    r.model = CLAUDE.into();
    elsewhere(&p, &r);
    let mut other = p.clone();
    other.provider = "q".into();
    r.model = "m".into();
    elsewhere(&other, &r);
    r.model = "other".into();
    let mut lenient = p.clone();
    lenient.quirks.replay = Some(Replay::SameProvider);
    assert_eq!(encode(&r, &lenient).unwrap().lost, unfinished);
    // Nor is it another codec's.
    for m in &mut r.messages {
        for n in m.blocks.iter_mut().filter_map(|b| b.native.as_mut()) {
            n.scope.codec = Codec::Messages;
        }
    }
    r.model = "m".into();
    elsewhere(&p, &r);
}

#[test]
fn what_is_in_the_wrong_place_is_said() {
    let mut r = request();
    let unknown = || Block::new(Content::Unknown { name: "u".into() });
    let thought = || Block::new(Content::Reasoning { text: "t".into() });
    r.messages = vec![
        user(vec![call("t", "f", ready()), thought(), unknown()]),
        assistant(vec![Block::result("t", "r", false), unknown(), thought()]),
        Message::user("x"),
    ];
    let e = encode(&r, &Profile::bedrock()).unwrap();
    let lost: Vec<_> = e.lost.iter().map(|l| (l.message, l.block, l.what)).collect();
    assert_eq!(
        lost,
        [
            (0, 0, Loss::Misplaced),
            (0, 1, Loss::Reasoning),
            (0, 2, Loss::Unknown),
            (1, 0, Loss::Misplaced),
            (1, 1, Loss::Unknown),
            (1, 2, Loss::Reasoning),
        ]
    );
    assert!(
        text_of(&e.body).ends_with(r#""messages":[{"role":"user","content":[{"text":"x"}]}]}"#)
    );
}

#[test]
fn a_broken_call_that_came_whole_is_not_sent_back() {
    let lines = [
        START.to_owned(),
        starts(0, use_of("t1", "f")),
        delta(0, json!({"toolUse":{"input":"[]"}})),
        stops(0),
        ended("tool_use"),
    ];
    let (r, _) = read(&amazon::stream(&lines.join("\n")));
    let mut q = request();
    q.messages = vec![Message::user("x"), r.message, results(&["t1"])];
    let e = encode(&q, &Profile::bedrock()).unwrap();
    assert!(!text_of(&e.body).contains("t1"));
    assert_eq!(e.lost.len(), 2);
}
