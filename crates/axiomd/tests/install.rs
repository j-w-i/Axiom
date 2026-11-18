//! `env/versions`, `env/planInstall`, `env/install`, `env/uninstall` against the real daemon,
//! with a loopback HTTP fixture server (M2-ENV-002).

mod common;
use axiom_protocol::env::{
    Install, InstallParams, InstallProgress, InstallResult, PlanInstall, PlanInstallParams,
    Uninstall, UninstallParams, Versions, VersionsParams,
};
use axiom_protocol::lifecycle::{Cancel, CancelParams};
use axiom_protocol::ErrorKind;
use common::*;
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

/// Serves one body at every path, 1 KiB per `pause`.
async fn serve(body: Vec<u8>, pause: Duration) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let body = Arc::new(body);
    tokio::spawn(async move {
        while let Ok((mut s, _)) = listener.accept().await {
            let body = body.clone();
            tokio::spawn(async move {
                let mut buf = Vec::new();
                let mut chunk = [0u8; 1024];
                while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    match s.read(&mut chunk).await {
                        Ok(0) | Err(_) => return,
                        Ok(n) => buf.extend_from_slice(&chunk[..n]),
                    }
                }
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                if s.write_all(head.as_bytes()).await.is_err() || buf.starts_with(b"HEAD") {
                    return;
                }
                for c in body.chunks(1024) {
                    if s.write_all(c).await.is_err() {
                        return;
                    }
                    tokio::time::sleep(pause).await;
                }
            });
        }
    });
    base
}

/// `demo/bin/demotool` printing `demo-tool <version>`, plus `extra` bytes of padding.
fn archive(version: &str, extra: usize) -> Vec<u8> {
    let (name, body) = if cfg!(windows) {
        ("demotool.bat", format!("@echo demo-tool {version}\r\n"))
    } else {
        (
            "demotool",
            format!("#!/bin/sh\necho \"demo-tool {version}\"\n"),
        )
    };
    let mut pad = Vec::new();
    let mut block = Sha256::digest(b"pad").to_vec();
    while pad.len() < extra {
        block = Sha256::digest(&block).to_vec();
        pad.extend_from_slice(&block);
    }
    let enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut b = tar::Builder::new(enc);
    for (path, data, mode) in [
        ("demo/pad".to_string(), pad, 0o644),
        (format!("demo/bin/{name}"), body.into_bytes(), 0o755),
    ] {
        let mut h = tar::Header::new_gnu();
        h.set_size(data.len() as u64);
        h.set_mode(mode);
        b.append_data(&mut h, path, data.as_slice()).unwrap();
    }
    b.into_inner().unwrap().finish().unwrap()
}

/// A user provider `demo` downloading `<base>/demo-${version}.tar.gz`.
fn user_provider(data: &Path, base: &str, version: &str, digest: &str) {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "windows" => "windows",
        _ => "linux",
    };
    let dir = data.join("providers/demo");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("provider.toml"),
        format!(
            r#"schema = "axiom.provider/1"
id = "demo"
name = "Demo"
description = "d"
version = "0.1.0"

[[tools]]
id = "demotool"
program = "demotool"
version_regex = 'demo-tool (\d+\.\d+\.\d+)'

[[install]]
kind = "archive"
versions = {{ kind = "static", versions = ["{version}"] }}
bin_dirs = ["bin"]

[[install.artifacts]]
platform = {{ os = "{os}" }}
url = "{base}/demo-${{version}}.tar.gz"
format = "tar.gz"
strip_components = 1
checksum = {{ kind = "sha256", values = {{ "{version}" = "{digest}" }} }}
"#
        ),
    )
    .unwrap();
}

async fn daemon(data: &Path) -> Daemon {
    let d = Daemon::spawn_configured(data, |c| {
        c.env("AXIOM_TEST_INSECURE_LOOPBACK_HTTP", "1");
    })
    .await;
    d.initialize().await;
    d
}

#[tokio::test]
async fn install_streams_progress_then_installed_then_responds() {
    let body = archive("3.1.4", 8 * 1024);
    let base = serve(body.clone(), Duration::ZERO).await;
    let data = TempDir::new().unwrap();
    user_provider(
        data.path(),
        &base,
        "3.1.4",
        &hex::encode(Sha256::digest(&body)),
    );
    let d = daemon(data.path()).await;

    let v = d
        .client
        .request::<Versions>(VersionsParams {
            provider_id: "demo".into(),
        })
        .await
        .unwrap();
    assert_eq!(
        (v.available.as_slice(), v.installed.len()),
        (&["3.1.4".to_string()][..], 0)
    );

    let plan = d
        .client
        .request::<PlanInstall>(PlanInstallParams {
            provider_id: "demo".into(),
            version: "3.1.4".into(),
            method: None,
        })
        .await
        .unwrap();
    assert_eq!(plan.download_bytes, Some(body.len() as u64));

    let r = d
        .client
        .request::<Install>(InstallParams {
            plan_id: plan.plan_id.clone(),
        })
        .await
        .unwrap();
    assert_eq!(r.verified[0].version, "3.1.4");
    let root = data.path().join("toolchains/demo/3.1.4");
    assert!(root.join("bin").is_dir());

    // Progress first (download bytes reached the total), then env/installed, all before the
    // response was read.
    let first = d.next_notification("env/installProgress", WAIT).await;
    let first: InstallProgress = serde_json::from_value(first).unwrap();
    assert_eq!(
        (first.plan_id.as_str(), first.step, first.steps),
        (plan.plan_id.as_str(), 0, 3)
    );
    let installed = d
        .next_notification("env/installed", Duration::from_millis(100))
        .await;
    let installed: InstallResult = serde_json::from_value(installed).unwrap();
    assert_eq!(installed, r);

    // Used plans are spent.
    let e = d
        .client
        .request::<Install>(InstallParams {
            plan_id: plan.plan_id.clone(),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::ApprovalDenied);

    d.client
        .request::<Uninstall>(UninstallParams {
            provider_id: "demo".into(),
            version: "3.1.4".into(),
        })
        .await
        .unwrap();
    assert!(!root.exists());
    let e = d
        .client
        .request::<Uninstall>(UninstallParams {
            provider_id: "demo".into(),
            version: "3.1.4".into(),
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
}

#[tokio::test]
async fn wrong_digest_is_checksum_mismatch() {
    let body = archive("3.1.4", 0);
    let base = serve(body, Duration::ZERO).await;
    let data = TempDir::new().unwrap();
    user_provider(data.path(), &base, "3.1.4", &"0".repeat(64));
    let d = daemon(data.path()).await;
    let plan = d
        .client
        .request::<PlanInstall>(PlanInstallParams {
            provider_id: "demo".into(),
            version: "3.1.4".into(),
            method: None,
        })
        .await
        .unwrap();
    let e = d
        .client
        .request::<Install>(InstallParams {
            plan_id: plan.plan_id,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::ChecksumMismatch);
    assert_eq!(e.data.details.unwrap()["expected"], "0".repeat(64));
    assert!(std::fs::read_dir(data.path().join("toolchains/demo"))
        .unwrap()
        .next()
        .is_none());
}

#[tokio::test]
async fn cancel_stops_the_install_and_leaves_nothing() {
    // 64 KiB at 1 KiB / 20 ms: ~1.3 s of download.
    let body = archive("3.1.4", 64 * 1024);
    let base = serve(body.clone(), Duration::from_millis(20)).await;
    let data = TempDir::new().unwrap();
    user_provider(
        data.path(),
        &base,
        "3.1.4",
        &hex::encode(Sha256::digest(&body)),
    );
    let d = daemon(data.path()).await;
    let plan = d
        .client
        .request::<PlanInstall>(PlanInstallParams {
            provider_id: "demo".into(),
            version: "3.1.4".into(),
            method: None,
        })
        .await
        .unwrap();
    let (id, fut) = d.client.request_with_id::<Install>(InstallParams {
        plan_id: plan.plan_id,
    });
    let pending = tokio::spawn(fut);
    // Wait until bytes are flowing, then cancel.
    loop {
        let p: InstallProgress =
            serde_json::from_value(d.next_notification("env/installProgress", WAIT).await).unwrap();
        if p.bytes_done.is_some_and(|b| b > 0) {
            break;
        }
    }
    d.client.notify::<Cancel>(CancelParams { id }).await;
    let e = pending.await.unwrap().unwrap_err();
    assert_eq!(kind(&e), ErrorKind::Cancelled);
    // The install task cleans up on its own shortly after.
    let provider_dir = data.path().join("toolchains/demo");
    let cache = data.path().join("cache");
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    let empty = |p: &Path| {
        std::fs::read_dir(p)
            .map(|mut r| r.next().is_none())
            .unwrap_or(true)
    };
    while !(empty(&provider_dir) && empty(&cache)) {
        assert!(
            std::time::Instant::now() < deadline,
            "staging or partial download left behind"
        );
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
}

#[tokio::test]
async fn unknown_provider_and_plain_http_without_the_test_switch() {
    let data = TempDir::new().unwrap();
    user_provider(data.path(), "http://127.0.0.1:9", "1.0.0", &"0".repeat(64));
    // No AXIOM_TEST_INSECURE_LOOPBACK_HTTP: planning works (the HEAD is refused quietly), the
    // install is refused before any connection.
    let d = Daemon::ready_in(data.path()).await;
    let e = d
        .client
        .request::<PlanInstall>(PlanInstallParams {
            provider_id: "nope".into(),
            version: "1".into(),
            method: None,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::NotFound);
    let plan = d
        .client
        .request::<PlanInstall>(PlanInstallParams {
            provider_id: "demo".into(),
            version: "1.0.0".into(),
            method: None,
        })
        .await
        .unwrap();
    assert_eq!(plan.download_bytes, None);
    let e = d
        .client
        .request::<Install>(InstallParams {
            plan_id: plan.plan_id,
        })
        .await
        .unwrap_err();
    assert_eq!(kind(&e), ErrorKind::InvalidParams, "{e:?}");
    assert!(e.message.contains("HTTPS"), "{}", e.message);
}
