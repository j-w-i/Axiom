//! However a harness's output is cut into pieces, the lines are the same, and so is where it fails.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_proto::Lines;
use proptest::prelude::*;

/// The lines, and whether it ended in one that is too long.
fn read(limit: usize, pieces: &[&[u8]]) -> (Vec<Vec<u8>>, bool) {
    let mut l = Lines::new(limit);
    let mut all = vec![];
    for p in pieces {
        l.push(p);
        loop {
            match l.next_line() {
                Ok(Some(x)) => all.push(x),
                Ok(None) => break,
                Err(_) => return (all, true),
            }
        }
        // What waits for its end is never more than a line.
        assert!(l.waiting() <= limit);
    }
    (all, false)
}

proptest! {
    #[test]
    fn any_pieces_make_the_same_lines(
        bytes in proptest::collection::vec(
            prop_oneof![4 => any::<u8>(), 2 => Just(b'\n'), 1 => Just(b'\r'), 4 => Just(b'{')], 0..400),
        cuts in proptest::collection::vec(1usize..40, 0..60),
        limit in 1usize..60,
    ) {
        let whole = read(limit, &[&bytes]);
        let (mut pieces, mut at) = (vec![], 0);
        for c in cuts {
            let end = (at + c).min(bytes.len());
            pieces.push(&bytes[at..end]);
            at = end;
        }
        pieces.push(&bytes[at..]);
        prop_assert_eq!(&read(limit, &pieces), &whole);
        let one: Vec<&[u8]> = bytes.chunks(1).collect();
        prop_assert_eq!(&read(limit, &one), &whole);
    }
}
