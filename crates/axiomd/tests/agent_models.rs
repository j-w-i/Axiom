//! `agent/models` and `agent/configureProvider` through the real daemon (M3-MODEL-002). The
//! credential store is the test file (`AXIOM_TEST_CREDENTIALS_FILE`), never the developer's
//! keychain; the model server is a loopback fixture.

mod common;

use axiom_protocol::agent::{
    ConfigureProvider, ConfigureProviderParams, Models, ModelsParams, ProviderConfig, ProviderKind,
    Support,
};
use common::{kind, Daemon};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

const SECRET: &str = "sk-integration-secret-7f3a";

/// Answers every request with the same JSON body; returns the base URL and the Authorization
/// headers it saw.
async fn model_server(
    body: &'static str,
) -> (String, std::sync::Arc<std::sync::Mutex<Vec<String>>>) {
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}/v1", l.local_addr().unwrap());
    let seen: std::sync::Arc<std::sync::Mutex<Vec<String>>> = Default::default();
    let s2 = seen.clone();
    tokio::spawn(async move {
        loop {
            let Ok((mut sock, _)) = l.accept().await else {
                return;
            };
            let seen = s2.clone();
            tokio::spawn(async move {
                let mut buf = vec![0u8; 8192];
                let n = sock.read(&mut buf).await.unwrap_or(0);
                let head = String::from_utf8_lossy(&buf[..n]).to_string();
                for line in head.lines() {
                    if let Some(v) = line
                        .strip_prefix("authorization: ")
                        .or(line.strip_prefix("Authorization: "))
                    {
                        seen.lock().unwrap().push(v.to_string());
                    }
                }
                let resp = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
                    body.len()
                );
                let _ = sock.write_all(resp.as_bytes()).await;
            });
        }
    });
    (url, seen)
}

#[tokio::test]
async fn agent_models_configure_round_trip_keeps_the_key_out_of_settings_and_logs() {
    let dir = tempfile::tempdir().unwrap();
    let creds = dir.path().join("credentials.json");
    let log = dir.path().join("axiomd.log");
    let (url, seen) =
        model_server(r#"{"object":"list","data":[{"id":"qwen3:8b"},{"id":"llama3.2"}]}"#).await;
    let d = Daemon::spawn_logged(
        dir.path(),
        |c| {
            c.env("AXIOM_TEST_CREDENTIALS_FILE", &creds)
                .args(["--log-level", "debug"]);
        },
        Some(&log),
    )
    .await;
    d.initialize().await;

    let before = d
        .client
        .request::<Models>(ModelsParams::default())
        .await
        .unwrap();
    // Only the built-in external agents (ADR-0016) before anything is configured.
    let ids: Vec<_> = before
        .providers
        .iter()
        .map(|p| p.config.id.as_str())
        .collect();
    assert_eq!(ids, ["claude-code", "codex", "gemini"]);

    d.client
        .request::<ConfigureProvider>(ConfigureProviderParams {
            id: "local".into(),
            config: Some(ProviderConfig {
                id: "local".into(),
                kind: ProviderKind::OpenaiChat,
                name: "Local server".into(),
                base_url: Some(url.clone()),
                api_key_env: None,
                models: vec![],
                capabilities: None,
                command: vec![],
                env: Default::default(),
            }),
            api_key: Some(SECRET.into()),
            clear_key: false,
        })
        .await
        .unwrap();

    let after = d
        .client
        .request::<Models>(ModelsParams {
            provider_id: Some("local".into()),
        })
        .await
        .unwrap();
    assert_eq!(after.providers.len(), 1);
    let p = &after.providers[0];
    assert!(p.has_key);
    assert_eq!(p.error, None);
    let ids: Vec<_> = p.models.iter().map(|m| m.id.as_str()).collect();
    assert_eq!(ids, ["llama3.2", "qwen3:8b"]);
    assert_eq!(p.models[0].capabilities.tools, Support::Unknown);
    // The key reached the model server, and the credential store.
    assert_eq!(
        seen.lock().unwrap().as_slice(),
        [format!("Bearer {SECRET}")]
    );
    assert!(std::fs::read_to_string(&creds).unwrap().contains(SECRET));

    // A bad URL is refused.
    let e = d
        .client
        .request::<ConfigureProvider>(ConfigureProviderParams {
            id: "remote".into(),
            config: Some(ProviderConfig {
                id: "remote".into(),
                kind: ProviderKind::OpenaiChat,
                name: "Remote".into(),
                base_url: Some("http://example.com/v1".into()),
                api_key_env: None,
                models: vec![],
                capabilities: None,
                command: vec![],
                env: Default::default(),
            }),
            api_key: None,
            clear_key: false,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), axiom_protocol::ErrorKind::InvalidParams);

    d.shutdown().await;
    tokio::time::sleep(Duration::from_millis(50)).await;
    // Nowhere but the credential store: not in the database (settings), not in the log.
    for f in std::fs::read_dir(dir.path()).unwrap() {
        let f = f.unwrap().path();
        if f == creds || f.is_dir() {
            continue;
        }
        let bytes = std::fs::read(&f).unwrap();
        assert!(
            !bytes.windows(SECRET.len()).any(|w| w == SECRET.as_bytes()),
            "the key is in {}",
            f.display()
        );
    }
    assert!(
        std::fs::metadata(&log).unwrap().len() > 0,
        "debug log is empty"
    );
}
