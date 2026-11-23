//! The permission gate through the real daemon (M3-GATE-001): `agent/toolCall` under the policy,
//! the approval round trip to a connection that declared `capabilities.approvals`, refusal
//! without one, workspace rules that only tighten, and the audit log.

mod common;

use axiom_protocol::agent::{
    ApprovalDecision, ApprovalRequest, Approve, ApproveParams, Decision, Effect, GetPolicy, Policy,
    PolicyParams, PolicyRule, SetPolicy, ToolCall, ToolCallParams, ToolCallResult,
};
use axiom_protocol::lifecycle::{ClientCapabilities, Initialize};
use axiom_protocol::{ErrorKind, RpcError, WorkspaceId};
use common::{client_info, kind, Daemon, WAIT};
use serde_json::{json, Value};

async fn approving_daemon() -> Daemon {
    let d = Daemon::spawn().await;
    let mut info = client_info();
    info.protocol_version = "0.3".into();
    info.capabilities = Some(ClientCapabilities { approvals: true });
    d.client.request::<Initialize>(info).await.unwrap();
    d
}

fn call(ws: &WorkspaceId, tool: &str, input: Value) -> ToolCallParams {
    ToolCallParams {
        workspace_id: ws.clone(),
        tool: tool.into(),
        input,
        principal: "mcp:test-client".into(),
    }
}

/// Runs the call while answering its approval request with `decision`.
async fn call_answering(
    d: &Daemon,
    p: ToolCallParams,
    decision: ApprovalDecision,
) -> (Result<ToolCallResult, RpcError>, ApprovalRequest) {
    let req = d.client.request::<ToolCall>(p);
    let answer = async {
        let n = d.next_notification("agent/approvalRequested", WAIT).await;
        let r: ApprovalRequest = serde_json::from_value(n).unwrap();
        d.client
            .request::<Approve>(ApproveParams {
                request_id: r.request_id.clone(),
                decision,
            })
            .await
            .unwrap();
        r
    };
    tokio::join!(req, answer)
}

#[tokio::test]
async fn approval_once_executes_and_deny_does_not() {
    let d = approving_daemon().await;
    let f = d.open_fixture().await;

    // Reads are allowed by the default policy: no approval.
    let r = d
        .client
        .request::<ToolCall>(call(&f.id, "fs.read", json!({"path": "README.md"})))
        .await
        .unwrap();
    assert!(!r.is_error, "{}", r.text);

    // A write asks; approved once, it runs.
    let (res, req) = call_answering(
        &d,
        call(
            &f.id,
            "fs.applyPatch",
            json!({"path": "notes.txt", "create": "hello\n"}),
        ),
        ApprovalDecision::Once,
    )
    .await;
    let res = res.unwrap();
    assert!(!res.is_error, "{}", res.text);
    assert_eq!(req.tool, "fs.applyPatch");
    assert_eq!(req.effect, Effect::WriteWorkspace);
    assert_eq!(req.targets, ["notes.txt"]);
    // The declared principal, made this connection's own (AUD-009).
    assert!(
        req.principal.starts_with("mcp:test-client#c"),
        "{}",
        req.principal
    );
    assert!(req.preview.as_deref().unwrap().contains("+hello"));
    assert!(req.can_remember && req.checkpoint_planned);
    let resolved = d.next_notification("agent/approvalResolved", WAIT).await;
    assert_eq!(resolved["decision"], "once");
    assert_eq!(
        std::fs::read_to_string(f.root.join("notes.txt")).unwrap(),
        "hello\n"
    );

    // Denied, it does not run.
    let (res, _) = call_answering(
        &d,
        call(
            &f.id,
            "fs.applyPatch",
            json!({"path": "notes.txt", "edits": [{"search": "hello", "replace": "bye"}]}),
        ),
        ApprovalDecision::Deny,
    )
    .await;
    assert_eq!(kind(&res.unwrap_err()), ErrorKind::ApprovalDenied);
    assert_eq!(
        std::fs::read_to_string(f.root.join("notes.txt")).unwrap(),
        "hello\n"
    );

    // Answering again is not found.
    let e = d
        .client
        .request::<Approve>(ApproveParams {
            request_id: "r1".into(),
            decision: ApprovalDecision::Once,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);

    // The audit log has every call with its decision.
    let audit_dir = d.data_dir().join("audit");
    let file = std::fs::read_dir(&audit_dir)
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let lines: Vec<Value> = std::fs::read_to_string(file)
        .unwrap()
        .lines()
        .map(|l| serde_json::from_str(l).unwrap())
        .collect();
    let decisions: Vec<&str> = lines
        .iter()
        .map(|l| l["decision"].as_str().unwrap())
        .collect();
    assert_eq!(
        decisions,
        ["allowed by policy", "approved (once)", "denied"]
    );
    assert!(lines[1]["principal"]
        .as_str()
        .unwrap()
        .starts_with("mcp:test-client#c"));
    assert_eq!(lines[2]["outcome"], "error");
}

#[tokio::test]
async fn approval_for_session_is_remembered_for_the_same_target() {
    let d = approving_daemon().await;
    let f = d.open_fixture().await;
    let (r, _) = call_answering(
        &d,
        call(&f.id, "task.run", json!({"argv": ["true"]})),
        ApprovalDecision::Session,
    )
    .await;
    assert!(!r.unwrap().is_error);
    // Same argv: no second approval.
    let r = tokio::time::timeout(
        WAIT,
        d.client
            .request::<ToolCall>(call(&f.id, "task.run", json!({"argv": ["true"]}))),
    )
    .await
    .expect("no approval asked the second time")
    .unwrap();
    assert!(!r.is_error);
}

#[tokio::test]
async fn approval_required_when_no_connection_can_approve() {
    // `initialize` without `capabilities.approvals`.
    let d = Daemon::ready().await;
    let f = d.open_fixture().await;
    let e = d
        .client
        .request::<ToolCall>(call(
            &f.id,
            "fs.applyPatch",
            json!({"path": "x.txt", "create": "x"}),
        ))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::ApprovalRequired);
    assert!(!f.root.join("x.txt").exists());
}

#[tokio::test]
async fn approval_policy_and_workspace_rules() {
    let d = approving_daemon().await;
    let f = d.open_fixture().await;
    // Default policy as shipped.
    let p = d
        .client
        .request::<GetPolicy>(PolicyParams::default())
        .await
        .unwrap();
    assert_eq!(p.user.default, Decision::Ask);
    assert_eq!(p.user.rules[0].effect, Some(Effect::Read));

    // The user allows `true`; the workspace denies it: the workspace wins (stricter).
    d.client
        .request::<SetPolicy>(Policy {
            default: Decision::Ask,
            rules: vec![PolicyRule {
                effect: Some(Effect::Execute),
                tool: None,
                path: None,
                argv_prefix: Some(vec!["true".into()]),
                decision: Decision::Allow,
            }],
        })
        .await
        .unwrap();
    let r = d
        .client
        .request::<ToolCall>(call(&f.id, "task.run", json!({"argv": ["true"]})))
        .await
        .unwrap();
    assert!(!r.is_error, "allowed by the user rule: {}", r.text);
    std::fs::create_dir_all(f.root.join(".axiom")).unwrap();
    std::fs::write(
        f.root.join(".axiom/project.toml"),
        "[agent.policy]\n[[agent.policy.rule]]\neffect = \"execute\"\ndecision = \"deny\"\n",
    )
    .unwrap();
    let e = d
        .client
        .request::<ToolCall>(call(&f.id, "task.run", json!({"argv": ["true"]})))
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::ApprovalDenied);
    assert!(e.message.contains("workspace"), "{}", e.message);
    let p = d
        .client
        .request::<GetPolicy>(PolicyParams {
            workspace_id: Some(f.id.clone()),
        })
        .await
        .unwrap();
    assert_eq!(p.workspace.unwrap().rules[0].decision, Decision::Deny);

    // A principal may not pose as a session.
    let mut bad = call(&f.id, "fs.read", json!({"path": "README.md"}));
    bad.principal = "session:s1".into();
    let e = d.client.request::<ToolCall>(bad).await.unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams);
}

/// "Allow for session" covers exactly the approved argv: a longer one starting with it asks
/// again (AUD-009: grants matched as a prefix).
#[tokio::test]
async fn a_session_grant_covers_only_the_exact_argv() {
    let d = approving_daemon().await;
    let f = d.open_fixture().await;
    let (r, _) = call_answering(
        &d,
        call(&f.id, "task.run", json!({"argv": ["echo", "a"]})),
        ApprovalDecision::Session,
    )
    .await;
    assert!(!r.unwrap().is_error);
    let (r, req) = call_answering(
        &d,
        call(
            &f.id,
            "task.run",
            json!({"argv": ["echo", "a", "-rf", "/"]}),
        ),
        ApprovalDecision::Deny,
    )
    .await;
    assert_eq!(kind(&r.unwrap_err()), ErrorKind::ApprovalDenied);
    assert_eq!(req.argv.unwrap(), ["echo", "a", "-rf", "/"]);
}

/// Writes to the workspace's Axiom configuration (its policy, its providers) and to `.git`
/// always ask, even under a policy that allows every write, and cannot be allowed for the
/// session (AUD-104).
#[tokio::test]
async fn writes_to_axiom_configuration_always_ask() {
    let d = approving_daemon().await;
    let f = d.open_fixture().await;
    d.client
        .request::<SetPolicy>(Policy {
            default: Decision::Ask,
            rules: vec![PolicyRule {
                effect: Some(Effect::WriteWorkspace),
                tool: None,
                path: None,
                argv_prefix: None,
                decision: Decision::Allow,
            }],
        })
        .await
        .unwrap();
    let r = d
        .client
        .request::<ToolCall>(call(
            &f.id,
            "fs.applyPatch",
            json!({"path": "ordinary.txt", "create": "x"}),
        ))
        .await
        .unwrap();
    assert!(!r.is_error, "{}", r.text);
    let (r, req) = call_answering(
        &d,
        call(
            &f.id,
            "fs.applyPatch",
            json!({"path": ".axiom/project.toml", "create": "[agent.policy]\ndefault = \"allow\"\n"}),
        ),
        ApprovalDecision::Deny,
    )
    .await;
    assert_eq!(kind(&r.unwrap_err()), ErrorKind::ApprovalDenied);
    assert!(!req.can_remember);
    assert!(req.rule.unwrap_or_default().contains("protected"));
    assert!(!f.root.join(".axiom/project.toml").exists());
}

/// A protected path reached through a symlink inside the workspace (`link -> .git`, which a
/// cloned repository can ship) still always asks, and the card names the real path; so does
/// a nested repository's `.git` and a case variant (AUD-104, review B4).
#[cfg(unix)]
#[tokio::test]
async fn protected_paths_through_a_symlink_or_nested_still_ask() {
    let d = approving_daemon().await;
    let f = d.open_fixture().await;
    std::fs::create_dir_all(f.root.join(".git/hooks")).unwrap();
    std::fs::create_dir_all(f.root.join("sub/.git")).unwrap();
    std::os::unix::fs::symlink(".git", f.root.join("link")).unwrap();
    d.client
        .request::<SetPolicy>(Policy {
            default: Decision::Ask,
            rules: vec![PolicyRule {
                effect: Some(Effect::WriteWorkspace),
                tool: None,
                path: None,
                argv_prefix: None,
                decision: Decision::Allow,
            }],
        })
        .await
        .unwrap();
    for (path, real) in [
        ("link/hooks/pre-commit", ".git/hooks/pre-commit"),
        ("sub/.git/config", "sub/.git/config"),
        // A case-insensitive disk may report either spelling; both are protected.
        (".GIT/hooks/post-merge", ".git/hooks/post-merge"),
    ] {
        let (r, req) = call_answering(
            &d,
            call(
                &f.id,
                "fs.applyPatch",
                json!({"path": path, "create": "#!/bin/sh\necho hook\n"}),
            ),
            ApprovalDecision::Deny,
        )
        .await;
        assert_eq!(kind(&r.unwrap_err()), ErrorKind::ApprovalDenied, "{path}");
        assert!(!req.can_remember, "{path}");
        assert!(
            req.rule
                .as_deref()
                .unwrap_or_default()
                .contains("protected"),
            "{path}: {:?}",
            req.rule
        );
        assert!(
            req.targets.iter().any(|t| t.eq_ignore_ascii_case(real)),
            "{path}: the card shows {:?}",
            req.targets
        );
    }
    assert!(!f.root.join(".git/hooks/pre-commit").exists());
    assert!(!f.root.join("sub/.git/config").exists());
}

/// A process an agent starts does not get the daemon's secrets (`security.md` §Processes;
/// AUD-010): variables that look like secrets and the providers' `apiKeyEnv` variables are
/// withheld (the card says so), unless settings `agent.passEnv` names them. The person's own
/// tasks keep them.
#[cfg(unix)]
#[tokio::test]
async fn agent_started_processes_do_not_get_the_daemons_secrets() {
    use axiom_protocol::settings::{Set, SetParams};
    use axiom_protocol::task::{Command, Start, StartParams};
    let data = tempfile::tempdir().unwrap();
    let d = Daemon::spawn_configured(data.path(), |c| {
        c.env("FOO_TOKEN", "s3cret-token")
            .env("MY_LLM_VAR", "s3cret-llm")
            .env("PLAIN_VALUE", "visible");
    })
    .await;
    let mut info = client_info();
    info.protocol_version = "0.3".into();
    info.capabilities = Some(ClientCapabilities { approvals: true });
    d.client.request::<Initialize>(info).await.unwrap();
    // A provider names MY_LLM_VAR as its key.
    d.client
        .request::<Set>(SetParams {
            values: [(
                "agent.providers".to_string(),
                json!([{"id": "mine", "kind": "openaiChat", "name": "Mine",
                        "baseUrl": "https://example.invalid/v1", "apiKeyEnv": "MY_LLM_VAR",
                        "models": ["m"]}]),
            )]
            .into(),
        })
        .await
        .unwrap();
    let f = d.open_fixture().await;
    let (r, req) = call_answering(
        &d,
        call(&f.id, "task.run", json!({"argv": ["env"]})),
        ApprovalDecision::Once,
    )
    .await;
    let r = r.unwrap();
    assert!(!r.is_error, "{}", r.text);
    assert!(!r.text.contains("s3cret"), "{}", r.text);
    assert!(r.text.contains("PLAIN_VALUE=visible") && r.text.contains("PATH="));
    let preview = req.preview.unwrap_or_default();
    assert!(
        preview.contains("withheld")
            && preview.contains("FOO_TOKEN")
            && preview.contains("MY_LLM_VAR"),
        "{preview}"
    );

    // Allowed by name.
    d.client
        .request::<Set>(SetParams {
            values: [("agent.passEnv".to_string(), json!(["FOO_TOKEN"]))].into(),
        })
        .await
        .unwrap();
    let (r, _) = call_answering(
        &d,
        call(&f.id, "task.run", json!({"argv": ["env"]})),
        ApprovalDecision::Once,
    )
    .await;
    let text = r.unwrap().text;
    assert!(
        text.contains("FOO_TOKEN=s3cret-token") && !text.contains("s3cret-llm"),
        "{text}"
    );

    // The person's own task: unchanged.
    let started = d
        .client
        .request::<Start>(StartParams {
            workspace_id: f.id.clone(),
            command: Command::Argv(vec!["env".into()]),
            cwd: None,
            env: Default::default(),
            label: None,
        })
        .await
        .unwrap();
    let (out, _) = d.task_events(&started.task_id.0).await;
    let all: String = out.iter().map(|o| o.data.as_str()).collect();
    assert!(all.contains("FOO_TOKEN=s3cret-token"), "{all}");
}

/// Review R4: an approval prompt waits for an approving connection that is busy (its queue full
/// of terminal output) but reading again within seconds, rather than answering "no approver"
/// after the half second other relays allow a slow connection.
#[cfg(unix)]
#[tokio::test]
async fn an_approver_that_is_slow_to_read_still_gets_the_prompt() {
    use axiom_protocol::terminal::{Close as CloseTerminal, Create, CreateParams, TerminalRef};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    // The stdio client cannot approve: the socket connection below is the only approver.
    let d = Daemon::ready().await;
    let f = d.open_fixture().await;
    let run = d.data_dir().join("run");
    let token = std::fs::read_to_string(run.join("axiomd.token")).unwrap();
    let stream = tokio::net::UnixStream::connect(run.join("axiomd.sock"))
        .await
        .unwrap();
    let (r, mut w) = stream.into_split();
    let mut r = BufReader::new(r);
    async fn send(w: &mut tokio::net::unix::OwnedWriteHalf, id: u64, method: &str, params: Value) {
        let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
        w.write_all(format!("{msg}\n").as_bytes()).await.unwrap();
    }
    async fn line(r: &mut BufReader<tokio::net::unix::OwnedReadHalf>) -> Value {
        let mut l = String::new();
        r.read_line(&mut l).await.unwrap();
        serde_json::from_str(&l).unwrap()
    }
    for (id, method, params) in [
        (
            1,
            "initialize",
            json!({"protocolVersion": "0.6", "client": {"name": "approver", "version": "0"},
                   "authToken": token, "capabilities": {"approvals": true}}),
        ),
        (2, "workspace/open", json!({"path": f.root})),
    ] {
        send(&mut w, id, method, params).await;
        loop {
            let v = line(&mut r).await;
            if v["id"] == id {
                assert!(v.get("error").is_none(), "{method}: {v}");
                break;
            }
        }
    }

    // Its queue fills with terminal output (to every connection of the workspace) while it
    // does not read; then the output stops, and this client drains its own.
    let mut terminals = Vec::new();
    for _ in 0..12 {
        let t = d
            .client
            .request::<Create>(CreateParams {
                workspace_id: f.id.clone(),
                argv: Some(vec![
                    "sh".into(),
                    "-c".into(),
                    "yes 0123456789012345678901234567890123456789".into(),
                ]),
                cols: 80,
                rows: 24,
            })
            .await
            .unwrap();
        terminals.push(t.terminal_id);
    }
    let mut n = 0usize;
    let flood_end = tokio::time::Instant::now() + std::time::Duration::from_secs(30);
    while n < 1500 && tokio::time::Instant::now() < flood_end {
        if !d
            .no_notification("terminal/output", std::time::Duration::from_millis(100))
            .await
        {
            n += 1;
        }
    }
    assert!(
        n >= 1500,
        "only {n} terminal outputs: the approver's queue may not be full"
    );
    for terminal_id in terminals {
        d.client
            .request::<CloseTerminal>(TerminalRef { terminal_id })
            .await
            .unwrap();
    }
    d.drain_until_quiet(std::time::Duration::from_millis(500))
        .await;

    // A write needs approval; the approver reads again two seconds later and approves it.
    let tool = d.client.request::<ToolCall>(call(
        &f.id,
        "fs.applyPatch",
        json!({"path": "approved.txt", "create": "yes\n"}),
    ));
    let approver = async {
        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        // Bounded: a prompt that was given up on never comes, and the test must fail, not hang.
        let request_id = tokio::time::timeout(std::time::Duration::from_secs(20), async {
            loop {
                let v = line(&mut r).await;
                if v["method"] == "agent/approvalRequested" {
                    break v["params"]["requestId"].as_str().unwrap().to_string();
                }
            }
        })
        .await
        .expect("the approval prompt never reached the approver");
        send(
            &mut w,
            3,
            "agent/approve",
            json!({"requestId": request_id, "decision": "once"}),
        )
        .await;
        loop {
            let v = line(&mut r).await;
            if v["id"] == 3 {
                assert!(v.get("error").is_none(), "approve: {v}");
                break;
            }
        }
    };
    let (result, ()) = tokio::join!(tool, approver);
    let result = result.unwrap();
    assert!(!result.is_error, "{}", result.text);
    assert_eq!(
        std::fs::read_to_string(f.root.join("approved.txt")).unwrap(),
        "yes\n"
    );
}

/// Review R5: what is written is what was approved. A directory swapped for a symlink into
/// `.git/hooks` while the person looks at the card makes the approved write fail, rather than
/// plant a hook the card never showed.
#[cfg(unix)]
#[tokio::test]
async fn a_path_swapped_after_the_card_was_shown_is_not_written() {
    let d = approving_daemon().await;
    let f = d.open_fixture().await;
    std::fs::create_dir_all(f.root.join(".git/hooks")).unwrap();
    std::fs::create_dir_all(f.root.join("sub")).unwrap();
    let req = d.client.request::<ToolCall>(call(
        &f.id,
        "fs.applyPatch",
        json!({"path": "sub/pre-commit", "create": "#!/bin/sh\necho planted\n"}),
    ));
    let answer = async {
        let n = d.next_notification("agent/approvalRequested", WAIT).await;
        let r: ApprovalRequest = serde_json::from_value(n).unwrap();
        assert_eq!(r.targets, vec!["sub/pre-commit".to_string()]);
        // The swap, while the card is open.
        std::fs::remove_dir(f.root.join("sub")).unwrap();
        std::os::unix::fs::symlink(".git/hooks", f.root.join("sub")).unwrap();
        d.client
            .request::<Approve>(ApproveParams {
                request_id: r.request_id,
                decision: ApprovalDecision::Once,
            })
            .await
            .unwrap();
    };
    let (r, ()) = tokio::join!(req, answer);
    match r {
        Ok(r) => assert!(r.is_error, "the swapped write went through: {}", r.text),
        Err(e) => assert_eq!(kind(&e), ErrorKind::Conflict, "{e:?}"),
    }
    assert!(
        !f.root.join(".git/hooks/pre-commit").exists(),
        "a hook was planted"
    );
}
