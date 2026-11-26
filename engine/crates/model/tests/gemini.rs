//! The Gemini codec, on answers from Google's documentation (fixtures/README.md) and on what may come
//! besides.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axiom_model::{
    Args, Block, Cache, Class, Codec, Content, Delta, EncodeError, Error, Event, Json, Loss, Lost,
    Message, Profile, Quirks, Reply, Request, Retry, Role, Settings, Start, Stop, Tool, ToolCall,
    Usage,
    codec::{MAX_ANSWER, MAX_BLOCKS, encode},
    profile::{Effort, Replay},
};
use common::*;
use serde_json::{Value, json};

fn read(bytes: &[u8]) -> (Reply, Option<Error>) {
    reply(Codec::Gemini, &Quirks::default(), bytes)
}
fn documented(name: &str) -> Reply {
    let (reply, error) = read(&fixture(&format!("gemini-from-documentation/{name}.sse")));
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
    assert_eq!(r.id.as_deref(), Some("mAitaLmkHPPlz7IPvtfUqQ4"));
    assert_eq!(r.model.as_deref(), Some("gemini-4-hydrogen"));
    assert_eq!((r.stop, r.message.role), (Some(Stop::EndTurn), Role::Assistant));
    // Pieces of one text, sent back as one.
    let [b] = r.message.blocks.as_slice() else { panic!() };
    let whole = "The image displays the following materials:\n\n*   **Wood:** The accordion and the violin are primarily";
    assert_eq!(b.content, Content::Text { text: whole.into() });
    assert_eq!(native(b), json!({ "text": whole }).to_string());
    assert_eq!(r.usage, Usage::default());
}

#[test]
fn signed_calls() {
    let r = documented("call");
    assert_eq!(r.stop, Some(Stop::ToolUse));
    // Without an id of its own it has one here.
    assert_eq!(calls(&r), [("call_0".into(), "check_flight".into(), json!({"flight":"AA100"}))]);
    assert_eq!(
        native(&r.message.blocks[0]),
        r#"{"functionCall":{"name":"check_flight","args":{"flight":"AA100"}},"thoughtSignature":"<Signature A>"}"#
    );
    // Of calls at once, the first is signed.
    let r = documented("parallel");
    let name = "get_current_temperature";
    assert_eq!(
        calls(&r),
        [
            ("call_0".into(), name.into(), json!({"location":"Paris"})),
            ("call_1".into(), name.into(), json!({"location":"London"})),
        ]
    );
    assert!(native(&r.message.blocks[0]).ends_with(r#""thoughtSignature":"<Signature_A>"}"#));
    assert!(!native(&r.message.blocks[1]).contains("thoughtSignature"));
}

#[test]
fn signed_text() {
    let r = documented("signed-text");
    let [b] = r.message.blocks.as_slice() else { panic!() };
    let text = "I need to calculate the risk. Let me think step-by-step...";
    assert_eq!(b.content, Content::Text { text: text.into() });
    assert!(native(b).ends_with(r#","thoughtSignature":"<Signature_C>"}"#));
}

#[test]
fn what_the_provider_runs_itself_is_kept_and_is_not_a_call() {
    let r = documented("tool-combination");
    let [call, response, own] = r.message.blocks.as_slice() else { panic!() };
    assert_eq!(call.content, Content::Unknown { name: "toolCall".into() });
    assert_eq!(
        native(call),
        r#"{"thoughtSignature":"...","toolCall":{"toolType":"GOOGLE_SEARCH_WEB","args":{"queries":["northernmost city in the United States"]},"id":"a7b3k9p2"}}"#
    );
    assert_eq!(response.content, Content::Unknown { name: "toolResponse".into() });
    assert!(native(own).contains("Utqiaġvik"));
    assert_eq!(
        calls(&r),
        [("m4q8z1v6".into(), "getWeather".into(), json!({"city":"Utqiaġvik, Alaska"}))]
    );
}

#[test]
fn an_answer_that_would_go_on_is_not_taken_for_one_that_ended() {
    let r = documented("continuation");
    assert_eq!(r.stop, Some(Stop::Other("CONTINUATION".into())));
    // Nothing was said, so there is no block.
    assert!(r.message.blocks.is_empty());
    let want = Usage {
        prompt: Some(142),
        output: Some(32768),
        reasoning: Some(32768),
        ..Usage::default()
    };
    assert_eq!(r.usage, want);
}

fn chunk(parts: Value) -> String {
    json!({"candidates":[{"content":{"role":"model","parts":parts},"index":0}],"responseId":"r1","modelVersion":"served"}).to_string()
}
fn ends(reason: &str) -> String {
    json!({"candidates":[{"finishReason":reason}]}).to_string()
}
fn call_part(args: Value) -> Value {
    json!({"functionCall":{"id":"t1","name":"f","args":args}})
}
fn stream(parts: &[&str]) -> Vec<u8> {
    lines(&parts.join("\n"))
}
fn blocks(parts: &[Value]) -> Vec<Block> {
    let mut chunks: Vec<String> = parts.iter().map(|p| chunk(p.clone())).collect();
    chunks.push(ends("STOP"));
    let chunks: Vec<&str> = chunks.iter().map(String::as_str).collect();
    let (r, e) = read(&stream(&chunks));
    assert_eq!((e, r.complete), (None, true));
    r.message.blocks
}

#[test]
fn what_is_signed_is_neither_joined_nor_moved() {
    // As the provider streams it: text in pieces, the last of which may be signed and empty.
    let got = blocks(&[
        json!([{"text":"why ","thought":true}]),
        json!([{"text":"not","thought":true}]),
        json!([{"text":"a"},{"text":"b","thought":false}]),
        json!([{"text":"c","thoughtSignature":"S1"}]),
        json!([{"text":"d","thoughtSignature":"S2"},{"text":"e"}]),
        json!([{"text":""}]),
        json!([{"text":"f"}]),
        json!([{"text":"","thoughtSignature":"S3"}]),
    ]);
    let [thought, text] = got.as_slice() else { panic!("{got:?}") };
    assert_eq!(thought.content, Content::Reasoning { text: "why not".into() });
    assert_eq!(native(thought), r#"{"text":"why not","thought":true}"#);
    assert_eq!(text.content, Content::Text { text: "abcdef".into() });
    assert_eq!(
        native(text),
        r#"[{"text":"ab"},{"text":"c","thoughtSignature":"S1"},{"text":"d","thoughtSignature":"S2"},{"text":"ef"},{"text":"","thoughtSignature":"S3"}]"#
    );

    // With nothing before, it is a thought of which nothing is shown.
    let got = blocks(&[
        json!([{"text":"","thoughtSignature":"S0"}]),
        json!([{"text":"a"}]),
        json!([call_part(json!({}))]),
        json!([{"text":"","thoughtSignature":"S1"}]),
    ]);
    let [first, text, _, last] = got.as_slice() else { panic!("{got:?}") };
    for (block, name) in [(first, "S0"), (last, "S1")] {
        assert_eq!(block.content, Content::Reasoning { text: String::new() });
        assert_eq!(native(block), format!(r#"{{"text":"","thoughtSignature":"{name}"}}"#));
    }
    assert_eq!(native(text), r#"{"text":"a"}"#);
    // After a thought it is its end, though it does not say that it is one.
    let got = blocks(&[json!([{"text":"t","thought":true},{"text":"","thoughtSignature":"S"}])]);
    let [one] = got.as_slice() else { panic!("{got:?}") };
    assert_eq!(one.content, Content::Reasoning { text: "t".into() });
    assert_eq!(native(one), r#"[{"text":"t","thought":true},{"text":"","thoughtSignature":"S"}]"#);
    // Text after a thought is not one, and the other way.
    let got = blocks(&[
        json!([{"text":"a"}]),
        json!([{"text":"t","thought":true,"thoughtSignature":"S"}]),
        json!([{"text":"b","thoughtSignature":"S"}]),
    ]);
    let kinds: Vec<_> = got.iter().map(|b| matches!(b.content, Content::Text { .. })).collect();
    assert_eq!(kinds, [true, false, true]);
    // Members of another kind are kept with their part.
    let got = blocks(&[json!([{"text":"a"},{"text":"b","partMetadata":{"x":1}},{"text":"c"}])]);
    assert_eq!(
        native(&got[0]),
        r#"[{"text":"a"},{"partMetadata":{"x":1},"text":"b"},{"text":"c"}]"#
    );
}

#[test]
fn a_call_is_run_only_if_everything_about_it_is_whole() {
    let run = |parts: &[&str]| {
        let (r, e) = read(&stream(parts));
        let Content::ToolCall(c) = r.message.blocks[0].content.clone() else { panic!() };
        assert_eq!(r.message.calls().count() == 1, matches!(c.args, Args::Ready(_)));
        (c, e.map(|e| e.class))
    };
    let whole = chunk(json!([call_part(json!({"a":1}))]));
    let (c, e) = run(&[&whole, &ends("STOP")]);
    assert_eq!(
        (c.id.as_str(), c.name.as_str(), c.raw.as_str(), e),
        ("t1", "f", r#"{"a":1}"#, None)
    );
    assert_eq!(c.args, Args::Ready(json!({"a":1}).as_object().unwrap().clone()));
    // Both in one.
    let one = json!({"candidates":[{"content":{"parts":[call_part(json!({"a":1}))]},"finishReason":"STOP"}]});
    assert!(matches!(run(&[&one.to_string()]).0.args, Args::Ready(_)));
    // No arguments are none.
    for none in [json!({"functionCall":{"id":"t","name":"f"}}), call_part(Value::Null)] {
        let (c, _) = run(&[&chunk(json!([none])), &ends("STOP")]);
        assert_eq!((c.args, c.raw.as_str()), (Args::Ready(Default::default()), "{}"));
    }

    let cut =
        |parts: &[&str], class| assert_eq!((run(parts).0.args, run(parts).1), (Args::Cut, class));
    cut(&[&whole], Some(Class::CutShort));
    for reason in [
        "FINISH_REASON_UNSPECIFIED",
        "MAX_TOKENS",
        "SAFETY",
        "RECITATION",
        "MALFORMED_FUNCTION_CALL",
        "UNEXPECTED_TOOL_CALL",
        "TOO_MANY_TOOL_CALLS",
        "MISSING_THOUGHT_SIGNATURE",
        "MALFORMED_RESPONSE",
        "CONTINUATION",
        "OTHER",
        "NEW",
    ] {
        cut(&[&whole, &ends(reason)], None);
    }
    // The last reason counts.
    cut(&[&whole, &ends("STOP"), &ends("MAX_TOKENS")], None);
    cut(&[&whole, &ends("STOP"), r#"{"promptFeedback":{"blockReason":"SAFETY"}}"#], None);
    let error = r#"{"error":{"code":503,"message":"m","status":"UNAVAILABLE"}}"#;
    cut(&[&whole, error], Some(Class::Overloaded));
    cut(&[&whole, &ends("STOP"), error], Some(Class::Overloaded));
    cut(&[&whole, &ends("STOP"), "not json"], Some(Class::Protocol));

    let broken = |part: Value| {
        let (c, _) = run(&[&chunk(json!([part])), &ends("STOP")]);
        assert!(matches!(c.args, Args::Broken(_)), "{part}: {c:?}");
    };
    for args in [json!([1]), json!("{}"), json!(7), json!(true)] {
        broken(call_part(args));
    }
    broken(json!({"functionCall":{"id":"t","args":{}}}));
    broken(json!({"functionCall":{"id":"t","name":7,"args":{}}}));
    // In pieces, which this API does not send.
    broken(json!({"functionCall":{"id":"t","name":"f","args":{},"willContinue":true}}));
    broken(
        json!({"functionCall":{"id":"t","name":"f","partialArgs":[{"jsonPath":"$.a","numberValue":1}]}}),
    );
    broken(json!({"functionCall":{"id":"t","name":"f","args":{"a":1},"willContinue":false}}));
}

#[test]
fn calls_without_ids_are_told_apart() {
    let part = |args: Value| json!({"functionCall":{"name":"f","args":args}});
    let with = |id: &str| json!({"functionCall":{"id":id,"name":"f","args":{}}});
    let (r, _) = read(&stream(&[
        &chunk(json!([part(json!({"n":1})), with("own"), with("")])),
        &chunk(json!([part(json!({"n":2})), with("own")])),
        &ends("STOP"),
    ]));
    let ids: Vec<_> = calls(&r).into_iter().map(|c| c.0).collect();
    // Of two with one id the first.
    assert_eq!(ids, ["r1_0", "own", "r1_2", "r1_3"]);
    // Named after the answer, so that another's are others.
    let (r, _) = read(&stream(&[
        &json!({"candidates":[{"content":{"parts":[part(json!({}))]},"finishReason":"STOP"}]})
            .to_string(),
    ]));
    assert_eq!(calls(&r)[0].0, "call_0");
}

#[test]
fn what_is_newer_than_this_build_survives() {
    let code = r#"{"executableCode":{"language":"PYTHON", "code":"1"},"thoughtSignature":"S"}"#;
    let raw = format!(
        r#"{{"candidates":[{{"content":{{"parts":[{code},{{"thought":true,"brandNew":1.0}},{{}}],"novel":1}},"groundingMetadata":{{}},"novel":[]}}],"novel":{{}},"usageMetadata":null}}"#
    );
    let (r, e) = read(&stream(&[&raw, r#"{"candidates":[{"content":{}}]}"#, "{}", &ends("STOP")]));
    assert_eq!(e, None);
    let [code_block, new, empty] = r.message.blocks.as_slice() else { panic!() };
    assert_eq!(code_block.content, Content::Unknown { name: "executableCode".into() });
    assert_eq!(native(code_block), code);
    assert_eq!(new.content, Content::Unknown { name: "brandNew".into() });
    assert_eq!(native(new), r#"{"thought":true,"brandNew":1.0}"#);
    assert_eq!((&empty.content, native(empty)), (&Content::Unknown { name: String::new() }, "{}"));
}

#[test]
fn usage_is_what_was_told() {
    let of = |usage: Value| {
        let u = json!({ "usageMetadata": usage }).to_string();
        read(&stream(&[&chunk(json!([])), &u, &ends("STOP")])).0.usage
    };
    let got = of(
        json!({"promptTokenCount":100,"cachedContentTokenCount":60,"candidatesTokenCount":7,"thoughtsTokenCount":5,"toolUsePromptTokenCount":11,"totalTokenCount":123}),
    );
    let want = Usage {
        prompt: Some(100),
        uncached: Some(40),
        cache_read: Some(60),
        output: Some(12),
        reasoning: Some(5),
        ..Usage::default()
    };
    assert_eq!(got, want);
    // Without thoughts among the counts, the total says how much there was.
    let got = of(json!({"promptTokenCount":10,"candidatesTokenCount":7,"totalTokenCount":22}));
    assert_eq!((got.output, got.reasoning, got.uncached), (Some(12), None, None));
    let got = of(json!({"candidatesTokenCount":7,"thoughtsTokenCount":5}));
    assert_eq!(got.output, Some(12));
    // What does not follow is not said.
    assert_eq!(of(json!({"candidatesTokenCount":7})).output, None);
    assert_eq!(of(json!({"promptTokenCount":3,"totalTokenCount":2})).output, None);
    assert_eq!(of(json!({"promptTokenCount":3,"cachedContentTokenCount":4})).uncached, None);
    for none in [Value::Null, json!(7), json!({"promptTokenCount":-1,"totalTokenCount":"9"})] {
        assert_eq!(of(none), Usage::default());
    }
    // It is told with each piece, and the last counts.
    let (a, b) = (
        json!({"usageMetadata":{"promptTokenCount":1,"thoughtsTokenCount":1}}).to_string(),
        json!({"usageMetadata":{"promptTokenCount":2}}).to_string(),
    );
    let got = read(&stream(&[&a, &b, &ends("STOP")])).0.usage;
    assert_eq!((got.prompt, got.reasoning), (Some(2), None));
}

#[test]
fn stops() {
    let stop = |e: &str| {
        let (r, e) = read(&stream(&[e]));
        assert_eq!((e, r.complete), (None, true));
        r.stop
    };
    assert_eq!(stop(&ends("STOP")), Some(Stop::EndTurn));
    assert_eq!(stop(&ends("MAX_TOKENS")), Some(Stop::MaxOutput));
    for reason in [
        "SAFETY",
        "RECITATION",
        "LANGUAGE",
        "BLOCKLIST",
        "PROHIBITED_CONTENT",
        "SPII",
        "IMAGE_SAFETY",
        "IMAGE_PROHIBITED_CONTENT",
        "IMAGE_RECITATION",
        "ESCALATION",
        "PUP_LIMITED_DISABLED",
    ] {
        assert_eq!(stop(&ends(reason)), Some(Stop::Refusal(Some(reason.into()))));
    }
    for reason in ["OTHER", "MALFORMED_FUNCTION_CALL", "NEW"] {
        assert_eq!(stop(&ends(reason)), Some(Stop::Other(reason.into())));
    }
    assert_eq!(
        stop(r#"{"promptFeedback":{"blockReason":"PROHIBITED_CONTENT"}}"#),
        Some(Stop::Refusal(Some("PROHIBITED_CONTENT".into())))
    );
    // Only one answer was asked for.
    let other = r#"{"candidates":[{"index":1,"content":{"parts":[{"text":"no"}]},"finishReason":"MAX_TOKENS"},{"content":{"parts":[{"text":"yes"}]}}]}"#;
    let (r, _) = read(&stream(&[other, &ends("STOP")]));
    assert_eq!(r.message.blocks[0].content, Content::Text { text: "yes".into() });
    assert_eq!((r.stop, r.message.blocks.len()), (Some(Stop::EndTurn), 1));
}

#[test]
fn what_goes_wrong_is_said() {
    // docs/api-errors: a wrong key is a 400 there.
    let page = fixture("gemini-from-documentation/error.json");
    let one = serde_json::from_slice::<Value>(&page).unwrap().to_string();
    let (r, e) = read(&stream(&[&chunk(json!([{"text":"half"}])), &one]));
    let e = e.unwrap();
    assert_eq!((e.class, e.retry), (Class::Auth, Retry::Never));
    assert_eq!(e.message, "API key not valid. Please pass a valid API key.");
    assert_eq!(e.code.as_deref(), Some("API_KEY_INVALID"));
    let [b] = r.message.blocks.as_slice() else { panic!() };
    assert_eq!(b.content, Content::Text { text: "half".into() });
    assert!(b.cut && b.native.is_none() && !r.complete);

    let class = |code: u16, status: &str| {
        let e = json!({"error":{"code":code,"message":"m","status":status}}).to_string();
        let e = read(&stream(&[&e])).1.unwrap();
        assert_eq!(e.code.as_deref(), Some(status));
        (e.class, e.retry)
    };
    for (code, status, want) in [
        (400, "INVALID_ARGUMENT", (Class::BadRequest, Retry::Never)),
        (400, "FAILED_PRECONDITION", (Class::BadRequest, Retry::Never)),
        (401, "UNAUTHENTICATED", (Class::Auth, Retry::Never)),
        (403, "PERMISSION_DENIED", (Class::Auth, Retry::Never)),
        (404, "NOT_FOUND", (Class::BadRequest, Retry::Never)),
        (429, "RESOURCE_EXHAUSTED", (Class::RateLimit, Retry::Restart)),
        (500, "INTERNAL", (Class::Server, Retry::Restart)),
        (503, "UNAVAILABLE", (Class::Overloaded, Retry::Restart)),
        (504, "DEADLINE_EXCEEDED", (Class::Server, Retry::Restart)),
        // By its name, without a number that says anything.
        (0, "UNAVAILABLE", (Class::Overloaded, Retry::Restart)),
        (0, "RESOURCE_EXHAUSTED", (Class::RateLimit, Retry::Restart)),
        (0, "UNAUTHENTICATED", (Class::Auth, Retry::Never)),
        (0, "PERMISSION_DENIED", (Class::Auth, Retry::Never)),
        (0, "INVALID_ARGUMENT", (Class::BadRequest, Retry::Never)),
        (0, "FAILED_PRECONDITION", (Class::BadRequest, Retry::Never)),
        (0, "NOT_FOUND", (Class::BadRequest, Retry::Never)),
        // By its number, where the name is new.
        (429, "BRAND_NEW", (Class::RateLimit, Retry::Restart)),
        (200, "BRAND_NEW", (Class::Server, Retry::Restart)),
    ] {
        assert_eq!(class(code, status), want, "{status}");
    }
    let class = |parts: &[&str]| read(&stream(parts)).1.map(|e| (e.class, e.retry));
    let broken = Some((Class::Protocol, Retry::Never));
    for bad in [
        "not json",
        // Not asked for as a stream of events.
        "[{}]",
        r#"{"candidates":{}}"#,
        r#"{"candidates":[{"content":{"parts":[7]}}]}"#,
        r#"{"candidates":[{"content":{"parts":[{"functionCall":7}]}}]}"#,
        r#"{"candidates":[{"finishReason":7}]}"#,
    ] {
        assert_eq!(class(&[bad]), broken, "{bad}");
    }
    let short = Some((Class::CutShort, Retry::Restart));
    assert_eq!(class(&[]), short);
    assert_eq!(class(&[&chunk(json!([{"text":"a"}]))]), short);
    assert_eq!(class(&[r#"{"candidates":[{"finishReason":null}],"error":null}"#]), short);
}

#[test]
fn an_answer_is_bounded() {
    let flip = |n: usize| chunk(json!([{"text":"x","thought":n.is_multiple_of(2)}]));
    for part in [
        flip as fn(usize) -> String,
        |_| chunk(json!([call_part(json!({}))])),
        |_| chunk(json!([{"new":1}])),
    ] {
        let many: Vec<String> = (0..=MAX_BLOCKS).map(part).collect();
        let parts: Vec<&str> = many.iter().map(String::as_str).collect();
        let (r, e) = read(&stream(&parts));
        assert_eq!((r.message.blocks.len(), e.unwrap().class), (MAX_BLOCKS, Class::Protocol));
    }
    let mut d = axiom_model::codec::Decoder::new(Codec::Gemini, &Quirks::default());
    let data = format!(r#"{{"novel":"{}"}}"#, "x".repeat((1 << 20) - 12));
    assert_eq!(data.len(), 1 << 20);
    let event = axiom_model::sse::Event { name: String::new(), data };
    let mut out = Vec::new();
    for _ in 0..MAX_ANSWER >> 20 {
        d.event(&event, &mut out).unwrap();
    }
    assert_eq!(d.event(&event, &mut out).unwrap_err().class, Class::Protocol);
    assert!(d.ended());
}

#[test]
fn events_as_they_come() {
    let parts = [
        chunk(json!([{"text":"a"}])),
        chunk(json!([{"text":"b"}, call_part(json!({"n":1}))])),
        ends("STOP"),
    ];
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    let (got, _) = events(Codec::Gemini, &Quirks::default(), &[&stream(&parts)]);
    let text = |t: &str| Event::Delta { block: 0, delta: Delta::Text(t.into()) };
    assert_eq!(got[0], Event::Started { id: Some("r1".into()), model: Some("served".into()) });
    assert_eq!(got[1], Event::BlockStart { block: 0, start: Start::Text });
    assert_eq!((&got[2], &got[3]), (&text("a"), &text("b")));
    assert!(matches!(&got[4], Event::BlockStop { block: 0, native: Some(_), broken: None }));
    let start = Start::ToolCall { id: "t1".into(), name: "f".into() };
    assert_eq!(got[5], Event::BlockStart { block: 1, start });
    assert_eq!(got[6], Event::Delta { block: 1, delta: Delta::Args(r#"{"n":1}"#.into()) });
    assert!(matches!(&got[7], Event::BlockStop { block: 1, native: Some(_), broken: None }));
    assert_eq!((got.len(), &got[8], &got[9]), (10, &Event::Stop(Stop::ToolUse), &Event::End));
}

// Requests.

fn profile() -> Profile {
    let mut p = Profile::gemini();
    p.provider = "p".into();
    p
}
fn request() -> Request {
    Request {
        model: "m".into(),
        system: "Be brief.".into(),
        tools: vec![Tool {
            name: "f".into(),
            description: "does \"f\"".into(),
            schema: Json::parse(r#"{"type":"object", "properties":{"z":{},"a":{}}}"#).unwrap(),
        }],
        messages: vec![Message::user("hi")],
        settings: Settings::default(),
    }
}
fn user(blocks: Vec<Block>) -> Message {
    Message { role: Role::User, blocks }
}
fn assistant(blocks: Vec<Block>) -> Message {
    Message { role: Role::Assistant, blocks }
}
fn results(ids: &[&str]) -> Message {
    user(ids.iter().map(|id| Block::result(*id, "ok", false)).collect())
}
fn text_of(body: &[u8]) -> String {
    String::from_utf8(body.to_vec()).unwrap()
}
fn body(request: &Request) -> String {
    text_of(&encode(request, &profile()).unwrap().body)
}
fn call(id: &str, args: Args) -> Block {
    let raw = r#"{"q": 1}"#.into();
    Block::new(Content::ToolCall(ToolCall { id: id.into(), name: "f".into(), raw, args }))
}
fn ready() -> Args {
    Args::Ready(json!({"q": 1}).as_object().unwrap().clone())
}

#[test]
fn a_request_to_the_byte() {
    let mut r = request();
    assert_eq!(
        body(&r),
        r#"{"systemInstruction":{"parts":[{"text":"Be brief."}]},"tools":[{"functionDeclarations":[{"name":"f","description":"does \"f\"","parametersJsonSchema":{"type":"object", "properties":{"z":{},"a":{}}}}]}],"contents":[{"role":"user","parts":[{"text":"hi"}]}]}"#
    );
    r.settings = Settings {
        max_output: Some(9),
        effort: Some("high".into()),
        stop: vec!["END".into()],
        cache: Cache::Long,
        ..Settings::default()
    };
    // Calls of another model: the first of each step is marked as not signed.
    r.messages.push(assistant(vec![
        Block::text("yes"),
        Block::text(""),
        call("a:1", ready()),
        call("b", ready()),
    ]));
    r.messages.push(user(vec![
        Block::text("one"),
        Block::text(""),
        Block::result("a:1", "", true),
        Block::result("b", "ok", false),
        Block::text("two"),
    ]));
    r.messages.push(assistant(vec![call("c", ready())]));
    (r.system, r.tools) = (String::new(), Vec::new());
    assert_eq!(
        body(&r),
        r#"{"generationConfig":{"maxOutputTokens":9,"stopSequences":["END"],"thinkingConfig":{"thinkingLevel":"high"}},"contents":[{"role":"user","parts":[{"text":"hi"}]},{"role":"model","parts":[{"text":"yes"},{"functionCall":{"id":"a:1","name":"f","args":{"q":1}},"thoughtSignature":"skip_thought_signature_validator"},{"functionCall":{"id":"b","name":"f","args":{"q":1}}}]},{"role":"user","parts":[{"functionResponse":{"id":"a:1","name":"f","response":{"error":""}}},{"functionResponse":{"id":"b","name":"f","response":{"output":"ok"}}},{"text":"one"},{"text":"two"}]},{"role":"model","parts":[{"functionCall":{"id":"c","name":"f","args":{"q":1}},"thoughtSignature":"skip_thought_signature_validator"}]}]}"#
    );
}

#[test]
fn what_cannot_be_asked_for_is_an_error() {
    let mut r = request();
    r.settings.effort = Some("high".into());
    let mut p = profile();
    for other in
        [None, Some(Effort::ReasoningEffort), Some(Effort::OutputConfig), Some(Effort::Reasoning)]
    {
        p.quirks.effort = other;
        assert_eq!(encode(&r, &p).unwrap_err(), EncodeError::Unsupported("effort"));
    }
    p.quirks.body.insert("safetySettings".into(), json!([]));
    assert!(
        text_of(&encode(&request(), &p).unwrap().body)
            .starts_with(r#"{"safetySettings":[],"systemInstruction""#)
    );
    for own in ["model", "contents", "systemInstruction", "tools", "generationConfig"] {
        p.quirks.body = [(own.to_owned(), json!(1))].into();
        assert_eq!(encode(&request(), &p).unwrap_err(), EncodeError::Reserved(own.into()));
    }
    // A schema the provider would not heed is said in words it has, or refused.
    let mut r = request();
    r.tools[0].schema =
        Json::parse(r#"{"type":"object","properties":{"p":{"pattern":"^a"}}}"#).unwrap();
    assert!(body(&r).contains(
        r#""parametersJsonSchema":{"type":"object","properties":{"p":{"description":"Must also satisfy: {\"pattern\":\"^a\"}"}}}"#
    ));
    r.tools[0].schema = Json::parse(r#"{"type":"string"}"#).unwrap();
    let e = encode(&r, &profile()).unwrap_err();
    assert!(matches!(&e, EncodeError::Schema { tool, .. } if tool == "f"));
    assert_eq!(
        e.to_string(),
        "the schema of the tool \"f\" cannot be sent to this provider: it does not describe an object"
    );
}

/// A turn with everything in it, in the provider's own words.
fn turn() -> Message {
    let (r, e) = read(&stream(&[
        &chunk(json!([{"text":"why","thought":true,"thoughtSignature":"S0"}])),
        &chunk(json!([{"text":"Look"}])),
        &chunk(json!([{"text":"ing.","thoughtSignature":"S1"}])),
        &chunk(json!([{"executableCode":{"code":"1"},"thoughtSignature":"S2"}])),
        &chunk(
            json!([{"functionCall":{"id":"t1","name":"f","args":{"n":1.0}},"thoughtSignature":"S3"}]),
        ),
        &chunk(json!([{"functionCall":{"name":"g","args":{}}}])),
        &ends("STOP"),
    ]));
    assert_eq!((e, calls(&r).len()), (None, 2));
    r.message
}
fn conversation() -> Vec<Message> {
    vec![
        Message::user("What is the risk?"),
        documented("signed-text").message,
        Message::user("and the weather?"),
        turn(),
        results(&["t1", "r1_1"]),
        assistant(vec![call("cut", Args::Cut)]),
        results(&["cut"]),
        documented("text").message,
        Message::user("thanks"),
    ]
}

#[test]
fn appending_changes_nothing_before() {
    for cache in [Cache::Off, Cache::Short, Cache::Long] {
        let mut r = request();
        r.settings.cache = cache;
        r.messages.clear();
        let mut before: Option<String> = None;
        for message in conversation() {
            r.messages.push(message);
            let now = body(&r);
            if let Some(before) = before {
                assert_eq!(now[..before.len() - 2], before[..before.len() - 2]);
            }
            before = Some(now);
        }
        assert!(before.unwrap().contains("S3"));
    }
}

#[test]
fn native_parts_go_back_to_the_byte_and_only_where_they_belong() {
    let mut r = request();
    r.messages = conversation();
    let encoded = encode(&r, &profile()).unwrap();
    let text = text_of(&encoded.body);
    // All of them, in their order, with nothing between; what was several parts is several again.
    let parts: Vec<&str> = r.messages[3]
        .blocks
        .iter()
        .map(|b| native(b).trim_start_matches('[').trim_end_matches(']'))
        .collect();
    assert!(
        text.contains(&format!(r#"{{"role":"model","parts":[{}]}}"#, parts.join(","))),
        "{text}"
    );
    for n in [1, 7] {
        for block in &r.messages[n].blocks {
            assert!(text.contains(native(block)));
        }
    }
    // A result has the id its call came with, and none if it came without.
    assert!(text.contains(
        r#""parts":[{"functionResponse":{"id":"t1","name":"f","response":{"output":"ok"}}},{"functionResponse":{"name":"g","response":{"output":"ok"}}}]"#
    ));
    assert!(!text.contains("skip_thought"));
    let unfinished = [
        Lost { message: 5, block: 0, what: Loss::UnfinishedCall },
        Lost { message: 6, block: 0, what: Loss::ResultOfUnfinishedCall },
    ];
    assert_eq!(encoded.lost, unfinished);
    let stored = serde_json::to_value(&r.messages).unwrap();
    let mut again = r.clone();
    again.messages = serde_json::from_value(stored).unwrap();
    assert_eq!((again == r, body(&again)), (true, text));

    // Elsewhere: what can be told, and an account of the rest, signatures included.
    let elsewhere = |p: &Profile, r: &Request| {
        let e = encode(r, p).unwrap();
        let text = text_of(&e.body);
        for gone in ["S0", "S1", "S2", "S3", "<Signature_C>", "executableCode", "why"] {
            assert!(!text.contains(gone), "{gone}: {text}");
        }
        assert!(text.contains(r#"{"text":"Looking."}"#));
        assert!(text.contains(r#"{"functionCall":{"id":"t1","name":"f","args":{"n":1.0}},"thoughtSignature":"skip_thought_signature_validator"},{"functionCall":{"id":"r1_1","name":"g","args":{}}}]"#));
        assert!(text.contains(r#"{"functionResponse":{"id":"r1_1","name":"g""#));
        let lost: Vec<_> = e.lost.iter().map(|l| (l.message, l.block, l.what)).collect();
        assert_eq!(
            lost,
            [
                (1, 0, Loss::Reasoning),
                (3, 0, Loss::Reasoning),
                (3, 1, Loss::Reasoning),
                (3, 2, Loss::Unknown),
                (3, 3, Loss::Reasoning),
                (5, 0, Loss::UnfinishedCall),
                (6, 0, Loss::ResultOfUnfinishedCall),
            ]
        );
    };
    r.model = "other".into();
    elsewhere(&profile(), &r);
    let mut other = profile();
    other.provider = "q".into();
    r.model = "m".into();
    elsewhere(&other, &r);
    // Where a provider drops what another of its models cannot read, all of it.
    r.model = "other".into();
    let mut lenient = profile();
    lenient.quirks.replay = Some(Replay::SameProvider);
    assert_eq!(encode(&r, &lenient).unwrap().lost, unfinished);
}

#[test]
fn a_result_is_named_after_its_call() {
    let mut r = request();
    let named = |id: &str, name: &str| {
        let raw = "{}".into();
        Block::new(Content::ToolCall(ToolCall {
            id: id.into(),
            name: name.into(),
            raw,
            args: ready(),
        }))
    };
    // Ids need not be unique in a history; a result belongs to the last call.
    r.messages = vec![
        assistant(vec![named("t", "f")]),
        results(&["t"]),
        assistant(vec![named("t", "g")]),
        results(&["t", "never"]),
    ];
    let e = encode(&r, &profile()).unwrap();
    let text = text_of(&e.body);
    assert!(text.contains(r#"{"functionResponse":{"id":"t","name":"f""#));
    assert!(text.contains(r#"{"functionResponse":{"id":"t","name":"g""#));
    assert_eq!(e.lost, [Lost { message: 3, block: 1, what: Loss::ResultWithoutCall }]);
    assert!(!text.contains("never"));
}

#[test]
fn a_step_whose_signed_call_is_left_out_is_still_signed() {
    let mut r = request();
    let mut half = turn();
    let Content::ToolCall(c) = &mut half.blocks[3].content else { panic!() };
    c.args = Args::Broken("why".into());
    r.messages = vec![Message::user("x"), half, results(&["t1", "r1_1"])];
    let e = encode(&r, &profile()).unwrap();
    let text = text_of(&e.body);
    assert!(text.contains(
        r#"{"functionCall":{"args":{},"name":"g"},"thoughtSignature":"skip_thought_signature_validator"}"#
    ), "{text}");
    let lost: Vec<_> = e.lost.iter().map(|l| (l.message, l.block, l.what)).collect();
    assert_eq!(lost, [(1, 3, Loss::UnfinishedCall), (2, 0, Loss::ResultOfUnfinishedCall)]);
    // Elsewhere too it is one loss, and not also that of its signature.
    let e = encode(&r, &Profile::openai()).unwrap();
    let lost: Vec<_> = e.lost.iter().filter(|l| l.block == 3).map(|l| l.what).collect();
    assert_eq!(lost, [Loss::UnfinishedCall]);
    // Only the first of those that are left.
    let (reply, _) = read(&stream(&[
        &chunk(json!([{"functionCall":{"id":"a","name":"f","args":[]},"thoughtSignature":"S"}])),
        &chunk(json!([{"functionCall":{"id":"b","name":"f","args":{}}}])),
        &chunk(json!([{"functionCall":{"id":"c","name":"f","args":{}}}])),
        &ends("STOP"),
    ]));
    assert_eq!(calls(&reply).len(), 2);
    r.messages = vec![Message::user("x"), reply.message.clone()];
    assert_eq!(body(&r).matches("skip_thought").count(), 1);
    assert!(body(&r).contains(
        r#""thoughtSignature":"skip_thought_signature_validator"},{"functionCall":{"args":{},"id":"c""#
    ));
    // Nor a call of another model after one that is signed.
    let mut mixed = documented("call").message;
    mixed.blocks.push(call("d", ready()));
    r.messages[1] = mixed;
    assert!(body(&r).contains("<Signature A>") && !body(&r).contains("skip_thought"));
    // One that was never signed is as it came.
    let (reply, _) =
        read(&stream(&[&chunk(json!([{"functionCall":{"name":"g","args":{}}}])), &ends("STOP")]));
    r.messages = vec![Message::user("x"), reply.message];
    assert!(!body(&r).contains("thoughtSignature"));
    // And one that is signed is too, whatever was left out before it.
    let mut cut = call("cut", Args::Cut);
    cut.native = None;
    let mut own = documented("call").message;
    own.blocks.insert(0, cut);
    r.messages = vec![Message::user("x"), own];
    assert!(body(&r).contains("<Signature A>") && !body(&r).contains("skip_thought"));
}

#[test]
fn what_is_in_the_wrong_place_is_said() {
    let mut r = request();
    let unknown = || Block::new(Content::Unknown { name: "u".into() });
    let thought = || Block::new(Content::Reasoning { text: "t".into() });
    r.messages = vec![
        user(vec![call("t", ready()), thought(), unknown()]),
        assistant(vec![Block::result("t", "r", false), unknown(), thought()]),
        Message::user("x"),
    ];
    let e = encode(&r, &profile()).unwrap();
    let lost: Vec<_> = e.lost.iter().map(|l| (l.message, l.block, l.what)).collect();
    assert_eq!(
        lost,
        [
            (0, 0, Loss::Misplaced),
            (0, 1, Loss::Misplaced),
            (0, 2, Loss::Unknown),
            (1, 0, Loss::Misplaced),
            (1, 1, Loss::Unknown),
            (1, 2, Loss::Reasoning),
        ]
    );
    // A message that would be empty is not there.
    assert!(text_of(&e.body).ends_with(r#""contents":[{"role":"user","parts":[{"text":"x"}]}]}"#));
}
