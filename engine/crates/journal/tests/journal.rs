// A test's helpers fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_journal::{Durability, Error, Journal};
use axiom_proto::*;
use proptest::prelude::*;

fn ev(seq: u64, body: Body) -> Event {
    Event { seq, at: seq, body }
}
fn started(seq: u64, i: &str) -> Event {
    ev(
        seq,
        Body::ItemStarted {
            item: ItemId(i.into()),
            run: RunId("r".into()),
            agent: AgentId("a".into()),
            kind: ItemKind::Prose,
            name: String::new(),
            args: None,
        },
    )
}
fn delta(seq: u64, i: &str, t: &str) -> Event {
    ev(
        seq,
        Body::ItemDelta {
            item: ItemId(i.into()),
            text: t.into(),
        },
    )
}
fn run(seq: u64) -> Event {
    ev(
        seq,
        Body::RunStarted {
            run: RunId("r".into()),
            agent: AgentId("a".into()),
        },
    )
}

#[test]
fn what_is_written_is_read_back() {
    let mut j = Journal::in_memory().unwrap();
    let s = SessionId::new();
    let first = j.create(&s, "T", Some("/w"), 5).unwrap();
    let more = [
        run(2),
        started(3, "i"),
        delta(4, "i", "he"),
        delta(5, "i", "llo"),
    ];
    j.append(&s, &more, Durability::Stream).unwrap();
    let mut all = vec![first];
    all.extend(more);
    assert_eq!(j.read(&s, 0, 100).unwrap(), all);
    assert_eq!(j.read(&s, 3, 1).unwrap(), all[3..4]);
    let i = j.session(&s).unwrap().unwrap();
    assert!(i.running);
    assert_eq!(
        (i.last_seq, i.updated, i.workspace.as_deref()),
        (5, 5, Some("/w"))
    );
}

#[test]
fn a_gap_or_a_repeat_is_refused_whole() {
    let mut j = Journal::in_memory().unwrap();
    let s = SessionId::new();
    j.create(&s, "T", None, 1).unwrap();
    for bad in [
        vec![run(3)],
        vec![run(1)],
        vec![run(2), run(2)],
        vec![run(2), run(4)],
    ] {
        assert!(matches!(
            j.append(&s, &bad, Durability::Stream),
            Err(Error::Damaged(_))
        ));
    }
    assert_eq!(j.session(&s).unwrap().unwrap().last_seq, 1);
    assert_eq!(j.read(&s, 0, 9).unwrap().len(), 1);
}

#[test]
fn a_failed_append_in_a_group_undoes_only_itself() {
    let mut j = Journal::in_memory().unwrap();
    let (a, b) = (SessionId::new(), SessionId::new());
    j.create(&a, "A", None, 1).unwrap();
    j.create(&b, "B", None, 1).unwrap();
    let r = j
        .batch(Durability::Intent, |j| {
            [
                j.append_in(&a, &[run(2)]).is_ok(),
                j.append_in(&b, &[run(2), run(9)]).is_ok(),
                j.append_in(&a, &[started(3, "i")]).is_ok(),
            ]
        })
        .unwrap();
    assert_eq!(r, [true, false, true]);
    assert_eq!(j.session(&a).unwrap().unwrap().last_seq, 3);
    assert_eq!(j.session(&b).unwrap().unwrap().last_seq, 1);
    assert_eq!(j.read(&b, 0, 9).unwrap().len(), 1);
    j.append(&b, &[run(2)], Durability::Stream).unwrap();
}

#[test]
fn ending_an_item_folds_its_deltas() {
    let mut j = Journal::in_memory().unwrap();
    let s = SessionId::new();
    j.create(&s, "T", None, 1).unwrap();
    let end = ev(
        6,
        Body::ItemEnded {
            item: ItemId("i".into()),
            outcome: Outcome::Completed,
            text: "hello".into(),
            result: None,
        },
    );
    j.append(
        &s,
        &[
            started(2, "i"),
            delta(3, "i", "hel"),
            started(4, "k"),
            delta(5, "i", "lo"),
            end.clone(),
        ],
        Durability::Stream,
    )
    .unwrap();
    let seqs: Vec<_> = j.read(&s, 0, 99).unwrap().iter().map(|e| e.seq).collect();
    assert_eq!(
        seqs,
        [1, 2, 4, 6],
        "order is kept; folded deltas leave holes a reader skips"
    );
}

#[test]
fn recovery_ends_what_was_open_and_keeps_the_text() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let s = SessionId::new();
    {
        let mut j = Journal::open(&p).unwrap();
        j.create(&s, "T", None, 1).unwrap();
        let ask = ev(
            6,
            Body::RequestOpened {
                request: RequestId("q".into()),
                agent: AgentId("a".into()),
                what: "w".into(),
                subject: "s".into(),
                scope: "c".into(),
                answerable: Answerable::Live,
            },
        );
        j.append(
            &s,
            &[
                run(2),
                started(3, "i"),
                delta(4, "i", "par"),
                delta(5, "i", "tial"),
                ask,
            ],
            Durability::Stream,
        )
        .unwrap();
        // no ending: the host is killed here
    }
    let mut j = Journal::open(&p).unwrap();
    assert_eq!(j.recover(9).unwrap(), 1);
    let tail: Vec<_> = j
        .read(&s, 6, 99)
        .unwrap()
        .into_iter()
        .map(|e| e.body)
        .collect();
    assert_eq!(
        tail,
        [
            Body::ItemEnded {
                item: ItemId("i".into()),
                outcome: Outcome::Interrupted,
                text: "partial".into(),
                result: None
            },
            Body::RequestClosed {
                request: RequestId("q".into()),
                answer: "expired".into()
            },
            Body::RunEnded {
                run: RunId("r".into()),
                outcome: Outcome::Interrupted,
                error: None
            },
        ]
    );
    assert!(!j.session(&s).unwrap().unwrap().running);
    assert_eq!(j.recover(10).unwrap(), 0, "recovery is idempotent");
}

#[test]
fn a_newer_schema_is_refused_and_left_alone() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    drop(Journal::open(&p).unwrap());
    let c = rusqlite::Connection::open(&p).unwrap();
    c.pragma_update(None, "user_version", 99).unwrap();
    drop(c);
    assert!(matches!(Journal::open(&p), Err(Error::TooNew(99))));
}

#[test]
fn a_damaged_file_is_an_error_not_a_fresh_journal() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    std::fs::write(&p, vec![7u8; 9000]).unwrap();
    assert!(Journal::open(&p).is_err());
    assert_eq!(
        std::fs::read(&p).unwrap().len(),
        9000,
        "the file is untouched"
    );
}

#[test]
fn sessions_do_not_see_each_other() {
    let mut j = Journal::in_memory().unwrap();
    let (a, b) = (SessionId::new(), SessionId::new());
    j.create(&a, "A", None, 1).unwrap();
    j.create(&b, "B", None, 2).unwrap();
    j.append(
        &a,
        &[started(2, "i"), delta(3, "i", "x")],
        Durability::Stream,
    )
    .unwrap();
    assert_eq!(j.read(&b, 0, 9).unwrap().len(), 1);
    assert!(j.delete(&a).unwrap());
    assert!(j.read(&a, 0, 9).unwrap().is_empty());
    assert_eq!(j.sessions().unwrap().len(), 1);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]
    /// However the events are cut into commits, the journal reads the same.
    #[test]
    fn commits_do_not_matter(cuts in proptest::collection::vec(1usize..6, 1..40), ends in any::<bool>()) {
        let mut events = vec![run(2), started(3, "i")];
        for n in 0..30u64 { events.push(delta(4 + n, "i", &format!("t{n}"))); }
        if ends {
            events.push(ev(34, Body::ItemEnded { item: ItemId("i".into()), outcome: Outcome::Completed, text: "all".into(), result: None }));
        }
        let s = SessionId("s".into());
        let mut whole = Journal::in_memory().unwrap();
        whole.create(&s, "T", None, 1).unwrap();
        whole.append(&s, &events, Durability::Stream).unwrap();
        let mut cut = Journal::in_memory().unwrap();
        cut.create(&s, "T", None, 1).unwrap();
        let (mut at, mut k) = (0, 0);
        while at < events.len() {
            let n = cuts[k % cuts.len()].min(events.len() - at);
            cut.append(&s, &events[at..at + n], Durability::Stream).unwrap();
            at += n; k += 1;
        }
        prop_assert_eq!(whole.read(&s, 0, 999).unwrap(), cut.read(&s, 0, 999).unwrap());
    }
}
