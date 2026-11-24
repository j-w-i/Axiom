//! Writes docs/protocol/fixtures.jsonl: one of every message and event body. The Dart tests decode
//! it, so a change here that the client does not know fails there.
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
            execution: Execution {
                harness: "axiom".into(),
                provider: Some("anthropic".into()),
                model: Some("m".into()),
                connection: Some("c_2".into()),
                effort: Some("high".into()),
                environment: Some("e".into()),
            },
            parent: Some(AgentId("a_0".into())),
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
        },
        Body::ItemDelta { item: i.clone(), text: "tö\n\"k\"".into() },
        Body::ItemEnded {
            item: i,
            outcome: Outcome::Interrupted,
            text: "t".into(),
            result: Some(json!({"lines": 3})),
        },
        Body::RequestOpened {
            request: q.clone(),
            agent: a.clone(),
            what: "w".into(),
            subject: "s".into(),
            scope: "c".into(),
            answerable: Answerable::Live,
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
            },
            provenance: Provenance::Estimated,
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
    ] {
        println!("{}", serde_json::to_string(&m)?);
    }
    Ok(())
}
