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
    setup_with(log, Daemon::spawn().await, None).await
}

/// [`setup`] on a given daemon; with `starts`, the fake agent can resume a conversation and
/// logs each start and load there.
async fn setup_with(
    log: &Path,
    d: Daemon,
    starts: Option<&Path>,
) -> (Daemon, common::Fixture, SessionId) {
    let mut info = client_info();
    info.protocol_version = "0.3".into();
    info.capabilities = Some(ClientCapabilities { approvals: true });
    d.client.request::<Initialize>(info).await.unwrap();
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "FAKE_ACP_LOG".to_string(),
        log.to_string_lossy().into_owned(),
    );
    if let Some(p) = starts {
        env.insert("FAKE_ACP_LOAD_SESSION".to_string(), "1".to_string());
        env.insert(
            "FAKE_ACP_STARTS".to_string(),
            p.to_string_lossy().into_owned(),
        );
    }
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

/// What the gate's approval covers is what was approved (AUD-039, AUD-040):
/// - a permission of kind `read`, allowed by the default policy without asking, does not let
///   the agent write that file unseen;
/// - a policy rule allowing the bare command does not allow it with variables the agent sets
///   (`LD_PRELOAD`), and the card lists them.
#[tokio::test]
async fn acp_approvals_cover_only_what_was_shown() {
    use axiom_protocol::agent::{Decision, Policy, PolicyRule, SetPolicy};
    let tmp = tempfile::tempdir().unwrap();
    let (d, f, sid) = setup(&tmp.path().join("acp.log")).await;
    let (evs, asked) = turn(&d, &f.id, &sid, "peek", Some(ApprovalDecision::Deny)).await;
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert_eq!(asked[0].tool, "acp.writeFile");
    assert!(said(&evs).starts_with("write refused"), "{}", said(&evs));
    assert!(!f.root.join("notes.md").exists());

    d.client
        .request::<SetPolicy>(Policy {
            default: Decision::Ask,
            rules: vec![
                PolicyRule {
                    effect: Some(axiom_protocol::agent::Effect::Read),
                    tool: None,
                    path: None,
                    argv_prefix: None,
                    decision: Decision::Allow,
                },
                PolicyRule {
                    effect: None,
                    tool: None,
                    path: None,
                    argv_prefix: Some(vec!["sh".into()]),
                    decision: Decision::Allow,
                },
            ],
        })
        .await
        .unwrap();
    let (evs, asked) = turn(&d, &f.id, &sid, "run", None).await;
    assert!(
        asked.is_empty(),
        "the rule allows the bare command: {asked:?}"
    );
    assert_eq!(said(&evs), "ran: term-out exit 0");
    let (evs, asked) = turn(&d, &f.id, &sid, "envrun", Some(ApprovalDecision::Deny)).await;
    assert_eq!(asked.len(), 1, "{asked:?}");
    assert!(
        asked[0]
            .targets
            .iter()
            .any(|t| t == "env LD_PRELOAD=/tmp/evil.so"),
        "{:?}",
        asked[0].targets
    );
    assert!(asked[0]
        .preview
        .as_deref()
        .unwrap_or_default()
        .contains("LD_PRELOAD=/tmp/evil.so"));
    assert!(said(&evs).starts_with("terminal refused"), "{}", said(&evs));
}

/// An agent that exits mid-turn is not kept: the next turn starts it again instead of failing
/// against the dead process until the session is deleted (AUD-032).
#[tokio::test]
async fn acp_an_agent_that_exited_is_started_again() {
    let tmp = tempfile::tempdir().unwrap();
    let (d, f, sid) = setup(&tmp.path().join("acp.log")).await;
    let (evs, _) = turn(&d, &f.id, &sid, "crash", None).await;
    assert_eq!(end(&evs), TurnEndReason::Error);
    let (evs, _) = turn(&d, &f.id, &sid, "read", None).await;
    assert_eq!(end(&evs), TurnEndReason::EndTurn, "{evs:#?}");
    assert!(said(&evs).starts_with("README: "), "{}", said(&evs));
}

/// AUD-032: an idle session's external agent is stopped when it can resume the conversation
/// (it starts again, resumed, on the next turn); one that cannot resume is kept running.
#[tokio::test]
async fn acp_idle_agents_that_can_resume_are_stopped() {
    let tmp = tempfile::tempdir().unwrap();
    let starts = tmp.path().join("starts.log");
    let d = Daemon::spawn_configured(&tmp.path().join("data"), |c| {
        c.env("AXIOM_TEST_AGENT_IDLE_MS", "400");
    })
    .await;
    let (d, f, sid) = setup_with(&tmp.path().join("acp.log"), d, Some(&starts)).await;
    idle_round_trip(&d, &f.id, &sid).await;
    let text = std::fs::read_to_string(&starts).unwrap();
    assert_eq!(
        text.lines().filter(|l| *l == "started").count(),
        2,
        "restarted: {text}"
    );
    assert!(
        text.lines().any(|l| l.starts_with("loaded")),
        "resumed: {text}"
    );
}

/// AUD-032: an agent that cannot resume a conversation is not stopped while idle.
#[tokio::test]
async fn acp_idle_agents_that_cannot_resume_are_kept() {
    let tmp = tempfile::tempdir().unwrap();
    let starts = tmp.path().join("starts.log");
    let d = Daemon::spawn_configured(&tmp.path().join("data"), |c| {
        c.env("AXIOM_TEST_AGENT_IDLE_MS", "400");
    })
    .await;
    let (d, f, sid) = setup_with(&tmp.path().join("acp.log"), d, Some(&starts)).await;
    // The same agent without loadSession: configure the provider again without the flag.
    let mut env = std::collections::BTreeMap::new();
    env.insert(
        "FAKE_ACP_STARTS".to_string(),
        starts.to_string_lossy().into_owned(),
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
    idle_round_trip(&d, &f.id, &sid).await;
    let text = std::fs::read_to_string(&starts).unwrap();
    assert_eq!(
        text.lines().filter(|l| *l == "started").count(),
        1,
        "kept: {text}"
    );
}

async fn idle_round_trip(d: &Daemon, ws: &WorkspaceId, sid: &SessionId) {
    let (evs, _) = turn(d, ws, sid, "read", None).await;
    assert_eq!(end(&evs), TurnEndReason::EndTurn, "{evs:#?}");
    tokio::time::sleep(Duration::from_millis(1500)).await;
    let (evs, _) = turn(d, ws, sid, "read", None).await;
    assert_eq!(end(&evs), TurnEndReason::EndTurn, "{evs:#?}");
}

/// Configures an external agent `id` that runs `command` with `env`.
async fn configure_agent(
    d: &Daemon,
    id: &str,
    command: Vec<String>,
    env: std::collections::BTreeMap<String, String>,
) {
    d.client
        .request::<ConfigureProvider>(ConfigureProviderParams {
            id: id.into(),
            config: Some(ProviderConfig {
                id: id.into(),
                kind: ProviderKind::Acp,
                name: id.into(),
                base_url: None,
                api_key_env: None,
                models: vec![],
                capabilities: None,
                command,
                env,
            }),
            api_key: None,
            clear_key: false,
        })
        .await
        .unwrap();
}

async fn create_session(d: &Daemon, ws: &WorkspaceId, provider: &str) -> SessionId {
    d.client
        .request::<SessionCreate>(SessionCreateParams {
            workspace_id: Some(ws.clone()),
            provider_id: provider.into(),
            model: "default".into(),
            title: None,
        })
        .await
        .unwrap()
        .session_id
}

/// A minimal ACP agent in `sh`: it answers `initialize` and `session/new`; on each prompt it
/// sends the requests `$ROLE` names, appends Axiom's answer to each to `$OUT`, and ends the
/// turn. `create` starts a terminal running `sleep 30` (and keeps it); `peek` reads the
/// terminal id from `$TERMFILE` and asks for that terminal's output, then kills, waits for and
/// releases it.
const SH_AGENT: &str = r#"
read -r l
echo '{"jsonrpc":"2.0","id":1,"result":{"protocolVersion":1,"agentCapabilities":{},"authMethods":[]}}'
read -r l
echo '{"jsonrpc":"2.0","id":2,"result":{"sessionId":"sh-1"}}'
ask() {
  echo "$1"
  read -r r
  printf '%s\n' "$r" >> "$OUT"
}
while read -r l; do
  case "$l" in
    *'"session/prompt"'*)
      id=$(printf '%s' "$l" | grep -o '"id":[0-9]*' | head -n 1 | cut -d: -f2)
      if [ "$ROLE" = create ]; then
        ask '{"jsonrpc":"2.0","id":100,"method":"terminal/create","params":{"sessionId":"sh-1","command":"sleep","args":["30"]}}'
      else
        t=$(sed -n 's/.*"terminalId":"\([^"]*\)".*/\1/p' "$TERMFILE" | head -n 1)
        for m in output kill wait_for_exit release; do
          ask '{"jsonrpc":"2.0","id":101,"method":"terminal/'"$m"'","params":{"sessionId":"sh-1","terminalId":"'"$t"'"}}'
        done
      fi
      echo '{"jsonrpc":"2.0","id":'"$id"',"result":{"stopReason":"end_turn"}}'
      ;;
  esac
done
"#;

/// One session's agent cannot reach another session's terminal by guessing its id (the ids
/// are sequential): `terminal/output|kill|wait_for_exit|release` on it are refused, and the
/// terminal keeps running (review N5).
#[tokio::test]
async fn acp_an_agent_reaches_only_its_own_terminals() {
    use axiom_protocol::task::{ListTasks, ListTasksParams};
    let tmp = tempfile::tempdir().unwrap();
    let script = tmp.path().join("agent.sh");
    std::fs::write(&script, SH_AGENT).unwrap();
    let (a_out, b_out) = (tmp.path().join("a.out"), tmp.path().join("b.out"));
    let (d, f, _) = setup(&tmp.path().join("acp.log")).await;
    let env = |pairs: &[(&str, &Path)]| {
        pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string_lossy().into_owned()))
            .collect::<std::collections::BTreeMap<_, _>>()
    };
    let sh = vec!["sh".to_string(), script.to_string_lossy().into_owned()];
    let mut a_env = env(&[("OUT", &a_out)]);
    a_env.insert("ROLE".into(), "create".into());
    configure_agent(&d, "sh-create", sh.clone(), a_env).await;
    let mut b_env = env(&[("OUT", &b_out), ("TERMFILE", &a_out)]);
    b_env.insert("ROLE".into(), "peek".into());
    configure_agent(&d, "sh-peek", sh, b_env).await;

    let a = create_session(&d, &f.id, "sh-create").await;
    let (evs, asked) = turn(&d, &f.id, &a, "go", Some(ApprovalDecision::Once)).await;
    assert_eq!(end(&evs), TurnEndReason::EndTurn, "{evs:#?}");
    assert_eq!(asked.len(), 1, "{asked:?}");
    let created = std::fs::read_to_string(&a_out).unwrap();
    assert!(created.contains("\"terminalId\""), "{created}");

    let b = create_session(&d, &f.id, "sh-peek").await;
    let (evs, _) = turn(&d, &f.id, &b, "go", None).await;
    assert_eq!(end(&evs), TurnEndReason::EndTurn, "{evs:#?}");
    let answers = std::fs::read_to_string(&b_out).unwrap();
    let answers: Vec<Value> = answers
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    assert_eq!(answers.len(), 4, "{answers:?}");
    for a in &answers {
        assert!(
            a.get("result").is_none() && a["error"]["message"].is_string(),
            "another session's terminal was reached: {answers:?}"
        );
    }
    // Still running: the other agent's kill did not reach it.
    let tasks = d
        .client
        .request::<ListTasks>(ListTasksParams {
            workspace_id: f.id.clone(),
        })
        .await
        .unwrap();
    assert!(tasks.tasks.iter().any(|t| t.exit.is_none()), "{tasks:?}");
}

/// AUD-032: an external agent that never answers its handshake holds up only its own session.
/// Another session's turn runs meanwhile, with the idle reaper running too (the session that is
/// starting is mid-turn, so it is not reaped).
#[tokio::test]
async fn acp_an_agent_that_hangs_in_its_handshake_holds_up_no_other_session() {
    let tmp = tempfile::tempdir().unwrap();
    let d = Daemon::spawn_configured(&tmp.path().join("data"), |c| {
        c.env("AXIOM_TEST_AGENT_IDLE_MS", "200");
    })
    .await;
    let (d, f, sid) = setup_with(&tmp.path().join("acp.log"), d, None).await;
    // Reads everything, answers nothing: `initialize` waits for its 60 s timeout.
    configure_agent(
        &d,
        "hang",
        vec![
            "sh".into(),
            "-c".into(),
            "while read -r l; do :; done".into(),
        ],
        Default::default(),
    )
    .await;
    let stuck = create_session(&d, &f.id, "hang").await;
    let mut rx = d.client.notifications();
    d.client
        .request::<Send>(send(&f.id, &stuck, "hello"))
        .await
        .unwrap();
    tokio::time::sleep(Duration::from_millis(500)).await;

    d.client
        .request::<Send>(send(&f.id, &sid, "read"))
        .await
        .unwrap();
    let mut said = String::new();
    let ended = tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let (m, p) = rx.recv().await.unwrap();
            if m != "agent/event" {
                continue;
            }
            let e: AgentEvent = serde_json::from_value(p).unwrap();
            let end = matches!(e.event, EventBody::TurnEnd { .. });
            assert!(
                !(end && e.session_id == stuck),
                "the hanging agent's turn ended: {e:?}"
            );
            if e.session_id != sid {
                continue;
            }
            match e.event {
                EventBody::AssistantText { text, kind: None } => said.push_str(&text),
                EventBody::TurnEnd { reason } => return reason,
                _ => {}
            }
        }
    })
    .await
    .expect("the other session's turn ran while an agent hung in its handshake");
    assert_eq!(ended, TurnEndReason::EndTurn);
    assert!(said.starts_with("README: "), "{said}");
}
