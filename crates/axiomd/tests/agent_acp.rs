//! External agents through ACP (M3-ACP-001), with the in-repo fake agent: its file reads,
//! writes and terminals go through Axiom's gate and checkpoints; its own permission requests
//! are Axiom's approval cards and cover the write they ask about; writes outside the workspace
//! and unapproved writes are refused; cancel answers a pending permission `cancelled`.
#![cfg(unix)]

mod common;

use axiom_protocol::agent::{
    AgentEvent, ApprovalDecision, ApprovalRequest, Approve, ApproveParams, ConfigureProvider,
    ConfigureProviderParams, EventBody, Models, ModelsParams, ProviderConfig, ProviderKind, Send,
    SendParams, SessionCreate, SessionCreateParams, SessionId, SessionRef, TurnEndReason,
};
use axiom_protocol::lifecycle::{ClientCapabilities, Initialize};
use axiom_protocol::WorkspaceId;
use common::{client_info, Daemon};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn fake_acp() -> PathBuf {
    let dir = Path::new(env!("CARGO_BIN_EXE_axiomd"))
        .parent()
        .unwrap()
        .to_path_buf();
    let p = dir.join("fake-acp");
    if !p.exists() {
        assert!(std::process::Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "fake-acp"])
            .status()
            .unwrap()
            .success());
    }
    p
}

async fn setup(log: &Path) -> (Daemon, common::Fixture, SessionId) {
    let d = Daemon::spawn().await;
    let mut info = client_info();
    info.protocol_version = "0.3".into();
    info.capabilities = Some(ClientCapabilities { approvals: true });
    d.client.request::<Initialize>(info).await.unwrap();
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "FAKE_ACP_LOG".to_string(),
        log.to_string_lossy().into_owned(),
    );
    d.client
        .request::<ConfigureProvider>(ConfigureProviderParams {
            id: "fake-agent".into(),
            config: Some(ProviderConfig {
                id: "fake-agent".into(),
                kind: ProviderKind::Acp,
                name: "Fake Agent".into(),
                base_url: None,
                api_key_env: None,
                models: vec![],
                capabilities: None,
                command: vec![fake_acp().to_string_lossy().into_owned()],
                env,
            }),
            api_key: None,
            clear_key: false,
        })
        .await
        .unwrap();
    let f = d.open_fixture().await;
    let sid = d
        .client
        .request::<SessionCreate>(SessionCreateParams {
            workspace_id: Some(f.id.clone()),
            provider_id: "fake-agent".into(),
            model: "default".into(),
            title: None,
        })
        .await
        .unwrap()
        .session_id;
    (d, f, sid)
}

fn send(ws: &WorkspaceId, s: &SessionId, text: &str) -> SendParams {
    SendParams {
        workspace_id: Some(ws.clone()),
        session_id: s.clone(),
        text: text.into(),
        attachments: vec![],
        images: vec![],
        from_turn: None,
    }
}

/// Runs a turn, answering every approval request with `answer` (and counting them).
async fn turn(
    d: &Daemon,
    ws: &WorkspaceId,
    s: &SessionId,
    text: &str,
    answer: Option<ApprovalDecision>,
) -> (Vec<AgentEvent>, Vec<ApprovalRequest>) {
    let mut rx = d.client.notifications();
    d.client.request::<Send>(send(ws, s, text)).await.unwrap();
    let mut evs = Vec::new();
    let mut asked = Vec::new();
    loop {
        let (m, p) = tokio::time::timeout(Duration::from_secs(60), rx.recv())
            .await
            .expect("turn ended")
            .unwrap();
        match m.as_str() {
            "agent/approvalRequested" => {
                let r: ApprovalRequest = serde_json::from_value(p).unwrap();
                asked.push(r.clone());
                if let Some(a) = answer {
                    d.client
                        .request::<Approve>(ApproveParams {
                            request_id: r.request_id,
                            decision: a,
                        })
                        .await
                        .unwrap();
                } else {
                    d.client
                        .request::<axiom_protocol::agent::Cancel>(SessionRef {
                            workspace_id: Some(ws.clone()),
                            session_id: s.clone(),
                        })
                        .await
                        .unwrap();
                }
            }
            "agent/event" => {
                let e: AgentEvent = serde_json::from_value(p).unwrap();
                let end = matches!(e.event, EventBody::TurnEnd { .. });
                evs.push(e);
                if end {
                    return (evs, asked);
                }
            }
            _ => {}
        }
    }
}

fn said(evs: &[AgentEvent]) -> String {
    evs.iter()
        .filter_map(|e| match &e.event {
            EventBody::AssistantText { text, kind: None } => Some(text.clone()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn end(evs: &[AgentEvent]) -> TurnEndReason {
    match evs.last().map(|e| &e.event) {
        Some(EventBody::TurnEnd { reason }) => *reason,
        _ => panic!("no turnEnd"),
    }
}

#[tokio::test]
async fn acp_agent_reads_writes_and_runs_through_axioms_gate() {
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("acp.log");
    let (d, f, sid) = setup(&log).await;

    // Plain answer, with a plan.
    let (evs, asked) = turn(&d, &f.id, &sid, "hello", None).await;
    assert_eq!(end(&evs), TurnEndReason::EndTurn);
    assert!(asked.is_empty());
    assert_eq!(said(&evs), "hello from the fake agent");
    assert!(evs.iter().any(|e| matches!(&e.event, EventBody::AssistantText { kind: Some(axiom_protocol::agent::TextKind::Plan), text } if text.contains("[x] say hello"))));
    assert!(evs
        .iter()
        .any(|e| matches!(e.event, EventBody::TextDelta { .. })));
    // session/new got the workspace and (when the CLI is built next to the daemon) Axiom's MCP.
    let new: Value = serde_json::from_str(
        std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(Path::new(new["cwd"].as_str().unwrap()), f.root);
    let cli = Path::new(env!("CARGO_BIN_EXE_axiomd"))
        .parent()
        .unwrap()
        .join("axiom");
    if cli.exists() {
        assert_eq!(new["mcpServers"][0]["name"], "axiom");
        assert_eq!(new["mcpServers"][0]["args"][0], "mcp");
    }

    // A read through fs/read_text_file: allowed by the default policy, no prompt.
    let (evs, asked) = turn(&d, &f.id, &sid, "read", None).await;
    assert!(asked.is_empty());
    assert!(said(&evs).starts_with("README: "), "{}", said(&evs));

    // A write: one approval (the agent's own permission request), which also covers the write.
    let (evs, asked) = turn(&d, &f.id, &sid, "write", Some(ApprovalDecision::Once)).await;
    assert_eq!(said(&evs), "wrote notes.md", "{evs:#?}");
    assert_eq!(asked.len(), 1, "asked once, not twice: {asked:#?}");
    assert_eq!(asked[0].tool, "fake-agent.Write_notes_md");
    assert_eq!(asked[0].targets, ["Write notes.md", "notes.md"]);
    assert!(asked[0].preview.as_deref().unwrap().contains("+hello"));
    assert_eq!(
        std::fs::read_to_string(f.root.join("notes.md")).unwrap(),
        "hello\n"
    );
    let kinds: Vec<String> = evs
        .iter()
        .map(|e| {
            serde_json::to_value(&e.event).unwrap()["type"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    let ckpt = kinds
        .iter()
        .position(|k| k == "checkpoint")
        .expect("checkpointed before the write");
    let result = kinds.iter().position(|k| k == "toolResult").unwrap();
    assert!(ckpt < result);

    // A terminal: execute asks (default policy), then runs through the executor.
    let (evs, asked) = turn(&d, &f.id, &sid, "run", Some(ApprovalDecision::Once)).await;
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].tool, "acp.terminal");
    assert_eq!(said(&evs), "ran: term-out exit 0", "{evs:#?}");

    // A write the agent did not ask about: Axiom asks itself; denied, nothing is written.
    let (evs, asked) = turn(&d, &f.id, &sid, "sneak", Some(ApprovalDecision::Deny)).await;
    assert_eq!(asked.len(), 1);
    assert_eq!(asked[0].tool, "acp.writeFile");
    assert!(said(&evs).starts_with("sneak refused"), "{}", said(&evs));
    assert!(!f.root.join("sneaky.txt").exists());

    // Outside the workspace: refused without asking.
    let (evs, asked) = turn(&d, &f.id, &sid, "outside", None).await;
    assert!(asked.is_empty());
    assert!(said(&evs).starts_with("outside refused"), "{}", said(&evs));

    // The audit log names the session's principal for the agent's file and terminal calls.
    let audit = std::fs::read_dir(d.data_dir().join("audit"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = std::fs::read_to_string(audit).unwrap();
    assert!(
        text.contains("\"tool\":\"acp.writeFile\"") && text.contains("\"tool\":\"acp.terminal\"")
    );
}

#[tokio::test]
async fn acp_cancel_answers_a_pending_permission_cancelled() {
    let tmp = tempfile::tempdir().unwrap();
    let (d, f, sid) = setup(&tmp.path().join("acp.log")).await;
    // `None`: the approval is answered with agent/cancel.
    let (evs, asked) = turn(&d, &f.id, &sid, "wait", None).await;
    assert_eq!(asked.len(), 1);
    assert_eq!(end(&evs), TurnEndReason::Cancelled);
    assert!(!f.root.join("notes.md").exists());
}

#[tokio::test]
async fn acp_builtin_agents_are_listed_with_availability() {
    let d = Daemon::ready().await;
    let m = d
        .client
        .request::<Models>(ModelsParams::default())
        .await
        .unwrap();
    for id in ["claude-code", "codex", "gemini"] {
        let p = m.providers.iter().find(|p| p.config.id == id).unwrap();
        assert_eq!(p.config.kind, ProviderKind::Acp);
        let installed = axiom_agent::acp::find_program(&p.config.command[0]).is_some();
        if installed {
            assert_eq!(p.models[0].id, "default");
        } else {
            assert!(
                p.error.as_deref().unwrap().contains("Install it"),
                "{:?}",
                p.error
            );
        }
    }
    // ADR-0020 §6 (changed from M3): a conversation without a folder can use an external agent;
    // it runs in a scratch folder (acp_agent_without_a_folder_runs_in_a_scratch_folder).
    d.client
        .request::<SessionCreate>(SessionCreateParams {
            workspace_id: None,
            provider_id: "claude-code".into(),
            model: "default".into(),
            title: None,
        })
        .await
        .expect("a conversation without a folder can use an external agent");
}

/// ADR-0020 §6: an external agent in a conversation without a folder runs in a scratch folder
/// of its own (`<data dir>/scratch/<session>`), with Axiom's gate and approvals as in a folder.
#[tokio::test]
async fn acp_agent_without_a_folder_runs_in_a_scratch_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let log = tmp.path().join("acp.log");
    let data = tempfile::tempdir().unwrap();
    let d = Daemon::spawn_in(data.path()).await;
    let mut info = client_info();
    info.capabilities = Some(ClientCapabilities { approvals: true });
    d.client.request::<Initialize>(info).await.unwrap();
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "FAKE_ACP_LOG".to_string(),
        log.to_string_lossy().into_owned(),
    );
    d.client
        .request::<ConfigureProvider>(ConfigureProviderParams {
            id: "fake-agent".into(),
            config: Some(ProviderConfig {
                id: "fake-agent".into(),
                kind: ProviderKind::Acp,
                name: "Fake Agent".into(),
                base_url: None,
                api_key_env: None,
                models: vec![],
                capabilities: None,
                command: vec![fake_acp().to_string_lossy().into_owned()],
                env,
            }),
            api_key: None,
            clear_key: false,
        })
        .await
        .unwrap();
    let sid = d
        .client
        .request::<SessionCreate>(SessionCreateParams {
            workspace_id: None,
            provider_id: "fake-agent".into(),
            model: "default".into(),
            title: None,
        })
        .await
        .expect("a conversation without a folder can use an external agent")
        .session_id;

    // A write, approved once: it lands in the conversation's scratch folder.
    let mut rx = d.client.notifications();
    d.client
        .request::<Send>(SendParams {
            workspace_id: None,
            session_id: sid.clone(),
            text: "write".into(),
            attachments: vec![],
            images: vec![],
            from_turn: None,
        })
        .await
        .unwrap();
    let mut evs = Vec::new();
    loop {
        let (m, p) = tokio::time::timeout(Duration::from_secs(60), rx.recv())
            .await
            .expect("turn ended")
            .unwrap();
        if m == "agent/approvalRequested" {
            let r: ApprovalRequest = serde_json::from_value(p).unwrap();
            d.client
                .request::<Approve>(ApproveParams {
                    request_id: r.request_id,
                    decision: ApprovalDecision::Once,
                })
                .await
                .unwrap();
        } else if m == "agent/event" {
            let e: AgentEvent = serde_json::from_value(p).unwrap();
            let done = matches!(e.event, EventBody::TurnEnd { .. });
            evs.push(e);
            if done {
                break;
            }
        }
    }
    assert_eq!(end(&evs), TurnEndReason::EndTurn, "{evs:?}");
    let scratch = data.path().join("scratch").join(&sid.0);
    let scratch = std::fs::canonicalize(&scratch).unwrap();
    assert_eq!(
        std::fs::read_to_string(scratch.join("notes.md")).unwrap(),
        "hello\n"
    );
    let new: Value = serde_json::from_str(
        std::fs::read_to_string(&log)
            .unwrap()
            .lines()
            .next()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(new["cwd"].as_str().unwrap(), scratch.to_string_lossy());
}

/// ADR-0020 §6: `agent/installAgent` installs a built-in agent with npm into the data
/// directory, after which it is available (here a fake `npm` that creates the program).
#[tokio::test]
async fn install_agent_puts_a_builtin_agent_in_the_data_directory() {
    use std::os::unix::fs::PermissionsExt;
    let bin = tempfile::tempdir().unwrap();
    let npm = bin.path().join("npm");
    std::fs::write(
        &npm,
        "#!/bin/sh\n# fake npm: install --no-fund --no-audit --prefix <dir> <package>\nwhile [ \"$1\" != \"--prefix\" ]; do shift; done\nprefix=\"$2\"; pkg=\"$3\"\necho \"$pkg\" > \"$prefix/installed.txt\"\nmkdir -p \"$prefix/node_modules/.bin\"\nprintf '#!/bin/sh\\n' > \"$prefix/node_modules/.bin/claude-agent-acp\"\nchmod +x \"$prefix/node_modules/.bin/claude-agent-acp\"\n",
    )
    .unwrap();
    std::fs::set_permissions(&npm, std::fs::Permissions::from_mode(0o755)).unwrap();
    let data = tempfile::tempdir().unwrap();
    let path = format!("{}:/usr/bin:/bin", bin.path().display());
    let d = Daemon::spawn_configured(data.path(), |c| {
        c.env("PATH", &path);
    })
    .await;
    d.client.request::<Initialize>(client_info()).await.unwrap();
    let r = d
        .client
        .request::<axiom_protocol::agent::InstallAgent>(axiom_protocol::agent::InstallAgentParams {
            provider_id: "claude-code".into(),
        })
        .await
        .unwrap();
    let agents = std::fs::canonicalize(data.path().join("agents")).unwrap();
    assert!(
        Path::new(&r.program).starts_with(&agents)
            || r.program.contains("/agents/node_modules/.bin/"),
        "{}",
        r.program
    );
    assert_eq!(
        std::fs::read_to_string(data.path().join("agents/installed.txt"))
            .unwrap()
            .trim(),
        "@agentclientprotocol/claude-agent-acp"
    );
    let m = d
        .client
        .request::<Models>(ModelsParams::default())
        .await
        .unwrap();
    let cc = m
        .providers
        .iter()
        .find(|p| p.config.id == "claude-code")
        .unwrap();
    assert!(cc.error.is_none(), "{:?}", cc.error);
    // Not a built-in agent: not_found.
    let e = d
        .client
        .request::<axiom_protocol::agent::InstallAgent>(axiom_protocol::agent::InstallAgentParams {
            provider_id: "nope".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(common::kind(&e), axiom_protocol::ErrorKind::NotFound);
}
