//! The Chat codec, on answers recorded from LM Studio (fixtures/README.md) and on what other servers
//! are known to send (research/providers.md §2.3).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axiom_model::{
    Args, Block, Cache, Class, Codec, Content, EncodeError, Event, Json, Loss, Lost, Message,
    Profile, Quirks, Reply, Request, Retry, Role, Settings, Stop, Tool, ToolCall, Usage,
    codec::{MAX_BLOCKS, encode},
    profile::{Effort, MaxTokens, Think},
};
use common::*;
use serde_json::{Value, json};

fn with(quirks: &Quirks, bytes: &[u8]) -> (Reply, Option<axiom_model::Error>) {
    reply(Codec::Chat, quirks, bytes)
}
fn read(bytes: &[u8]) -> (Reply, Option<axiom_model::Error>) {
    with(&Quirks::default(), bytes)
}
fn record(name: &str) -> Reply {
    let (head, body) = recorded(name);
    assert!(head.starts_with("HTTP/1.1 200 "));
    let (reply, error) = with(&Profile::lm_studio().quirks, &body);
    assert_eq!(error, None);
    assert!(reply.complete);
    reply
}
fn text_of(message: &Message) -> String {
    let texts = message.blocks.iter().filter_map(|b| match &b.content {
        Content::Text { text } => Some(text.as_str()),
        _ => None,
    });
    texts.collect()
}
fn calls(reply: &Reply) -> Vec<(String, String, Value)> {
    let calls = reply.message.calls();
    calls.map(|c| (c.id().into(), c.name().into(), Value::Object(c.args().clone()))).collect()
}
fn native(block: &Block) -> &str {
    block.native.as_ref().unwrap().json.get()
}

#[test]
fn recorded_text() {
    let r = record("text");
    assert_eq!(r.id.as_deref(), Some("chatcmpl-91cxgcaqybto6o3u8hq35"));
    assert_eq!(r.model.as_deref(), Some("qwen2.5-0.5b-instruct-mlx"));
    assert_eq!((text_of(&r.message).as_str(), r.message.blocks.len()), ("Paris", 1));
    assert_eq!(r.stop, Some(Stop::EndTurn));
    // Caches are not mentioned, so how much was not read from one is unknown.
    let want = Usage { prompt: Some(25), output: Some(2), reasoning: Some(0), ..Usage::default() };
    assert_eq!(r.usage, want);
    assert_eq!(text_of(&record("text-second-model").message), "4");
    assert_eq!(
        text_of(&record("tool-result").message),
        "The current weather in Paris is 18 degrees with rain."
    );
}

#[test]
fn recorded_ends() {
    let r = record("length");
    assert_eq!((r.stop, text_of(&r.message).as_str()), (Some(Stop::MaxOutput), "Of course, I'd"));
    // It does not say that it was the stop sequence, and leaves it out.
    let r = record("stop-sequence");
    assert_eq!(r.stop, Some(Stop::EndTurn));
    assert!(!text_of(&r.message).contains('4') && r.usage.output.is_some());
}

#[test]
fn recorded_calls() {
    let r = record("tool");
    assert_eq!(r.stop, Some(Stop::ToolUse));
    assert_eq!(calls(&r), [("764517396".into(), "get_weather".into(), json!({"city":"Paris"}))]);
    assert_eq!(
        native(&r.message.blocks[0]),
        r#"{"id":"764517396","type":"function","function":{"name":"get_weather","arguments":"{\"city\": \"Paris\"}"}}"#
    );
    let r = record("tool-no-args");
    assert_eq!(calls(&r), [("193478395".into(), "get_time".into(), json!({}))]);
    // Two, with a line break as text between them.
    let r = record("tool-parallel");
    assert_eq!(
        calls(&r),
        [
            ("213011562".into(), "get_weather".into(), json!({"city":"Paris"})),
            ("704331796".into(), "get_weather".into(), json!({"city":"Rome"})),
        ]
    );
    assert_eq!(text_of(&r.message), "\n");
}

#[test]
fn recorded_call_that_was_cut_is_not_run() {
    let r = record("tool-cut");
    assert_eq!(r.stop, Some(Stop::MaxOutput));
    let Content::ToolCall(c) = &r.message.blocks[0].content else { panic!() };
    assert_eq!((c.raw.as_str(), &c.args, calls(&r).len()), ("", &Args::Cut, 0));
    // Not even where an empty string means no arguments.
    let (_, body) = recorded("tool-cut");
    let quirks = Quirks { empty_args: Some(true), ..Quirks::default() };
    assert_eq!(calls(&with(&quirks, &body).0).len(), 0);
}

#[test]
fn recorded_errors() {
    // The head says 200, and the prompt was never read.
    let (head, body) = recorded("too-long");
    assert!(head.starts_with("HTTP/1.1 200 OK"));
    let (r, e) = read(&body);
    let e = e.unwrap();
    assert_eq!((e.class, e.retry), (Class::ContextTooLong, Retry::Never));
    assert!(e.message.starts_with("The number of tokens to keep"));
    assert!(r.message.blocks.is_empty() && !r.complete);
    // A model it does not have is answered by one it has. Only the stream says so.
    let r = record("unknown-model");
    assert_eq!(r.model.as_deref(), Some("qwen/qwen3-4b-2507"));
    assert!(
        fixture("lmstudio-recorded/unknown-model.request.json")
            .starts_with(br#"{"model":"no-such-model""#)
    );
}

#[test]
fn the_recorded_requests_are_what_is_still_sent() {
    // The fixtures answer these bytes; a change here means they should be recorded again.
    let sent: Value =
        serde_json::from_slice(&fixture("lmstudio-recorded/tool.request.json")).unwrap();
    let mut r = request();
    r.model = sent["model"].as_str().unwrap().into();
    r.system = "Use the tools.".into();
    r.tools = vec![Tool {
        name: "get_weather".into(),
        description: "The weather now at a place.".into(),
        schema: Json::parse(
            r#"{"type":"object","properties":{"city":{"type":"string"}},"required":["city"]}"#,
        )
        .unwrap(),
    }];
    r.messages = vec![Message::user("Weather in Paris?")];
    r.settings.max_output = Some(120);
    let body = encode(&r, &Profile::lm_studio()).unwrap().body;
    assert_eq!(body, fixture("lmstudio-recorded/tool.request.json"));
}

// Other servers.

/// A delta as a whole event.
fn delta(delta: Value) -> String {
    json!({"id":"c1","model":"m","choices":[{"index":0,"delta":delta,"finish_reason":null}]})
        .to_string()
}
fn piece(call: Value) -> String {
    delta(json!({ "tool_calls": [call] }))
}
fn ends(reason: &str) -> String {
    json!({"id":"c1","model":"m","choices":[{"index":0,"delta":{},"finish_reason":reason}]})
        .to_string()
}
fn stream(parts: &[String]) -> Vec<u8> {
    let mut all = parts.join("\n");
    all.push_str("\n[DONE]");
    lines(&all)
}
fn function(name: &str, arguments: &str) -> Value {
    json!({ "name": name, "arguments": arguments })
}

#[test]
fn a_call_in_one_piece_or_in_many() {
    let run = |parts: &[Value]| {
        let mut all: Vec<String> = parts.iter().cloned().map(piece).collect();
        all.push(ends("tool_calls"));
        let (r, e) = read(&stream(&all));
        assert_eq!(e, None);
        (calls(&r), r)
    };
    let one = |id: &str, n: u32| (id.to_owned(), "f".to_owned(), json!({ "n": n }));
    let whole = |n: u32| format!(r#"{{"n":{n}}}"#);
    // Whole, with everything, without an index, without an id, without either.
    let (got, _) =
        run(&[json!({"index":0,"id":"a","type":"function","function":function("f", &whole(1))})]);
    assert_eq!(got, [one("a", 1)]);
    let (got, _) = run(&[
        json!({"id":"a","function":function("f", &whole(1))}),
        json!({"id":"b","function":function("f", &whole(2))}),
    ]);
    assert_eq!(got, [one("a", 1), one("b", 2)]);
    let (got, _) = run(&[
        json!({"index":0,"function":function("f", &whole(1))}),
        json!({"index":1,"function":function("f", &whole(2))}),
    ]);
    assert_eq!(got, [one("c1_0", 1), one("c1_1", 2)]);
    let (got, _) = run(&[
        json!({"function":function("f", &whole(1))}),
        json!({"function":function("f", &whole(2))}),
    ]);
    assert_eq!(got, [one("c1_0", 1), one("c1_1", 2)]);
    // In pieces, by index and interleaved; by id; by order alone.
    let (got, _) = run(&[
        json!({"index":0,"id":"a","function":function("f", "")}),
        json!({"index":1,"id":"b","function":function("f", r#"{"n""#)}),
        json!({"index":0,"function":{"arguments":r#"{"n":"#}}),
        json!({"index":1,"function":{"arguments":":2}"}}),
        json!({"index":0,"id":"","function":{"name":"","arguments":"1}"}}),
    ]);
    assert_eq!(got, [one("a", 1), one("b", 2)]);
    let (got, _) = run(&[
        json!({"id":"a","function":function("f", r#"{"n""#)}),
        json!({"id":"b","function":function("f", r#"{"n":2"#)}),
        json!({"id":"a","function":{"arguments":":1}"}}),
        json!({"id":"b","function":{"arguments":"}"}}),
    ]);
    assert_eq!(got, [one("a", 1), one("b", 2)]);
    let (got, _) = run(&[
        json!({"id":"a","function":function("f", r#"{"n""#)}),
        json!({"function":{"arguments":":1}"}}),
    ]);
    assert_eq!(got, [one("a", 1)]);
    // Every call under the same number, told apart by id.
    let (got, _) = run(&[
        json!({"index":0,"id":"a","function":function("f", &whole(1))}),
        json!({"index":0,"id":"b","function":function("f", r#"{"n""#)}),
        json!({"index":0,"function":{"arguments":":2}"}}),
    ]);
    assert_eq!(got, [one("a", 1), one("b", 2)]);
    // The id and the name on every piece; the arguments before the name; an id too late to count.
    let (got, _) = run(&[
        json!({"index":0,"id":"a","function":function("f", r#"{"n""#)}),
        json!({"index":0,"id":"a","function":function("f", ":1}")}),
    ]);
    assert_eq!(got, [one("a", 1)]);
    let (got, r) = run(&[
        json!({"index":0,"function":{"arguments":r#"{"n""#}}),
        json!({"index":0,"id":"a","function":function("f", ":1")}),
        json!({"index":0,"function":{"arguments":"}"}}),
    ]);
    assert_eq!(got, [one("a", 1)]);
    let Content::ToolCall(c) = &r.message.blocks[0].content else { panic!() };
    assert_eq!(c.raw, r#"{"n":1}"#);
    // The id before the name, and not with it.
    let (got, _) = run(&[
        json!({"index":0,"id":"a","function":{}}),
        json!({"index":0,"function":function("f", r#"{"n":1}"#)}),
    ]);
    assert_eq!(got, [one("a", 1)]);
    // The arguments as an object, which is not a string.
    let (got, r) = run(&[json!({"index":0,"id":"a","function":{"name":"f","arguments":{"n":1}}})]);
    assert_eq!(got, [one("a", 1)]);
    assert!(native(&r.message.blocks[0]).contains(r#""arguments":"{\"n\":1}""#));
    let (got, _) = run(&[
        json!({"index":0,"id":"a","function":{"name":"f","arguments":null}}),
        json!({"index":0,"function":{"arguments":r#"{"n":1}"#}}),
    ]);
    assert_eq!(got, [one("a", 1)]);
    // Something more beside it, which goes back with it.
    let (_, r) = run(&[
        json!({"index":0,"id":"a","function":function("f", "{}"),"extra_content":{"google":{"thought_signature":"s"}}}),
    ]);
    assert_eq!(
        native(&r.message.blocks[0]),
        r#"{"id":"a","type":"function","function":{"name":"f","arguments":"{}"},"extra_content":{"google":{"thought_signature":"s"}}}"#
    );
}

#[test]
fn a_late_id_belongs_to_the_call_that_was_announced_without_one() {
    let (r, _) = read(&stream(&[
        piece(json!({"index":0,"function":function("f", "{")})),
        piece(json!({"index":0,"id":"c1_0","function":{"arguments":"}"}})),
        ends("tool_calls"),
    ]));
    assert_eq!(calls(&r), [("c1_0".into(), "f".into(), json!({}))]);
    // Another id under the same number is another call; what is left of the first cannot run.
    let (r, _) = read(&stream(&[
        piece(json!({"index":0,"function":function("f", "{")})),
        piece(json!({"index":0,"id":"late","function":{"arguments":"}"}})),
        ends("tool_calls"),
    ]));
    assert_eq!((calls(&r).len(), r.message.blocks.len()), (0, 2));
}

#[test]
fn a_call_that_is_not_right_is_not_run() {
    let broken = |parts: &[Value], reason: &str| {
        let mut all: Vec<String> = parts.iter().cloned().map(piece).collect();
        all.push(ends(reason));
        let (r, e) = read(&stream(&all));
        assert_eq!((calls(&r).len(), e), (0, None), "{parts:?}");
        let Content::ToolCall(c) = &r.message.blocks[0].content else { panic!() };
        c.args.clone()
    };
    let f = |a: &str| json!({"index":0,"id":"a","function":function("f", a)});
    for args in ["", " ", "{", "[]", "\"{}\"", "null", r#"{"n":1"#, "{}{}"] {
        assert!(matches!(broken(&[f(args)], "tool_calls"), Args::Broken(_)), "{args}");
    }
    let two = [f("{}"), json!({"index":0,"function":function("g", "")})];
    assert_eq!(broken(&two, "tool_calls"), Args::Broken("the call was given two names".into()));
    let nameless = [json!({"index":0,"id":"a","function":{"arguments":"{}"}})];
    assert!(matches!(broken(&nameless, "tool_calls"), Args::Broken(_)));
    assert!(matches!(broken(&[json!({"index":0,"id":"a"})], "tool_calls"), Args::Broken(_)));
    for reason in ["length", "content_filter", "function_call", "eos", ""] {
        assert_eq!(broken(&[f("{}")], reason), Args::Cut, "{reason}");
    }
    // `stop` is what some servers say after calls.
    let (r, _) = read(&stream(&[piece(f("{}")), ends("stop")]));
    assert_eq!(calls(&r).len(), 1);
    // An empty string is no arguments only where that is known of a server.
    let quirks = Quirks { empty_args: Some(true), ..Quirks::default() };
    let (r, _) = with(&quirks, &stream(&[piece(f(" ")), ends("tool_calls")]));
    assert_eq!(calls(&r).len(), 1);
    let (r, _) = with(&quirks, &stream(&[piece(f("")), ends("tool_calls")]));
    assert_eq!(calls(&r), [("a".into(), "f".into(), json!({}))]);
    assert!(native(&r.message.blocks[0]).contains(r#""arguments":"{}""#));
    let (r, _) = with(&quirks, &stream(&[piece(f("{")), ends("tool_calls")]));
    assert_eq!(calls(&r).len(), 0);
}

#[test]
fn reasoning_by_either_name() {
    for key in ["reasoning_content", "reasoning"] {
        let (r, _) = read(&stream(&[
            delta(json!({"role":"assistant","content":"", key: "a"})),
            delta(json!({ key: "b", "content": null })),
            delta(json!({ key: "", "content": "c" })),
            delta(json!({ "content": "d" })),
            ends("stop"),
        ]));
        let [reasoning, text] = r.message.blocks.as_slice() else { panic!() };
        assert_eq!(reasoning.content, Content::Reasoning { text: "ab".into() });
        assert_eq!(native(reasoning), format!(r#"{{"{key}":"ab"}}"#));
        assert_eq!(text.content, Content::Text { text: "cd".into() });
    }
    // An empty one leaves it to the other.
    let both = json!({"reasoning_content":"","reasoning":"a"});
    let (r, _) = read(&stream(&[delta(both), ends("stop")]));
    assert_eq!(r.message.blocks[0].content, Content::Reasoning { text: "a".into() });
    // A server that sends both says the same thing twice.
    let (r, _) =
        read(&stream(&[delta(json!({"reasoning_content":"a","reasoning":"a"})), ends("stop")]));
    assert_eq!(r.message.blocks.len(), 1);
    assert_eq!(r.message.blocks[0].content, Content::Reasoning { text: "a".into() });
}

#[test]
fn reasoning_between_tags_where_the_profile_says_so() {
    let parts =
        ["<th", "ink>a", "b</", "think>", "c</think>"].map(|t| delta(json!({ "content": t })));
    let mut parts = parts.to_vec();
    parts.push(ends("stop"));
    let bytes = stream(&parts);
    let (r, _) = read(&bytes);
    assert_eq!(text_of(&r.message), "<think>ab</think>c</think>");
    let quirks = Quirks { think: Some(Think::Leading), ..Quirks::default() };
    let (r, _) = with(&quirks, &bytes);
    let contents: Vec<_> = r.message.blocks.iter().map(|b| b.content.clone()).collect();
    let want =
        [Content::Reasoning { text: "ab".into() }, Content::Text { text: "c</think>".into() }];
    assert_eq!(contents, want);
    assert_eq!(r.message.blocks[0].native, None);
    // What looked like the beginning of a tag and was not, at the very end.
    let (r, _) = with(&quirks, &stream(&[delta(json!({"content":"<thi"})), ends("stop")]));
    assert_eq!(text_of(&r.message), "<thi");
}

#[test]
fn members_that_are_not_known_survive() {
    let (r, e) = read(&stream(&[
        delta(json!({"reasoning":"x","reasoning_details":[{"type":"reasoning.text","n":1}]})),
        delta(
            json!({"reasoning":"y","reasoning_details":[{"type":"reasoning.encrypted","data":"e"}]}),
        ),
        delta(json!({"refusal":"no","audio":null,"odd":{"a":1}})),
        delta(json!({"refusal":"!","odd":{"b":1},"mixed":[1]})),
        delta(
            json!({"mixed":"s","tool_calls":"nonsense","content":[{"type":"text","text":"t"},{"type":"image","i":1}]}),
        ),
        ends("stop"),
    ]));
    assert_eq!(e, None);
    let find = |name: &str| {
        let is = |b: &&Block| b.content == Content::Unknown { name: name.into() };
        r.message.blocks.iter().find(is).unwrap()
    };
    assert_eq!(
        native(find("reasoning_details")),
        r#"{"reasoning_details":[{"n":1,"type":"reasoning.text"},{"data":"e","type":"reasoning.encrypted"}]}"#
    );
    assert!(find("reasoning_details").extra.is_empty());
    assert_eq!(native(find("refusal")), r#"{"refusal":"no!"}"#);
    assert_eq!((find("odd").native.as_ref(), find("odd").extra.len()), (None, 2));
    assert_eq!((find("mixed").native.as_ref(), find("mixed").extra.len()), (None, 2));
    assert_eq!(native(find("tool_calls")), r#"{"tool_calls":"nonsense"}"#);
    assert_eq!(find("content").extra, [Json::parse(r#"{"i":1,"type":"image"}"#).unwrap()]);
    assert_eq!(text_of(&r.message), "t");
    // What is null is not there.
    assert!(
        !r.message.blocks.iter().any(|b| b.content == Content::Unknown { name: "audio".into() })
    );
    let reasoning = &r.message.blocks[0];
    assert_eq!(reasoning.content, Content::Reasoning { text: "xy".into() });
    // One object is whole.
    let (r, _) = read(&stream(&[delta(json!({"odd":{"a":1}})), ends("stop")]));
    assert_eq!(native(&r.message.blocks[0]), r#"{"odd":{"a":1}}"#);
}

#[test]
fn usage() {
    let usage = |u: Value| {
        let last = json!({"choices":[],"usage":u}).to_string();
        read(&stream(&[ends("stop"), last])).0.usage
    };
    assert_eq!(
        usage(
            json!({"prompt_tokens":100,"completion_tokens":9,"prompt_tokens_details":{"cached_tokens":60},"completion_tokens_details":{"reasoning_tokens":4}})
        ),
        Usage {
            prompt: Some(100),
            uncached: Some(40),
            cache_read: Some(60),
            output: Some(9),
            reasoning: Some(4),
            ..Usage::default()
        }
    );
    // More read from the cache than there was: said, and nothing made of it.
    let u = usage(json!({"prompt_tokens":1,"prompt_tokens_details":{"cached_tokens":60}}));
    assert_eq!((u.prompt, u.uncached, u.cache_read), (Some(1), None, Some(60)));
    assert_eq!(usage(json!(null)), Usage::default());
    assert_eq!(usage(json!({"prompt_tokens":"9","completion_tokens":-1})), Usage::default());
    // In every event, the last counts; beside the end; before it.
    let (r, _) = read(&stream(&[
        json!({"choices":[],"usage":{"completion_tokens":1}}).to_string(),
        json!({"choices":[{"delta":{"content":"a"},"finish_reason":"stop"}],"usage":{"completion_tokens":2}}).to_string(),
    ]));
    assert_eq!((r.usage.output, r.complete, text_of(&r.message).as_str()), (Some(2), true, "a"));
}

#[test]
fn how_an_answer_ends() {
    let text = delta(json!({"content":"a"}));
    let end = |parts: &[&str]| {
        let (r, e) = read(&lines(&parts.join("\n")));
        (r.complete, r.stop, e.map(|e| (e.class, e.retry)))
    };
    let (stop, length) = (ends("stop"), ends("length"));
    let short = Some((Class::CutShort, Retry::Restart));
    assert_eq!(end(&[&text, &stop, "[DONE]"]), (true, Some(Stop::EndTurn), None));
    assert_eq!(end(&[&text, &stop]), (true, Some(Stop::EndTurn), None));
    assert_eq!(end(&[&text, &length]), (true, Some(Stop::MaxOutput), None));
    assert_eq!(end(&[&text]), (false, None, short));
    assert_eq!(end(&[]), (false, None, short));
    assert_eq!(end(&[&text, "[DONE]"]), (false, None, short));
    assert_eq!(end(&[&text, &stop, "[DONE]", "not json"]), (true, Some(Stop::EndTurn), None));
    let broken = Some((Class::Protocol, Retry::Never));
    assert_eq!(end(&[&text, &stop, "not json"]), (false, Some(Stop::EndTurn), broken));
    assert_eq!(end(&[&text, "[]"]).2, broken);
    assert_eq!(
        end(&[r#"{"error":{"message":"gone","code":500}}"#]).2,
        Some((Class::Server, Retry::Restart))
    );
    assert_eq!(
        end(&[&text, r#"{"error":"gone"}"#, &stop]),
        (false, None, Some((Class::Server, Retry::Restart)))
    );
    let filtered = end(&[&ends("content_filter"), "[DONE]"]).1;
    assert_eq!(filtered, Some(Stop::Refusal(Some("content_filter".into()))));
    assert_eq!(end(&[&ends("eos"), "[DONE]"]).1, Some(Stop::Other("eos".into())));
    // The text of a cut answer is kept and marked.
    let (r, _) = read(&lines(&text));
    assert!(r.message.blocks[0].cut && text_of(&r.message) == "a");
    let (r, _) = read(&lines(&format!("{text}\n{stop}")));
    assert!(!r.message.blocks[0].cut);
    // An error by its event's name; a server that has nothing to say between.
    let (_, e) = read(b"event: error\ndata: {\"message\":\"gone\"}\n\n");
    assert_eq!(e.unwrap().message, "gone");
    let (r, e) = read(&stream(&[String::new(), " ".into(), ends("stop")]));
    assert!(r.complete && e.is_none());
    let (r, e) = read(&[b"data:\n\ndata: \n\n".as_slice(), &stream(&[ends("stop")])].concat());
    assert!(r.complete && e.is_none());
}

#[test]
fn only_the_choice_that_was_asked_for() {
    let other = r#"{"choices":[{"index":1,"delta":{"content":"no"},"finish_reason":"length"},{"delta":{"content":"yes"}}]}"#;
    let (r, _) = read(&stream(&[other.into(), ends("stop")]));
    assert_eq!((text_of(&r.message).as_str(), r.stop), ("yes", Some(Stop::EndTurn)));
    let (r, e) = read(&stream(&[
        r#"{"choices":null}"#.into(),
        r#"{"choices":[{"delta":null}]}"#.into(),
        ends("stop"),
    ]));
    assert!(r.message.blocks.is_empty() && e.is_none());
}

#[test]
fn an_answer_is_bounded() {
    let flip = |n: usize| {
        let key = if n.is_multiple_of(2) { "content" } else { "reasoning" };
        delta(json!({ key: "x" }))
    };
    let parts: Vec<String> = (0..=MAX_BLOCKS).map(flip).collect();
    let (r, e) = read(&stream(&parts));
    assert_eq!((r.message.blocks.len(), e.unwrap().class), (MAX_BLOCKS, Class::Protocol));
    let parts: Vec<String> =
        (0..=MAX_BLOCKS).map(|n| piece(json!({"index":n,"id":format!("{n}")}))).collect();
    assert_eq!(read(&stream(&parts)).1.unwrap().class, Class::Protocol);
    let parts: Vec<String> =
        (0..=MAX_BLOCKS).map(|n| delta(json!({ format!("k{n}"): 1 }))).collect();
    assert_eq!(read(&stream(&parts)).1.unwrap().class, Class::Protocol);
    // As many as there may be still end one by one.
    let parts: Vec<String> = (0..MAX_BLOCKS)
        .map(|n| piece(json!({"index":n,"id":format!("{n}"),"function":function("f", "{}")})))
        .chain([ends("stop")])
        .collect();
    let (got, e) = events(Codec::Chat, &Quirks::default(), &[&stream(&parts)]);
    assert_eq!((got.last(), e), (Some(&Event::End), None));
    assert_eq!(fold(Codec::Chat, &got).message.calls().count(), MAX_BLOCKS);
}

// Requests.

fn profile() -> Profile {
    Profile::openai_compatible("p", "https://example.com/v1")
}
fn request() -> Request {
    Request {
        model: "m".into(),
        system: "Be brief.".into(),
        tools: vec![Tool {
            name: "f".into(),
            description: "d".into(),
            schema: Json::parse(r#"{"type":"object", "properties":{"z":{},"a":{}}}"#).unwrap(),
        }],
        messages: vec![Message::user("hi")],
        settings: Settings::default(),
    }
}
fn body(request: &Request, profile: &Profile) -> String {
    String::from_utf8(encode(request, profile).unwrap().body).unwrap()
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
        body(&r, &profile()),
        r#"{"model":"m","stream":true,"stream_options":{"include_usage":true},"tools":[{"type":"function","function":{"name":"f","description":"d","parameters":{"type":"object", "properties":{"z":{},"a":{}}}}}],"messages":[{"role":"system","content":"Be brief."},{"role":"user","content":"hi"}]}"#
    );
    (r.system, r.tools) = (String::new(), Vec::new());
    r.settings = Settings {
        max_output: Some(9),
        effort: Some("low".into()),
        stop: vec!["END".into()],
        cache: Cache::Long,
    };
    r.messages.push(Message {
        role: Role::Assistant,
        blocks: vec![Block::text("a"), call("c", ready()), Block::text("b")],
    });
    r.messages.push(Message {
        role: Role::User,
        blocks: vec![
            Block::result("c", "", true),
            Block::text("one"),
            Block::text(""),
            Block::result("d", "r", false),
            Block::text("two"),
        ],
    });
    let mut p = profile();
    p.quirks.effort = Some(Effort::ReasoningEffort);
    p.quirks.stream_usage = Some(false);
    p.quirks.max_tokens_field = Some(MaxTokens::MaxCompletionTokens);
    p.quirks.body.insert("seed".into(), json!(1));
    assert_eq!(
        body(&r, &p),
        r#"{"model":"m","stream":true,"max_completion_tokens":9,"stop":["END"],"reasoning_effort":"low","seed":1,"messages":[{"role":"user","content":"hi"},{"role":"assistant","content":"ab","tool_calls":[{"id":"c","type":"function","function":{"arguments":"{\"q\": 1}","name":"f"}}]},{"role":"tool","tool_call_id":"c","content":""},{"role":"tool","tool_call_id":"d","content":"r"},{"role":"user","content":"one\n\ntwo"}]}"#
    );
    p.quirks.max_tokens_field = None;
    assert!(body(&r, &p).contains(r#""max_tokens":9"#));
}

#[test]
fn what_cannot_be_asked_for_is_an_error() {
    let mut r = request();
    r.settings.effort = Some("low".into());
    assert_eq!(encode(&r, &profile()).unwrap_err(), EncodeError::Unsupported("effort"));
    let mut p = profile();
    p.quirks.effort = Some(Effort::OutputConfig);
    assert_eq!(encode(&r, &p).unwrap_err(), EncodeError::Unsupported("effort"));
    let own = [
        "model",
        "stream",
        "stream_options",
        "max_tokens",
        "max_completion_tokens",
        "stop",
        "reasoning_effort",
        "tools",
        "messages",
        "n",
    ];
    for own in own {
        p.quirks.body = [(own.to_owned(), json!(1))].into();
        assert_eq!(encode(&request(), &p).unwrap_err(), EncodeError::Reserved(own.into()));
    }
}

#[test]
fn appending_changes_nothing_before() {
    let odd = read(&stream(&[
        delta(json!({"reasoning_content":"why","reasoning_details":[{"d":1}],"content":"t"})),
        piece(json!({"index":0,"id":"x","function":function("f", "{}"),"sig":"s"})),
        ends("tool_calls"),
    ]));
    let mut p = profile();
    p.quirks.replay_reasoning = Some(true);
    for p in [profile(), p] {
        let mut r = request();
        r.messages.clear();
        let mut before: Option<String> = None;
        for message in [
            Message::user("one"),
            record("tool").message,
            Message { role: Role::User, blocks: vec![Block::result("764517396", "r", false)] },
            odd.0.message.clone(),
            Message {
                role: Role::User,
                blocks: vec![Block::result("x", "r", false), Block::text("two")],
            },
            Message { role: Role::Assistant, blocks: vec![call("cut", Args::Cut)] },
            Message { role: Role::User, blocks: vec![Block::result("cut", "r", false)] },
            record("tool-result").message,
        ] {
            r.messages.push(message);
            let now = body(&r, &p);
            if let Some(before) = before {
                assert_eq!(now[..before.len() - 2], before[..before.len() - 2]);
            }
            before = Some(now);
        }
        assert!(before.unwrap().contains(r#""sig":"s""#));
    }
}

#[test]
fn what_goes_back_and_what_is_lost() {
    let (reply, _) = read(&stream(&[
        delta(json!({"reasoning_content":"why","reasoning_details":[{"z":1,"a":2}],"role":"x"})),
        delta(json!({"content":"t"})),
        piece(json!({"index":0,"id":"x","function":function("f", "{ }"),"sig":"s"})),
        ends("tool_calls"),
    ]));
    let mut r = request();
    (r.system, r.tools, r.messages) = (String::new(), Vec::new(), vec![reply.message]);
    let mut p = profile();
    let with_call = r#""tool_calls":[{"id":"x","type":"function","function":{"name":"f","arguments":"{ }"},"sig":"s"}]}]}"#;
    let e = encode(&r, &p).unwrap();
    let text = String::from_utf8(e.body).unwrap();
    assert!(
        text.ends_with(&format!(r#""messages":[{{"role":"assistant","content":"t",{with_call}"#)),
        "{text}"
    );
    let lost: Vec<_> = e.lost.iter().map(|l| (l.block, l.what)).collect();
    assert_eq!(lost, [(0, Loss::Reasoning), (1, Loss::Unknown)]);
    // Back as it came, where it is known to be wanted.
    p.quirks.replay_reasoning = Some(true);
    let e = encode(&r, &p).unwrap();
    let text = String::from_utf8(e.body).unwrap();
    assert!(text.ends_with(&format!(r#"{{"role":"assistant","content":"t","reasoning_content":"why","reasoning_details":[{{"a":2,"z":1}}],{with_call}"#)), "{text}");
    assert_eq!(e.lost, []);
    // Not to another model, and not what another codec kept.
    r.model = "other".into();
    let e = encode(&r, &p).unwrap();
    assert!(e.lost.len() == 2 && !String::from_utf8(e.body).unwrap().contains("sig"));
    // What cannot run is not there, nor is a message that would be empty.
    r.messages = vec![
        Message { role: Role::Assistant, blocks: vec![call("a", Args::Cut)] },
        Message { role: Role::User, blocks: vec![Block::result("a", "r", false)] },
        Message { role: Role::Assistant, blocks: vec![Block::result("a", "r", false)] },
        Message { role: Role::User, blocks: vec![call("a", ready()), Block::text("x")] },
        Message {
            role: Role::User,
            blocks: vec![Block::new(Content::Unknown { name: "u".into() })],
        },
    ];
    let e = encode(&r, &p).unwrap();
    let lost = [
        Loss::UnfinishedCall,
        Loss::ResultOfUnfinishedCall,
        Loss::Misplaced,
        Loss::Misplaced,
        Loss::Unknown,
    ];
    let want: Vec<_> = lost
        .into_iter()
        .enumerate()
        .map(|(message, what)| Lost { message, block: 0, what })
        .collect();
    assert_eq!(e.lost, want);
    assert!(
        String::from_utf8(e.body)
            .unwrap()
            .ends_with(r#""messages":[{"role":"user","content":"x"}]}"#)
    );
}
