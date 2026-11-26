// A test's helpers fail the test.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use axiom_journal::{Error, Journal, Reader};
use axiom_proto::*;
use proptest::prelude::*;

const ALL: usize = usize::MAX;

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
            args_cut: 0,
        },
    )
}
fn delta(seq: u64, i: &str, t: &str) -> Event {
    ev(seq, Body::ItemDelta { item: ItemId(i.into()), text: t.into() })
}
fn run(seq: u64) -> Event {
    ev(seq, Body::RunStarted { run: RunId("r".into()), agent: AgentId("a".into()) })
}

#[test]
fn what_is_written_is_read_back() {
    let mut j = Journal::in_memory().unwrap();
    let s = SessionId::new();
    let first = j.create(&s, "T", Some("/w"), 5).unwrap();
    let more = [run(2), started(3, "i"), delta(4, "i", "he"), delta(5, "i", "llo")];
    j.append(&s, &more).unwrap();
    let mut all = vec![first];
    all.extend(more);
    assert_eq!(j.read(&s, 0, ALL).unwrap(), all);
    assert_eq!(j.read(&s, 3, 1).unwrap(), all[3..4]);
    let i = j.session(&s).unwrap().unwrap();
    assert!(i.running);
    assert_eq!((i.last_seq, i.updated, i.workspace.as_deref()), (5, 5, Some("/w")));
}

#[test]
fn a_gap_or_a_repeat_is_refused_whole() {
    let mut j = Journal::in_memory().unwrap();
    let s = SessionId::new();
    j.create(&s, "T", None, 1).unwrap();
    for bad in [vec![run(3)], vec![run(1)], vec![run(2), run(2)], vec![run(2), run(4)]] {
        assert!(matches!(j.append(&s, &bad), Err(Error::Damaged(_))));
    }
    assert_eq!(j.session(&s).unwrap().unwrap().last_seq, 1);
    assert_eq!(j.read(&s, 0, ALL).unwrap().len(), 1);
}

#[test]
fn a_failed_append_in_a_group_undoes_only_itself() {
    let mut j = Journal::in_memory().unwrap();
    let (a, b) = (SessionId::new(), SessionId::new());
    j.create(&a, "A", None, 1).unwrap();
    j.create(&b, "B", None, 1).unwrap();
    let none = SessionId::new();
    let r = j
        .append_group(&[
            (&a, &[run(2)]),
            (&b, &[run(2), run(9)]),
            (&none, &[run(1)]),
            (&a, &[started(3, "i")]),
        ])
        .unwrap();
    assert_eq!(r.iter().map(Result::is_ok).collect::<Vec<_>>(), [true, false, false, true]);
    assert_eq!(j.session(&a).unwrap().unwrap().last_seq, 3);
    assert_eq!(j.session(&b).unwrap().unwrap().last_seq, 1);
    assert_eq!(j.read(&b, 0, ALL).unwrap().len(), 1);
    assert!(!j.session(&b).unwrap().unwrap().running, "what was opened is undone too");
    j.append(&b, &[run(2)]).unwrap();
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
            result_cut: 0,
        },
    );
    j.append(
        &s,
        &[started(2, "i"), delta(3, "i", "hel"), started(4, "k"), delta(5, "i", "lo"), end.clone()],
    )
    .unwrap();
    let seqs: Vec<_> = j.read(&s, 0, ALL).unwrap().iter().map(|e| e.seq).collect();
    assert_eq!(seqs, [1, 2, 4, 6], "order is kept; folded deltas leave holes a reader skips");
}

fn asks(seq: u64, q: &str) -> Event {
    ev(
        seq,
        Body::RequestOpened {
            request: RequestId(q.into()),
            agent: AgentId("a".into()),
            what: "w".into(),
            subject: "s".into(),
            scope: "c".into(),
            answerable: Answerable::Live,
            digest: String::new(),
            item: None,
            session: false,
        },
    )
}
fn state(seq: u64, a: &str, state: AgentState) -> Event {
    ev(seq, Body::AgentState { agent: AgentId(a.into()), state })
}
fn cut_short(i: &str, text: &str) -> Body {
    Body::ItemEnded {
        item: ItemId(i.into()),
        outcome: Outcome::Interrupted,
        text: text.into(),
        result: None,
        result_cut: 0,
    }
}

#[test]
fn recovery_ends_what_was_open_and_keeps_the_text() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let (s, done) = (SessionId::new(), SessionId::new());
    {
        let mut j = Journal::open(&p).unwrap();
        j.create(&s, "T", None, 1).unwrap();
        let ended = ev(
            9,
            Body::ItemEnded {
                item: ItemId("b".into()),
                outcome: Outcome::Completed,
                text: "b".into(),
                result: None,
                result_cut: 0,
            },
        );
        j.append(
            &s,
            &[
                run(2),
                state(3, "a", AgentState::Running),
                state(4, "paused", AgentState::Running),
                // Items end in the order in which they began, whatever their names are.
                started(5, "z"),
                delta(6, "z", "par"),
                started(7, "b"),
                started(8, "c"),
                ended,
                delta(10, "z", "tial"),
                delta(11, "c", "more"),
                asks(12, "q"),
                state(13, "paused", AgentState::Paused),
                state(14, "asks", AgentState::Asking),
            ],
        )
        .unwrap();
        j.create(&done, "D", None, 1).unwrap();
        // no ending: the host is killed here
    }
    let mut j = Journal::open(&p).unwrap();
    assert!(j.session(&s).unwrap().unwrap().running);
    let (n, failed) = j.recover(9).unwrap();
    assert_eq!((n, failed.len()), (1, 0));
    let tail: Vec<_> = j.read(&s, 14, ALL).unwrap().into_iter().map(|e| e.body).collect();
    let paused = |a: &str| Body::AgentState { agent: AgentId(a.into()), state: AgentState::Paused };
    assert_eq!(
        tail,
        [
            cut_short("z", "partial"),
            cut_short("c", "more"),
            Body::RequestClosed { request: RequestId("q".into()), answer: "expired".into() },
            Body::RunEnded { run: RunId("r".into()), outcome: Outcome::Interrupted, error: None },
            paused("a"),
            paused("asks"),
        ]
    );
    assert!(!j.session(&s).unwrap().unwrap().running);
    assert_eq!(j.recover(10).unwrap().0, 0, "recovery is idempotent");
    let r = Reader::open(&p).unwrap();
    assert_eq!(r.ending(&s, 10).unwrap(), Some((20, vec![])));
    assert_eq!(r.ending(&done, 10).unwrap(), Some((1, vec![])));
    assert_eq!(r.ending(&SessionId::new(), 10).unwrap(), None);
}

/// Damages one event behind the journal's back.
fn damage(p: &std::path::Path, s: &SessionId, seq: i64, body: &str) {
    let c = rusqlite::Connection::open(p).unwrap();
    let n = c
        .execute(
            "update event set body=? where session=? and seq=?",
            rusqlite::params![body, s.0, seq],
        )
        .unwrap();
    assert_eq!(n, 1);
}

#[test]
fn a_damaged_event_costs_what_needs_it_and_nothing_else() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let ids: Vec<_> = ["a", "b", "c", "d"].into_iter().map(|s| SessionId(s.into())).collect();
    {
        let mut j = Journal::open(&p).unwrap();
        for s in &ids {
            j.create(s, "T", None, 1).unwrap();
            let texts = ["x", "y", "z", "!"];
            let deltas = texts.iter().zip(4..).map(|(t, n)| delta(n, "i", t));
            let events: Vec<_> = [run(2), started(3, "i")].into_iter().chain(deltas).collect();
            j.append(s, &events).unwrap();
        }
    }
    // Old history of a; unfinished text of b; the list of what is open in d. None of it keeps the
    // others from opening, and text that can be read is not lost with text that cannot.
    damage(&p, &ids[0], 2, r#"{"type":"run.started"}"#);
    damage(&p, &ids[1], 5, "{");
    damage(&p, &ids[1], 6, "[]");
    let c = rusqlite::Connection::open(&p).unwrap();
    c.execute("update open set kind=9 where session='d'", []).unwrap();
    let mut j = Journal::open(&p).unwrap();
    let (n, failed) = j.recover(9).unwrap();
    assert_eq!(n, 3, "recovery reads what is open, not the history");
    assert_eq!(failed.iter().map(|(s, _)| s).collect::<Vec<_>>(), [&ids[3]]);
    assert_eq!(j.read(&ids[2], 7, ALL).unwrap()[0].body, cut_short("i", "xyz!"));
    let lost = "x\n[a part of this is damaged and could not be read]\n!";
    assert_eq!(j.read(&ids[1], 7, ALL).unwrap()[0].body, cut_short("i", lost));
    let whole: Vec<_> = j.read(&ids[1], 0, ALL).unwrap().iter().map(|e| e.seq).collect();
    assert_eq!(whole, [1, 2, 3, 8, 9], "the damaged part has gone with the others");
    // What lies before the damage is readable; then it is an error, every time.
    let r = Reader::open(&p).unwrap();
    for _ in 0..2 {
        let page = r.read(&ids[0], 0, ALL).unwrap();
        assert_eq!(page.iter().map(|e| e.seq).collect::<Vec<_>>(), [1]);
        assert!(matches!(r.read(&ids[0], 1, ALL), Err(Error::Damaged(_))));
    }
    assert!(!r.read(&ids[0], 2, ALL).unwrap().is_empty());
}

#[test]
fn a_page_is_bounded_by_bytes_and_never_empty() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let mut j = Journal::open(&p).unwrap();
    let s = SessionId::new();
    j.create(&s, "T", None, 1).unwrap();
    let text = "x".repeat(1000);
    let events: Vec<_> = (2..102).map(|n| delta(n, "i", &text)).collect();
    j.append(&s, &events).unwrap();
    let r = Reader::open(&p).unwrap();
    let (mut after, mut seen) = (0, 0);
    loop {
        let page = r.read(&s, after, 5000).unwrap();
        let Some(last) = page.last() else { break };
        after = last.seq;
        seen += page.len();
        let size: usize = page.iter().map(axiom_proto::limits::encoded_len).sum();
        assert!(size <= 5000, "{size}");
    }
    assert_eq!(seen, 101);
    assert_eq!(r.read(&s, 5, 1).unwrap().len(), 1, "one event is a page, however long");
}

#[test]
fn a_reader_sees_every_commit_and_writes_nothing() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let mut j = Journal::open(&p).unwrap();
    let r = Reader::open(&p).unwrap();
    let mut ids = Vec::new();
    for n in 0..5 {
        let s = SessionId::new();
        j.create(&s, &format!("T{n}"), None, n).unwrap();
        assert_eq!(r.read(&s, 0, ALL).unwrap().len(), 1);
        ids.insert(0, s);
    }
    let list = |limit, offset| -> Vec<_> {
        r.sessions(limit, offset).unwrap().into_iter().map(|s| s.id).collect()
    };
    assert_eq!(list(9, 0), ids, "newest first");
    assert_eq!([list(2, 0), list(2, 2), list(2, 4), list(2, 6)].concat(), ids);
    assert_eq!(list(0, 0), []);
    r.check().unwrap();
}

#[test]
fn schema_1_is_brought_forward() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let c = rusqlite::Connection::open(&p).unwrap();
    c.execute_batch(
        r#"pragma journal_mode=WAL;
        create table session(
          id text primary key, title text not null, workspace text,
          created integer not null, updated integer not null,
          last_seq integer not null, open_runs integer not null default 0);
        create table event(
          session text not null references session(id) on delete cascade,
          seq integer not null, at integer not null,
          delta_of text, body text not null,
          primary key(session, seq)) without rowid;
        create index event_delta on event(session, delta_of) where delta_of is not null;
        insert into session values('s','T',null,1,1,5,1), ('t','U','/w',1,1,3,0);
        insert into event values
          ('s',1,1,null,'{"type":"session.created","title":"T"}'),
          ('s',2,1,null,'{"type":"run.started","run":"r","agent":"a"}'),
          ('s',3,1,null,'{"type":"agent.state","agent":"a","state":"running"}'),
          ('s',4,1,null,'{"type":"item.started","item":"i","run":"r","agent":"a","kind":"prose"}'),
          ('s',5,1,'i','{"type":"item.delta","item":"i","text":"half"}'),
          ('t',1,1,null,'{"type":"session.created","title":"U"}'),
          ('t',2,1,null,'{"type":"run.started","run":"k","agent":"a"}'),
          ('t',3,1,null,'{"type":"run.ended","run":"k","outcome":"completed"}');
        pragma user_version=1;"#,
    )
    .unwrap();
    drop(c);
    for _ in 0..2 {
        let j = Journal::open(&p).unwrap();
        let running: Vec<_> = j.sessions(9, 0).unwrap().iter().map(|s| s.running).collect();
        assert_eq!(running, [false, true]);
    }
    let mut j = Journal::open(&p).unwrap();
    let s = SessionId("s".into());
    assert_eq!(j.recover(9).unwrap().0, 1);
    let tail: Vec<_> = j.read(&s, 5, ALL).unwrap().into_iter().map(|e| e.body).collect();
    assert_eq!(tail[0], cut_short("i", "half"));
    assert!(matches!(tail[1], Body::RunEnded { .. }));
    assert!(matches!(tail[2], Body::AgentState { state: AgentState::Paused, .. }));
}

#[cfg(unix)]
#[test]
fn the_files_are_private_even_if_they_were_not() {
    use std::os::unix::fs::PermissionsExt;
    let d = tempfile::tempdir().unwrap();
    let mode = |n: &str| std::fs::metadata(d.path().join(n)).unwrap().permissions().mode() & 0o777;
    let p = d.path().join("j.db");
    let j = Journal::open(&p).unwrap();
    for n in ["j.db", "j.db-wal", "j.db-shm"] {
        assert_eq!(mode(n), 0o600, "{n}");
        let open = std::fs::Permissions::from_mode(0o644);
        std::fs::set_permissions(d.path().join(n), open).unwrap();
    }
    let again = Journal::open(&p).unwrap();
    for n in ["j.db", "j.db-wal", "j.db-shm"] {
        assert_eq!(mode(n), 0o600, "{n}");
    }
    drop((j, again));
}

#[test]
fn old_rows_that_nest_too_deep_are_cut_though_they_are_short() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let s = SessionId::new();
    let mut j = Journal::open(&p).unwrap();
    j.create(&s, "T", None, 1).unwrap();
    for n in [60, 100, 120] {
        let args = format!("{}{}", "[".repeat(n), "]".repeat(n));
        let body = format!(
            r#"{{"type":"item.started","item":"i","run":"r","agent":"a","kind":"tool","args":{args}}}"#
        );
        damage(&p, &s, 1, &body);
        let event = j.read(&s, 0, ALL).unwrap().remove(0);
        let Body::ItemStarted { args, args_cut, .. } = &event.body else { panic!() };
        assert_eq!(
            (args.is_none(), *args_cut),
            if n > 64 { (true, 2 * n as u64) } else { (false, 0) }
        );
        // As it goes over the wire, a client can read it.
        let m = Message::Events { session: s.clone(), events: vec![event] };
        serde_json::from_str::<Message>(&serde_json::to_string(&m).unwrap()).unwrap();
    }
}

#[test]
fn old_rows_over_the_limits_are_cut_when_read() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let s = SessionId::new();
    let mut j = Journal::open(&p).unwrap();
    j.create(&s, "T", None, 1).unwrap();
    let long = serde_json::to_string(&Body::ItemEnded {
        item: ItemId("i".into()),
        outcome: Outcome::Completed,
        text: "\0".repeat(3 << 20),
        result: None,
        result_cut: 0,
    });
    damage(&p, &s, 1, &long.unwrap());
    let c = rusqlite::Connection::open(&p).unwrap();
    c.execute("update session set title=?1, workspace=?1", ["/".repeat(5 << 20)]).unwrap();
    let e = &j.read(&s, 0, ALL).unwrap()[0];
    assert!(axiom_proto::limits::encoded_len(e) < MAX_LINE);
    assert!(axiom_proto::limits::encoded_len(&j.sessions(9, 0).unwrap()) < 8000);
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
    assert_eq!(std::fs::read(&p).unwrap().len(), 9000, "the file is untouched");
}

#[test]
fn a_damaged_page_is_found_by_the_check() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let s = SessionId::new();
    let mut j = Journal::open(&p).unwrap();
    j.create(&s, "T", None, 1).unwrap();
    let events: Vec<_> = (2..200).map(|n| delta(n, "i", &"x".repeat(500))).collect();
    j.append(&s, &events).unwrap();
    drop(j); // the last connection folds the WAL into the file
    Reader::open(&p).unwrap().check().unwrap();
    let mut bytes = std::fs::read(&p).unwrap();
    let middle = bytes.len() / 2 / 4096 * 4096;
    bytes[middle..middle + 4096].fill(7);
    std::fs::write(&p, bytes).unwrap();
    assert!(Reader::open(&p).unwrap().check().is_err());
}

#[test]
fn sessions_do_not_see_each_other() {
    let mut j = Journal::in_memory().unwrap();
    let (a, b) = (SessionId::new(), SessionId::new());
    j.create(&a, "A", None, 1).unwrap();
    j.create(&b, "B", None, 2).unwrap();
    j.append(&a, &[started(2, "i"), delta(3, "i", "x")]).unwrap();
    assert_eq!(j.read(&b, 0, ALL).unwrap().len(), 1);
    assert!(j.delete(&a).unwrap());
    assert!(j.read(&a, 0, ALL).unwrap().is_empty());
    assert_eq!(j.sessions(9, 0).unwrap().len(), 1);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(40))]
    /// However the events are cut into commits, the journal reads the same.
    #[test]
    fn commits_do_not_matter(cuts in proptest::collection::vec(1usize..6, 1..40), ends in any::<bool>()) {
        let mut events = vec![run(2), started(3, "i")];
        for n in 0..30u64 { events.push(delta(4 + n, "i", &format!("t{n}"))); }
        if ends {
            events.push(ev(34, Body::ItemEnded { item: ItemId("i".into()), outcome: Outcome::Completed, text: "all".into(), result: None, result_cut: 0 }));
        }
        let s = SessionId("s".into());
        let mut whole = Journal::in_memory().unwrap();
        whole.create(&s, "T", None, 1).unwrap();
        whole.append(&s, &events).unwrap();
        let mut cut = Journal::in_memory().unwrap();
        cut.create(&s, "T", None, 1).unwrap();
        let (mut at, mut k) = (0, 0);
        while at < events.len() {
            let n = cuts[k % cuts.len()].min(events.len() - at);
            cut.append(&s, &events[at..at + n]).unwrap();
            at += n; k += 1;
        }
        prop_assert_eq!(whole.read(&s, 0, ALL).unwrap(), cut.read(&s, 0, ALL).unwrap());
    }
}

#[test]
fn what_the_host_needs_is_kept_beside_the_events() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j");
    let mut j = Journal::open(&p).unwrap();
    let (s, t) = (SessionId::new(), SessionId::new());
    for x in [&s, &t] {
        j.create(x, "t", None, 1).unwrap();
    }
    let (a, b) = (AgentId("a".into()), AgentId("b".into()));
    j.put_agent(&s, &a, "one").unwrap();
    j.put_agent(&s, &a, "two").unwrap();
    j.put_agent(&s, &b, "b").unwrap();
    for (n, m) in ["m0", "m1"].iter().enumerate() {
        j.put_message(&s, &a, n as u64, m, None).unwrap();
    }
    j.put_message(&s, &b, 0, "other", None).unwrap();
    for (n, why) in [(1, "a repeat"), (3, "a gap")] {
        assert!(j.put_message(&s, &a, n, "x", None).is_err(), "{why}");
    }
    assert!(j.put_message(&SessionId::new(), &a, 0, "x", None).is_err(), "no such session");
    assert_eq!(j.last_audit().unwrap(), None);
    j.put_audit(1, "e1").unwrap();
    j.put_audit(2, "e2").unwrap();
    for n in [0, 2, 4] {
        assert!(j.put_audit(n, "x").is_err());
    }
    drop(j);
    let mut j = Journal::open(&p).unwrap();
    let r = Reader::open(&p).unwrap();
    assert_eq!(r.agents(&s).unwrap(), [(a.clone(), "two".into()), (b.clone(), "b".into())]);
    assert_eq!(r.messages(&s, &a).unwrap(), ["m0", "m1"]);
    assert_eq!(r.messages(&s, &b).unwrap(), ["other"]);
    assert!(r.messages(&t, &a).unwrap().is_empty() && r.agents(&t).unwrap().is_empty());
    assert_eq!(r.audit(0, 10).unwrap(), ["e1", "e2"]);
    assert_eq!(r.audit(1, 10).unwrap(), ["e2"]);
    assert_eq!(j.last_audit().unwrap().as_deref(), Some("e2"));
    // A message taken out from behind.
    rusqlite::Connection::open(&p)
        .unwrap()
        .execute("delete from message where n=0 and agent='a'", [])
        .unwrap();
    assert!(r.messages(&s, &a).is_err());
    j.delete(&s).unwrap();
    assert!(r.messages(&s, &b).unwrap().is_empty() && r.agents(&s).unwrap().is_empty());
    assert_eq!(r.audit(0, 10).unwrap().len(), 2, "what was done stays on record");
}

#[test]
fn schema_2_is_brought_forward() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let c = rusqlite::Connection::open(&p).unwrap();
    c.execute_batch(
        r#"pragma journal_mode=WAL;
        create table session(
          id text primary key, title text not null, workspace text,
          created integer not null, updated integer not null, last_seq integer not null);
        create table event(
          session text not null references session(id) on delete cascade,
          seq integer not null, at integer not null, delta_of text, body text not null,
          primary key(session, seq)) without rowid;
        create index event_delta on event(session, delta_of) where delta_of is not null;
        create table open(
          session text not null references session(id) on delete cascade,
          kind integer not null, id text not null, seq integer not null,
          primary key(session, kind, id)) without rowid;
        insert into session values('s','T',null,1,1,2);
        insert into event values
          ('s',1,1,null,'{"type":"session.created","title":"T"}'),
          ('s',2,1,null,'{"type":"run.started","run":"r","agent":"a"}');
        insert into open values('s',2,'r',2);
        pragma user_version=2;"#,
    )
    .unwrap();
    drop(c);
    let s = SessionId("s".into());
    for n in 0..2 {
        let mut j = Journal::open(&p).unwrap();
        assert_eq!(j.read(&s, 0, ALL).unwrap().len(), 2);
        assert!(j.session(&s).unwrap().unwrap().running);
        j.put_message(&s, &AgentId("a".into()), n, "m", None).unwrap();
    }
    assert_eq!(Journal::open(&p).unwrap().recover(9).unwrap().0, 1);
}

/// From 4, in which there were no places to branch at.
#[test]
fn schema_4_is_brought_forward() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let s = SessionId("s".into());
    let a = AgentId("a".into());
    {
        let mut j = Journal::open(&p).unwrap();
        j.create(&s, "t", None, 1).unwrap();
        j.put_message(&s, &a, 0, "m", None).unwrap();
        j.put_owed(&s, &a, &RunId("r".into()), &a).unwrap();
    }
    let c = rusqlite::Connection::open(&p).unwrap();
    c.execute_batch("drop table cut; pragma user_version=4;").unwrap();
    drop(c);
    for _ in 0..2 {
        let mut j = Journal::open(&p).unwrap();
        j.put_cut(&s, &a, &ItemId("i".into()), 1, true).unwrap();
        let r = Reader::open(&p).unwrap();
        assert_eq!(r.messages(&s, &a).unwrap(), ["m"]);
        assert_eq!(r.owed(&s, &a).unwrap().len(), 1);
        assert_eq!(r.cut(&s, &a, &ItemId("i".into())).unwrap(), Some((1, true)));
    }
}

#[test]
fn a_branch_has_the_first_turns_and_the_places_in_them() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j");
    let mut j = Journal::open(&p).unwrap();
    let (s, other) = (SessionId("s".into()), SessionId("z".into()));
    j.create(&s, "t", None, 1).unwrap();
    j.create(&other, "t", None, 1).unwrap();
    let (a, b, c) = (AgentId("a".into()), AgentId("b".into()), AgentId("c".into()));
    let i = |n: &str| ItemId(n.into());
    for (n, text) in ["p1", "a1", "p2", "a2"].into_iter().enumerate() {
        j.put_message(&s, &a, n as u64, text, None).unwrap();
        j.put_message(&other, &a, n as u64, "another's", None).unwrap();
    }
    j.put_cut(&s, &a, &i("p1"), 0, false).unwrap();
    j.put_cut(&s, &a, &i("a1"), 2, true).unwrap();
    j.put_cut(&s, &a, &i("p2"), 2, false).unwrap();
    j.put_cut(&s, &a, &i("a2"), 4, true).unwrap();
    j.put_cut(&other, &a, &i("x"), 0, true).unwrap();
    let r = Reader::open(&p).unwrap();
    assert_eq!((r.turns(&s, &a).unwrap(), r.turns(&s, &b).unwrap()), (4, 0));
    assert_eq!(r.cut(&s, &a, &i("a1")).unwrap(), Some((2, true)));
    assert_eq!(r.cut(&s, &a, &i("p2")).unwrap(), Some((2, false)));
    assert_eq!(r.cut(&s, &b, &i("a1")).unwrap(), None);
    assert_eq!(r.cut(&other, &a, &i("a1")).unwrap(), None);

    j.branch(&s, &a, 2, &b, "setup").unwrap();
    assert_eq!(r.messages(&s, &b).unwrap(), ["p1", "a1"]);
    assert_eq!(r.messages(&s, &a).unwrap().len(), 4);
    assert_eq!(r.agents(&s).unwrap(), [(b.clone(), "setup".into())]);
    let places = |x: &AgentId| ["p1", "a1", "p2", "a2", "x"].map(|n| r.cut(&s, x, &i(n)).unwrap());
    // What begins behind it is not in it.
    assert_eq!(places(&b), [Some((0, false)), Some((2, true)), None, None, None]);
    // It goes on as its own.
    assert!(j.put_message(&s, &b, 1, "x", None).is_err());
    j.put_message(&s, &b, 2, "other", None).unwrap();
    j.branch(&s, &b, 3, &c, "setup").unwrap();
    assert_eq!(r.messages(&s, &c).unwrap(), ["p1", "a1", "other"]);
    // Not over one that is there, nor more than there is: then nothing.
    assert!(j.branch(&s, &a, 2, &b, "again").is_err());
    let d = AgentId("d".into());
    assert!(j.branch(&s, &a, 5, &d, "setup").is_err());
    assert_eq!(r.messages(&s, &d).unwrap().len(), 0);
    assert_eq!(r.agents(&s).unwrap().len(), 2);
    // One nobody was told of leaves nothing.
    j.remove_agent(&s, &b).unwrap();
    assert!(r.messages(&s, &b).unwrap().is_empty() && places(&b).iter().all(Option::is_none));
    assert_eq!(r.messages(&s, &c).unwrap().len(), 3);
    assert_eq!(r.messages(&s, &a).unwrap().len(), 4);
}

#[test]
fn what_was_said_waits_until_it_is_heard() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j");
    let mut j = Journal::open(&p).unwrap();
    let s = SessionId::new();
    j.create(&s, "t", None, 1).unwrap();
    let (a, b) = (AgentId("a".into()), AgentId("b".into()));
    let i = |n: &str| ItemId(n.into());
    for (x, t) in [("i1", "one"), ("i2", "two"), ("i3", "three")] {
        j.put_pending(&s, &a, &i(x), t).unwrap();
    }
    j.put_pending(&s, &b, &i("k"), "other").unwrap();
    let r = Reader::open(&p).unwrap();
    let texts =
        |a: &AgentId| r.pending(&s, a).unwrap().into_iter().map(|(_, t)| t).collect::<Vec<_>>();
    assert_eq!(texts(&a), ["one", "two", "three"]);
    assert!(j.put_message(&s, &a, 0, "m", Some(&i("i2"))).unwrap());
    assert_eq!(texts(&a), ["one", "three"]);
    // Neither, if the message is refused.
    assert!(j.put_message(&s, &a, 5, "m", Some(&i("i1"))).is_err());
    assert_eq!(texts(&a), ["one", "three"]);
    j.put_pending(&s, &a, &i("i4"), "four").unwrap();
    assert_eq!(texts(&a), ["one", "three", "four"]);
    assert_eq!(texts(&b), ["other"]);
    assert_eq!(
        j.heard(&s, &a, &[i("i1"), i("nobody"), i("i4")]).unwrap(),
        [i("i1"), i("i4")],
        "those that waited"
    );
    assert_eq!((texts(&a), texts(&b)), (vec!["three".to_string()], vec!["other".to_string()]));
    j.put_agent(&s, &a, "x").unwrap();
    j.remove_agent(&s, &a).unwrap();
    assert!(r.agents(&s).unwrap().is_empty());
    j.delete(&s).unwrap();
    assert!(texts(&a).is_empty());
}

#[test]
fn what_waits_is_taken_back_or_heard_and_never_both() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j");
    let mut j = Journal::open(&p).unwrap();
    let s = SessionId::new();
    j.create(&s, "t", None, 1).unwrap();
    let (a, b) = (AgentId("a".into()), AgentId("b".into()));
    let i = |n: &str| ItemId(n.into());
    for (x, t) in [("i1", "one"), ("i2", "two"), ("i3", "three")] {
        j.put_pending(&s, &a, &i(x), t).unwrap();
    }
    assert_eq!(j.take_back(&s, &b, &i("i1")).unwrap(), None, "of another agent");
    assert_eq!(j.take_back(&SessionId::new(), &a, &i("i1")).unwrap(), None);
    assert_eq!(j.take_back(&s, &a, &i("i1")).unwrap().as_deref(), Some("one"));
    assert_eq!(j.take_back(&s, &a, &i("i1")).unwrap(), None, "once");
    // Taken back, it is not heard, by either way of hearing; and nothing is added in its place.
    assert!(!j.put_message(&s, &a, 0, "m", Some(&i("i1"))).unwrap());
    let r = Reader::open(&p).unwrap();
    assert!(r.messages(&s, &a).unwrap().is_empty());
    assert_eq!(j.heard(&s, &a, &[i("i1"), i("i2")]).unwrap(), [i("i2")]);
    // Heard, it is not taken back.
    assert_eq!(j.take_back(&s, &a, &i("i2")).unwrap(), None);
    assert!(j.put_message(&s, &a, 0, "m", Some(&i("i3"))).unwrap());
    assert_eq!(j.take_back(&s, &a, &i("i3")).unwrap(), None);
    assert!(r.pending(&s, &a).unwrap().is_empty());
}

#[test]
fn the_last_frames_are_kept() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("j.db");
    let mut j = Journal::open(&p).unwrap();
    let s = SessionId::new();
    j.create(&s, "t", None, 1).unwrap();
    let (a, b) = (AgentId("a".into()), AgentId("b".into()));
    let r = Reader::open(&p).unwrap();
    j.put_frames(&s, &b, &[(1, "other".into())]).unwrap();
    let most = axiom_journal::FRAMES;
    for n in 0..most + 10 {
        j.put_frames(&s, &a, &[(n, format!("{n}"))]).unwrap();
    }
    let all = r.frames(&s, &a).unwrap();
    assert_eq!(all.len() as u64, most);
    assert_eq!((all[0].as_str(), all.last().unwrap().clone()), ("10", format!("{}", most + 9)));
    let more: Vec<_> = (0..3).map(|n| (n, format!("x{n}"))).collect();
    j.put_frames(&s, &a, &more).unwrap();
    assert_eq!(r.frames(&s, &a).unwrap().last().unwrap(), "x2");
    assert_eq!(r.frames(&s, &a).unwrap().len() as u64, most);
    assert_eq!(r.frames(&s, &b).unwrap(), ["other"]);
    j.delete(&s).unwrap();
    assert!(r.frames(&s, &b).unwrap().is_empty());
}
