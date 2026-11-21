//! `axiom mcp` as an MCP client sees it (M3-MCP-001): initialize, tools/list, tools/call under
//! the daemon's policy, with approvals answered by another connection (as the Axiom window
//! does) and refusals as tool errors. Unix: the MCP server reaches the daemon over the socket.
#![cfg(unix)]

use axiom_protocol::agent::{
    ApprovalDecision, ApprovalRequest, Approve, ApproveParams, Decision, Effect, Policy,
    PolicyRule, SetPolicy,
};
use axiom_protocol::lifecycle::{ClientCapabilities, Initialize, InitializeParams, PeerInfo};
use axiom_rpc::Client;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

fn axiomd() -> PathBuf {
    let dir = Path::new(env!("CARGO_BIN_EXE_axiom"))
        .parent()
        .unwrap()
        .to_path_buf();
    let p = dir.join("axiomd");
    if !p.exists() {
        assert!(std::process::Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", "axiomd"])
            .status()
            .unwrap()
            .success());
    }
    p
}

struct Mcp {
    stdin: tokio::process::ChildStdin,
    lines: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    _child: tokio::process::Child,
    next: i64,
}

impl Mcp {
    async fn call(&mut self, method: &str, params: Value) -> Value {
        self.next += 1;
        let id = self.next;
        let line = json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}).to_string();
        self.stdin
            .write_all(format!("{line}\n").as_bytes())
            .await
            .unwrap();
        loop {
            let l = tokio::time::timeout(Duration::from_secs(60), self.lines.next_line())
                .await
                .expect("an MCP reply")
                .unwrap()
                .expect("open stream");
            let v: Value = serde_json::from_str(&l).unwrap();
            if v["id"] == id {
                return v;
            }
        }
    }
    async fn notify(&mut self, method: &str) {
        let line = json!({"jsonrpc":"2.0","method":method}).to_string();
        self.stdin
            .write_all(format!("{line}\n").as_bytes())
            .await
            .unwrap();
    }
}

#[tokio::test]
async fn mcp_lists_and_calls_tools_under_the_same_policy() {
    let data = tempfile::tempdir().unwrap();
    let ws = tempfile::tempdir().unwrap();
    std::fs::write(ws.path().join("README.md"), "# Demo\n").unwrap();

    // The "Axiom window": the daemon's stdio client, able to approve.
    let mut d = tokio::process::Command::new(axiomd())
        .env("AXIOM_DATA_DIR", data.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let app = Client::new(d.stdout.take().unwrap(), d.stdin.take().unwrap());
    app.request::<Initialize>(InitializeParams {
        protocol_version: "0.3".into(),
        client: PeerInfo {
            name: "window".into(),
            version: "0".into(),
        },
        auth_token: None,
        capabilities: Some(ClientCapabilities { approvals: true }),
    })
    .await
    .unwrap();
    app.request::<SetPolicy>(Policy {
        default: Decision::Ask,
        rules: vec![
            PolicyRule {
                effect: Some(Effect::Read),
                tool: None,
                path: None,
                argv_prefix: None,
                decision: Decision::Allow,
            },
            PolicyRule {
                effect: None,
                tool: Some("fs.applyPatch".into()),
                path: Some("secret/**".into()),
                argv_prefix: None,
                decision: Decision::Deny,
            },
        ],
    })
    .await
    .unwrap();
    for _ in 0..100 {
        if data.path().join("run/axiomd.token").exists() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_axiom"))
        .env("AXIOM_DATA_DIR", data.path())
        .env("AXIOM_DAEMON", "/nonexistent")
        .args(["mcp", "--workspace"])
        .arg(ws.path())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let mut mcp = Mcp {
        stdin: child.stdin.take().unwrap(),
        lines: BufReader::new(child.stdout.take().unwrap()).lines(),
        _child: child,
        next: 0,
    };

    let init = mcp
        .call("initialize", json!({"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"claude-code","version":"2.0"}}))
        .await;
    assert_eq!(init["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(init["result"]["serverInfo"]["name"], "axiom");
    assert!(init["result"]["capabilities"]["tools"].is_object());
    mcp.notify("notifications/initialized").await;
    // An unknown revision gets ours.
    let other = mcp
        .call(
            "initialize",
            json!({"protocolVersion":"1999-01-01","clientInfo":{"name":"claude-code"}}),
        )
        .await;
    assert_eq!(other["result"]["protocolVersion"], "2025-06-18");
    assert_eq!(mcp.call("ping", json!({})).await["result"], json!({}));

    let tools = mcp.call("tools/list", json!({})).await;
    let list = tools["result"]["tools"].as_array().unwrap();
    let read = list
        .iter()
        .find(|t| t["name"] == "fs.read")
        .expect("fs.read listed");
    assert_eq!(read["inputSchema"]["type"], "object");
    assert_eq!(read["annotations"]["readOnlyHint"], true);
    let patch = list.iter().find(|t| t["name"] == "fs.applyPatch").unwrap();
    assert_eq!(patch["annotations"]["destructiveHint"], true);

    // A read: allowed by policy.
    let r = mcp
        .call(
            "tools/call",
            json!({"name":"fs.read","arguments":{"path":"README.md"}}),
        )
        .await;
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert!(r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .contains("# Demo"));
    assert_eq!(r["result"]["structuredContent"]["totalLines"], 1);

    // A write: the window is asked, approves once, it runs.
    let mut approvals = app.notifications();
    let call = mcp.call(
        "tools/call",
        json!({"name":"fs.applyPatch","arguments":{"path":"notes.md","create":"hi\n"}}),
    );
    let approve = async {
        loop {
            let (m, p) = approvals.recv().await.unwrap();
            if m == "agent/approvalRequested" {
                let r: ApprovalRequest = serde_json::from_value(p).unwrap();
                assert_eq!(r.principal, "mcp:claude-code");
                app.request::<Approve>(ApproveParams {
                    request_id: r.request_id,
                    decision: ApprovalDecision::Once,
                })
                .await
                .unwrap();
                return;
            }
        }
    };
    let (r, ()) = tokio::join!(call, approve);
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert_eq!(
        std::fs::read_to_string(ws.path().join("notes.md")).unwrap(),
        "hi\n"
    );

    // Denied by a rule: a tool error, nothing written.
    let r = mcp
        .call(
            "tools/call",
            json!({"name":"fs.applyPatch","arguments":{"path":"secret/k.txt","create":"x"}}),
        )
        .await;
    assert_eq!(r["result"]["isError"], true);
    assert!(
        r["result"]["content"][0]["text"]
            .as_str()
            .unwrap()
            .starts_with("Not run: The agent policy denies"),
        "{r}"
    );
    assert!(!ws.path().join("secret").exists());

    // Unknown method and tool.
    assert_eq!(
        mcp.call("resources/list", json!({})).await["error"]["code"],
        -32601
    );
    let r = mcp
        .call("tools/call", json!({"name":"no.such","arguments":{}}))
        .await;
    assert_eq!(r["result"]["isError"], true);

    // The audit log names the MCP client.
    let audit = std::fs::read_dir(data.path().join("audit"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    let text = std::fs::read_to_string(audit).unwrap();
    assert!(text.contains("\"principal\":\"mcp:claude-code\""));
    assert!(text.contains("approved (once)") && text.contains("denied"));
    // The write was checkpointed under the external principal.
    let refs = std::process::Command::new("git")
        .env(
            "GIT_DIR",
            std::fs::read_dir(data.path().join("checkpoints"))
                .unwrap()
                .next()
                .unwrap()
                .unwrap()
                .path(),
        )
        .args(["for-each-ref", "--format=%(refname)"])
        .output()
        .unwrap();
    assert!(String::from_utf8_lossy(&refs.stdout).contains("refs/axiom/ext-mcp_claude-code/0001"));
}
