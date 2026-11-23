//! "Allow for session" grants end where the documentation says (AUD-009, review N6): a restore
//! and a rewind forget the session's grants (the next same call asks again), a closed
//! connection's grants are not inherited by the next connection that gets its number, and a
//! deleted session's grants are gone.

mod common;

use axiom_protocol::agent::{
    AgentEvent, ApprovalDecision, ApprovalRequest, Approve, ApproveParams, Checkpoints, Decision,
    EventBody, Policy, Restore, RestoreParams, Send, SendParams, SessionCreate,
    SessionCreateParams, SessionDelete, SessionId, SessionRef, SetPolicy, TurnEndReason,
};
use axiom_protocol::lifecycle::{ClientCapabilities, Initialize};
use axiom_protocol::WorkspaceId;
use axiom_rpc::Client;
use common::{client_info, Daemon, WAIT};
use serde_json::json;
use std::time::Duration;

const TURN_WAIT: Duration = Duration::from_secs(60);

/// Asks for every call that runs something: grants are what lets a call through unasked.
async fn ask_all(c: &Client) {
    c.request::<SetPolicy>(Policy {
        default: Decision::Ask,
        rules: vec![],
    })
    .await
    .unwrap();
}

async fn init_approving(c: &Client) {
    let mut info = client_info();
    info.protocol_version = "0.3".into();
    info.capabilities = Some(ClientCapabilities { approvals: true });
    c.request::<Initialize>(info).await.unwrap();
}

fn send(ws: &WorkspaceId, s: &SessionId, text: &str, from_turn: Option<u32>) -> SendParams {
    SendParams {
        workspace_id: Some(ws.clone()),
        session_id: s.clone(),
        text: text.into(),
        attachments: vec![],
        images: vec![],
        from_turn,
    }
}

/// Runs a turn, answering each approval request with `answer`; the requests asked.
async fn turn(d: &Daemon, p: SendParams, answer: ApprovalDecision) -> Vec<ApprovalRequest> {
    let mut rx = d.client.notifications();
    let sid = p.session_id.clone();
    d.client.request::<Send>(p).await.unwrap();
    let mut asked = Vec::new();
    loop {
        let (m, v) = tokio::time::timeout(TURN_WAIT, rx.recv())
            .await
            .expect("the turn ended")
            .unwrap();
        match m.as_str() {
            "agent/approvalRequested" => {
                let r: ApprovalRequest = serde_json::from_value(v).unwrap();
                d.client
                    .request::<Approve>(ApproveParams {
                        request_id: r.request_id.clone(),
                        decision: answer,
                    })
                    .await
                    .unwrap();
                asked.push(r);
            }
            "agent/event" => {
                let e: AgentEvent = serde_json::from_value(v).unwrap();
                if e.session_id != sid {
                    continue;
                }
                if let EventBody::TurnEnd { reason } = e.event {
                    assert_eq!(reason, TurnEndReason::EndTurn);
                    return asked;
                }
            }
            _ => {}
        }
    }
}

/// A restore and a rewind each forget what was allowed for the session: the same call, granted
/// for the session before, asks again after either (`handlers/sessions.rs`: `restore` and
/// `send` with `fromTurn`).
#[tokio::test]
async fn a_restore_and_a_rewind_forget_the_sessions_grants() {
    let tmp = tempfile::tempdir().unwrap();
    let run = |id: &str| json!({"events": [{"type":"toolCall","id":id,"name":"task.run","input":{"argv":["true"]}}]});
    let done = json!({"events": [{"type":"text","text":"Done."}]});
    let script = tmp.path().join("s.json");
    std::fs::write(
        &script,
        json!({"turns": [
            run("c1"), done.clone(), run("c2"), done.clone(),
            run("c3"), done.clone(), run("c4"), done
        ]})
        .to_string(),
    )
    .unwrap();
    let d = Daemon::spawn_configured(&tmp.path().join("data"), |c| {
        c.env("AXIOM_TEST_SCRIPTED_MODEL", &script);
    })
    .await;
    init_approving(&d.client).await;
    ask_all(&d.client).await;
    let f = d.open_fixture().await;
    let sid = d
        .client
        .request::<SessionCreate>(SessionCreateParams {
            workspace_id: Some(f.id.clone()),
            provider_id: "scripted".into(),
            model: "scripted".into(),
            title: None,
        })
        .await
        .unwrap()
        .session_id;

    let asked = turn(
        &d,
        send(&f.id, &sid, "one", None),
        ApprovalDecision::Session,
    )
    .await;
    assert_eq!(asked.len(), 1, "{asked:?}");
    let asked = turn(&d, send(&f.id, &sid, "two", None), ApprovalDecision::Deny).await;
    assert!(
        asked.is_empty(),
        "the grant covers the same call: {asked:?}"
    );

    let cps = d
        .client
        .request::<Checkpoints>(SessionRef {
            workspace_id: Some(f.id.clone()),
            session_id: sid.clone(),
        })
        .await
        .unwrap();
    d.client
        .request::<Restore>(RestoreParams {
            workspace_id: Some(f.id.clone()),
            session_id: sid.clone(),
            checkpoint: cps.checkpoints[0].number,
            skip_conflicts: true,
        })
        .await
        .unwrap();
    let asked = turn(
        &d,
        send(&f.id, &sid, "three", None),
        ApprovalDecision::Session,
    )
    .await;
    assert_eq!(asked.len(), 1, "a restore forgets the grant: {asked:?}");

    let asked = turn(
        &d,
        send(&f.id, &sid, "three again", Some(3)),
        ApprovalDecision::Once,
    )
    .await;
    assert_eq!(asked.len(), 1, "a rewind forgets the grant: {asked:?}");
}

/// Connections reach the daemon through its socket: Unix only.
#[cfg(unix)]
mod connections {
    use super::*;
    use axiom_protocol::agent::{ToolCall, ToolCallParams};
    use axiom_protocol::ErrorKind;
    use common::kind;
    use serde_json::Value;
    use std::path::Path;

    async fn socket_client(data: &Path) -> Client {
        use axiom_protocol::lifecycle::{InitializeParams, PeerInfo};
        let run = data.join("run");
        for _ in 0..250 {
            if run.join("axiomd.token").exists() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        let stream = tokio::net::UnixStream::connect(run.join("axiomd.sock"))
            .await
            .unwrap();
        let (r, w) = stream.into_split();
        let c = Client::new(r, w);
        let token = std::fs::read_to_string(run.join("axiomd.token")).unwrap();
        c.request::<Initialize>(InitializeParams {
            protocol_version: "0.6".into(),
            client: PeerInfo {
                name: "grants-test".into(),
                version: "0".into(),
            },
            auth_token: Some(token),
            capabilities: None,
        })
        .await
        .unwrap();
        c
    }

    /// A connection's grants end with it: the next connection, which gets the closed one's number
    /// and so the same principal (`mcp:<client>#c<n>`), is asked again (`handlers/agent.rs`
    /// `connection_principal`). The grants are forgotten on the next `agent/toolCall`.
    #[tokio::test]
    async fn a_closed_connections_grants_are_not_inherited() {
        let data = tempfile::tempdir().unwrap();
        let d = Daemon::spawn_in(data.path()).await;
        init_approving(&d.client).await;
        ask_all(&d.client).await;
        let f = d.open_fixture().await;
        let call = || ToolCallParams {
            workspace_id: f.id.clone(),
            tool: "task.run".into(),
            input: json!({"argv": ["true"]}),
            principal: "mcp:grants".into(),
        };
        let mut approvals = d.client.notifications();
        let next_request = |rx: &mut tokio::sync::broadcast::Receiver<(String, Value)>| {
            let mut rx = rx.resubscribe();
            async move {
                loop {
                    let (m, v) = rx.recv().await.unwrap();
                    if m == "agent/approvalRequested" {
                        return serde_json::from_value::<ApprovalRequest>(v).unwrap();
                    }
                }
            }
        };

        // The first connection is granted the call for its lifetime.
        let a = socket_client(data.path()).await;
        let asked = next_request(&mut approvals);
        let (r, req) = tokio::join!(a.request::<ToolCall>(call()), async {
            let req = tokio::time::timeout(WAIT, asked).await.expect("asked");
            d.client
                .request::<Approve>(ApproveParams {
                    request_id: req.request_id.clone(),
                    decision: ApprovalDecision::Session,
                })
                .await
                .unwrap();
            req
        });
        assert!(!r.unwrap().is_error);
        let principal = req.principal;
        assert!(principal.starts_with("mcp:grants#c"), "{principal}");
        let r = tokio::time::timeout(WAIT, a.request::<ToolCall>(call()))
            .await
            .expect("granted: not asked")
            .unwrap();
        assert!(!r.is_error);
        drop(a);
        // Let the daemon see the connection end.
        tokio::time::sleep(Duration::from_millis(500)).await;

        // The next connection has the same principal and is asked; denied, the call fails.
        let b = socket_client(data.path()).await;
        let asked = next_request(&mut approvals);
        let r = b.request::<ToolCall>(call());
        let answer = async {
            let req = tokio::time::timeout(WAIT, asked).await.ok()?;
            d.client
                .request::<Approve>(ApproveParams {
                    request_id: req.request_id.clone(),
                    decision: ApprovalDecision::Deny,
                })
                .await
                .unwrap();
            Some(req)
        };
        let (r, req) = tokio::join!(r, answer);
        let req = req.expect("the closed connection's grant covered the next connection's call");
        assert_eq!(req.principal, principal, "the same principal, asked again");
        assert_eq!(kind(&r.unwrap_err()), ErrorKind::ApprovalDenied);
    }
}

/// A deleted session's grants are forgotten (`handlers/sessions.rs` `delete`). Nothing outside
/// the daemon can use a deleted session's principal, so this runs the daemon in process and
/// looks at the gate.
#[tokio::test]
async fn a_deleted_sessions_grants_are_forgotten() {
    use axiom_agent::gate::Policies;
    use axiom_agent::tools::ToolCtx;
    use axiom_protocol::agent::{ConfigureProvider, ConfigureProviderParams};
    use axiom_protocol::agent::{ProviderConfig, ProviderKind};
    use axiom_protocol::workspace::{Open, OpenParams};
    use std::sync::Arc;

    let data = tempfile::tempdir().unwrap();
    let ws_dir = tempfile::tempdir().unwrap();
    std::fs::write(ws_dir.path().join("README.md"), "# demo\n").unwrap();
    let app = axiomd::App::with_data_dir(
        axiom_store::Store::open_in_memory().unwrap(),
        data.path().into(),
    );
    let router = Arc::new(axiomd::build_router(app.clone()));
    let (ours, theirs) = tokio::io::duplex(1 << 20);
    let (sr, sw) = tokio::io::split(theirs);
    let info = axiom_rpc::ServerInfo {
        name: "axiomd".into(),
        version: "0".into(),
    };
    tokio::spawn(axiom_rpc::serve(sr, sw, router, info));
    let (cr, cw) = tokio::io::split(ours);
    let c = Client::new(cr, cw);
    init_approving(&c).await;
    // An external agent that is never started: the session only needs a provider.
    c.request::<ConfigureProvider>(ConfigureProviderParams {
        id: "idle".into(),
        config: Some(ProviderConfig {
            id: "idle".into(),
            kind: ProviderKind::Acp,
            name: "Idle".into(),
            base_url: None,
            api_key_env: None,
            models: vec![],
            capabilities: None,
            command: vec!["true".into()],
            env: Default::default(),
        }),
        api_key: None,
        clear_key: false,
    })
    .await
    .unwrap();
    let ws = c
        .request::<Open>(OpenParams {
            path: ws_dir.path().to_string_lossy().into_owned(),
        })
        .await
        .unwrap()
        .workspace_id;
    let sid = c
        .request::<SessionCreate>(SessionCreateParams {
            workspace_id: Some(ws.clone()),
            provider_id: "idle".into(),
            model: "default".into(),
            title: None,
        })
        .await
        .unwrap()
        .session_id;

    // The session's principal is granted a call for the session.
    let principal = format!("session:{}", sid.0);
    let ctx = ToolCtx {
        workspace_id: ws.clone(),
        principal: principal.clone(),
        cancel: Default::default(),
    };
    let prepared = app
        .agent
        .tools
        .prepare(&ctx, "task.run", json!({"argv": ["true"]}))
        .await
        .unwrap();
    let policies = Policies {
        user: Policy {
            default: Decision::Ask,
            rules: vec![],
        },
        workspace: None,
        protected: vec![],
    };
    let mut rx = c.notifications();
    let answer = async {
        loop {
            let (m, v) = rx.recv().await.unwrap();
            if m == "agent/approvalRequested" {
                let r: ApprovalRequest = serde_json::from_value(v).unwrap();
                c.request::<Approve>(ApproveParams {
                    request_id: r.request_id,
                    decision: ApprovalDecision::Session,
                })
                .await
                .unwrap();
                return;
            }
        }
    };
    let (checked, ()) = tokio::time::timeout(
        WAIT,
        futures_util::future::join(
            app.agent.gate.check(
                &principal,
                &ws,
                Some(&sid),
                &prepared,
                &policies,
                false,
                &ctx.cancel,
            ),
            answer,
        ),
    )
    .await
    .expect("approved");
    assert!(checked.is_ok());
    assert!(app.agent.gate.has_grants(&principal));

    c.request::<SessionDelete>(SessionRef {
        workspace_id: Some(ws),
        session_id: sid,
    })
    .await
    .unwrap();
    assert!(!app.agent.gate.has_grants(&principal));
}
