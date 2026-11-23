//! What an external agent (ACP) or an MCP server started does not outlive a killed daemon
//! (AUD-027, review N13): each runs in a process group of its own, registered with the exit
//! watchdog. The agent and the server here each leave a background `sleep` behind, which only
//! the watchdog's signal to their group ends (they themselves exit when their pipe closes).
#![cfg(unix)]

mod common;

use axiom_protocol::agent::{
    AgentEvent, ConfigureProvider, ConfigureProviderParams, EventBody, ProviderConfig,
    ProviderKind, Send, SendParams, SessionCreate, SessionCreateParams, TurnEndReason,
};
use common::Daemon;
use serde_json::json;
use std::path::{Path, PathBuf};
use std::time::Duration;

fn helper(name: &str) -> PathBuf {
    let p = Path::new(env!("CARGO_BIN_EXE_axiomd"))
        .parent()
        .unwrap()
        .join(name);
    if !p.exists() {
        assert!(std::process::Command::new(env!("CARGO"))
            .args(["build", "-q", "-p", name])
            .status()
            .unwrap()
            .success());
    }
    p
}

fn alive(pid: i32) -> bool {
    nix::sys::signal::kill(nix::unistd::Pid::from_raw(pid), None).is_ok()
}

/// A command that runs `program` after starting `sleep 600` in the background (in its process
/// group) and writing that sleep's pid to `pid_file`.
fn leaving_a_sleep(program: &Path, pid_file: &Path) -> Vec<String> {
    vec![
        "sh".into(),
        "-c".into(),
        format!(
            "sleep 600 & echo $! > '{}'; exec '{}'",
            pid_file.display(),
            program.display()
        ),
    ]
}

async fn pid_in(file: &Path) -> i32 {
    for _ in 0..500 {
        if let Some(pid) = std::fs::read_to_string(file)
            .ok()
            .and_then(|s| s.trim().parse().ok())
        {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("no pid in {}", file.display());
}

/// Kills the daemon and says whether `pid` ended within three seconds (and kills it if not).
async fn dies_with_the_daemon(mut d: Daemon, pid: i32) -> bool {
    assert!(alive(pid));
    d.child.start_kill().unwrap();
    let _ = d.child.wait().await;
    let end = tokio::time::Instant::now() + Duration::from_secs(3);
    while tokio::time::Instant::now() < end {
        if !alive(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    let _ = nix::sys::signal::kill(
        nix::unistd::Pid::from_raw(pid),
        nix::sys::signal::Signal::SIGKILL,
    );
    false
}

fn send(ws: &axiom_protocol::WorkspaceId, sid: &axiom_protocol::agent::SessionId) -> SendParams {
    SendParams {
        workspace_id: Some(ws.clone()),
        session_id: sid.clone(),
        text: "read".into(),
        attachments: vec![],
        images: vec![],
        from_turn: None,
    }
}

async fn turn_ends(d: &Daemon) {
    loop {
        let v = d
            .next_notification("agent/event", Duration::from_secs(60))
            .await;
        let e: AgentEvent = serde_json::from_value(v).unwrap();
        if let EventBody::TurnEnd { reason } = e.event {
            assert_eq!(reason, TurnEndReason::EndTurn);
            return;
        }
    }
}

#[tokio::test]
async fn a_killed_daemon_takes_its_external_agents_with_it() {
    let tmp = tempfile::tempdir().unwrap();
    let pid_file = tmp.path().join("sleep.pid");
    let d = Daemon::ready_in(&tmp.path().join("data")).await;
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
                command: leaving_a_sleep(&helper("fake-acp"), &pid_file),
                env: Default::default(),
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
    d.client.request::<Send>(send(&f.id, &sid)).await.unwrap();
    turn_ends(&d).await;
    let pid = pid_in(&pid_file).await;
    assert!(
        dies_with_the_daemon(d, pid).await,
        "what the external agent started ({pid}) outlived the daemon"
    );
}

#[tokio::test]
async fn a_killed_daemon_takes_its_mcp_servers_with_it() {
    let tmp = tempfile::tempdir().unwrap();
    let pid_file = tmp.path().join("sleep.pid");
    let script = tmp.path().join("s.json");
    std::fs::write(
        &script,
        json!({"turns": [{"events": [{"type":"text","text":"Fine."}]}]}).to_string(),
    )
    .unwrap();
    let d = Daemon::spawn_configured(&tmp.path().join("data"), |c| {
        c.env("AXIOM_TEST_SCRIPTED_MODEL", &script);
    })
    .await;
    d.initialize().await;
    d.client
        .request::<axiom_protocol::settings::Set>(axiom_protocol::settings::SetParams {
            values: [(
                "agent.mcpServers".to_string(),
                json!([{"name": "fake", "argv": leaving_a_sleep(&helper("fake-mcp"), &pid_file)}]),
            )]
            .into_iter()
            .collect(),
        })
        .await
        .unwrap();
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
    d.client.request::<Send>(send(&f.id, &sid)).await.unwrap();
    turn_ends(&d).await;
    let pid = pid_in(&pid_file).await;
    assert!(
        dies_with_the_daemon(d, pid).await,
        "what the MCP server started ({pid}) outlived the daemon"
    );
}
