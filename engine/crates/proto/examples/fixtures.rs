//! Writes docs/protocol/fixtures.jsonl: one of every message and event body. The Dart tests decode
//! it, so a change here that the client does not know fails there.
// What it prints is what it is for.
#![allow(clippy::print_stdout)]

use axiom_proto::*;
use serde_json::json;

fn main() -> Result<(), serde_json::Error> {
    let (a, r, i, q) =
        (AgentId("a_1".into()), RunId("r_1".into()), ItemId("i_1".into()), RequestId("q_1".into()));
    let bodies = vec![
        Body::SessionCreated { title: "T".into(), workspace: Some("/w".into()) },
        Body::SessionTitled { title: "U".into() },
        Body::AgentAdded {
            agent: a.clone(),
            name: "Lead".into(),
            execution: Box::new(Execution {
                harness: "axiom".into(),
                provider: Some("anthropic".into()),
                model: Some("m".into()),
                connection: Some("c_2".into()),
                effort: Some("high".into()),
                environment: Some("e".into()),
                speed: Some("fast".into()),
            }),
            parent: Some(AgentId("a_0".into())),
            access: Some(Access::Ask),
            apart: true,
            from: Some(Box::new(Branched {
                agent: AgentId("a_0".into()),
                item: ItemId("i_0".into()),
                with: true,
            })),
            plan: true,
        },
        Body::AgentChanged {
            agent: a.clone(),
            execution: Box::new(Execution {
                harness: "axiom".into(),
                provider: None,
                model: Some("n".into()),
                connection: None,
                effort: None,
                environment: None,
                ..Execution::default()
            }),
            access: Some(Access::Work),
            plan: true,
        },
        Body::AgentState { agent: a.clone(), state: AgentState::Asking },
        Body::RunStarted { run: r.clone(), agent: a.clone() },
        Body::RunEnded { run: r.clone(), outcome: Outcome::Failed, error: Some("why".into()) },
        Body::ItemStarted {
            item: i.clone(),
            run: r,
            agent: a.clone(),
            kind: ItemKind::Tool,
            name: "read".into(),
            args: Some(json!({"path": "x"})),
            args_cut: 0,
        },
        Body::ItemDelta { item: i.clone(), text: "tö\n\"k\"".into() },
        Body::ItemEnded {
            item: i,
            outcome: Outcome::Interrupted,
            text: "t".into(),
            result: Some(json!({"lines": 3})),
            result_cut: 0,
        },
        Body::RequestOpened {
            request: q.clone(),
            agent: a.clone(),
            what: "w".into(),
            subject: "s".into(),
            scope: "c".into(),
            answerable: Answerable::Live,
            digest: "d".into(),
            item: Some(ItemId("i_1".into())),
            session: true,
        },
        Body::RequestClosed { request: q, answer: "once".into() },
        Body::Usage {
            agent: a,
            usage: Usage {
                input: Some(1),
                cache_read: Some(2),
                cache_write: Some(3),
                output: Some(4),
                reasoning: Some(5),
                context: Some(0.5),
                prompt: Some(6),
                cache_write_long: Some(7),
            },
            provenance: Provenance::Estimated,
            of: Covers::Turn,
            run: Some(RunId("r_1".into())),
            on: Some(Box::new(UsedOn {
                harness: "claude".into(),
                lane: Lane::Program,
                provider: Some("anthropic".into()),
                connection: Some("c_2".into()),
                model: Some("m".into()),
                answered: Some("m-1".into()),
                speed: Some("fast".into()),
            })),
            cost: Some(Box::new(Cost {
                amount: 0.25,
                currency: "USD".into(),
                provenance: Provenance::Estimated,
                of: Covers::Session,
            })),
            totals: vec![ModelTotal {
                model: Some("m-1".into()),
                usage: Usage { output: Some(8), ..Usage::default() },
                cost: None,
            }],
            cut: true,
            aside: true,
        },
        Body::Todo {
            agent: AgentId("a_1".into()),
            run: Some(RunId("r_1".into())),
            steps: vec![
                Todo { text: "Look".into(), state: TodoState::Done },
                Todo { text: "Do".into(), state: TodoState::Doing },
                Todo { text: "Say".into(), state: TodoState::Todo },
            ],
            text: Some("# P".into()),
        },
    ];
    let cut = vec![
        Event {
            seq: 13,
            at: 1013,
            body: Body::ItemStarted {
                item: ItemId("i_2".into()),
                run: RunId("r_1".into()),
                agent: AgentId("a_1".into()),
                kind: ItemKind::Tool,
                name: "write".into(),
                args: None,
                args_cut: 300_000,
            },
        },
        Event {
            seq: 14,
            at: 1014,
            body: Body::ItemEnded {
                item: ItemId("i_2".into()),
                outcome: Outcome::Completed,
                text: String::new(),
                result: None,
                result_cut: 400_000,
            },
        },
    ];
    let session = SessionId("s_1".into());
    let events: Vec<_> = bodies
        .into_iter()
        .zip(1..)
        .map(|(body, seq)| Event { seq, at: 1000 + seq, body })
        .collect();
    let types: Vec<_> = KNOWN.to_vec();
    println!("{}", json!({"protocol": PROTOCOL, "types": types}));
    for m in [
        Message::Events { session: session.clone(), events },
        Message::Resync { session: session.clone(), resync: 9 },
        Message::Ok {
            id: 1,
            result: serde_json::to_value([SessionInfo {
                id: session,
                title: "T".into(),
                workspace: None,
                created: 1,
                updated: 2,
                last_seq: 3,
                running: true,
            }])?,
        },
        Message::Err { id: 2, error: WireError::new(ErrorCode::NotFound, "no") },
        Message::Fault { session: SessionId("s_1".into()), fault: "cannot be read".into() },
        Message::Ok {
            id: 3,
            result: json!({
                "version": "0", "pid": 4, "clients": 1, "runs": 0, "damaged": "why",
                "faults": [Fault { session: SessionId("s_1".into()), words: "w".into() }],
            }),
        },
        Message::Events { session: SessionId("s_1".into()), events: cut },
        Message::Ok {
            id: 4,
            result: serde_json::to_value([
                Limit {
                    of: "A run".into(),
                    what: "Steps".into(),
                    most: Some(200),
                    unit: Unit::Count,
                },
                Limit {
                    of: "A run".into(),
                    what: "Output tokens".into(),
                    most: None,
                    unit: Unit::Count,
                },
                Limit {
                    of: "Files".into(),
                    what: "One file".into(),
                    most: Some(1024),
                    unit: Unit::Bytes,
                },
                Limit {
                    of: "Files".into(),
                    what: "One command".into(),
                    most: Some(600),
                    unit: Unit::Seconds,
                },
            ])?,
        },
        Message::Ok {
            id: 7,
            result: serde_json::to_value([ModelList {
                harness: "axiom".into(),
                connection: Some("c_2".into()),
                at: 9,
                models: vec![ModelInfo {
                    id: "m".into(),
                    name: Some("M".into()),
                    context: Some(1000),
                    effort: true,
                    efforts: vec!["low".into(), "max".into()],
                    adaptive: true,
                    speeds: vec!["fast".into()],
                }],
            }])?,
        },
        Message::Ok {
            id: 8,
            result: serde_json::to_value([ProgramInfo {
                harness: "claude".into(),
                account: Some("c_2".into()),
                name: Some("N".into()),
                path: Some("/p".into()),
                args: vec!["a".into()],
                vars: vec![
                    VarInfo { name: "A".into(), value: Some("1".into()), lasts: true },
                    VarInfo { name: "B".into(), value: None, lasts: false },
                ],
            }])?,
        },
        Message::Open { open: "/w".into() },
    ] {
        println!("{}", serde_json::to_string(&m)?);
    }
    Ok(())
}
