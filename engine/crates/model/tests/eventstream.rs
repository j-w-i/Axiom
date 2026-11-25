//! Amazon's event streams: the vectors of AWS's own SDK, frames made here, and damage of every kind.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod common;

use axiom_model::eventstream::{Decoder, FrameError, Message, Value};
use common::*;
use proptest::prelude::*;

const LIMIT: usize = 1 << 16;

fn push(pieces: &[&[u8]]) -> (Vec<Message>, Option<FrameError>) {
    let (mut d, mut out) = (Decoder::new(LIMIT), Vec::new());
    for piece in pieces {
        if let Err(e) = d.push(piece, &mut out) {
            // And nothing after it.
            let seen = out.len();
            assert_eq!(d.push(&amazon::event("e", "{}"), &mut out), Err(e));
            assert_eq!(out.len(), seen);
            return (out, Some(e));
        }
    }
    (out, None)
}
fn hex(text: &str) -> Vec<u8> {
    (0..text.len()).step_by(2).map(|i| u8::from_str_radix(&text[i..i + 2], 16).unwrap()).collect()
}
fn text(name: &str, value: &str) -> (String, Value) {
    (name.into(), Value::Text(value.into()))
}

/// `aws/protocol/eventstream/testdata` of github.com/aws/aws-sdk-go-v2 (Apache-2.0), read 2026-11-25.
#[test]
fn the_vectors_of_the_sdk() {
    let payload = b"{'foo':'bar'}".to_vec();
    let json = || text("content-type", "application/json");
    let one = || ("event-type".to_owned(), Value::Int(40972));
    for (encoded, headers, payload) in [
        ("000000100000000005c248eb7d98c8ff", vec![], vec![]),
        ("0000001d00000000fd528c5a7b27666f6f273a27626172277dc3653936", vec![], payload.clone()),
        (
            "0000002d0000001041c424b80a6576656e742d74797065040000a00c7b27666f6f273a27626172277d36f480a0",
            vec![one()],
            payload.clone(),
        ),
        (
            "0000003d0000002007fd83960c636f6e74656e742d747970650700106170706c69636174696f6e2f6a736f6e7b27666f6f273a27626172277d8d9c08b1",
            vec![json()],
            payload.clone(),
        ),
        (
            "000000cc000000af0fae64ca0a6576656e742d74797065040000a00c0c636f6e74656e742d747970650700106170706c69636174696f6e2f6a736f6e0a626f6f6c2066616c73650109626f6f6c207472756500046279746502cf08627974652062756606001449276d2061206c6974746c6520746561706f74210974696d657374616d70080000000000845fed05696e74313603002a05696e7436340500000000028757b20475756964090102030405060708090a0b0c0d0e0f107b27666f6f273a27626172277daba5f10c",
            vec![
                one(),
                json(),
                ("bool false".into(), Value::Bool(false)),
                ("bool true".into(), Value::Bool(true)),
                ("byte".into(), Value::Byte(-49)),
                ("byte buf".into(), Value::Bytes(b"I'm a little teapot!".into())),
                ("timestamp".into(), Value::Time(8_675_309)),
                ("int16".into(), Value::Short(42)),
                ("int64".into(), Value::Long(42_424_242)),
                (
                    "uuid".into(),
                    Value::Uuid([1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]),
                ),
            ],
            payload,
        ),
    ] {
        let want = Message { headers, payload };
        assert_eq!(push(&[&hex(encoded)]), (vec![want], None), "{encoded}");
    }
    for damaged in [
        // Its prelude: a length of the headers, and of the message.
        "0000003d0000002107fd83960c636f6e74656e742d747970650700106170706c69636174696f6e2f6a736f6e7b27666f6f273a27626172277d8d9c08b1",
        "0000003e0000002007fd83960c636f6e74656e742d747970650700106170706c69636174696f6e2f6a736f6e7b27666f6f273a27626172277d8d9c08b1",
        // The rest.
        "0000003d0000002007fd83960c636f6e74656e742d747970650700106170706c69636174696f6e2f6a736f6e7b61666f6f273a27626172277d8d9c08b1",
        "0000001d00000000fd528c5a5b27666f6f273a27626172277dc3653936",
    ] {
        assert_eq!(push(&[&hex(damaged)]), (vec![], Some(FrameError::Checksum)), "{damaged}");
    }
}

#[test]
fn a_flipped_bit_is_found_wherever_it_is() {
    let whole = [amazon::event("a", "{}"), amazon::event("messageStop", r#"{"x":1}"#)].concat();
    let first = amazon::event("a", "{}").len();
    for at in 0..whole.len() {
        for bit in 0..8 {
            let mut damaged = whole.clone();
            damaged[at] ^= 1 << bit;
            let (got, e) = push(&[&damaged]);
            assert_eq!(e, Some(FrameError::Checksum), "{at}.{bit}");
            // What came before is what it was.
            assert_eq!(got.len(), usize::from(at >= first));
        }
    }
}

#[test]
fn headers_that_cannot_be_read_are_an_error() {
    let a = amazon::text("a", "x");
    let long = [&[1, b'a', 7, 0, 9][..], b"short"].concat();
    for (head, why) in [
        (vec![0, 0], "no name"),
        (vec![2, b'a'], "a name that does not end"),
        (vec![1, b'a'], "no type"),
        (vec![1, b'a', 10], "an unknown type"),
        (vec![1, b'a', 255], "an unknown type"),
        (vec![1, 0xff, 0], "a name that is not text"),
        (vec![1, b'a', 7, 0, 1, 0xff], "a value that is not text"),
        (vec![1, b'a', 7, 0], "half a length"),
        (vec![1, b'a', 2], "no byte"),
        (vec![1, b'a', 3, 0], "half a short"),
        (vec![1, b'a', 4, 0, 0, 0], "most of an integer"),
        (vec![1, b'a', 5, 0, 0, 0, 0, 0, 0, 0], "most of a long"),
        (vec![1, b'a', 6, 0, 3, 1, 2], "bytes that end early"),
        (vec![1, b'a', 8, 0, 0, 0, 0, 0, 0, 0], "most of a time"),
        (vec![1, b'a', 9, 0], "most of a uuid"),
        (long, "a text that ends early"),
        ([a.clone(), a.clone()].concat(), "a name twice"),
        ([a.clone(), vec![1]].concat(), "one, and half of another"),
    ] {
        let payload = b"{\"not\":\"a header\"}";
        let (got, e) = push(&[&amazon::framed(&head, payload)]);
        assert_eq!((got, e), (vec![], Some(FrameError::Headers)), "{why}");
    }
    // They end where their length says, not where a value would reach.
    let (got, e) = push(&[&amazon::framed(&a, b"rest")]);
    assert_eq!(e, None);
    assert_eq!(got, [Message { headers: vec![text("a", "x")], payload: b"rest".into() }]);
    assert_eq!((got[0].text("a"), got[0].text("b")), (Some("x"), None));
    // One that is not text is not read as one.
    let (got, _) = push(&[&amazon::framed(&[1, b'a', 0], b"")]);
    assert_eq!((got[0].text("a"), &got[0].headers[0].1), (None, &Value::Bool(true)));
}

#[test]
fn lengths_that_cannot_be_are_an_error() {
    let prelude = |total: u32, headers: u32| {
        let mut p = [total.to_be_bytes(), headers.to_be_bytes()].concat();
        p.extend(amazon::crc(&p).to_be_bytes());
        p
    };
    let limit = u32::try_from(LIMIT).unwrap();
    for (total, headers, want) in [
        (0, 0, FrameError::Length),
        (15, 0, FrameError::Length),
        (16, 1, FrameError::Length),
        (32, 17, FrameError::Length),
        (32, u32::MAX, FrameError::Length),
        (limit + 1, 0, FrameError::TooLarge(LIMIT)),
        (u32::MAX, u32::MAX, FrameError::TooLarge(LIMIT)),
    ] {
        // With the prelude alone, before anything is waited for.
        let (got, e) = push(&[&prelude(total, headers)]);
        assert_eq!((got.len(), e), (0, Some(want)), "{total} {headers}");
    }
    assert_eq!(push(&[&prelude(32, 16)]), (vec![], None));
    assert_eq!(push(&[&prelude(limit, 0)]), (vec![], None));
    // As large as may be.
    let largest = amazon::framed(&[], &vec![b'x'; LIMIT - 16]);
    assert_eq!(push(&[&largest]).0[0].payload.len(), LIMIT - 16);
}

#[test]
fn a_message_that_did_not_end_is_none() {
    let whole = amazon::event("messageStop", r#"{"stopReason":"end_turn"}"#);
    for to in 0..whole.len() {
        assert_eq!(push(&[&whole[..to]]), (vec![], None));
    }
    let (got, e) = push(&[&whole, &whole[..whole.len() - 1]]);
    assert_eq!((got.len(), e), (1, None));
}

proptest! {
    #[test]
    fn no_bytes_make_it_panic(bytes in proptest::collection::vec(any::<u8>(), 0..300)) {
        push(&[&bytes]);
        // Also those that say they are whole.
        push(&[&amazon::framed(&bytes, b"p")]);
    }

    #[test]
    fn any_cut(
        messages in proptest::collection::vec(
            (proptest::collection::vec(any::<u8>(), 0..40), "[a-z]{1,9}", ".{0,20}"), 0..6),
        cuts in proptest::collection::vec(any::<prop::sample::Index>(), 0..20),
    ) {
        let bytes: Vec<u8> = messages
            .iter()
            .flat_map(|(payload, name, value)| amazon::message(&[(name, value)], payload))
            .collect();
        let mut at: Vec<usize> = cuts.iter().map(|c| c.index(bytes.len() + 1)).collect();
        at.sort_unstable();
        at.push(bytes.len());
        let mut from = 0;
        let pieces: Vec<&[u8]> =
            at.into_iter().map(|to| &bytes[std::mem::replace(&mut from, to)..to]).collect();
        let want: Vec<Message> = messages
            .iter()
            .map(|(payload, name, value)| Message {
                headers: vec![text(name, value)],
                payload: payload.clone(),
            })
            .collect();
        prop_assert_eq!(push(&pieces), (want, None));
    }
}
