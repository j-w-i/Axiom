//! One conversation that went on with every provider: each is told what it can be told, and what is
//! lost is said.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axiom_model::{
    Accumulator, Block, Codec, Encoded, Event, Loss, Message, Profile, Quirks, Request, Role,
    Scope, Settings, codec::encode,
};
use common::*;

fn profiles() -> [Profile; 5] {
    [
        Profile::anthropic(),
        Profile::openai(),
        Profile::gemini(),
        Profile::openrouter(),
        Profile::openai_compatible("local", "http://localhost:1/v1"),
    ]
}

/// What `profile` is sure to have said.
fn said(profile: &Profile, path: &str) -> Message {
    let quirks = Quirks::default();
    let (events, e) = events(profile.codec, &quirks, &[&fixture(path)]);
    assert_eq!((e, events.last()), (None, Some(&Event::End)));
    let mut all = Accumulator::new(Scope {
        codec: profile.codec,
        provider: profile.provider.clone(),
        model: "m".into(),
    });
    events.iter().for_each(|e| all.push(e).unwrap());
    all.finish().message
}
fn results(of: &Message) -> Message {
    let blocks = of.calls().map(|c| Block::result(c.id(), "18 degrees", false)).collect();
    Message { role: Role::User, blocks }
}

/// A question, a thought or the like, a call, its result; by each in turn.
fn conversation() -> Vec<Message> {
    let [anthropic, openai, gemini, openrouter, _] = profiles();
    let mut out = Vec::new();
    for (profile, thinks, calls) in [
        (&anthropic, "messages", ["thinking", "tool"]),
        (&openai, "responses", ["reasoning", "tool"]),
        (&gemini, "gemini", ["signed-text", "parallel"]),
    ]
    .map(|(p, place, [a, b])| {
        (
            p,
            format!("{place}-from-documentation/{a}.sse"),
            format!("{place}-from-documentation/{b}.sse"),
        )
    }) {
        out.push(Message::user("What is the weather?"));
        let mut message = said(profile, &thinks);
        message.blocks.append(&mut said(profile, &calls).blocks);
        let results = results(&message);
        out.extend([message, results]);
    }
    out.push(Message::user("and now?"));
    out.push(said(&openrouter, "openrouter-from-documentation/reasoning.sse"));
    out
}
fn request() -> Request {
    Request {
        model: "m".into(),
        system: "Be brief.".into(),
        messages: conversation(),
        settings: Settings { max_output: Some(9), ..Settings::default() },
        ..Request::default()
    }
}
fn text(e: &Encoded) -> String {
    String::from_utf8(e.body.clone()).unwrap()
}

#[test]
fn what_is_lost_is_said_to_each() {
    use Loss::*;
    let of = |codec| match codec {
        // What thinking showed, the summary, signatures: each is another's alone.
        Codec::Messages => {
            vec![(4, 0, Reasoning), (7, 0, Reasoning), (7, 1, Reasoning), (10, 0, Unknown)]
        }
        Codec::Responses => {
            vec![(1, 0, Reasoning), (7, 0, Reasoning), (7, 1, Reasoning), (10, 0, Unknown)]
        }
        Codec::Gemini => vec![(1, 0, Reasoning), (4, 0, Reasoning), (10, 0, Unknown)],
        Codec::Chat => {
            vec![
                (1, 0, Reasoning),
                (4, 0, Reasoning),
                (7, 0, Reasoning),
                (7, 1, Reasoning),
                (10, 0, Unknown),
            ]
        }
    };
    for profile in profiles() {
        let e = encode(&request(), &profile).unwrap();
        let lost: Vec<_> = e.lost.iter().map(|l| (l.message, l.block, l.what)).collect();
        let mut want = of(profile.codec);
        if profile.provider == "openrouter" {
            // It has its own back.
            want.pop();
        }
        assert_eq!(lost, want, "{}", profile.provider);
        let text = text(&e);
        // Everyone is told what was said, what was called, and what came of it.
        for told in [
            "The greatest common divisor of 1071 and 462 is **21**.",
            "The capital of France is Paris.",
            "I need to calculate the risk.",
        ] {
            assert!(text.contains(told), "{}: {told}", profile.provider);
        }
        assert_eq!(text.matches("18 degrees").count(), 4);
        assert_eq!(
            text.matches("get_current_temperature").count(),
            match profile.codec {
                // It names results, too.
                Codec::Gemini => 4,
                _ => 2,
            }
        );
        // And nobody what only another can read.
        let of = [
            (Codec::Messages, "EqQBCgIYAhIM1gbcDa9GJwZA2b3h"),
            (Codec::Responses, "Answering a simple question"),
            (Codec::Responses, "msg_6876cf054f58819284ecc1058131305506fa2fcced15a5ac"),
            (Codec::Gemini, "<Signature_"),
        ];
        for (owner, secret) in of {
            assert_eq!(
                text.contains(secret),
                owner == profile.codec,
                "{} {secret}",
                profile.provider
            );
        }
        assert_eq!(text.contains("reasoning-text-1"), profile.provider == "openrouter");
    }
}

#[test]
fn every_call_has_its_result_under_one_id() {
    for profile in profiles() {
        let text = text(&encode(&request(), &profile).unwrap());
        for id in ["toolu_01T1x1fJ34qAmk2tNTrN7Up6", "call_1234xyz", "call_0", "call_1"] {
            let quoted = text.matches(&format!("\"{id}\"")).count();
            // Gemini's own come without, and go back without.
            let want = if profile.codec == Codec::Gemini && ["call_0", "call_1"].contains(&id) {
                0
            } else {
                2
            };
            assert_eq!(quoted, want, "{} {id}", profile.provider);
        }
    }
}

#[test]
fn appending_changes_nothing_before_whoever_it_is_from() {
    for profile in profiles() {
        let mut r = request();
        r.messages.clear();
        let mut before: Option<String> = None;
        for message in conversation() {
            r.messages.push(message);
            let now = text(&encode(&r, &profile).unwrap());
            if let Some(before) = before {
                assert_eq!(now[..before.len() - 2], before[..before.len() - 2]);
            }
            before = Some(now);
        }
    }
}
