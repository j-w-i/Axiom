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
                    default_effort: Some("max".into()),
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
        Message::Ok {
            id: 9,
            result: serde_json::to_value(FileText {
                path: "a/b.png".into(),
                size: 8,
                digest: "d".into(),
                kind: "image".into(),
                text: Some("t".into()),
                image: Some("png".into()),
                bytes: Some("iVBORw0KGgo=".into()),
                whole: true,
            })?,
        },
        Message::Ok {
            id: 10,
            result: serde_json::to_value(FileList {
                path: "a".into(),
                entries: vec![
                    FileEntry { name: "b".into(), folder: true },
                    FileEntry { name: "c.rs".into(), folder: false },
                ],
                hidden: 2,
                whole: false,
            })?,
        },
        Message::Ok {
            id: 11,
            result: serde_json::to_value(Found {
                hits: vec![Hit { path: "a/c.rs".into(), line: 4, text: "fn x".into() }],
                files: 1,
                unread: 3,
                whole: false,
                ended: Some("too many files".into()),
            })?,
        },
        Message::Ok {
            id: 12,
            result: serde_json::to_value([JobInfo {
                session: SessionId("s_1".into()),
                agent: AgentId("a_1".into()),
                job: 2,
                command: "npm run dev".into(),
                age: 30,
                ended: Some("ended: exit 1".into()),
                last: "ready\n".into(),
                processes: vec![ProcessInfo {
                    pid: 41,
                    parent: 1,
                    name: "node".into(),
                    cpu_ms: 1500,
                    memory: 4096,
                    orphan: true,
                    ports: vec![5173],
                }],
            }])?,
        },
        Message::Ok {
            id: 13,
            result: serde_json::to_value([EditorInfo {
                id: "zed".into(),
                name: "Zed".into(),
                path: "/usr/local/bin/zed".into(),
            }])?,
        },
        Message::Ok {
            id: 14,
            result: serde_json::to_value([FolderInfo {
                id: "f_1".into(),
                path: "/w".into(),
                name: Some("Web".into()),
                pinned: true,
                execution: Some(Execution {
                    harness: "axiom".into(),
                    model: Some("m".into()),
                    connection: Some("c_2".into()),
                    ..Execution::default()
                }),
                access: Some(Access::Read),
                commands: vec![FolderCommand {
                    name: "Dev".into(),
                    command: "npm run dev".into(),
                    icon: Some("play".into()),
                    key: Some(1),
                }],
                moved: true,
            }])?,
        },
        Message::Ok {
            id: 15,
            result: serde_json::to_value([TerminalInfo {
                terminal: "t_1".into(),
                session: SessionId("s_1".into()),
                title: "vim".into(),
                busy: true,
                ended: Some("ended: exit 0".into()),
            }])?,
        },
        Message::Ok {
            id: 16,
            result: serde_json::to_value(Screen {
                terminal: "t_1".into(),
                version: 7,
                cols: 80,
                rows: vec![
                    vec![
                        Span { text: "$ ".into(), fg: None, bg: None, style: 0 },
                        Span {
                            text: "red".into(),
                            fg: Some("1".into()),
                            bg: Some("#102030".into()),
                            style: 17,
                        },
                    ],
                    vec![],
                ],
                cursor: Some((0, 5)),
                title: "T".into(),
                back: 3,
                most_back: 40,
                app_cursor: true,
                bracketed: true,
                ended: None,
            })?,
        },
        Message::Ok {
            id: 17,
            result: serde_json::to_value(Changes {
                repository: true,
                branch: Some("topic".into()),
                files: vec![Changed {
                    path: "a.rs".into(),
                    what: "changed".into(),
                    digest: Some("d1".into()),
                }],
                diff: "--- a.rs\n+++ a.rs\n".into(),
                whole: false,
                repositories: vec![String::new(), "api".into()],
                upstream: Some("origin/topic".into()),
                ahead: Some(2),
                behind: Some(1),
                against: Some("main".into()),
                default_branch: Some("main".into()),
                hooks: true,
                remote: Some("origin".into()),
                remote_url: Some("git@example.com:a/b.git".into()),
            })?,
        },
        Message::Ok {
            id: 18,
            result: serde_json::to_value(CommitShown {
                commit: CommitInfo {
                    id: "c1".into(),
                    author: "Ada".into(),
                    at: 1_700_000_000,
                    subject: "S".into(),
                },
                message: "S\n\nB".into(),
                diff: "x".into(),
                whole: true,
            })?,
        },
        Message::Ok {
            id: 19,
            result: serde_json::to_value(Committed {
                commit: CommitInfo {
                    id: "c2".into(),
                    author: "Ada".into(),
                    at: 1,
                    subject: "T".into(),
                },
                branch: Some("b".into()),
                hooks: true,
                unsigned: true,
                changed: vec!["a.rs".into()],
            })?,
        },
        Message::Ok {
            id: 20,
            result: serde_json::to_value(vec![BrowserInfo {
                session: SessionId("s_1".into()),
                agent: AgentId("a_1".into()),
                url: "http://127.0.0.1:5173/".into(),
                title: "App".into(),
                person: true,
            }])?,
        },
        Message::Ok {
            id: 21,
            result: serde_json::to_value(BrowserView {
                version: 7,
                frame: Some("/9j/".into()),
                width: 1280,
                height: 800,
                url: "http://127.0.0.1:5173/".into(),
                title: "App".into(),
                person: true,
                dark: true,
                loading: false,
                refused: vec!["https://example.com/a.png".into()],
                ended: None,
            })?,
        },
        Message::Ok {
            id: 22,
            result: serde_json::to_value(Pointed {
                selector: "main > h1.big".into(),
                text: "Hello".into(),
                styles: vec![("font-size".into(), "32px".into())],
                rect: [8.0, 21.5, 300.0, 37.0],
                picture: Some("iVBO".into()),
                url: "http://127.0.0.1:5173/".into(),
            })?,
        },
        Message::Open { open: "/w".into() },
    ] {
        println!("{}", serde_json::to_string(&m)?);
    }
    Ok(())
}
