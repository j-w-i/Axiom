//! The Responses codec, on streams and bodies from OpenAI's documentation (fixtures/README.md) and on
//! what may come besides.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axiom_model::{
    Args, Block, Cache, Class, Codec, Content, Delta, EncodeError, Error, Event, Json, Loss, Lost,
    Message, Profile, Quirks, Reply, Request, Retry, Role, Settings, Start, Stop, Tool, ToolCall,
    Usage,
    codec::{MAX_ANSWER, MAX_BLOCKS, encode},
    profile::Effort,
};
use common::*;
use serde_json::{Value, json};

fn read(bytes: &[u8]) -> (Reply, Option<Error>) {
    reply(Codec::Responses, &Quirks::default(), bytes)
}
fn page(name: &str) -> Vec<u8> {
    fixture(&format!("responses-from-documentation/{name}.sse"))
}
fn documented(name: &str) -> Reply {
    let (reply, error) = read(&page(name));
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
    assert_eq!(r.id.as_deref(), Some("resp_67c9fdcecf488190bdd9a0409de3a1ec07b8b0ad4e5eb654"));
    assert_eq!(r.model.as_deref(), Some("gpt-6.2-astra"));
    assert_eq!((r.stop, r.message.role), (Some(Stop::EndTurn), Role::Assistant));
    let [b] = r.message.blocks.as_slice() else { panic!() };
    // The page leaves out all pieces but the first; the item has the whole.
    let whole = "Hi there! How can I assist you today?";
    assert_eq!(b.content, Content::Text { text: whole.into() });
    // To the byte.
    let item = r#"{"id":"msg_67c9fdcf37fc8190ba82116e33fb28c507b8b0ad4e5eb654","type":"message","status":"completed","role":"assistant","content":[{"type":"output_text","text":"Hi there! How can I assist you today?","annotations":[],"logprobs":[]}]}"#;
    assert_eq!((native(b), b.cut, b.extra.is_empty()), (item, false, true));
    let want = Usage {
        prompt: Some(37),
        uncached: Some(37),
        cache_read: Some(0),
        cache_write: Some(0),
        output: Some(11),
        reasoning: Some(0),
        ..Usage::default()
    };
    assert_eq!(r.usage, want);
}

#[test]
fn a_tool_call() {
    let r = documented("tool");
    assert_eq!(r.stop, Some(Stop::ToolUse));
    let [call] = r.message.blocks.as_slice() else { panic!() };
    assert_eq!(
        native(call),
        r#"{"type":"function_call","id":"fc_1234xyz","call_id":"call_1234xyz","name":"get_weather","arguments":"{\"location\":\"Paris, France\"}"}"#
    );
    let Content::ToolCall(c) = &call.content else { panic!() };
    // Once, though it is told in pieces, as a whole, and with its item.
    assert_eq!(c.raw, r#"{"location":"Paris, France"}"#);
    let calls: Vec<_> = r.message.calls().collect();
    let [c] = calls.as_slice() else { panic!() };
    // Known by the id its result has to name, which is not the item's.
    assert_eq!((c.id(), c.name()), ("call_1234xyz", "get_weather"));
    assert_eq!(c.args()["location"], "Paris, France");
}

#[test]
fn reasoning_and_what_it_shows() {
    let r = documented("reasoning");
    let [reasoning, text] = r.message.blocks.as_slice() else { panic!() };
    let Content::Reasoning { text: shown } = &reasoning.content else { panic!() };
    assert!(shown.starts_with("**Answering a simple question**\n\nI’m looking"));
    assert!(native(reasoning).starts_with(
        r#"{"id":"rs_6876cf02e0bc8192b74af0fb64b715ff06fa2fcced15a5ac","type":"reasoning","summary":["#
    ));
    assert_eq!(text.content, Content::Text { text: "The capital of France is Paris.".into() });
}

#[test]
fn what_goes_wrong_in_the_stream() {
    let (r, e) = read(&page("failed"));
    let e = e.unwrap();
    assert_eq!((e.class, e.retry, e.status), (Class::Server, Retry::Restart, None));
    assert_eq!(e.message, "The model failed to generate a response.");
    assert_eq!(e.code.as_deref(), Some("server_error"));
    assert!(!r.complete && r.stop.is_none());
    let (_, e) = read(&page("error"));
    let e = e.unwrap();
    assert_eq!((e.class, e.retry), (Class::Server, Retry::Restart));
    assert_eq!(
        (e.message.as_str(), e.code.as_deref()),
        ("Something went wrong", Some("ERR_SOMETHING"))
    );
    let (r, e) = read(&page("incomplete"));
    assert_eq!((r.stop, r.complete, e), (Some(Stop::MaxOutput), true, None));

    let failed = |error: Value, usage: Value| {
        let e = json!({"type":"response.failed","response":{"error":error,"usage":usage}});
        let (r, e) = read(&stream(&[CREATED, MESSAGE, &text_delta("half"), &e.to_string()]));
        let [b] = r.message.blocks.as_slice() else { panic!() };
        // What came before is kept, as far as it came.
        assert_eq!(b.content, Content::Text { text: "half".into() });
        assert!(b.cut && b.native.is_none() && !r.complete && r.stop.is_none());
        (e.unwrap(), r.usage)
    };
    for (code, class, retry) in [
        ("rate_limit_exceeded", Class::RateLimit, Retry::Restart),
        ("slow_down", Class::RateLimit, Retry::Restart),
        ("server_is_overloaded", Class::Overloaded, Retry::Restart),
        ("invalid_prompt", Class::BadRequest, Retry::Never),
        ("bio_policy", Class::BadRequest, Retry::Never),
        ("misalignment_policy_violation", Class::BadRequest, Retry::Never),
        ("data_residency_mismatch", Class::BadRequest, Retry::Never),
        ("credit_balance_exhausted", Class::Billing, Retry::Never),
        ("context_length_exceeded", Class::ContextTooLong, Retry::Never),
        ("vector_store_timeout", Class::Server, Retry::Restart),
    ] {
        let (e, usage) = failed(json!({"code":code,"message":"m"}), json!({"input_tokens":3}));
        assert_eq!((e.class, e.retry, e.message.as_str()), (class, retry, "m"), "{code}");
        assert_eq!(usage.prompt, Some(3));
    }
    let (e, _) = failed(Value::Null, Value::Null);
    assert_eq!(
        (e.class, e.message.as_str()),
        (Class::Server, "the provider reported an error and said nothing more")
    );
    // An error as other parts of the API tell one, and one without a code.
    let error = |data: &str| read(&stream(&[CREATED, data])).1.unwrap();
    let e = error(
        r#"{"type":"error","error":{"type":"invalid_request_error","code":null,"message":"Your input exceeds the context window of this model."}}"#,
    );
    assert_eq!(e.class, Class::ContextTooLong);
    let e = error(r#"{"type":"error","code":null,"message":"m","param":null}"#);
    assert_eq!((e.class, e.code, e.message.as_str()), (Class::Server, None, "m"));
}

const CREATED: &str = r#"{"type":"response.created","response":{"id":"r1","model":"served"}}"#;
const MESSAGE: &str = r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"message","id":"m1","role":"assistant","content":[]}}"#;
const CALL: &str = r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"function_call","id":"fc","call_id":"t1","name":"f","arguments":""}}"#;
const COMPLETED: &str = r#"{"type":"response.completed","response":{"status":"completed"}}"#;

fn stream(parts: &[&str]) -> Vec<u8> {
    lines(&parts.join("\n"))
}
fn text_delta(text: &str) -> String {
    json!({"type":"response.output_text.delta","output_index":0,"content_index":0,"delta":text})
        .to_string()
}
fn args(text: &str) -> String {
    json!({"type":"response.function_call_arguments.delta","output_index":0,"delta":text})
        .to_string()
}
fn done(item: Value) -> String {
    json!({"type":"response.output_item.done","output_index":0,"item":item}).to_string()
}
fn call_item(arguments: &str) -> Value {
    json!({"type":"function_call","id":"fc","call_id":"t1","name":"f","arguments":arguments,"status":"completed"})
}
fn ends(reason: Value) -> String {
    json!({"type":"response.incomplete","response":{"incomplete_details":reason}}).to_string()
}

#[test]
fn usage_is_what_was_told() {
    let of = |usage: Value| {
        let e = json!({"type":"response.completed","response":{"usage":usage}}).to_string();
        read(&stream(&[CREATED, &e])).0.usage
    };
    // guides/prompt-caching#monitor-cache-performance: of 12000, 4000 read and 7000 written leave 1000.
    let got = of(
        json!({"input_tokens":12000,"input_tokens_details":{"cached_tokens":4000,"cache_write_tokens":7000,"cache_write_tokens_details":{"ttl_30m_tokens":2000,"ttl_12h_tokens":5000}},"output_tokens":9,"output_tokens_details":{"reasoning_tokens":5},"total_tokens":12009}),
    );
    let want = Usage {
        prompt: Some(12000),
        uncached: Some(1000),
        cache_read: Some(4000),
        cache_write: Some(7000),
        cache_write_long: None,
        output: Some(9),
        reasoning: Some(5),
    };
    assert_eq!(got, want);
    let got = of(json!({"input_tokens":10,"input_tokens_details":{"cached_tokens":4}}));
    assert_eq!((got.uncached, got.cache_write), (Some(6), None));
    // Without what was read, what was not is unknown. Nonsense is not a count.
    assert_eq!(of(json!({"input_tokens":10})).uncached, None);
    let odd = of(
        json!({"input_tokens":3,"input_tokens_details":{"cached_tokens":4},"output_tokens":-1,"output_tokens_details":{"reasoning_tokens":"5"}}),
    );
    let want = Usage { prompt: Some(3), cache_read: Some(4), ..Usage::default() };
    assert_eq!(odd, want);
    for none in [Value::Null, json!(7), json!([])] {
        assert_eq!(of(none), Usage::default());
    }
}

#[test]
fn a_call_is_run_only_if_everything_about_it_is_whole() {
    let whole = r#"{"a":1}"#;
    let run = |parts: &[&str]| {
        let (r, e) = read(&stream(parts));
        let Content::ToolCall(c) = r.message.blocks[0].content.clone() else { panic!() };
        assert_eq!(r.message.calls().count() == 1, matches!(c.args, Args::Ready(_)));
        (c.args, e.map(|e| e.class))
    };
    let ready = Args::Ready(json!({"a":1}).as_object().unwrap().clone());
    let item = done(call_item(whole));
    assert_eq!(run(&[CREATED, CALL, &args(whole), &item, COMPLETED]), (ready.clone(), None));
    assert_eq!(
        run(&[CREATED, CALL, &args(r#"{"a":"#), &args("1}"), &item, COMPLETED]),
        (ready.clone(), None)
    );
    // Pieces may be left out.
    assert_eq!(run(&[CREATED, CALL, &args(r#"{"a":"#), &item, COMPLETED]), (ready.clone(), None));
    assert_eq!(run(&[CREATED, CALL, &item, COMPLETED]), (ready.clone(), None));
    let mut bare = call_item(whole);
    bare.as_object_mut().unwrap().remove("status");
    assert_eq!(run(&[CREATED, CALL, &done(bare.clone()), COMPLETED]), (ready, None));

    let cut = |parts: &[&str], class| assert_eq!(run(parts), (Args::Cut, class), "{parts:?}");
    let short = Some(Class::CutShort);
    cut(&[CREATED, CALL, &args(whole), &item], short);
    cut(&[CREATED, CALL, &args(whole)], short);
    cut(&[CREATED, CALL, &args(whole), COMPLETED], None);
    for reason in ["max_output_tokens", "content_filter", "max_messages", "steered", "new"] {
        cut(&[CREATED, CALL, &item, &ends(json!({"reason":reason}))], None);
    }
    cut(&[CREATED, CALL, &item, &ends(Value::Null)], None);
    let failed =
        r#"{"type":"response.failed","response":{"error":{"code":"server_error","message":"m"}}}"#;
    cut(&[CREATED, CALL, &item, failed, COMPLETED], Some(Class::Server));
    // The model declined in the same answer.
    let refusal = r#"{"type":"response.output_item.added","output_index":1,"item":{"type":"message","content":[]}}
        {"type":"response.output_item.done","output_index":1,"item":{"type":"message","content":[{"type":"refusal","refusal":"no"}]}}"#;
    cut(&[CREATED, CALL, &item, refusal, COMPLETED], None);

    let broken = |item: &Value, before: &str| {
        let parts = [CREATED, CALL, before, &done(item.clone()), COMPLETED];
        let (args, _) = run(&parts);
        assert!(matches!(args, Args::Broken(_)), "{item} {before}: {args:?}");
    };
    let plain = text_delta("not for a call");
    for text in [r#"{"a":"#, r#"{"a":1}}"#, "[1]", "null", "", " ", "{} {}"] {
        broken(&call_item(text), &plain);
    }
    for (key, value) in [
        ("status", json!("incomplete")),
        ("status", json!("in_progress")),
        ("call_id", json!("other")),
        ("name", json!("other")),
        ("arguments", json!({"a":1})),
        ("arguments", Value::Null),
    ] {
        let mut item = call_item(whole);
        item[key] = value;
        broken(&item, &plain);
    }
    // What was passed on is not what it came to.
    broken(&call_item(whole), &args(r#"{"b":"#));
    broken(&call_item(whole), &args(r#"{"a":1} "#));
    bare.as_object_mut().unwrap().remove("arguments");
    broken(&bare, &args(whole));
    for bad in [CALL.replace("\"t1\"", "\"\""), CALL.replace("\"name\":\"f\"", "\"name\":\"\"")] {
        let mut item = call_item(whole);
        let key = if bad.contains("\"call_id\":\"\"") { "call_id" } else { "name" };
        item[key] = json!("");
        let (args, _) = run(&[CREATED, &bad, &done(item), COMPLETED]);
        assert!(matches!(args, Args::Broken(_)), "{bad}");
    }
}

#[test]
fn several_calls_at_once() {
    let at =
        |n: u32, part: &str| part.replace("\"output_index\":0", &format!("\"output_index\":{n}"));
    let two = CALL.replace("t1", "t2");
    let item = |id: &str, a: &str| {
        let mut item = call_item(a);
        item["call_id"] = json!(id);
        done(item)
    };
    let (r, e) = read(&stream(&[
        CREATED,
        &at(3, CALL),
        &at(1, &two),
        &at(7, CALL),
        &at(1, &args("{\"n\":")),
        &at(3, &args("{\"n\":1}")),
        &at(1, &args("2}")),
        &at(1, &item("t2", "{\"n\":2}")),
        &at(3, &item("t1", "{\"n\":1}")),
        &at(7, &item("t1", "{\"n\":3}")),
        COMPLETED,
    ]));
    assert_eq!(e, None);
    // In the order they began; of two with one id the first.
    let calls: Vec<_> =
        r.message.calls().map(|c| (c.id().to_owned(), c.args()["n"].clone())).collect();
    assert_eq!(calls, [("t1".into(), json!(1)), ("t2".into(), json!(2))]);
    assert_eq!(r.message.blocks.len(), 3);
}

#[test]
fn what_is_newer_than_this_build_survives() {
    let (got, e) = events(
        Codec::Responses,
        &Quirks::default(),
        &[&stream(&[
            CREATED,
            r#"{"type":"response.in_progress","response":{}}"#,
            r#"{"type":"response.brand_new","output_index":"not a number"}"#,
            r#"{"type":"response.brand_new","output_index":99}"#,
            r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"web_search_call","id":"ws","status":"in_progress"}}"#,
            r#"{"type":"response.web_search_call.searching","output_index":0,"item_id":"ws"}"#,
            r#"{"type":"response.output_item.done","output_index":0,"item":{"type":"web_search_call", "id":"ws","x":1.0}}"#,
            &MESSAGE.replace("\"output_index\":0", "\"output_index\":1"),
            r#"{"type":"response.output_text.novel","output_index":1,"y":2}"#,
            r#"{"type":"response.output_text.delta","output_index":1,"content_index":0,"delta":7}"#,
            r#"{"type":"response.function_call_arguments.delta","output_index":1,"delta":"not here"}"#,
            r#"{"type":"response.output_text.delta","output_index":1,"content_index":0,"delta":"t","obfuscation":"abc"}"#,
            r#"{"type":"response.content_part.done","output_index":1,"content_index":0,"part":{"type":"output_text","text":"t"}}"#,
            r#"{"type":"response.output_item.done","output_index":1,"item":{"type":"message","content":[{"type":"output_text","text":"t","novel":[]},{"type":"novel_part"}],"phase":"commentary"}}"#,
            // What has no type, and one that did not end.
            r#"{"type":"response.output_item.added","output_index":2,"item":{"a":1}}"#,
            r#"{"type":"response.output_item.done","output_index":2,"item":{"a":2}}"#,
            r#"{"type":"response.output_item.added","output_index":3,"item":{"type":"program"}}"#,
            r#"{"type":"response.program.progress","output_index":3}"#,
            COMPLETED,
        ])],
    );
    assert_eq!(e, None);
    let r = fold(Codec::Responses, &got);
    let [search, text, untyped, open] = r.message.blocks.as_slice() else { panic!() };
    assert_eq!(search.content, Content::Unknown { name: "web_search_call".into() });
    assert_eq!(native(search), r#"{"type":"web_search_call", "id":"ws","x":1.0}"#);
    assert!(search.extra.is_empty());
    assert_eq!(text.content, Content::Text { text: "t".into() });
    assert!(native(text).ends_with(r#","phase":"commentary"}"#));
    assert_eq!(text.extra.len(), 3);
    assert_eq!(
        text.extra[0],
        Json::parse(r#"{"type":"response.output_text.novel","output_index":1,"y":2}"#).unwrap()
    );
    assert_eq!(
        (&untyped.content, native(untyped)),
        (&Content::Unknown { name: String::new() }, r#"{"a":2}"#)
    );
    assert!(open.cut && open.native.is_none() && open.extra.len() == 1);
    assert_eq!(r.message.calls().count(), 0);
}

#[test]
fn parts_of_an_item_are_one_block() {
    let reasoning = r#"{"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","summary":[],"encrypted_content":"half"}}"#;
    let summary = |n: u32, t: &str| {
        json!({"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":n,"delta":t}).to_string()
    };
    let content = |n: u32, t: &str| {
        json!({"type":"response.reasoning_text.delta","output_index":0,"content_index":n,"delta":t})
            .to_string()
    };
    let item = json!({"type":"reasoning","summary":[{"type":"summary_text","text":"ab"},{"type":"summary_text","text":"c"}],"content":[{"type":"reasoning_text","text":"de"}],"encrypted_content":"all"});
    let shown = |parts: &[String]| {
        let parts: Vec<&str> = [CREATED, reasoning]
            .into_iter()
            .chain(parts.iter().map(String::as_str))
            .chain([COMPLETED])
            .collect();
        let (r, e) = read(&stream(&parts));
        assert_eq!(e, None);
        let Content::Reasoning { text } = &r.message.blocks[0].content else { panic!() };
        (text.clone(), r.message.blocks[0].clone())
    };
    let all = [
        summary(0, "a"),
        summary(0, "b"),
        summary(1, "c"),
        content(0, "d"),
        content(0, "e"),
        done(item.clone()),
    ];
    let (text, block) = shown(&all);
    assert_eq!(text, "ab\n\nc\n\nde");
    // What is encrypted is whole only in the finished item.
    assert!(native(&block).contains(r#""encrypted_content":"all""#) && !block.cut);
    // However many of the pieces came.
    for n in 0..all.len() - 1 {
        let some: Vec<String> = all[..n].iter().chain(&all[5..]).cloned().collect();
        assert_eq!(shown(&some).0, "ab\n\nc\n\nde");
    }
    // Pieces that are not its beginning stand; the item has the rest.
    let (text, block) = shown(&[summary(0, "x"), done(item)]);
    assert_eq!(text, "x");
    assert!(native(&block).contains("\"ab\""));

    // A message's parts are one text, and to decline is a text too.
    let refusal = r#"{"type":"response.refusal.delta","output_index":0,"content_index":1,"delta":"I'm sorry"}"#;
    let item = json!({"type":"message","content":[{"type":"output_text","text":"a"},{"type":"refusal","refusal":"I'm sorry, no."}]});
    let parts = [CREATED, MESSAGE, &text_delta("a"), refusal, &done(item.clone()), COMPLETED];
    let (r, _) = read(&stream(&parts));
    assert_eq!(r.message.blocks[0].content, Content::Text { text: "aI'm sorry, no.".into() });
    assert_eq!(r.stop, Some(Stop::Refusal(None)));
    let (r, _) = read(&stream(&[CREATED, MESSAGE, &done(item), COMPLETED]));
    assert_eq!(r.message.blocks[0].content, Content::Text { text: "aI'm sorry, no.".into() });
}

#[test]
fn stops() {
    let stop = |e: &str| {
        let (r, _) = read(&stream(&[CREATED, e]));
        assert!(r.complete);
        r.stop
    };
    assert_eq!(stop(COMPLETED), Some(Stop::EndTurn));
    assert_eq!(stop(&ends(json!({"reason":"max_output_tokens"}))), Some(Stop::MaxOutput));
    assert_eq!(
        stop(&ends(json!({"reason":"content_filter"}))),
        Some(Stop::Refusal(Some("content_filter".into())))
    );
    assert_eq!(stop(&ends(json!({"reason":"steered"}))), Some(Stop::Other("steered".into())));
    assert_eq!(stop(&ends(Value::Null)), Some(Stop::Other("incomplete".into())));
}

#[test]
fn what_breaks_the_protocol_is_said() {
    let class = |parts: &[&str]| read(&stream(parts)).1.map(|e| (e.class, e.retry));
    let broken = Some((Class::Protocol, Retry::Never));
    let item = done(call_item("{}"));
    let became = done(json!({"type":"message","content":[]}));
    for parts in [
        &[CREATED, "not json"][..],
        &[CREATED, "[]"],
        &[r#"{"type":"response.created"}"#],
        &[CREATED, r#"{"type":"response.completed"}"#],
        &[CREATED, r#"{"type":"response.incomplete"}"#],
        &[CREATED, r#"{"type":"response.failed"}"#],
        &[CREATED, &item],
        &[CREATED, &args("{}")],
        &[CREATED, CALL, CALL],
        &[CREATED, CALL, &item, &item],
        &[CREATED, CALL, &item, &args("{}")],
        &[CREATED, CALL, &became],
        &[CREATED, CALL, r#"{"type":"response.function_call_arguments.delta","output_index":0}"#],
        &[CREATED, CALL, r#"{"type":"response.function_call_arguments.delta","delta":"{}"}"#],
        &[CREATED, r#"{"type":"response.output_item.added","output_index":0}"#],
        &[CREATED, r#"{"type":"response.output_item.added","item":{"type":"message"}}"#],
        &[CREATED, r#"{"type":"response.output_item.added","output_index":0,"item":7}"#],
        &[CREATED, CALL, r#"{"type":"response.output_item.done","output_index":0}"#],
    ] {
        assert_eq!(class(parts), broken, "{parts:?}");
    }
    assert_eq!(class(&[]), Some((Class::CutShort, Retry::Restart)));
    assert_eq!(class(&[CREATED]), Some((Class::CutShort, Retry::Restart)));
    assert_eq!(class(&[CREATED, COMPLETED, "not json"]), None);
    // The event's name stands in for a type that is missing.
    assert_eq!(read(b"event: response.completed\ndata: {\"response\":{}}\n\n").1, None);
}

#[test]
fn an_answer_is_bounded() {
    let start = |n: usize| MESSAGE.replace("\"output_index\":0", &format!("\"output_index\":{n}"));
    let many: Vec<String> = (0..=MAX_BLOCKS).map(start).collect();
    let parts: Vec<&str> = many.iter().map(String::as_str).collect();
    let (r, e) = read(&stream(&parts));
    assert_eq!((r.message.blocks.len(), e.unwrap().class), (MAX_BLOCKS, Class::Protocol));
    let mut d = axiom_model::codec::Decoder::new(Codec::Responses, &Quirks::default());
    let data = format!(r#"{{"type":"response.x","x":"{}"}}"#, "x".repeat((1 << 20) - 28));
    assert_eq!(data.len(), 1 << 20);
    let event = axiom_model::sse::Event { name: String::new(), data };
    let mut out = Vec::new();
    for _ in 0..MAX_ANSWER >> 20 {
        d.event(&event, &mut out).unwrap();
    }
    assert_eq!(d.event(&event, &mut out).unwrap_err().class, Class::Protocol);
    assert!(d.ended() && out.is_empty());
}

#[test]
fn events_as_they_come() {
    let (got, _) = events(
        Codec::Responses,
        &Quirks::default(),
        &[&stream(&[CREATED, CALL, &args("{"), &done(call_item("{}")), COMPLETED])],
    );
    let block = 0;
    assert_eq!(got[0], Event::Started { id: Some("r1".into()), model: Some("served".into()) });
    let start = Start::ToolCall { id: "t1".into(), name: "f".into() };
    assert_eq!(got[1], Event::BlockStart { block, start });
    assert_eq!(got[2], Event::Delta { block, delta: Delta::Args("{".into()) });
    assert_eq!(got[3], Event::Delta { block, delta: Delta::Args("}".into()) });
    assert!(matches!(&got[4], Event::BlockStop { block: 0, native: Some(_), broken: None }));
    assert_eq!((got.len(), &got[5], &got[6]), (7, &Event::Stop(Stop::ToolUse), &Event::End));
}

// Requests.

fn profile() -> Profile {
    let mut p = Profile::openai();
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
fn results(ids: &[&str]) -> Message {
    let blocks = ids.iter().map(|id| Block::result(*id, "ok", false)).collect();
    Message { role: Role::User, blocks }
}
fn assistant(blocks: Vec<Block>) -> Message {
    Message { role: Role::Assistant, blocks }
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
    // Schemas are not to be made strict, which would change them.
    assert_eq!(
        body(&r),
        r#"{"model":"m","store":false,"stream":true,"tools":[{"type":"function","name":"f","description":"does \"f\"","parameters":{"type":"object", "properties":{"z":{},"a":{}}},"strict":false}],"input":[{"role":"developer","content":[{"type":"input_text","text":"Be brief."}]},{"role":"user","content":[{"type":"input_text","text":"hi"}]}]}"#
    );
    r.settings = Settings {
        max_output: Some(9),
        effort: Some("high".into()),
        stop: Vec::new(),
        cache: Cache::Long,
    };
    r.messages.push(assistant(vec![
        Block::text("yes"),
        Block::text(""),
        call("functions.f:0", ready()),
    ]));
    r.messages.push(Message {
        role: Role::User,
        blocks: vec![
            Block::text("one"),
            Block::text(""),
            Block::result("functions.f:0", "", true),
            Block::text("two"),
        ],
    });
    (r.system, r.tools) = (String::new(), Vec::new());
    assert_eq!(
        body(&r),
        r#"{"model":"m","store":false,"stream":true,"max_output_tokens":9,"reasoning":{"effort":"high"},"input":[{"role":"user","content":[{"type":"input_text","text":"hi"}]},{"role":"assistant","content":"yes"},{"type":"function_call","call_id":"functions.f:0","name":"f","arguments":"{\"q\": 1}"},{"type":"function_call_output","call_id":"functions.f:0","output":""},{"role":"user","content":[{"type":"input_text","text":"one"},{"type":"input_text","text":"two"}]}]}"#
    );
    // Another provider's id, if it is longer than is taken, is the same for a call and its result.
    let long = "x".repeat(65);
    r.messages =
        vec![assistant(vec![call(&long, ready())]), results(&[&long]), Message::user("go on")];
    let sent: Value = serde_json::from_str(&body(&r)).unwrap();
    let id = sent["input"][0]["call_id"].as_str().unwrap();
    assert_eq!((id.len(), &sent["input"][1]["call_id"]), (64, &json!(id)));
}

#[test]
fn what_cannot_be_asked_for_is_an_error() {
    let mut r = request();
    r.settings.stop = vec!["END".into()];
    assert_eq!(encode(&r, &profile()).unwrap_err(), EncodeError::Unsupported("a stop sequence"));
    let mut r = request();
    r.settings.effort = Some("high".into());
    let mut p = profile();
    for other in [
        None,
        Some(Effort::ReasoningEffort),
        Some(Effort::OutputConfig),
        Some(Effort::ThinkingLevel),
    ] {
        p.quirks.effort = other;
        assert_eq!(encode(&r, &p).unwrap_err(), EncodeError::Unsupported("effort"));
    }
    p.quirks.body.insert("include".into(), json!(["reasoning.encrypted_content"]));
    assert!(
        text_of(&encode(&request(), &p).unwrap().body)
            .contains(r#","include":["reasoning.encrypted_content"],"tools""#)
    );
    for own in [
        "model",
        "store",
        "stream",
        "max_output_tokens",
        "reasoning",
        "tools",
        "input",
        "instructions",
        "previous_response_id",
        "conversation",
        "background",
    ] {
        p.quirks.body = [(own.to_owned(), json!(true))].into();
        assert_eq!(encode(&request(), &p).unwrap_err(), EncodeError::Reserved(own.into()));
    }
}

/// A turn with everything in it, in the provider's own words.
fn turn() -> Message {
    let items = [
        r#"{"type":"reasoning","id":"rs_1","summary":[],"encrypted_content":"gAAA=="}"#,
        r#"{"type":"web_search_call", "id":"ws_1","status":"completed"}"#,
        r#"{"type":"message","id":"msg_1","role":"assistant","phase":"commentary","content":[{"type":"output_text","text":"Looking.","annotations":[]}]}"#,
        r#"{"type":"reasoning","id":"rs_2","summary":[],"encrypted_content":"gBBB=="}"#,
        r#"{"type":"function_call","id":"fc_1","call_id":"call_1","name":"f","arguments":"{ }"}"#,
        r#"{"type":"function_call","id":"fc_2","call_id":"call_2","name":"f","arguments":"{}"}"#,
    ];
    let mut parts = vec![CREATED.to_owned()];
    for (n, item) in items.iter().enumerate() {
        for kind in ["added", "done"] {
            parts.push(format!(
                r#"{{"type":"response.output_item.{kind}","output_index":{n},"item":{item}}}"#
            ));
        }
    }
    parts.push(COMPLETED.into());
    let parts: Vec<&str> = parts.iter().map(String::as_str).collect();
    let (r, e) = read(&stream(&parts));
    assert_eq!((e, r.message.calls().count()), (None, 2));
    for (block, item) in r.message.blocks.iter().zip(items) {
        assert_eq!(native(block), item);
    }
    r.message
}
fn conversation() -> Vec<Message> {
    vec![
        Message::user("What is the capital?"),
        documented("reasoning").message,
        Message::user("and the weather?"),
        turn(),
        results(&["call_1", "call_2"]),
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
        assert!(before.unwrap().contains("gAAA=="));
    }
}

#[test]
fn native_items_go_back_to_the_byte_and_only_where_they_belong() {
    let mut r = request();
    r.messages = conversation();
    let encoded = encode(&r, &profile()).unwrap();
    let text = text_of(&encoded.body);
    let items: Vec<&str> = r.messages[3].blocks.iter().map(native).collect();
    // All of them, in their order, with nothing between.
    assert!(text.contains(&items.join(",")), "{text}");
    for n in [1, 7] {
        for block in &r.messages[n].blocks {
            assert!(text.contains(native(block)));
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
    assert_eq!((again == r, body(&again)), (true, text.clone()));

    // To another model of the provider, which leaves out what that cannot read.
    r.model = "other".into();
    let other = encode(&r, &profile()).unwrap();
    assert_eq!(
        (other.lost.as_slice(), text_of(&other.body)),
        (&unfinished[..], text.replace(r#""model":"m""#, r#""model":"other""#))
    );
    // Elsewhere: what can be told, and an account of the rest.
    let elsewhere = |p: &Profile, r: &Request| {
        let e = encode(r, p).unwrap();
        let text = text_of(&e.body);
        for gone in ["gAAA", "gBBB", "rs_", "ws_1", "msg_", "fc_", "commentary"] {
            assert!(!text.contains(gone), "{gone}: {text}");
        }
        assert!(text.contains(r#"{"role":"assistant","content":"Looking."}"#));
        assert!(text.contains(r#""call_id":"call_1","name":"f","arguments":"{ }"}"#));
        let lost: Vec<_> = e.lost.iter().map(|l| (l.message, l.block, l.what)).collect();
        assert_eq!(
            lost,
            [
                (1, 0, Loss::Reasoning),
                (3, 0, Loss::Reasoning),
                (3, 1, Loss::Unknown),
                (3, 3, Loss::Reasoning),
                (5, 0, Loss::UnfinishedCall),
                (6, 0, Loss::ResultOfUnfinishedCall),
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
    for n in r.messages.iter_mut().flat_map(|m| &mut m.blocks).filter_map(|b| b.native.as_mut()) {
        n.scope.codec = Codec::Chat;
    }
    elsewhere(&profile(), &r);
}

#[test]
fn reasoning_is_not_sent_without_what_came_of_it() {
    let mut r = request();
    let mut half = turn();
    // Both calls were cut: the reasoning that led to them has nothing after it.
    half.blocks.truncate(5);
    let Content::ToolCall(c) = &mut half.blocks[4].content else { panic!() };
    c.args = Args::Cut;
    r.messages = vec![Message::user("x"), half.clone(), Message::user("y")];
    let e = encode(&r, &profile()).unwrap();
    let text = text_of(&e.body);
    assert!(text.contains("gAAA==") && !text.contains("gBBB==") && !text.contains("fc_1"));
    let lost: Vec<_> = e.lost.iter().map(|l| (l.block, l.what)).collect();
    assert_eq!(lost, [(3, Loss::Reasoning), (4, Loss::UnfinishedCall)]);
    // One call of two is enough.
    let mut one = turn();
    let Content::ToolCall(c) = &mut one.blocks[4].content else { panic!() };
    c.args = Args::Broken("why".into());
    r.messages[1] = one;
    let e = encode(&r, &profile()).unwrap();
    let text = text_of(&e.body);
    assert!(text.contains("gBBB==") && text.contains("fc_2") && !text.contains("fc_1"));
    assert_eq!(e.lost, [Lost { message: 1, block: 4, what: Loss::UnfinishedCall }]);
    // Nor does it wait for the next message.
    half.blocks.truncate(1);
    r.messages[1] = half;
    r.messages.push(documented("text").message);
    assert!(!body(&r).contains("gAAA=="));
}

#[test]
fn what_is_in_the_wrong_place_is_said() {
    let mut r = request();
    let unknown = || Block::new(Content::Unknown { name: "u".into() });
    let thought = || Block::new(Content::Reasoning { text: "t".into() });
    r.messages = vec![
        Message { role: Role::User, blocks: vec![call("t", ready()), thought(), unknown()] },
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
    assert!(text_of(&e.body).ends_with(
        r#"{"role":"developer","content":[{"type":"input_text","text":"Be brief."}]},{"role":"user","content":[{"type":"input_text","text":"x"}]}]}"#
    ));
}
