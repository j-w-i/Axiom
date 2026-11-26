//! The Messages codec, on streams and bodies from Anthropic's documentation (fixtures/README.md) and
//! on what may come besides.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axiom_model::{
    Args, Block, Cache, Class, Codec, Content, Delta, EncodeError, Event, Json, Loss, Lost,
    Message, Profile, Quirks, Request, Retry, Role, Settings, Start, Stop, Tool, ToolCall, Usage,
    codec::{MAX_ANSWER, MAX_BLOCKS, encode},
};
use common::*;
use serde_json::{Value, json};

fn read(bytes: &[u8]) -> (axiom_model::Reply, Option<axiom_model::Error>) {
    reply(Codec::Messages, &Quirks::default(), bytes)
}
fn documented(name: &str) -> axiom_model::Reply {
    let (reply, error) = read(&fixture(&format!("messages-from-documentation/{name}.sse")));
    assert_eq!(error, None);
    assert!(reply.complete);
    reply
}
fn native(block: &Block) -> &str {
    block.native.as_ref().unwrap().json.get()
}

#[test]
fn text() {
    let r = documented("text");
    assert_eq!(r.id.as_deref(), Some("msg_1nZdL29xx5MUA1yADyHTEsnR8uuvGzszyY"));
    assert_eq!(r.model.as_deref(), Some("claude-opus-5-5"));
    assert_eq!(r.stop, Some(Stop::EndTurn));
    assert_eq!(r.message.role, Role::Assistant);
    let [b] = r.message.blocks.as_slice() else { panic!() };
    assert_eq!(b.content, Content::Text { text: "Hello!".into() });
    assert_eq!(native(b), r#"{"type":"text","text":"Hello!"}"#);
    assert_eq!((b.cut, b.extra.is_empty()), (false, true));
    // Nothing was said of caches, so the prompt's size does not follow.
    let want = Usage { uncached: Some(25), output: Some(15), ..Usage::default() };
    assert_eq!(r.usage, want);
}

#[test]
fn a_tool_call() {
    let r = documented("tool");
    assert_eq!(r.stop, Some(Stop::ToolUse));
    let [text, call] = r.message.blocks.as_slice() else { panic!() };
    assert_eq!(
        text.content,
        Content::Text { text: "Okay, let's check the weather for San Francisco, CA:".into() }
    );
    // The arguments to the byte, space included.
    assert_eq!(
        native(call),
        r#"{"type":"tool_use","id":"toolu_01T1x1fJ34qAmk2tNTrN7Up6","name":"get_weather","input":{"location": "San Francisco, CA"}}"#
    );
    let calls: Vec<_> = r.message.calls().collect();
    let [c] = calls.as_slice() else { panic!() };
    assert_eq!((c.id(), c.name()), ("toolu_01T1x1fJ34qAmk2tNTrN7Up6", "get_weather"));
    assert_eq!(c.args()["location"], "San Francisco, CA");
}

#[test]
fn signed_thinking() {
    let r = documented("thinking");
    let [thinking, text] = r.message.blocks.as_slice() else { panic!() };
    let Content::Reasoning { text: shown } = &thinking.content else { panic!() };
    assert!(shown.starts_with("I need to find the GCD") && shown.ends_with("= 21."));
    let v: Value = serde_json::from_str(native(thinking)).unwrap();
    assert_eq!(v["signature"], "EqQBCgIYAhIM1gbcDa9GJwZA2b3hGgxBdjrkzLoky3dl1pkiMOYds...");
    assert_eq!(v["thinking"].as_str().unwrap(), shown);
    assert!(thinking.extra.is_empty());
    assert!(matches!(&text.content, Content::Text { text } if text.contains("**21**")));
    assert_eq!(r.usage, Usage::default());
}

#[test]
fn what_the_provider_runs_itself_is_kept_and_is_not_a_call() {
    let r = documented("web-search");
    let [_, used, result, _] = r.message.blocks.as_slice() else { panic!() };
    assert_eq!(used.content, Content::Unknown { name: "server_tool_use".into() });
    assert_eq!(
        native(used),
        r#"{"type":"server_tool_use","id":"srvtoolu_014hJH82Qum7Td6UV8gDXThB","name":"web_search","input":{"query": "weather NYC today"}}"#
    );
    assert_eq!(result.content, Content::Unknown { name: "web_search_tool_result".into() });
    assert!(native(result).contains(r#""encrypted_content":"Ev0DCioIAxgCIiQ3NmU4ZmI4OC1k...""#));
    assert_eq!(r.message.calls().count(), 0);
    assert_eq!(
        r.usage,
        Usage {
            prompt: Some(10682),
            uncached: Some(10682),
            cache_read: Some(0),
            cache_write: Some(0),
            output: Some(510),
            ..Usage::default()
        }
    );
}

const START: &str = r#"{"type":"message_start","message":{"id":"m","model":"served","usage":{"input_tokens":10,"cache_read_input_tokens":200,"cache_creation_input_tokens":30,"cache_creation":{"ephemeral_5m_input_tokens":20,"ephemeral_1h_input_tokens":10},"output_tokens":1}}}"#;
const USE: &str = r#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"t1","name":"f","input":{}}}"#;

fn stream(parts: &[&str]) -> Vec<u8> {
    lines(&parts.join("\n"))
}
fn args(text: &str) -> String {
    json!({"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":text}})
        .to_string()
}
fn ends(reason: &str) -> String {
    json!({"type":"message_delta","delta":{"stop_reason":reason},"usage":{"output_tokens":7,"output_tokens_details":{"thinking_tokens":5}}})
        .to_string()
}
const BLOCK_STOP: &str = r#"{"type":"content_block_stop","index":0}"#;
const STOP: &str = r#"{"type":"message_stop"}"#;

#[test]
fn counts_are_kept_until_they_are_told_again() {
    let (r, _) = read(&stream(&[START, &ends("end_turn"), STOP]));
    let want = Usage {
        prompt: Some(240),
        uncached: Some(10),
        cache_read: Some(200),
        cache_write: Some(30),
        cache_write_long: Some(10),
        output: Some(7),
        reasoning: Some(5),
        ..Usage::default()
    };
    assert_eq!(r.usage, want);
    // How fast it was done, whichever event says so.
    for (first, last) in [(json!("fast"), json!(null)), (json!("standard"), json!("fast"))] {
        let a = json!({"type":"message_start","message":{"usage":{"speed":first}}}).to_string();
        let z = json!({"type":"message_delta","delta":{},"usage":{"speed":last}}).to_string();
        assert_eq!(read(&stream(&[&a, &z, STOP])).0.usage.speed.as_deref(), Some("fast"));
    }
    // Nonsense is not a count.
    let odd = r#"{"type":"message_start","message":{"usage":{"input_tokens":-1,"cache_read_input_tokens":"5","cache_creation_input_tokens":1.5,"output_tokens":null}}}"#;
    assert_eq!(read(&stream(&[odd])).0.usage, Usage::default());
    let vast = r#"{"type":"message_start","message":{"usage":{"input_tokens":18446744073709551615,"cache_read_input_tokens":1,"cache_creation_input_tokens":0}}}"#;
    assert_eq!(read(&stream(&[vast])).0.usage.prompt, None);
}

#[test]
fn a_call_is_run_only_if_everything_about_it_is_whole() {
    let whole = args(r#"{"a":1}"#);
    let run = |parts: &[&str]| {
        let (r, e) = read(&stream(parts));
        assert_eq!(
            r.message.calls().count() == 0,
            r.message.blocks.iter().all(|b| {
                !matches!(&b.content, Content::ToolCall(c) if matches!(c.args, Args::Ready(_)))
            })
        );
        let Content::ToolCall(c) = r.message.blocks[0].content.clone() else { panic!() };
        (c.args, e.map(|e| e.class))
    };
    let ready = |v: Value| Args::Ready(v.as_object().unwrap().clone());
    let (tool_use, end_turn) = (ends("tool_use"), ends("end_turn"));
    assert_eq!(
        run(&[START, USE, &whole, BLOCK_STOP, &tool_use, STOP]),
        (ready(json!({"a":1})), None)
    );
    assert_eq!(
        run(&[START, USE, &whole, BLOCK_STOP, &end_turn, STOP]),
        (ready(json!({"a":1})), None)
    );
    // No piece of the arguments at all: the block's own input, as documented for tools without.
    assert_eq!(run(&[START, USE, BLOCK_STOP, &tool_use, STOP]), (ready(json!({})), None));
    let given = USE.replace(r#""input":{}"#, r#""input":{"b":[2]}"#);
    assert_eq!(run(&[START, &given, BLOCK_STOP, &tool_use, STOP]), (ready(json!({"b":[2]})), None));

    let cut = |parts: &[&str], class| assert_eq!(run(parts), (Args::Cut, class), "{parts:?}");
    let short = Some(Class::CutShort);
    cut(&[START, USE, &whole, BLOCK_STOP, &tool_use], short);
    cut(&[START, USE, &whole, BLOCK_STOP], short);
    cut(&[START, USE, &whole], short);
    cut(&[START, USE, &whole, &tool_use, STOP], None);
    cut(&[START, USE, &whole, BLOCK_STOP, STOP], None);
    for reason in [
        "max_tokens",
        "model_context_window_exceeded",
        "refusal",
        "pause_turn",
        "stop_sequence",
        "new",
    ] {
        cut(&[START, USE, &whole, BLOCK_STOP, &ends(reason), STOP], None);
        // What is left may look like a call without arguments.
        cut(&[START, USE, BLOCK_STOP, &ends(reason), STOP], None);
    }
    let overloaded =
        r#"{"type":"error","error":{"type":"overloaded_error","message":"Overloaded"}}"#;
    cut(&[START, USE, &whole, BLOCK_STOP, &tool_use, overloaded, STOP], Some(Class::Overloaded));

    let broken = |text: &str| {
        let (args, _) = run(&[START, USE, &args(text), BLOCK_STOP, &tool_use, STOP]);
        assert!(matches!(args, Args::Broken(_)), "{text}: {args:?}");
    };
    for text in [r#"{"a":"#, r#"{"a":1}}"#, "[1]", "null", "\"{}\"", "7", "{'a':1}", "{} {}", " "] {
        broken(text);
    }
    broken(&format!(r#"{{"a":{}}}"#, "[".repeat(200) + &"]".repeat(200)));
    for bad in [
        USE.replace("\"t1\"", "\"\""),
        USE.replace("\"f\"", "\"\""),
        USE.replace("\"name\":\"f\",", ""),
    ] {
        let (args, _) = run(&[START, &bad, &whole, BLOCK_STOP, &tool_use, STOP]);
        assert!(matches!(args, Args::Broken(_)), "{bad}");
    }
}

#[test]
fn of_two_calls_with_one_id_the_first_runs() {
    let at = |n: u32, part: &str| part.replace("\"index\":0", &format!("\"index\":{n}"));
    let call = |n, a: &str| [at(n, USE), at(n, &args(a)), at(n, BLOCK_STOP)].join("\n");
    let (r, _) = read(&stream(&[
        START,
        &call(0, r#"{"n":1}"#),
        &call(1, r#"{"n":2}"#),
        &ends("tool_use"),
        STOP,
    ]));
    let calls: Vec<_> = r.message.calls().map(|c| c.args()["n"].clone()).collect();
    assert_eq!(calls, [json!(1)]);
    // Told apart when it goes back, too: the second and no result is left out.
    let mut request = request();
    request.messages = vec![Message::user("x"), r.message, results(&["t1"])];
    let encoded = encode(&request, &profile()).unwrap();
    assert_eq!(encoded.lost, [Lost { message: 1, block: 1, what: Loss::UnfinishedCall }]);
    assert!(text_of(&encoded.body).contains(r#""tool_use_id":"t1""#));
}

#[test]
fn blocks_may_be_open_together() {
    let (r, e) = read(&stream(&[
        START,
        r#"{"type":"content_block_start","index":4,"content_block":{"type":"text","text":"a"}}"#,
        r#"{"type":"content_block_start","index":2,"content_block":{"type":"thinking","thinking":"t","signature":""}}"#,
        r#"{"type":"content_block_delta","index":4,"delta":{"type":"text_delta","text":"b"}}"#,
        r#"{"type":"content_block_delta","index":2,"delta":{"type":"thinking_delta","thinking":"u"}}"#,
        r#"{"type":"content_block_stop","index":4}"#,
        r#"{"type":"content_block_stop","index":2}"#,
        &ends("end_turn"),
        STOP,
    ]));
    assert_eq!(e, None);
    let contents: Vec<_> = r.message.blocks.iter().map(|b| b.content.clone()).collect();
    let want = [Content::Text { text: "ab".into() }, Content::Reasoning { text: "tu".into() }];
    assert_eq!(contents, want);
    // Nothing signed it.
    assert_eq!(
        native(&r.message.blocks[1]),
        r#"{"type":"thinking","thinking":"tu","signature":""}"#
    );
}

#[test]
fn what_is_newer_than_this_build_survives() {
    let (events, e) = events(
        Codec::Messages,
        &Quirks::default(),
        &[&stream(&[
            START,
            r#"{"type":"ping"}"#,
            r#"{"type":"brand_new","index":"not a number","delta":[1]}"#,
            r#"{"type":"content_block_start","index":0,"content_block":{"type":"novel","x":1.0}}"#,
            r#"{"type":"content_block_stop","index":0}"#,
            r#"{"type":"content_block_start","index":1,"content_block":{"type":"novel","x":1.0}}"#,
            r#"{"type":"content_block_delta","index":1,"delta":{"type":"novel_delta","y":2}}"#,
            r#"{"type":"content_block_stop","index":1}"#,
            r#"{"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"novel_delta"}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":7}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"thinking_delta","thinking":"not here"}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"citations_delta","citation":{"cited_text":"c"}}}"#,
            r#"{"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"t"}}"#,
            r#"{"type":"content_block_stop","index":2}"#,
            r#"{"type":"content_block_start","index":3,"content_block":{"type":"redacted_thinking","data":"EmwKAhgBEgy3va3pzix/LafPsn4="}}"#,
            r#"{"type":"content_block_delta","index":3,"delta":{"type":"input_json_delta","partial_json":"{}"}}"#,
            r#"{"type":"content_block_stop","index":3}"#,
            r#"{"type":"message_delta","delta":{"stop_reason":"novel_reason"},"usage":null}"#,
            STOP,
        ])],
    );
    assert_eq!(e, None);
    let r = fold(Codec::Messages, &events);
    let [whole, pieces, text, redacted] = r.message.blocks.as_slice() else { panic!() };
    assert_eq!(whole.content, Content::Unknown { name: "novel".into() });
    assert_eq!(native(whole), r#"{"type":"novel","x":1.0}"#);
    // What cannot be put together is kept in pieces and not sent back.
    assert_eq!(pieces.native, None);
    assert_eq!(pieces.extra, [Json::parse(r#"{"type":"novel_delta","y":2}"#).unwrap()]);
    assert_eq!(native(text), r#"{"type":"text","text":"t","citations":[{"cited_text":"c"}]}"#);
    assert_eq!(text.extra.len(), 3);
    assert_eq!(redacted.content, Content::Reasoning { text: String::new() });
    assert_eq!(
        native(redacted),
        r#"{"type":"redacted_thinking","data":"EmwKAhgBEgy3va3pzix/LafPsn4="}"#
    );
    assert_eq!(r.stop, Some(Stop::Other("novel_reason".into())));
    assert_eq!(r.usage.prompt, Some(240));
}

#[test]
fn stops() {
    let stop = |delta: Value| {
        let delta = json!({ "type": "message_delta", "delta": delta }).to_string();
        read(&stream(&[START, &delta, STOP])).0.stop
    };
    assert_eq!(stop(json!({"stop_reason":"max_tokens"})), Some(Stop::MaxOutput));
    assert_eq!(
        stop(json!({"stop_reason":"model_context_window_exceeded"})),
        Some(Stop::ContextFull)
    );
    assert_eq!(stop(json!({"stop_reason":"pause_turn"})), Some(Stop::Pause));
    assert_eq!(
        stop(json!({"stop_reason":"stop_sequence","stop_sequence":"END"})),
        Some(Stop::Sequence(Some("END".into())))
    );
    assert_eq!(
        stop(json!({"stop_reason":"refusal","stop_details":{"type":"refusal","category":"cyber"}})),
        Some(Stop::Refusal(Some("cyber".into())))
    );
    assert_eq!(
        stop(json!({"stop_reason":"refusal","stop_details":null})),
        Some(Stop::Refusal(None))
    );
    assert_eq!(stop(json!({"stop_reason":null})), None);
}

#[test]
fn an_error_in_the_stream_keeps_what_came_before() {
    let text =
        r#"{"type":"content_block_start","index":0,"content_block":{"type":"text","text":"half"}}"#;
    let mut bytes = stream(&[START, text]);
    bytes.extend(fixture("messages-from-documentation/error.sse"));
    let (r, e) = read(&bytes);
    let e = e.unwrap();
    assert_eq!((e.class, e.retry, e.status), (Class::Overloaded, Retry::Restart, None));
    assert_eq!((e.message.as_str(), e.code.as_deref()), ("Overloaded", Some("overloaded_error")));
    let [b] = r.message.blocks.as_slice() else { panic!() };
    assert_eq!(b.content, Content::Text { text: "half".into() });
    assert!(b.cut && b.native.is_none() && !r.complete && r.stop.is_none());
}

#[test]
fn what_breaks_the_protocol_is_said() {
    let class = |parts: &[&str]| read(&stream(parts)).1.map(|e| (e.class, e.retry));
    let broken = Some((Class::Protocol, Retry::Never));
    for parts in [
        &[START, "not json"][..],
        &[START, "[]"],
        &[r#"{"type":"message_start"}"#],
        &[START, BLOCK_STOP],
        &[START, &args("{}")],
        &[START, USE, USE],
        &[START, r#"{"type":"content_block_start","index":0}"#],
        &[START, r#"{"type":"content_block_start","content_block":{"type":"text"}}"#],
        &[START, USE, r#"{"type":"content_block_delta","index":0}"#],
        &[START, USE, BLOCK_STOP, BLOCK_STOP],
        &[START, USE, BLOCK_STOP, &args("{}")],
        &[START, r#"{"type":"content_block_stop"}"#],
    ] {
        assert_eq!(class(parts), broken, "{parts:?}");
    }
    assert_eq!(class(&[]), Some((Class::CutShort, Retry::Restart)));
    assert_eq!(class(&[START, STOP, "not json"]), None);
    // The event's name stands in for a type that is missing.
    assert_eq!(read(b"event: message_stop\ndata: {}\n\n").1, None);
}

#[test]
fn an_answer_is_bounded() {
    let start = |n: usize| {
        json!({"type":"content_block_start","index":n,"content_block":{"type":"text","text":""}})
            .to_string()
    };
    let many: Vec<String> = (0..=MAX_BLOCKS).map(start).collect();
    let parts: Vec<&str> = many.iter().map(String::as_str).collect();
    let (r, e) = read(&stream(&parts));
    assert_eq!((r.message.blocks.len(), e.unwrap().class), (MAX_BLOCKS, Class::Protocol));

    let mut d = axiom_model::codec::Decoder::new(Codec::Messages, &Quirks::default());
    let data = format!(r#"{{"type":"ping","x":"{}"}}"#, "x".repeat((1 << 20) - 22));
    assert_eq!(data.len(), 1 << 20);
    let event = axiom_model::sse::Event { name: String::new(), data };
    let mut out = Vec::new();
    for _ in 0..MAX_ANSWER >> 20 {
        d.event(&event, &mut out).unwrap();
    }
    assert_eq!(d.event(&event, &mut out).unwrap_err().class, Class::Protocol);
    assert!(d.ended() && out.is_empty());
    // And then it stays still.
    assert!(d.event(&event, &mut out).is_ok() && d.finish(&mut out).is_ok() && out.is_empty());
}

#[test]
fn events_as_they_come() {
    let (got, _) = events(
        Codec::Messages,
        &Quirks::default(),
        &[&stream(&[START, USE, &args("{"), &args("}"), BLOCK_STOP, STOP])],
    );
    let block = 0;
    assert_eq!(got[0], Event::Started { id: Some("m".into()), model: Some("served".into()) });
    assert_eq!(
        got[2],
        Event::BlockStart { block, start: Start::ToolCall { id: "t1".into(), name: "f".into() } }
    );
    assert_eq!(got[3], Event::Delta { block, delta: Delta::Args("{".into()) });
    assert_eq!(got[4], Event::Delta { block, delta: Delta::Args("}".into()) });
    assert!(matches!(&got[5], Event::BlockStop { block: 0, native: Some(_), broken: None }));
    assert_eq!((got.len(), &got[6]), (7, &Event::End));
}

// Requests.

fn profile() -> Profile {
    let mut p = Profile::anthropic();
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
        settings: Settings { max_output: Some(100), ..Settings::default() },
    }
}
fn results(ids: &[&str]) -> Message {
    let blocks = ids.iter().map(|id| Block::result(*id, "ok", false)).collect();
    Message { role: Role::User, blocks }
}
fn text_of(body: &[u8]) -> String {
    String::from_utf8(body.to_vec()).unwrap()
}
fn body(request: &Request) -> String {
    text_of(&encode(request, &profile()).unwrap().body)
}
fn call(id: &str, args: Args) -> Block {
    Block::new(Content::ToolCall(ToolCall {
        id: id.into(),
        name: "f".into(),
        raw: String::new(),
        args,
    }))
}
fn ready() -> Args {
    Args::Ready(json!({"q": 1}).as_object().unwrap().clone())
}

#[test]
fn a_request_to_the_byte() {
    let mut r = request();
    assert_eq!(
        body(&r),
        r#"{"model":"m","max_tokens":100,"stream":true,"tools":[{"name":"f","description":"does \"f\"","input_schema":{"type":"object", "properties":{"z":{},"a":{}}}}],"system":[{"type":"text","text":"Be brief."}],"messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}]}"#
    );
    r.settings.stop = vec!["END".into()];
    r.settings.effort = Some("high".into());
    r.messages.push(Message {
        role: Role::Assistant,
        blocks: vec![Block::text("yes"), call("functions.f:0", ready())],
    });
    r.messages.push(Message {
        role: Role::User,
        blocks: vec![
            Block::result("functions.f:0", "", true),
            Block::text("  "),
            Block::text("go on"),
        ],
    });
    (r.system, r.tools) = (String::new(), Vec::new());
    assert_eq!(
        body(&r),
        r#"{"model":"m","max_tokens":100,"stream":true,"stop_sequences":["END"],"output_config":{"effort":"high"},"messages":[{"role":"user","content":[{"type":"text","text":"hi"}]},{"role":"assistant","content":[{"type":"text","text":"yes"},{"type":"tool_use","id":"functions_2ef_3a0","name":"f","input":{"q":1}}]},{"role":"user","content":[{"type":"tool_result","tool_use_id":"functions_2ef_3a0","is_error":true},{"type":"text","text":"go on"}]}]}"#
    );
}

#[test]
fn how_to_think_and_how_fast() {
    let p = Profile::anthropic();
    let mut r = request();
    (r.system, r.tools) = (String::new(), Vec::new());
    r.settings.adaptive = true;
    let body = |r: &Request| String::from_utf8(encode(r, &p).unwrap().body).unwrap();
    assert!(body(&r).contains(r#""thinking":{"type":"adaptive"}"#), "{}", body(&r));
    let mut other = p.clone();
    other.quirks.thinking = None;
    assert_eq!(encode(&r, &other).unwrap_err(), EncodeError::Unsupported("how to think"));
    r.settings.adaptive = false;
    assert!(!body(&r).contains("thinking"), "as its provider does");
    // Only where it is documented, and as it is.
    r.settings.speed = Some("fast".into());
    assert_eq!(encode(&r, &p).unwrap_err(), EncodeError::Unsupported("the speed"));
    r.model = "claude-opus-5-5".into();
    assert!(body(&r).contains(r#""speed":"fast""#));
    r.settings.speed = Some("slow".into());
    assert_eq!(encode(&r, &p).unwrap_err(), EncodeError::Unsupported("the speed"));
    // Nowhere else.
    for p in [
        Profile::openai(),
        Profile::gemini(),
        Profile::bedrock(),
        Profile::openai_compatible("x", "http://localhost"),
    ] {
        for settings in [
            Settings { adaptive: true, ..request().settings },
            Settings { speed: Some("fast".into()), ..request().settings },
        ] {
            let mut r = request();
            r.settings = settings;
            let e = encode(&r, &p).unwrap_err();
            assert!(matches!(e, EncodeError::Unsupported(_)), "{} {e:?}", p.provider);
        }
    }
}

#[test]
fn what_cannot_be_asked_for_is_an_error() {
    let mut r = request();
    r.settings.max_output = None;
    assert!(matches!(encode(&r, &profile()), Err(EncodeError::Missing(_))));
    let mut r = request();
    r.settings.effort = Some("high".into());
    let mut p = profile();
    p.quirks.effort = None;
    assert_eq!(encode(&r, &p).unwrap_err(), EncodeError::Unsupported("effort"));
    p.quirks.effort = Some(axiom_model::profile::Effort::ReasoningEffort);
    assert_eq!(encode(&r, &p).unwrap_err(), EncodeError::Unsupported("effort"));
    let mut p = profile();
    p.quirks.body.insert("metadata".into(), json!({"user_id":"x"}));
    assert!(
        text_of(&encode(&request(), &p).unwrap().body)
            .contains(r#","metadata":{"user_id":"x"},"tools""#)
    );
    for own in [
        "messages",
        "stream",
        "system",
        "model",
        "max_tokens",
        "tools",
        "cache_control",
        "output_config",
        "stop_sequences",
        "thinking",
        "speed",
    ] {
        p.quirks.body = [(own.to_owned(), json!(1))].into();
        assert_eq!(encode(&request(), &p).unwrap_err(), EncodeError::Reserved(own.into()));
    }
    // By model.
    let mut p = profile();
    p.models.insert("m".into(), Quirks { eager_tool_args: Some(true), ..Quirks::default() });
    assert!(
        text_of(&encode(&request(), &p).unwrap().body).contains(r#""eager_input_streaming":true"#)
    );
}

/// A conversation with everything in it.
fn conversation() -> Vec<Message> {
    let mut thinking = documented("thinking").message;
    let mut tool = documented("tool").message;
    thinking.blocks.append(&mut tool.blocks);
    vec![
        Message::user("What is the GCD?"),
        thinking,
        results(&["toolu_01T1x1fJ34qAmk2tNTrN7Up6"]),
        documented("web-search").message,
        Message::user("and?"),
        Message { role: Role::Assistant, blocks: vec![call("cut", Args::Cut)] },
        results(&["cut"]),
        documented("text").message,
        Message::user("thanks"),
    ]
}
const LONG_MARKER: &str = r#","cache_control":{"ttl":"1h","type":"ephemeral"}"#;
const MARKER: &str = r#","cache_control":{"type":"ephemeral"}"#;

#[test]
fn appending_changes_nothing_before_but_markers() {
    for cache in [Cache::Off, Cache::Short, Cache::Long] {
        let mut r = request();
        r.settings.cache = cache;
        r.messages.clear();
        let mut before: Option<String> = None;
        for message in conversation() {
            r.messages.push(message);
            let now = body(&r).replace(MARKER, "").replace(LONG_MARKER, "");
            assert!(!now.contains("cache_control"));
            if let Some(before) = before {
                assert_eq!(now[..before.len() - 2], before[..before.len() - 2]);
            }
            before = Some(now);
        }
    }
}

#[test]
fn markers_are_few_and_last() {
    let mut r = request();
    r.messages = conversation();
    r.settings.cache = Cache::Short;
    let text = body(&r);
    assert_eq!(text.matches(MARKER).count(), 3);
    assert!(text.contains(r#""text":"Be brief.","cache_control""#));
    assert!(text.contains(r#""text":"and?","cache_control""#));
    assert!(text.ends_with(r#""text":"thanks","cache_control":{"type":"ephemeral"}}]}]}"#));
    // Without a system prompt the tools are the prefix.
    r.system = String::new();
    let text = body(&r);
    assert!(text.contains(r#""a":{}}},"cache_control""#));
    assert_eq!(text.matches(MARKER).count(), 3);
    r.settings.cache = Cache::Long;
    assert_eq!(body(&r).matches(r#""cache_control":{"ttl":"1h","type":"ephemeral"}"#).count(), 3);
    // A provider without markers is not sent any.
    let mut p = profile();
    p.quirks.cache_markers = None;
    assert!(!text_of(&encode(&r, &p).unwrap().body).contains("cache_control"));
    // Never on the assistant's own, which must go back as it came.
    r.messages.truncate(2);
    let v: Value = serde_json::from_str(&body(&r)).unwrap();
    assert!(
        v["messages"][1].to_string().contains("signature")
            && !v["messages"][1].to_string().contains("cache_control")
    );
}

#[test]
fn native_blocks_go_back_to_the_byte_and_only_where_they_belong() {
    let mut r = request();
    r.messages = conversation();
    let encoded = encode(&r, &profile()).unwrap();
    let text = text_of(&encoded.body);
    for message in [&r.messages[1], &r.messages[3], &r.messages[7]] {
        for block in &message.blocks {
            assert!(text.contains(native(block)), "{}", native(block));
        }
    }
    let unfinished = [
        Lost { message: 5, block: 0, what: Loss::UnfinishedCall },
        Lost { message: 6, block: 0, what: Loss::ResultOfUnfinishedCall },
    ];
    assert_eq!(encoded.lost, unfinished);
    // Stored and read again, also through a Value.
    let stored = serde_json::to_value(&r.messages).unwrap();
    let mut again = r.clone();
    again.messages = serde_json::from_value(stored).unwrap();
    assert_eq!(again, r);
    assert_eq!(body(&again), text);

    // Another model of a provider that drops what it cannot read: all of it.
    r.model = "other".into();
    assert_eq!(encode(&r, &profile()).unwrap().lost, unfinished);
    // Elsewhere: what can be told, and an account of the rest.
    let elsewhere = |p: &Profile, r: &Request| {
        let e = encode(r, p).unwrap();
        let text = text_of(&e.body);
        assert!(
            !text.contains("signature")
                && !text.contains("srvtoolu")
                && !text.contains("encrypted")
        );
        assert!(
            text.contains(r#""text":"The greatest common divisor of 1071 and 462 is **21**.""#)
        );
        assert!(text.contains(r#""name":"get_weather","input":{"location":"San Francisco, CA"}"#));
        let lost: Vec<_> = e.lost.iter().map(|l| (l.message, l.block, l.what)).collect();
        assert_eq!(
            lost,
            [
                (1, 0, Loss::Reasoning),
                (3, 1, Loss::Unknown),
                (3, 2, Loss::Unknown),
                (5, 0, Loss::UnfinishedCall),
                (6, 0, Loss::ResultOfUnfinishedCall)
            ]
        );
    };
    let mut strict = profile();
    strict.quirks.replay = None;
    elsewhere(&strict, &r);
    let mut other = profile();
    other.provider = "q".into();
    r.model = "m".into();
    elsewhere(&other, &r);
    for m in &mut r.messages {
        for n in m.blocks.iter_mut().filter_map(|b| b.native.as_mut()) {
            n.scope.codec = Codec::Chat;
        }
    }
    elsewhere(&profile(), &r);
}

#[test]
fn a_result_belongs_to_the_last_message_that_called() {
    let assistant = |blocks| Message { role: Role::Assistant, blocks };
    let mut r = request();
    r.messages = vec![
        Message::user("x"),
        assistant(vec![call("t", Args::Broken("why".into()))]),
        results(&["t"]),
        assistant(vec![call("t", ready())]),
        results(&["t"]),
        assistant(vec![call("t", Args::Cut)]),
        results(&["t"]),
    ];
    let e = encode(&r, &profile()).unwrap();
    let lost: Vec<_> = e.lost.iter().map(|l| l.message).collect();
    assert_eq!(lost, [1, 2, 5, 6]);
    assert_eq!(text_of(&e.body).matches("tool_result").count(), 1);
    // A whole call that came natively is still not sent back if the answer was cut after it.
    let (events, _) = events(
        Codec::Messages,
        &Quirks::default(),
        &[&stream(&[START, USE, &args("{}"), BLOCK_STOP])],
    );
    let cut = fold(Codec::Messages, &events).message;
    assert!(cut.blocks[0].native.is_some());
    r.messages = vec![Message::user("x"), cut, results(&["t1"]), Message::user("y")];
    let e = encode(&r, &profile()).unwrap();
    assert!(!text_of(&e.body).contains("t1") && e.lost.len() == 2);
}

#[test]
fn what_is_in_the_wrong_place_is_said() {
    let mut r = request();
    r.messages = vec![
        Message { role: Role::User, blocks: vec![call("t", ready()), Block::text("x")] },
        Message { role: Role::Assistant, blocks: vec![Block::result("t", "r", false)] },
    ];
    let e = encode(&r, &profile()).unwrap();
    let misplaced = |message| Lost { message, block: 0, what: Loss::Misplaced };
    assert_eq!(e.lost, [misplaced(0), misplaced(1)]);
    assert!(
        text_of(&e.body)
            .ends_with(r#""messages":[{"role":"user","content":[{"type":"text","text":"x"}]}]}"#)
    );
}
