//! However an answer is cut into chunks, and wherever it ends: the same message, and never a call that
//! was not whole.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axiom_model::{
    Accumulator, Args, Block, Codec, Content, Delta, Event, Json, OutOfOrder, Quirks, Reply, Start,
    Stop, profile::Think,
};
use common::*;
use proptest::prelude::*;

fn streams() -> Vec<(Codec, Vec<u8>)> {
    let chat = ["text", "tool", "tool-parallel", "tool-cut", "tool-no-args", "too-long", "length"];
    let from = |codec, place: &str, names: &[&str]| -> Vec<_> {
        names
            .iter()
            .map(|n| (codec, fixture(&format!("{place}-from-documentation/{n}.sse"))))
            .collect()
    };
    let messages = ["text", "tool", "thinking", "web-search", "error"];
    let responses = ["text", "tool", "reasoning", "failed", "incomplete", "error"];
    let gemini = ["text", "call", "parallel", "signed-text", "tool-combination", "continuation"];
    chat.iter()
        .map(|n| (Codec::Chat, recorded(n).1))
        .chain(from(Codec::Messages, "messages", &messages))
        .chain(from(Codec::Responses, "responses", &responses))
        .chain(from(Codec::Gemini, "gemini", &gemini))
        .chain(from(Codec::Chat, "openrouter", &["reasoning", "error"]))
        .chain([
            (Codec::Chat, made_up()),
            (Codec::Responses, made_up_responses()),
            (Codec::Gemini, made_up_gemini()),
        ])
        .collect()
}

/// What no documented answer has: items open together, calls at once, parts and events of other kinds,
/// characters of several bytes.
fn made_up_responses() -> Vec<u8> {
    lines(
        r#"
        {"type":"response.created","response":{"id":"r","model":"m"}}
        {"type":"response.output_item.added","output_index":0,"item":{"type":"reasoning","summary":[]}}
        {"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":0,"delta":"é"}
        {"type":"response.output_item.added","output_index":1,"item":{"type":"function_call","call_id":"a","name":"f"}}
        {"type":"response.reasoning_summary_text.delta","output_index":0,"summary_index":1,"delta":"…"}
        {"type":"response.output_item.added","output_index":2,"item":{"type":"function_call","call_id":"b","name":"g"}}
        {"type":"response.function_call_arguments.delta","output_index":2,"delta":"{"}
        {"type":"response.novel","output_index":1,"x":1}
        {"type":"response.function_call_arguments.delta","output_index":1,"delta":"{\"n\":"}
        {"type":"response.output_item.done","output_index":0,"item":{"type":"reasoning","summary":[{"text":"é"},{"text":"… and"}],"encrypted_content":"e"}}
        {"type":"response.output_item.done","output_index":2,"item":{"type":"function_call","call_id":"b","name":"g","arguments":"{}"}}
        {"type":"response.output_item.done","output_index":1,"item":{"type":"function_call","call_id":"a","name":"f","arguments":"{\"n\":1}"}}
        {"type":"response.output_item.added","output_index":3,"item":{"type":"program"}}
        {"type":"response.output_item.done","output_index":3,"item":{"type":"program","code":"1"}}
        {"type":"response.completed","response":{"usage":{"input_tokens":1}}}"#,
    )
}
fn made_up_gemini() -> Vec<u8> {
    lines(
        r#"
        {"candidates":[{"content":{"parts":[{"text":"é","thought":true}]}}],"responseId":"r"}
        {"candidates":[{"content":{"parts":[{"text":"…","thought":true,"thoughtSignature":"S"}]}}]}
        {"candidates":[{"content":{"parts":[{"text":"a"}]}}],"usageMetadata":{"promptTokenCount":1}}
        {"candidates":[{"content":{"parts":[{"text":"b"},{"novel":[1]},{"functionCall":{"name":"f","args":{"n":1}},"thoughtSignature":"S"}]}}]}
        {"candidates":[{"content":{"parts":[{"functionCall":{"name":"f","args":{"n":2}}},{"text":"","thoughtSignature":"S"}]}}]}
        {"candidates":[{"finishReason":"STOP"}],"usageMetadata":{"promptTokenCount":1,"totalTokenCount":3}}"#,
    )
}

/// What no recorded answer has: tags, interleaved calls, members of its own.
fn made_up() -> Vec<u8> {
    lines(
        r#"
        {"id":"c","choices":[{"delta":{"content":"<thi"}}]}
        {"choices":[{"delta":{"content":"nk>é</th"}}]}
        {"choices":[{"delta":{"content":"ink>b","odd":[1]}}]}
        {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"a","function":{"name":"f","arguments":"{\"n\""}},{"index":1,"id":"b","function":{"name":"g","arguments":"{"}}]}}]}
        {"choices":[{"delta":{"odd":[2],"tool_calls":[{"index":1,"function":{"arguments":"}"}},{"index":0,"function":{"arguments":":1}"}}]}}]}
        {"choices":[{"delta":{},"finish_reason":"tool_calls"}]}
        [DONE]"#,
    )
}

fn quirks() -> Quirks {
    Quirks { think: Some(Think::Leading), ..Quirks::default() }
}

#[test]
fn what_is_made_up_says_something() {
    let calls = |codec, bytes: &[u8]| {
        let (r, e) = reply(codec, &quirks(), bytes);
        assert_eq!((e, r.complete), (None, true));
        runs(&r)
    };
    assert_eq!(calls(Codec::Chat, &made_up()), 2);
    assert_eq!(calls(Codec::Responses, &made_up_responses()), 2);
    assert_eq!(calls(Codec::Gemini, &made_up_gemini()), 2);
}

#[test]
fn byte_by_byte() {
    for (codec, bytes) in streams() {
        let pieces: Vec<&[u8]> = bytes.chunks(1).collect();
        assert_eq!(events(codec, &quirks(), &pieces), events(codec, &quirks(), &[&bytes]));
    }
}

fn runs(reply: &Reply) -> usize {
    reply.message.calls().count()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(64))]
    #[test]
    fn any_cut(cuts in proptest::collection::vec(any::<prop::sample::Index>(), 0..40)) {
        for (codec, bytes) in streams() {
            let mut at: Vec<usize> = cuts.iter().map(|c| c.index(bytes.len() + 1)).collect();
            at.sort_unstable();
            at.push(bytes.len());
            let mut from = 0;
            let pieces: Vec<&[u8]> =
                at.into_iter().map(|to| &bytes[std::mem::replace(&mut from, to)..to]).collect();
            let (whole, e) = events(codec, &quirks(), &[&bytes]);
            let (cut, f) = events(codec, &quirks(), &pieces);
            prop_assert_eq!(fold(codec, &cut), fold(codec, &whole));
            prop_assert_eq!(f, e);
        }
    }
}

#[test]
fn wherever_an_answer_ends_early_nothing_in_it_runs() {
    for (codec, bytes) in streams() {
        let (whole, _) = reply(codec, &quirks(), &bytes);
        for to in 0..bytes.len() {
            let (cut, e) = reply(codec, &quirks(), &bytes[..to]);
            // An end that was said before the last byte (usage, `[DONE]`) is an end.
            if cut.complete {
                assert_eq!((runs(&cut), e), (runs(&whole), None));
                assert_eq!(
                    cut.message.calls().collect::<Vec<_>>(),
                    whole.message.calls().collect::<Vec<_>>()
                );
            } else {
                assert_eq!(runs(&cut), 0);
                assert!(e.is_some());
                let sent_back = |b: &Block| matches!(&b.content, Content::ToolCall(c) if matches!(c.args, Args::Ready(_)));
                assert!(!cut.message.blocks.iter().any(sent_back));
                // Nor is a block that did not end taken for one that did.
                assert!(cut.message.blocks.iter().all(|b| !b.cut || b.native.is_none()));
            }
        }
    }
}

#[test]
fn replies_survive_storage() {
    for (codec, bytes) in streams() {
        let (r, _) = reply(codec, &quirks(), &bytes);
        let text = serde_json::to_string(&r).unwrap();
        assert_eq!(serde_json::from_str::<Reply>(&text).unwrap(), r);
        let value = serde_json::to_value(&r).unwrap();
        assert_eq!(serde_json::from_value::<Reply>(value).unwrap(), r);
    }
    assert!(serde_json::from_str::<Json>(r#""{""#).is_err());
}

#[test]
fn events_out_of_order_are_refused() {
    let fold = |events: &[Event]| {
        let mut a = Accumulator::new(scope(Codec::Chat));
        events.iter().try_for_each(|e| a.push(e))
    };
    let start = |block, start| Event::BlockStart { block, start };
    let delta = |block, delta| Event::Delta { block, delta };
    let stop = |block| Event::BlockStop { block, native: None, broken: None };
    let (text, args) = (Delta::Text("t".into()), Delta::Args("a".into()));
    let call = Start::ToolCall { id: "i".into(), name: "n".into() };
    let unknown = Start::Unknown { name: "u".into() };
    assert_eq!(fold(&[start(0, Start::Text), delta(0, text.clone()), stop(0), Event::End]), Ok(()));
    assert_eq!(fold(&[start(0, unknown), delta(0, args.clone()), stop(0)]), Ok(()));
    for bad in [
        &[start(1, Start::Text)][..],
        &[start(0, Start::Text), start(0, Start::Text)],
        &[delta(0, text.clone())],
        &[stop(0)],
        &[start(0, Start::Text), stop(0), stop(0)],
        &[start(0, Start::Text), stop(0), delta(0, text.clone())],
        &[start(0, Start::Text), delta(0, args)],
        &[start(0, call), delta(0, text.clone())],
        &[start(0, Start::Reasoning), delta(0, text)],
        &[Event::End, Event::Stop(Stop::ToolUse)],
        &[Event::End, Event::End],
    ] {
        assert_eq!(fold(bad), Err(OutOfOrder), "{bad:?}");
    }
}
