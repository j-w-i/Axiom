//! Managed archive installs against a local HTTP fixture server (no internet).

use axiom_env::{EnvError, Http, HttpConfig, Installer};
use axiom_protocol::env::{InstallProgress, Sha256Source, Step, ToolLocation};
use axiom_protocol::provider::ProviderManifest;
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

// ------------------------------------------------------------- fixture server

#[derive(Clone)]
struct File {
    body: Vec<u8>,
    /// Pause between 1 KiB chunks (to test cancellation mid-download).
    slow: Option<Duration>,
}

struct Server {
    base: String,
    files: Arc<Mutex<HashMap<String, File>>>,
}

impl Server {
    async fn start() -> Server {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let files: Arc<Mutex<HashMap<String, File>>> = Arc::default();
        let f = files.clone();
        tokio::spawn(async move {
            loop {
                let Ok((mut s, _)) = listener.accept().await else {
                    return;
                };
                let files = f.clone();
                tokio::spawn(async move {
                    let mut buf = Vec::new();
                    let mut chunk = [0u8; 1024];
                    while !buf.windows(4).any(|w| w == b"\r\n\r\n") {
                        match s.read(&mut chunk).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => buf.extend_from_slice(&chunk[..n]),
                        }
                    }
                    let head = String::from_utf8_lossy(&buf).into_owned();
                    let mut parts = head.split_whitespace();
                    let (method, path) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""));
                    let file = files.lock().unwrap().get(path).cloned();
                    let Some(file) = file else {
                        let _ = s
                            .write_all(b"HTTP/1.1 404 Not Found\r\ncontent-length: 0\r\nconnection: close\r\n\r\n")
                            .await;
                        return;
                    };
                    let hdr = format!(
                        "HTTP/1.1 200 OK\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                        file.body.len()
                    );
                    if s.write_all(hdr.as_bytes()).await.is_err() || method == "HEAD" {
                        return;
                    }
                    for c in file.body.chunks(1024) {
                        if s.write_all(c).await.is_err() {
                            return;
                        }
                        if let Some(d) = file.slow {
                            tokio::time::sleep(d).await;
                        }
                    }
                });
            }
        });
        Server { base, files }
    }

    fn put(&self, path: &str, body: Vec<u8>) -> String {
        self.put_file(path, File { body, slow: None })
    }

    fn put_file(&self, path: &str, f: File) -> String {
        self.files.lock().unwrap().insert(path.to_string(), f);
        format!("{}{path}", self.base)
    }
}

// ------------------------------------------------------------- archives

fn sha256(b: &[u8]) -> String {
    hex::encode(Sha256::digest(b))
}

/// The fake tool's file name and body: prints `demo-tool <version>`.
fn tool_entry(version: &str) -> (&'static str, Vec<u8>) {
    if cfg!(windows) {
        (
            "demotool.bat",
            format!("@echo demo-tool {version}\r\n").into_bytes(),
        )
    } else {
        (
            "demotool",
            format!("#!/bin/sh\necho \"demo-tool {version}\"\n").into_bytes(),
        )
    }
}

enum Entry {
    File(String, Vec<u8>),
    /// Raw name, written without the builder's path checks.
    RawFile(String, Vec<u8>),
    Link(String, String),
}

fn tar_gz(entries: &[Entry]) -> Vec<u8> {
    let enc = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
    let mut b = tar::Builder::new(enc);
    for e in entries {
        match e {
            Entry::File(name, data) => {
                let mut h = tar::Header::new_gnu();
                h.set_size(data.len() as u64);
                h.set_mode(0o755);
                h.set_entry_type(tar::EntryType::Regular);
                b.append_data(&mut h, name, data.as_slice()).unwrap();
            }
            Entry::RawFile(name, data) => {
                let mut h = tar::Header::new_gnu();
                h.set_size(data.len() as u64);
                h.set_mode(0o644);
                h.set_entry_type(tar::EntryType::Regular);
                let raw = &mut h.as_gnu_mut().unwrap().name;
                raw[..name.len()].copy_from_slice(name.as_bytes());
                h.set_cksum();
                b.append(&h, data.as_slice()).unwrap();
            }
            Entry::Link(name, target) => {
                let mut h = tar::Header::new_gnu();
                h.set_size(0);
                h.set_mode(0o777);
                h.set_entry_type(tar::EntryType::Symlink);
                let raw = &mut h.as_gnu_mut().unwrap().linkname;
                raw[..target.len()].copy_from_slice(target.as_bytes());
                h.set_cksum();
                b.append_data(&mut h, name, std::io::empty()).unwrap();
            }
        }
    }
    b.into_inner().unwrap().finish().unwrap()
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let opts: zip::write::SimpleFileOptions =
        zip::write::SimpleFileOptions::default().unix_permissions(0o755);
    for (name, data) in entries {
        w.start_file(*name, opts).unwrap();
        w.write_all(data).unwrap();
    }
    w.finish().unwrap().into_inner()
}

/// A good archive: `demo-1.2.3/bin/<tool>`, stripped by one component.
fn good_tar(version: &str) -> Vec<u8> {
    let (name, body) = tool_entry(version);
    tar_gz(&[
        Entry::File(format!("demo-{version}/README"), b"hi".to_vec()),
        Entry::File(format!("demo-{version}/bin/{name}"), body),
    ])
}

fn manifest(url: &str, format: &str, checksum: &str) -> ProviderManifest {
    let host = axiom_env::install::host_platform();
    let os = serde_json::to_value(host.os).unwrap();
    let text = format!(
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
versions = {{ kind = "static", versions = ["1.2.3", "1.10.0"] }}
bin_dirs = ["bin"]

[[install.artifacts]]
platform = {{ os = "{os}" }}
url = "{url}"
format = "{format}"
strip_components = 1
checksum = {checksum}
"#,
        os = os.as_str().unwrap()
    );
    axiom_env::manifest::parse(&text).unwrap_or_else(|e| panic!("{e:?}\n{text}"))
}

fn digest_checksum(version: &str, digest: &str) -> String {
    format!(r#"{{ kind = "sha256", values = {{ "{version}" = "{digest}" }} }}"#)
}

fn installer(data: &Path) -> Installer {
    let http = Http::new(HttpConfig {
        allow_http: true,
        ..HttpConfig::default()
    })
    .unwrap();
    Installer::new(data.to_path_buf(), http)
}

/// Every path under `dir`, relative, sorted.
fn snapshot(dir: &Path) -> Vec<PathBuf> {
    fn walk(base: &Path, d: &Path, out: &mut Vec<PathBuf>) {
        for e in std::fs::read_dir(d).into_iter().flatten().flatten() {
            let p = e.path();
            out.push(p.strip_prefix(base).unwrap().to_path_buf());
            if e.file_type().unwrap().is_dir() {
                walk(base, &p, out);
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

type Events = Arc<Mutex<Vec<InstallProgress>>>;

fn recorder() -> (Events, impl Fn(InstallProgress) + Send + Sync) {
    let ev: Events = Arc::default();
    let e = ev.clone();
    (ev, move |p| e.lock().unwrap().push(p))
}

// ------------------------------------------------------------- tests

#[tokio::test]
async fn install_publishes_atomically() {
    let srv = Server::start().await;
    let archive = good_tar("1.2.3");
    let url = srv.put("/demo-1.2.3.tar.gz", archive.clone());
    let data = tempfile::tempdir().unwrap();
    let inst = installer(data.path());
    let m = manifest(
        &url.replace("1.2.3", "${version}"),
        "tar.gz",
        &digest_checksum("1.2.3", &sha256(&archive)),
    );

    let v = inst.versions(&m).await.unwrap();
    assert_eq!(v.available, ["1.10.0", "1.2.3"]);
    assert!(v.installed.is_empty());

    let plan = inst.plan(&m, "1.2.3", None).await.unwrap();
    assert_eq!(plan.download_bytes, Some(archive.len() as u64));
    assert!(
        matches!(&plan.steps[0], Step::Download { url: u, sha256: Sha256Source::Digest { .. }, .. } if *u == url)
    );
    assert!(matches!(
        plan.steps[1],
        Step::Extract {
            strip_components: 1,
            ..
        }
    ));
    assert!(matches!(&plan.steps[2], Step::Verify { tool_id } if tool_id == "demotool"));

    let (events, progress) = recorder();
    let r = inst
        .install(
            &plan.plan_id,
            Some(&m),
            &CancellationToken::new(),
            &progress,
        )
        .await
        .unwrap();
    let root = data.path().join("toolchains").join("demo").join("1.2.3");
    assert_eq!(r.root.as_deref(), Some(root.to_str().unwrap()));
    assert_eq!(r.verified.len(), 1);
    assert_eq!(r.verified[0].version, "1.2.3");
    assert_eq!(
        r.verified[0].location,
        ToolLocation::Managed {
            root: root.to_string_lossy().into_owned()
        }
    );
    assert!(root.join("README").is_file() && root.join("bin").join(tool_entry("x").0).is_file());
    // Nothing else in the provider directory: the staging directory was renamed, not copied.
    assert_eq!(
        snapshot(&data.path().join("toolchains/demo"))
            .iter()
            .filter(|p| p.components().count() == 1)
            .count(),
        1
    );
    // The verified download is in the cache under its digest; no partial file is left.
    let cache: Vec<_> = snapshot(&data.path().join("cache"));
    assert_eq!(cache, [PathBuf::from(sha256(&archive))]);
    let ev = events.lock().unwrap().clone();
    assert!(ev
        .iter()
        .any(|e| e.bytes_done == Some(archive.len() as u64)));
    assert_eq!(ev.iter().map(|e| e.step).max(), Some(2));
    assert!(ev.iter().all(|e| e.steps == 3 && e.plan_id == plan.plan_id));

    // Detection now finds it as a managed install, and versions lists it.
    let host = axiom_env::HostPaths {
        toolchains_dir: data.path().join("toolchains"),
        path: None,
    };
    let found = axiom_env::detect::detect_tool(&m, &m.tools[0], &host)
        .await
        .unwrap();
    assert_eq!(found.version, "1.2.3");
    assert_eq!(inst.versions(&m).await.unwrap().installed, ["1.2.3"]);
    // Planning the same version again: already installed.
    assert!(matches!(
        inst.plan(&m, "1.2.3", None).await,
        Err(EnvError::AlreadyExists(_))
    ));
}

#[tokio::test]
async fn zip_and_checksum_url_install() {
    let srv = Server::start().await;
    let (name, body) = tool_entry("1.10.0");
    let path = format!("demo-1.10.0/bin/{name}");
    let archive = zip_of(&[(path.as_str(), &body)]);
    let url = srv.put("/demo-1.10.0.zip", archive.clone());
    let sums = srv.put(
        "/SHASUMS256.txt",
        format!(
            "{}  other.zip\n{}  demo-1.10.0.zip\n",
            "0".repeat(64),
            sha256(&archive)
        )
        .into_bytes(),
    );
    let data = tempfile::tempdir().unwrap();
    let inst = installer(data.path());
    let m = manifest(
        &url.replace("1.10.0", "${version}"),
        "zip",
        &format!(r#"{{ kind = "sha256_url", url = "{sums}" }}"#),
    );
    let plan = inst.plan(&m, "1.10.0", None).await.unwrap();
    let (_, progress) = recorder();
    let r = inst
        .install(
            &plan.plan_id,
            Some(&m),
            &CancellationToken::new(),
            &progress,
        )
        .await
        .unwrap();
    assert_eq!(r.verified[0].version, "1.10.0");
}

#[tokio::test]
async fn checksum_mismatch_cleans_up() {
    let srv = Server::start().await;
    let archive = good_tar("1.2.3");
    let url = srv.put("/demo-1.2.3.tar.gz", archive.clone());
    let data = tempfile::tempdir().unwrap();
    let inst = installer(data.path());
    let wrong = sha256(b"something else");
    let m = manifest(&url, "tar.gz", &digest_checksum("1.2.3", &wrong));
    let plan = inst.plan(&m, "1.2.3", None).await.unwrap();
    let (_, progress) = recorder();
    let e = inst
        .install(
            &plan.plan_id,
            Some(&m),
            &CancellationToken::new(),
            &progress,
        )
        .await
        .unwrap_err();
    match e {
        EnvError::ChecksumMismatch {
            expected, actual, ..
        } => {
            assert_eq!(expected, wrong);
            assert_eq!(actual, sha256(&archive));
        }
        other => panic!("{other:?}"),
    }
    let rpc: axiom_protocol::RpcError = EnvError::ChecksumMismatch {
        url: url.clone(),
        expected: wrong,
        actual: "x".into(),
    }
    .into();
    assert_eq!(rpc.data.kind, axiom_protocol::ErrorKind::ChecksumMismatch);
    // Nothing behind: no version, no staging, no cache entry, no partial download.
    assert!(snapshot(&data.path().join("toolchains/demo")).is_empty());
    assert!(snapshot(&data.path().join("cache")).is_empty());
}

/// An artifact checked by SHA-512 (Microsoft's .NET publishes no SHA-256) installs, and the
/// plan carries the digest; a wrong one is refused with nothing left behind (M5.5).
#[tokio::test]
async fn a_sha512_checksum_installs_and_a_wrong_one_is_refused() {
    use sha2::Sha512;
    let srv = Server::start().await;
    let archive = good_tar("1.2.3");
    let url = srv.put("/demo-1.2.3.tar.gz", archive.clone());
    let digest = hex::encode(Sha512::digest(&archive));
    let sha512 = |d: &str| format!(r#"{{ kind = "sha512", values = {{ "1.2.3" = "{d}" }} }}"#);
    let data = tempfile::tempdir().unwrap();
    let inst = installer(data.path());
    let m = manifest(&url, "tar.gz", &sha512(&digest));
    let plan = inst.plan(&m, "1.2.3", None).await.unwrap();
    assert!(plan.steps.iter().any(|s| matches!(
        s,
        Step::Download { sha256: Sha256Source::Sha512Digest { value }, .. } if *value == digest
    )));
    let (_, progress) = recorder();
    inst.install(
        &plan.plan_id,
        Some(&m),
        &CancellationToken::new(),
        &progress,
    )
    .await
    .unwrap();
    assert!(data.path().join("toolchains/demo/1.2.3/bin").is_dir());

    let data = tempfile::tempdir().unwrap();
    let inst = installer(data.path());
    let wrong = hex::encode(Sha512::digest(b"something else"));
    let m = manifest(&url, "tar.gz", &sha512(&wrong));
    let plan = inst.plan(&m, "1.2.3", None).await.unwrap();
    let e = inst
        .install(
            &plan.plan_id,
            Some(&m),
            &CancellationToken::new(),
            &progress,
        )
        .await
        .unwrap_err();
    match e {
        EnvError::ChecksumMismatch {
            expected, actual, ..
        } => {
            assert_eq!(expected, wrong);
            assert_eq!(actual, digest);
        }
        other => panic!("{other:?}"),
    }
    assert!(snapshot(&data.path().join("toolchains/demo")).is_empty());
    assert!(snapshot(&data.path().join("cache")).is_empty());
}

async fn install_archive(archive: Vec<u8>) -> (tempfile::TempDir, Result<(), EnvError>) {
    let srv = Server::start().await;
    let url = srv.put("/bad.tar.gz", archive.clone());
    let data = tempfile::tempdir().unwrap();
    let inst = installer(data.path());
    let m = manifest(&url, "tar.gz", &digest_checksum("1.2.3", &sha256(&archive)));
    let plan = inst.plan(&m, "1.2.3", None).await.unwrap();
    let (_, progress) = recorder();
    let r = inst
        .install(
            &plan.plan_id,
            Some(&m),
            &CancellationToken::new(),
            &progress,
        )
        .await
        .map(|_| ());
    (data, r)
}

#[tokio::test]
async fn traversal_rejected() {
    let (name, body) = tool_entry("1.2.3");
    let archive = tar_gz(&[
        Entry::File(format!("demo/bin/{name}"), body),
        Entry::RawFile("demo/../../escaped".into(), b"x".to_vec()),
    ]);
    let (data, r) = install_archive(archive).await;
    assert!(
        matches!(r, Err(EnvError::UnsafeArchive(ref s)) if s.contains("..")),
        "{r:?}"
    );
    assert!(!data.path().join("toolchains/escaped").exists());
    assert!(!data.path().join("escaped").exists());
    assert!(
        snapshot(&data.path().join("toolchains/demo")).is_empty(),
        "staging removed"
    );
}

#[tokio::test]
async fn symlink_escape_rejected() {
    let (name, body) = tool_entry("1.2.3");
    // A link pointing out of the root.
    let archive = tar_gz(&[
        Entry::File(format!("demo/bin/{name}"), body.clone()),
        Entry::Link("demo/bin/evil".into(), "../../../../outside".into()),
    ]);
    let (data, r) = install_archive(archive).await;
    assert!(
        matches!(r, Err(EnvError::UnsafeArchive(ref s)) if s.contains("outside")),
        "{r:?}"
    );
    assert!(snapshot(&data.path().join("toolchains/demo")).is_empty());

    // A link inside the root is fine, but nothing may be written through it.
    let archive = tar_gz(&[
        Entry::File(format!("demo/bin/{name}"), body),
        Entry::Link("demo/lib".into(), "bin".into()),
        Entry::File("demo/lib/x".into(), b"x".to_vec()),
    ]);
    let (data, r) = install_archive(archive).await;
    assert!(
        matches!(r, Err(EnvError::UnsafeArchive(ref s)) if s.contains("through a symbolic link")),
        "{r:?}"
    );
    assert!(snapshot(&data.path().join("toolchains/demo")).is_empty());
}

#[tokio::test]
async fn uninstall_removes_only_root() {
    let srv = Server::start().await;
    let data = tempfile::tempdir().unwrap();
    let inst = installer(data.path());
    for v in ["1.2.3", "1.10.0"] {
        let archive = good_tar(v);
        let url = srv.put(&format!("/demo-{v}.tar.gz"), archive.clone());
        let m = manifest(&url, "tar.gz", &digest_checksum(v, &sha256(&archive)));
        let plan = inst.plan(&m, v, None).await.unwrap();
        let (_, p) = recorder();
        inst.install(&plan.plan_id, Some(&m), &CancellationToken::new(), &p)
            .await
            .unwrap();
    }
    // A sibling outside the toolchains root, and a link that points at it.
    std::fs::write(data.path().join("keep.txt"), "k").unwrap();
    let before = snapshot(data.path());
    inst.uninstall("demo", "1.2.3", None).await.unwrap();
    let after = snapshot(data.path());
    let removed: Vec<_> = before.iter().filter(|p| !after.contains(p)).collect();
    assert!(!removed.is_empty());
    assert!(
        removed
            .iter()
            .all(|p| p.starts_with("toolchains/demo/1.2.3")),
        "{removed:?}"
    );
    assert!(after.iter().all(|p| before.contains(p)));
    assert!(!data.path().join("toolchains/demo/1.2.3").exists());
    assert!(data.path().join("toolchains/demo/1.10.0").is_dir());

    assert!(matches!(
        inst.uninstall("demo", "1.2.3", None).await,
        Err(EnvError::NotFound(_))
    ));
    assert!(matches!(
        inst.uninstall("demo", "..", None).await,
        Err(EnvError::InvalidParams(_))
    ));
    assert!(matches!(
        inst.uninstall("../demo", "1.10.0", None).await,
        Err(EnvError::InvalidParams(_))
    ));
    #[cfg(unix)]
    {
        // A version "directory" that is a link out of the root is refused, and its target kept.
        let outside = data.path().join("outside");
        std::fs::create_dir(&outside).unwrap();
        std::fs::write(outside.join("f"), "f").unwrap();
        std::os::unix::fs::symlink(&outside, data.path().join("toolchains/demo/9.9.9")).unwrap();
        assert!(matches!(
            inst.uninstall("demo", "9.9.9", None).await,
            Err(EnvError::UnsafeArchive(_))
        ));
        assert!(outside.join("f").is_file());
    }
}

#[tokio::test]
async fn plan_immutable_and_expiring() {
    let srv = Server::start().await;
    let archive = good_tar("1.2.3");
    let url = srv.put("/demo.tar.gz", archive.clone());
    let data = tempfile::tempdir().unwrap();
    let m = manifest(&url, "tar.gz", &digest_checksum("1.2.3", &sha256(&archive)));
    let (_, p) = recorder();
    let tok = CancellationToken::new();

    // Expired.
    let short = installer(data.path()).with_plan_lifetime(Duration::from_millis(200));
    let plan = short.plan(&m, "1.2.3", None).await.unwrap();
    tokio::time::sleep(Duration::from_millis(300)).await;
    assert!(matches!(
        short.install(&plan.plan_id, Some(&m), &tok, &p).await,
        Err(EnvError::ApprovalDenied(_))
    ));

    let inst = installer(data.path());
    // Unknown.
    assert!(matches!(
        inst.install("feedface", Some(&m), &tok, &p).await,
        Err(EnvError::ApprovalDenied(_))
    ));
    // The provider changed after planning (a different URL, digest or step source).
    let plan = inst.plan(&m, "1.2.3", None).await.unwrap();
    let changed = manifest(&url, "tar.gz", &digest_checksum("1.2.3", &sha256(b"other")));
    assert!(matches!(
        inst.install(&plan.plan_id, Some(&changed), &tok, &p).await,
        Err(EnvError::ApprovalDenied(_))
    ));
    // ...and that refusal used the plan up.
    assert!(matches!(
        inst.install(&plan.plan_id, Some(&m), &tok, &p).await,
        Err(EnvError::ApprovalDenied(_))
    ));
    // The provider is gone.
    let plan = inst.plan(&m, "1.2.3", None).await.unwrap();
    assert!(matches!(
        inst.install(&plan.plan_id, None, &tok, &p).await,
        Err(EnvError::ApprovalDenied(_))
    ));
    // A good plan runs once.
    let plan = inst.plan(&m, "1.2.3", None).await.unwrap();
    assert!(plan.expires_ms > 0 && plan.plan_id.len() == 32);
    inst.install(&plan.plan_id, Some(&m), &tok, &p)
        .await
        .unwrap();
    inst.uninstall("demo", "1.2.3", None).await.unwrap();
    assert!(matches!(
        inst.install(&plan.plan_id, Some(&m), &tok, &p).await,
        Err(EnvError::ApprovalDenied(_))
    ));
    // Plan ids differ.
    let a = inst.plan(&m, "1.2.3", None).await.unwrap();
    let b = inst.plan(&m, "1.2.3", None).await.unwrap();
    assert_ne!(a.plan_id, b.plan_id);
    // Bad versions and methods are refused at planning.
    assert!(matches!(
        inst.plan(&m, "../1", None).await,
        Err(EnvError::InvalidParams(_))
    ));
    assert!(matches!(
        inst.plan(&m, "1.2.3", Some(7)).await,
        Err(EnvError::InvalidParams(_))
    ));
    assert!(
        matches!(
            inst.plan(&m, "9.9.9", None).await,
            Err(EnvError::NotFound(_))
        ),
        "no checksum for that version"
    );
}

#[tokio::test]
async fn cancel_removes_staging() {
    let srv = Server::start().await;
    // ~64 KiB of incompressible data (a SHA-256 chain), sent 1 KiB every 20 ms (~1.3 s).
    let mut pad = Vec::new();
    let mut block = Sha256::digest(b"seed").to_vec();
    while pad.len() < 64 * 1024 {
        block = Sha256::digest(&block).to_vec();
        pad.extend_from_slice(&block);
    }
    let (name, body) = tool_entry("1.2.3");
    let archive = tar_gz(&[
        Entry::File("demo/pad".into(), pad),
        Entry::File(format!("demo/bin/{name}"), body),
    ]);
    let url = srv.put_file(
        "/slow.tar.gz",
        File {
            body: archive.clone(),
            slow: Some(Duration::from_millis(20)),
        },
    );
    let data = tempfile::tempdir().unwrap();
    let inst = Arc::new(installer(data.path()));
    let m = manifest(&url, "tar.gz", &digest_checksum("1.2.3", &sha256(&archive)));
    let plan = inst.plan(&m, "1.2.3", None).await.unwrap();
    let tok = CancellationToken::new();
    let t2 = tok.clone();
    let progress = move |p: InstallProgress| {
        if p.bytes_done.is_some_and(|b| b >= 4096) {
            t2.cancel();
        }
    };
    let started = std::time::Instant::now();
    let r = inst.install(&plan.plan_id, Some(&m), &tok, &progress).await;
    assert!(matches!(r, Err(EnvError::Cancelled)), "{r:?}");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "stopped promptly"
    );
    assert!(
        snapshot(&data.path().join("toolchains/demo")).is_empty(),
        "staging removed"
    );
    assert!(
        snapshot(&data.path().join("cache")).is_empty(),
        "partial download removed"
    );
}
