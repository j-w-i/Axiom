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

/// Hooks git runs from outside `.git` are protected too (review rev3): the directory the
/// workspace's `.git/config` names in `core.hooksPath`, husky's `.husky/`, and a bare
/// repository's `<name>.git/` in the workspace. Writes there always ask, under a policy that
/// allows every write; the folder beside the configured one does not.
#[tokio::test]
async fn hook_directories_outside_git_always_ask() {
    let d = approving_daemon().await;
    let f = d.open_fixture().await;
    std::fs::create_dir_all(f.root.join(".git")).unwrap();
    std::fs::write(
        f.root.join(".git/config"),
        "[core]\n\trepositoryformatversion = 0\n\thooksPath = ./tools/hooks/\n",
    )
    .unwrap();
    std::fs::create_dir_all(f.root.join("tools/other")).unwrap();
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
    for path in [
        "tools/hooks/pre-commit",
        ".husky/pre-commit",
        "mirror.git/hooks/post-update",
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
        assert!(!f.root.join(path).exists(), "{path}");
    }
    let r = d
        .client
        .request::<ToolCall>(call(
            &f.id,
            "fs.applyPatch",
            json!({"path": "tools/other/x.txt", "create": "x"}),
        ))
        .await
        .unwrap();
    assert!(!r.is_error, "{}", r.text);
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
    // The cap only matters on a slow runner: a run ends at 1500 outputs (CI 000d4e8: Ubuntu
    // reached about 1430 in 30 s with these floods running side by side).
    let flood_end = tokio::time::Instant::now() + std::time::Duration::from_secs(120);
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

/// A socket connection that declared `capabilities.approvals` and opened `root`, for the tests
/// with more than one approver.
#[cfg(unix)]
mod approver {
    use serde_json::{json, Value};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    use tokio::net::unix::{OwnedReadHalf, OwnedWriteHalf};

    pub struct Approver {
        pub r: BufReader<OwnedReadHalf>,
        pub w: OwnedWriteHalf,
    }

    impl Approver {
        pub async fn attach(d: &super::Daemon, root: &std::path::Path) -> Approver {
            let run = d.data_dir().join("run");
            let token = std::fs::read_to_string(run.join("axiomd.token")).unwrap();
            let stream = tokio::net::UnixStream::connect(run.join("axiomd.sock"))
                .await
                .unwrap();
            let (r, w) = stream.into_split();
            let mut a = Approver {
                r: BufReader::new(r),
                w,
            };
            a.request(
                1,
                "initialize",
                json!({"protocolVersion": "0.6", "client": {"name": "approver", "version": "0"},
                       "authToken": token, "capabilities": {"approvals": true}}),
            )
            .await;
            a.request(2, "workspace/open", json!({"path": root})).await;
            a
        }

        pub async fn send(&mut self, id: u64, method: &str, params: Value) {
            let msg = json!({"jsonrpc": "2.0", "id": id, "method": method, "params": params});
            self.w
                .write_all(format!("{msg}\n").as_bytes())
                .await
                .unwrap();
        }

        pub async fn line(&mut self) -> Value {
            let mut l = String::new();
            self.r.read_line(&mut l).await.unwrap();
            serde_json::from_str(&l).unwrap()
        }

        /// Sends a request and reads until its reply, which must not be an error.
        pub async fn request(&mut self, id: u64, method: &str, params: Value) {
            self.send(id, method, params).await;
            loop {
                let v = self.line().await;
                if v["id"] == id {
                    assert!(v.get("error").is_none(), "{method}: {v}");
                    return;
                }
            }
        }
    }
}

/// Review C3 and D1, with one approver that stopped reading (its queue full of terminal output):
/// - an approver that attaches while a prompt is being delivered to the stuck one is kept as an
///   approver (the list used to be replaced by the connections that delivery was sent to);
/// - its answer settles the call at once, and the reply to its `agent/approve` comes at once,
///   instead of each waiting out the stuck delivery (up to ten seconds).
#[cfg(unix)]
#[tokio::test]
async fn an_approver_that_attaches_during_a_slow_delivery_is_asked_and_answers_at_once() {
    use approver::Approver;
    use axiom_protocol::terminal::{Close as CloseTerminal, Create, CreateParams, TerminalRef};
    use std::time::{Duration, Instant};
    let d = Daemon::ready().await;
    let f = d.open_fixture().await;
    // A: an approver that never reads again.
    let _a = Approver::attach(&d, &f.root).await;
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
    // The cap only matters on a slow runner: a run ends at 1500 outputs (CI 000d4e8: Ubuntu
    // reached about 1430 in 30 s with these floods running side by side).
    let flood_end = tokio::time::Instant::now() + Duration::from_secs(120);
    while n < 1500 && tokio::time::Instant::now() < flood_end {
        if !d
            .no_notification("terminal/output", Duration::from_millis(100))
            .await
        {
            n += 1;
        }
    }
    assert!(
        n >= 1500,
        "only {n} terminal outputs: the stuck approver's queue may not be full"
    );
    for terminal_id in terminals {
        d.client
            .request::<CloseTerminal>(TerminalRef { terminal_id })
            .await
            .unwrap();
    }
    d.drain_until_quiet(Duration::from_millis(500)).await;

    // Call 1: its prompt waits on A. Meanwhile B attaches.
    let patch = |path: &str| {
        call(
            &f.id,
            "fs.applyPatch",
            json!({"path": path, "create": "x\n"}),
        )
    };
    let first = d.client.request::<ToolCall>(patch("one.txt"));
    let attach = async {
        tokio::time::sleep(Duration::from_secs(1)).await;
        Approver::attach(&d, &f.root).await
    };
    let (_, mut b) = tokio::join!(first, attach);

    // Call 2: B is attached and reading. It is asked, and its answer settles the call well
    // within the stuck delivery's bound.
    let t0 = Instant::now();
    let second = d.client.request::<ToolCall>(patch("two.txt"));
    let answer = async {
        let id = tokio::time::timeout(Duration::from_secs(20), async {
            loop {
                let v = b.line().await;
                if v["method"] == "agent/approvalRequested"
                    && v["params"].to_string().contains("two.txt")
                {
                    break v["params"]["requestId"].as_str().unwrap().to_string();
                }
            }
        })
        .await
        .expect("B, attached and reading, was never asked");
        let asked = t0.elapsed();
        b.request(
            9,
            "agent/approve",
            json!({"requestId": id, "decision": "once"}),
        )
        .await;
        (asked, t0.elapsed())
    };
    let (r2, (asked, replied)) = tokio::join!(second, answer);
    let took = t0.elapsed();
    let r2 = r2.unwrap();
    assert!(!r2.is_error, "{}", r2.text);
    assert_eq!(
        std::fs::read_to_string(f.root.join("two.txt")).unwrap(),
        "x\n"
    );
    for (what, t) in [
        ("asked", asked),
        ("approve replied", replied),
        ("call", took),
    ] {
        assert!(t < Duration::from_secs(5), "{what} after {t:?}");
    }
}

// ---- Review E2, E3: what git reads, as git itself answers ----

fn git_in(dir: &std::path::Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "init.defaultBranch=main"])
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

async fn allow_every_write(d: &Daemon) {
    let allow = |effect| PolicyRule {
        effect: Some(effect),
        tool: None,
        path: None,
        argv_prefix: None,
        decision: Decision::Allow,
    };
    d.client
        .request::<SetPolicy>(Policy {
            default: Decision::Ask,
            rules: vec![allow(Effect::WriteWorkspace), allow(Effect::Delete)],
        })
        .await
        .unwrap();
}

/// The write asks (the test denies it) as a protected path, under a policy that allows every
/// write, and nothing changes.
async fn write_asks(d: &Daemon, f: &common::Fixture, input: Value) {
    let path = input["path"].as_str().unwrap().to_string();
    let existed = f.root.join(&path).exists();
    let (r, req) = call_answering(
        d,
        call(&f.id, "fs.applyPatch", input),
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
        "{path}: {req:?}"
    );
    assert_eq!(f.root.join(&path).exists(), existed, "{path}");
}

/// The write runs without asking.
async fn write_unasked(d: &Daemon, f: &common::Fixture, path: &str) {
    let r = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        d.client.request::<ToolCall>(call(
            &f.id,
            "fs.applyPatch",
            json!({"path": path, "create": "x\n"}),
        )),
    )
    .await
    .unwrap_or_else(|_| panic!("{path}: the write asked"))
    .unwrap();
    assert!(!r.is_error, "{path}: {}", r.text);
}

fn create(path: &str) -> Value {
    json!({"path": path, "create": "#!/bin/sh\necho owned\n"})
}

/// Review E2 (a), the reviewer's probe: a file in the workspace that `.git/config` includes
/// (`include.path`, `includeIf`) can set `core.hooksPath`, `core.fsmonitor` or
/// `core.sshCommand`. Writing it always asks, whether it exists yet or not, and so does what
/// it includes in turn and the hooks directory it names. An ordinary file, or a file named
/// like a bare repository that is not one, does not ask. Review G8: none of these names looks
/// like git configuration (`team.cfg`, `tools/scripts/`), so only git's own answer protects
/// them, not the fallback's name patterns.
#[tokio::test]
async fn files_git_configuration_includes_always_ask() {
    let d = approving_daemon().await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    git_in(&f.root, &["init", "-q"]);
    git_in(&f.root, &["config", "include.path", "../team.cfg"]);
    git_in(
        &f.root,
        &["config", "includeIf.onbranch:main.path", "../branch.cfg"],
    );
    std::fs::create_dir_all(f.root.join("tools/scripts")).unwrap();
    std::fs::create_dir_all(f.root.join("vendor/lib.git")).unwrap();
    allow_every_write(&d).await;
    write_asks(
        &d,
        &f,
        json!({"path": "team.cfg", "create": "[core]\n\thooksPath = tools/scripts\n"}),
    )
    .await;
    write_asks(&d, &f, create("branch.cfg")).await;
    // Once the person has written it, what it names is protected too.
    std::fs::write(
        f.root.join("team.cfg"),
        "[core]\n\thooksPath = tools/scripts\n[include]\n\tpath = nested/more.cfg\n",
    )
    .unwrap();
    write_asks(&d, &f, create("tools/scripts/pre-commit")).await;
    write_asks(&d, &f, create("nested/more.cfg")).await;
    write_asks(&d, &f, json!({"path": "team.cfg", "delete": true})).await;
    write_unasked(&d, &f, "tools/other.txt").await;
    write_unasked(&d, &f, "deps.git").await;
    write_unasked(&d, &f, "vendor/lib.git/README.md").await;
}

/// Review E2 (b), the reviewer's probe: a `.git` file (a worktree, a submodule, a separate git
/// directory). The hooks directory its repository's configuration names asks; so does the
/// repository itself when it is inside the workspace.
#[tokio::test]
async fn a_git_file_repository_is_protected_too() {
    let d = approving_daemon().await;
    allow_every_write(&d).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let outside = tempfile::tempdir().unwrap();
    let gd = outside.path().join("repo.git");
    git_in(
        &f.root,
        &[
            "init",
            "-q",
            "--separate-git-dir",
            gd.to_str().unwrap(),
            f.root.to_str().unwrap(),
        ],
    );
    git_in(&f.root, &["config", "core.hooksPath", "tools/hooks"]);
    write_asks(&d, &f, create("tools/hooks/pre-commit")).await;
    write_unasked(&d, &f, "other.txt").await;

    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let meta = f.root.join("meta");
    git_in(
        &f.root,
        &[
            "init",
            "-q",
            "--separate-git-dir",
            meta.to_str().unwrap(),
            f.root.to_str().unwrap(),
        ],
    );
    write_asks(&d, &f, create("meta/hooks/pre-commit")).await;
    write_asks(&d, &f, create("meta/info/attributes")).await;
    write_asks(&d, &f, json!({"path": "meta/config", "delete": true})).await;
    write_unasked(&d, &f, "src.txt").await;
}

/// Review E3: the hooks directory is git's own reading of the configuration, so what the old
/// parser missed is protected: a value continued with a backslash, a configuration with bytes
/// that are not UTF-8 or larger than a file the workspace reads as text, quotes and comments,
/// and `hooksPath = .` (the hooks are then the hook names at the workspace root, not every
/// file).
#[tokio::test]
async fn the_hooks_directory_is_the_one_git_uses() {
    let d = approving_daemon().await;
    allow_every_write(&d).await;
    let cases: Vec<(Vec<u8>, &str)> = vec![
        (
            b"[core]\n\thooksPath = \\\n\ttools/hooks\n".to_vec(),
            "tools/hooks/pre-commit",
        ),
        (
            b"# \xff\xfe not UTF-8\n[core]\n\thooksPath = \"tools/my hooks\" ; a comment\n"
                .to_vec(),
            "tools/my hooks/pre-push",
        ),
        (
            {
                let mut v = b"[core]\n\thooksPath = tools/hooks\n".to_vec();
                let pad = format!("# {}\n", "x".repeat(1022));
                while v.len() <= 17 * 1024 * 1024 {
                    v.extend_from_slice(pad.as_bytes());
                }
                v
            },
            "tools/hooks/post-checkout",
        ),
        (b"[core]\n\thooksPath = .\n".to_vec(), "pre-commit"),
    ];
    for (config, hook) in cases {
        let f = d.open_dir(tempfile::tempdir().unwrap()).await;
        git_in(&f.root, &["init", "-q"]);
        let mut c = std::fs::read(f.root.join(".git/config")).unwrap();
        c.extend_from_slice(&config);
        std::fs::write(f.root.join(".git/config"), c).unwrap();
        write_asks(&d, &f, create(hook)).await;
        write_unasked(&d, &f, "notes.txt").await;
    }
}

/// Reviews E2, G7: where git cannot answer for a repository that is there (here a
/// configuration it refuses to read), the gate does not fall back to nothing, nor to names
/// alone (an include or a hooks folder can be named anything): every write asks, and the card
/// says why. Once git can answer again, an ordinary write does not ask.
#[tokio::test]
async fn without_an_answer_from_git_every_write_asks() {
    let d = approving_daemon().await;
    allow_every_write(&d).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    git_in(&f.root, &["init", "-q"]);
    let good = std::fs::read_to_string(f.root.join(".git/config")).unwrap();
    std::fs::write(f.root.join(".git/config"), format!("{good}[core\n")).unwrap();
    write_asks(&d, &f, create("shared.gitconfig")).await;
    write_asks(&d, &f, create(".githooks/pre-commit")).await;
    write_asks(&d, &f, create("team.cfg")).await;
    let (r, req) = call_answering(
        &d,
        call(&f.id, "fs.applyPatch", create("src.txt")),
        ApprovalDecision::Deny,
    )
    .await;
    assert_eq!(kind(&r.unwrap_err()), ErrorKind::ApprovalDenied);
    assert!(
        req.rule
            .as_deref()
            .unwrap_or_default()
            .contains("every write asks"),
        "{req:?}"
    );
    assert!(!f.root.join("src.txt").exists());
    std::fs::write(f.root.join(".git/config"), good).unwrap();
    write_unasked(&d, &f, "src.txt").await;
}

/// Review G7: without git at all, a workspace with a repository asks for every write; one
/// with none asks for what may be git's by name (configuration, attributes, hooks, a git
/// directory's entries) and not for an ordinary file (which then fails only because
/// checkpoints need git).
#[tokio::test]
async fn without_git_a_repository_asks_for_every_write_and_a_folder_for_gits_names() {
    let data = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    let d = Daemon::spawn_configured(data.path(), |c| {
        c.env("PATH", empty.path());
    })
    .await;
    let mut info = client_info();
    info.protocol_version = "0.3".into();
    info.capabilities = Some(ClientCapabilities { approvals: true });
    d.client.request::<Initialize>(info).await.unwrap();
    allow_every_write(&d).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    std::fs::create_dir(f.root.join("src")).unwrap();
    for path in [
        "shared.gitconfig",
        ".githooks/pre-commit",
        "tools/hooks/pre-commit",
        "src/HEAD",
        "src/config",
        ".gitattributes",
    ] {
        write_asks(&d, &f, create(path)).await;
    }
    let r = tokio::time::timeout(
        std::time::Duration::from_secs(20),
        d.client
            .request::<ToolCall>(call(&f.id, "fs.applyPatch", create("src/main.txt"))),
    )
    .await
    .expect("an ordinary write asked")
    .unwrap();
    assert!(r.is_error && r.text.contains("git"), "{}", r.text);

    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    std::fs::create_dir(f.root.join(".git")).unwrap();
    std::fs::write(f.root.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    write_asks(&d, &f, create("src.txt")).await;
}

/// A first commit (checkpoints need a nested repository to have one).
fn git_commit(dir: &std::path::Path) {
    git_in(
        dir,
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "-c",
            "commit.gpgsign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "first",
        ],
    );
}

/// Review G2, the reviewer's `probe_b`: every repository in the workspace is asked, not only
/// the root's. A root that is no repository but holds a clone (`app/`, its `core.hooksPath`
/// and an include); a clone nested in that one (`app/vendor/x`); a submodule's work tree,
/// whose configuration is in the superproject's `.git/modules/`. Their hooks folders, includes
/// and programs ask; an ordinary file beside them does not.
#[tokio::test]
async fn every_repository_in_the_workspace_is_asked() {
    let d = approving_daemon().await;
    allow_every_write(&d).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    let app = f.root.join("app");
    std::fs::create_dir(&app).unwrap();
    git_in(&app, &["init", "-q"]);
    git_in(&app, &["config", "core.hooksPath", "tools/gh"]);
    git_in(&app, &["config", "include.path", "../team.cfg"]);
    git_in(&app, &["config", "core.fsmonitor", "tools/fsmon.sh"]);
    git_commit(&app);
    let x = app.join("vendor/x");
    std::fs::create_dir_all(&x).unwrap();
    git_in(&x, &["init", "-q"]);
    git_in(&x, &["config", "core.hooksPath", "scripts/gh"]);
    git_commit(&x);
    write_asks(&d, &f, create("app/tools/gh/pre-commit")).await;
    write_asks(&d, &f, create("app/team.cfg")).await;
    write_asks(&d, &f, create("app/tools/fsmon.sh")).await;
    write_asks(&d, &f, create("app/vendor/x/scripts/gh/post-checkout")).await;
    write_unasked(&d, &f, "app/notes.txt").await;
    write_unasked(&d, &f, "notes.txt").await;

    // A submodule.
    let src = tempfile::tempdir().unwrap();
    git_in(src.path(), &["init", "-q"]);
    git_commit(src.path());
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    git_in(&f.root, &["init", "-q"]);
    git_in(
        &f.root,
        &[
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            "-q",
            src.path().to_str().unwrap(),
            "sm",
        ],
    );
    git_commit(&f.root);
    git_in(&f.root.join("sm"), &["config", "core.hooksPath", "hk"]);
    write_asks(&d, &f, create("sm/hk/pre-commit")).await;
    write_unasked(&d, &f, "sm/notes.txt").await;
}

/// Review G3, the reviewer's `probe_c`: the programs git's configuration names, where they are
/// in the workspace, ask: `core.fsmonitor`, a filter's `clean` (with its `%f`), `core.sshCommand`,
/// a `!` alias's script (`sh scripts/x.sh`), `core.attributesFile`. So does every
/// `.gitattributes`, which routes files through filters and diff drivers. A script nothing
/// names does not ask.
#[tokio::test]
async fn programs_gits_configuration_names_always_ask() {
    let d = approving_daemon().await;
    allow_every_write(&d).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    git_in(&f.root, &["init", "-q"]);
    for (k, v) in [
        ("core.fsmonitor", "tools/fsmon.sh"),
        ("filter.strip.clean", "tools/clean.sh %f"),
        ("core.sshCommand", "tools/ssh.sh"),
        ("alias.x", "!sh scripts/x.sh --fast"),
        ("core.attributesFile", "conf/attrs"),
    ] {
        git_in(&f.root, &["config", k, v]);
    }
    std::fs::create_dir_all(f.root.join("tools")).unwrap();
    for path in [
        "tools/fsmon.sh",
        "tools/clean.sh",
        "tools/ssh.sh",
        "scripts/x.sh",
        "conf/attrs",
        ".gitattributes",
        "web/.gitattributes",
    ] {
        write_asks(&d, &f, create(path)).await;
    }
    write_unasked(&d, &f, "tools/other.sh").await;
}

/// Review G4, the reviewer's `probe_a`: a file included under a condition that is false today
/// (`onbranch:release`) is read all the same: the hooks folder it would set and the file it
/// would include ask before the condition becomes true.
#[tokio::test]
async fn what_a_false_include_would_set_always_asks() {
    let d = approving_daemon().await;
    allow_every_write(&d).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    git_in(&f.root, &["init", "-q"]);
    git_in(
        &f.root,
        &[
            "config",
            "includeIf.onbranch:release.path",
            "../release.cfg",
        ],
    );
    std::fs::write(
        f.root.join("release.cfg"),
        "[core]\n\thooksPath = tools/relhooks\n[include]\n\tpath = nested.cfg\n",
    )
    .unwrap();
    write_asks(&d, &f, create("tools/relhooks/pre-commit")).await;
    write_asks(&d, &f, create("nested.cfg")).await;
    std::fs::create_dir_all(f.root.join("tools")).unwrap();
    write_unasked(&d, &f, "tools/other.txt").await;
}

/// Review G5, the reviewer's `probe_d`: an ordinary folder cannot be made a git directory one
/// plain file at a time. `src/HEAD` alone in an ordinary folder does not ask; once it is there,
/// `src/commondir` and `src/config.worktree` do. A folder that already has `objects/` and
/// `refs/` asks for its `HEAD` and `config`. Other files there, and a `hooks/` folder in an
/// ordinary one, do not ask.
#[tokio::test]
async fn an_ordinary_folder_is_not_made_a_git_directory_unasked() {
    let d = approving_daemon().await;
    allow_every_write(&d).await;
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    git_in(&f.root, &["init", "-q"]);
    git_commit(&f.root);
    std::fs::create_dir_all(f.root.join("src")).unwrap();
    std::fs::create_dir_all(f.root.join("web/hooks")).unwrap();
    write_unasked(&d, &f, "web/hooks/useAuth.ts").await;
    write_unasked(&d, &f, "src/HEAD").await;
    write_asks(
        &d,
        &f,
        json!({"path": "src/commondir", "create": "../.git\n"}),
    )
    .await;
    write_asks(&d, &f, create("src/config.worktree")).await;
    write_asks(&d, &f, create("src/hooks/pre-commit")).await;
    write_unasked(&d, &f, "src/main.rs").await;
    std::fs::create_dir_all(f.root.join("objs/objects")).unwrap();
    std::fs::create_dir_all(f.root.join("objs/refs")).unwrap();
    write_asks(&d, &f, create("objs/HEAD")).await;
    write_asks(&d, &f, create("objs/config")).await;
    write_unasked(&d, &f, "objs/README.md").await;
}

/// Review G6: git's answer is not computed once per turn. In one turn of a session, the
/// agent writes an ordinary file (the answer is computed), runs `git config core.hooksPath
/// tools/gh` (allowed), and then writing `tools/gh/pre-commit` asks. The person then approves
/// an edit of `.git/config` to `tools/gh2` (a write, not a command: the answer follows the
/// file), and writing `tools/gh2/pre-commit` asks too.
#[tokio::test]
async fn a_configuration_changed_mid_turn_counts_from_the_next_call() {
    use axiom_protocol::agent::{
        AgentEvent, EventBody, Send, SendParams, SessionCreate, SessionCreateParams,
    };
    let data = tempfile::tempdir().unwrap();
    let script = data.path().join("script.json");
    let call = |id: &str, name: &str, input: Value| json!({"events": [{"type": "toolCall", "id": id, "name": name, "input": input}]});
    std::fs::write(
        &script,
        json!({"turns": [
            call("w0", "fs.applyPatch", json!({"path": "notes.txt", "create": "n\n"})),
            call("t1", "task.run", json!({"argv": ["git", "config", "core.hooksPath", "tools/gh"]})),
            call("w2", "fs.applyPatch", create("tools/gh/pre-commit")),
            call("w3", "fs.applyPatch", json!({"path": ".git/config",
                "edits": [{"search": "hooksPath = tools/gh", "replace": "hooksPath = tools/gh2"}]})),
            call("w4", "fs.applyPatch", create("tools/gh2/pre-commit")),
            {"events": [{"type": "text", "text": "done"}]}
        ]})
        .to_string(),
    )
    .unwrap();
    let d = Daemon::spawn_configured(data.path(), |c| {
        c.env("AXIOM_TEST_SCRIPTED_MODEL", &script);
    })
    .await;
    let mut info = client_info();
    info.protocol_version = "0.3".into();
    info.capabilities = Some(ClientCapabilities { approvals: true });
    d.client.request::<Initialize>(info).await.unwrap();
    d.client
        .request::<SetPolicy>(Policy {
            default: Decision::Allow,
            rules: vec![],
        })
        .await
        .unwrap();
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    git_in(&f.root, &["init", "-q"]);
    std::fs::create_dir_all(f.root.join("tools/gh")).unwrap();
    std::fs::create_dir_all(f.root.join("tools/gh2")).unwrap();
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
    // The approver: `.git/config` once, everything else denied; what was asked is kept.
    let mut rx = d.client.notifications();
    let client = d.client.clone();
    let asked = std::sync::Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
    let seen = asked.clone();
    let approver = tokio::spawn(async move {
        while let Ok((m, v)) = rx.recv().await {
            if m != "agent/approvalRequested" {
                continue;
            }
            let r: ApprovalRequest = serde_json::from_value(v).unwrap();
            let target = r.targets.join(" ");
            let decision = if target.contains(".git/config") {
                ApprovalDecision::Once
            } else {
                ApprovalDecision::Deny
            };
            seen.lock().unwrap().push(target);
            client
                .request::<Approve>(ApproveParams {
                    request_id: r.request_id,
                    decision,
                })
                .await
                .unwrap();
        }
    });
    d.client
        .request::<Send>(SendParams {
            workspace_id: Some(f.id.clone()),
            session_id: sid.clone(),
            text: "go".into(),
            attachments: vec![],
            images: vec![],
            from_turn: None,
        })
        .await
        .unwrap();
    loop {
        let v = d
            .next_notification("agent/event", std::time::Duration::from_secs(120))
            .await;
        let e: AgentEvent = serde_json::from_value(v).unwrap();
        if e.session_id == sid && matches!(e.event, EventBody::TurnEnd { .. }) {
            break;
        }
    }
    approver.abort();
    let asked = asked.lock().unwrap().clone();
    assert!(f.root.join("notes.txt").exists(), "{asked:?}");
    let config = std::fs::read_to_string(f.root.join(".git/config")).unwrap();
    assert!(config.contains("hooksPath = tools/gh2"), "{config}");
    for hook in ["tools/gh/pre-commit", "tools/gh2/pre-commit"] {
        assert!(!f.root.join(hook).exists(), "{hook} was written: {asked:?}");
        assert!(asked.iter().any(|t| t.contains(hook)), "{hook}: {asked:?}");
    }
}

/// Review E5: where the policy allows a command, no card is shown, so the result itself says
/// which variables the process did not get, and for `SSH_AUTH_SOCK` what that breaks and how
/// to pass it. `preview.start` (a dev server the agent starts) says so too, on the card and in
/// its result.
#[cfg(unix)]
#[tokio::test]
async fn an_allowed_command_says_in_its_result_what_it_did_not_get() {
    use axiom_protocol::preview::{PreviewInfo, PreviewRef, Stop};
    let data = tempfile::tempdir().unwrap();
    let d = Daemon::spawn_configured(data.path(), |c| {
        c.env("SSH_AUTH_SOCK", "/nonexistent/agent.sock")
            .env("FOO_TOKEN", "s3cret-token");
    })
    .await;
    let mut info = client_info();
    info.protocol_version = "0.3".into();
    info.capabilities = Some(ClientCapabilities { approvals: true });
    d.client.request::<Initialize>(info).await.unwrap();
    let f = d.open_dir(tempfile::tempdir().unwrap()).await;
    d.client
        .request::<SetPolicy>(Policy {
            default: Decision::Ask,
            rules: vec![PolicyRule {
                effect: Some(Effect::Execute),
                tool: Some("task.run".into()),
                path: None,
                argv_prefix: None,
                decision: Decision::Allow,
            }],
        })
        .await
        .unwrap();
    let r = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        d.client
            .request::<ToolCall>(call(&f.id, "task.run", json!({"argv": ["env"]}))),
    )
    .await
    .expect("task.run asked")
    .unwrap();
    assert!(!r.is_error, "{}", r.text);
    assert!(
        !r.text.contains("s3cret") && !r.text.contains("/nonexistent/"),
        "{}",
        r.text
    );
    let last = r.text.lines().last().unwrap_or_default();
    assert!(
        last.contains("SSH_AUTH_SOCK")
            && last.contains("FOO_TOKEN")
            && last.contains("git over SSH")
            && last.contains("agent.passEnv"),
        "{}",
        r.text
    );

    // The dev server: asked (no rule allows it), and the card and the result both say it.
    let server = json!({"argv": ["sh", "-c",
        "echo '  Local:   http://127.0.0.1:5998/'; exec sleep 60"]});
    let (r, req) = call_answering(
        &d,
        call(&f.id, "preview.start", server),
        ApprovalDecision::Once,
    )
    .await;
    let r = r.unwrap();
    assert!(!r.is_error, "{}", r.text);
    let preview = req.preview.unwrap_or_default();
    assert!(
        preview.contains("withheld") && preview.contains("SSH_AUTH_SOCK"),
        "{preview}"
    );
    assert!(
        r.text.contains("withheld") && r.text.contains("SSH_AUTH_SOCK"),
        "{}",
        r.text
    );
    let started: PreviewInfo = serde_json::from_value(r.output).unwrap();
    d.client
        .request::<Stop>(PreviewRef {
            preview_id: started.preview_id,
        })
        .await
        .unwrap();
}
