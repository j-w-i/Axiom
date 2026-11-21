//! Shared harness: spawn the real `axiomd`, talk to it with `axiom_rpc::Client`.
#![allow(dead_code)]

use axiom_protocol::fs::{
    EntryKind, FileVersion, List, ListParams, ReadFile, ReadFileParams, ReadFileResult, WriteFile,
    WriteFileParams,
};
use axiom_protocol::lifecycle::{
    Empty, Initialize, InitializeParams, InitializeResult, PeerInfo, Shutdown,
};
use axiom_protocol::task::{ExitedParams, OutputParams};
use axiom_protocol::workspace::{Open, OpenParams, OpenResult};
use axiom_protocol::{ErrorKind, RpcError, WorkspaceId, WorkspacePath};
use axiom_rpc::Client;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::{ExitStatus, Stdio};
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::sync::{broadcast, Mutex};

pub const WAIT: Duration = Duration::from_secs(5);

pub fn axiomd() -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_axiomd"));
    c.kill_on_drop(true);
    c
}

pub fn client_info() -> InitializeParams {
    InitializeParams {
        protocol_version: "0.1".into(),
        client: PeerInfo {
            name: "axiomd-tests".into(),
            version: "0".into(),
        },
        auth_token: None,
        capabilities: None,
    }
}

/// Asserts the invariant of the error model and returns the kind.
pub fn kind(e: &RpcError) -> ErrorKind {
    assert_eq!(
        e.code,
        e.data.kind.code(),
        "code must equal kind.code(): {e:?}"
    );
    assert!(!e.message.is_empty());
    e.data.kind
}

pub fn wp(s: &str) -> WorkspacePath {
    WorkspacePath(s.to_string())
}

// ------------------------------------------------------------- daemon

pub struct Daemon {
    pub client: Client,
    pub child: Child,
    /// Owned only when `spawn()` created it.
    data: Option<TempDir>,
    data_path: PathBuf,
    rx: Mutex<broadcast::Receiver<(String, Value)>>,
}

impl Daemon {
    pub async fn spawn() -> Daemon {
        let dir = TempDir::new().unwrap();
        let mut d = Self::spawn_in(dir.path()).await;
        d.data = Some(dir);
        d
    }

    /// Spawns against an existing data dir (for restart tests).
    pub async fn spawn_in(data_dir: &Path) -> Daemon {
        Self::spawn_configured(data_dir, |_| {}).await
    }

    /// Spawns with extra process configuration (e.g. a controlled `PATH`).
    pub async fn spawn_configured(data_dir: &Path, configure: impl FnOnce(&mut Command)) -> Daemon {
        let mut cmd = axiomd();
        configure(&mut cmd);
        let mut child = cmd
            .env("AXIOM_DATA_DIR", data_dir)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn axiomd");
        let stdin = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let client = Client::new(stdout, stdin);
        let rx = Mutex::new(client.notifications());
        Daemon {
            client,
            child,
            data: None,
            data_path: data_dir.to_path_buf(),
            rx,
        }
    }

    /// Spawn + `initialize`.
    pub async fn ready() -> Daemon {
        let d = Self::spawn().await;
        d.initialize().await;
        d
    }

    pub async fn ready_in(data_dir: &Path) -> Daemon {
        let d = Self::spawn_in(data_dir).await;
        d.initialize().await;
        d
    }

    pub fn data_dir(&self) -> &Path {
        &self.data_path
    }

    pub async fn initialize(&self) -> InitializeResult {
        self.client
            .request::<Initialize>(client_info())
            .await
            .expect("initialize")
    }

    /// Sends `shutdown` and waits (bounded) for the process to exit.
    pub async fn shutdown(mut self) -> ExitStatus {
        self.client
            .request::<Shutdown>(Empty {})
            .await
            .expect("shutdown");
        tokio::time::timeout(Duration::from_secs(5), self.child.wait())
            .await
            .expect("daemon did not exit after shutdown")
            .unwrap()
    }

    /// Next notification with this method; others are skipped.
    pub async fn next_notification(&self, method: &str, timeout: Duration) -> Value {
        let mut rx = self.rx.lock().await;
        let end = tokio::time::Instant::now() + timeout;
        loop {
            match tokio::time::timeout_at(end, rx.recv()).await {
                Ok(Ok((m, p))) if m == method => return p,
                Ok(Ok(_)) | Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(e)) => panic!("notification stream closed: {e}"),
                Err(_) => panic!("timed out waiting for notification `{method}`"),
            }
        }
    }

    /// True if no notification with this method arrives within `window`.
    /// Discard notifications until none has arrived for `quiet`. Lets a test
    /// separate what the fixture's own setup caused (macOS FSEvents may report
    /// writes made just before the watcher started) from what the test does.
    pub async fn drain_until_quiet(&self, quiet: Duration) {
        let mut rx = self.rx.lock().await;
        let give_up = tokio::time::Instant::now() + Duration::from_secs(10);
        while tokio::time::Instant::now() < give_up {
            if tokio::time::timeout(quiet, rx.recv()).await.is_err() {
                return;
            }
        }
    }

    pub async fn no_notification(&self, method: &str, window: Duration) -> bool {
        let mut rx = self.rx.lock().await;
        let end = tokio::time::Instant::now() + window;
        loop {
            match tokio::time::timeout_at(end, rx.recv()).await {
                Ok(Ok((m, _))) if m == method => return false,
                Ok(_) => continue,
                Err(_) => return true,
            }
        }
    }

    /// All `task/output` and the `task/exited` of `task_id`, in arrival order.
    pub async fn task_events(&self, task_id: &str) -> (Vec<OutputParams>, ExitedParams) {
        let mut rx = self.rx.lock().await;
        let mut out = Vec::new();
        let end = tokio::time::Instant::now() + Duration::from_secs(30);
        loop {
            match tokio::time::timeout_at(end, rx.recv()).await {
                Ok(Ok((m, p))) if m == "task/output" => {
                    let o: OutputParams = serde_json::from_value(p).unwrap();
                    if o.task_id.0 == task_id {
                        out.push(o);
                    }
                }
                Ok(Ok((m, p))) if m == "task/exited" => {
                    let x: ExitedParams = serde_json::from_value(p).unwrap();
                    if x.task_id.0 == task_id {
                        return (out, x);
                    }
                }
                Ok(Ok(_)) | Ok(Err(broadcast::error::RecvError::Lagged(_))) => continue,
                Ok(Err(e)) => panic!("notification stream closed: {e}"),
                Err(_) => panic!("timed out waiting for task {task_id} to exit"),
            }
        }
    }

    /// Copies `tests/fixtures/basic/` into a temp dir and opens it.
    pub async fn open_fixture(&self) -> Fixture {
        let dir = TempDir::new().unwrap();
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/fixtures/basic");
        copy_dir(&src, dir.path());
        // Not committed (git would honour the fixture's own .gitignore).
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::create_dir(dir.path().join("target")).unwrap();
        std::fs::write(dir.path().join("target/debug.o"), "obj").unwrap();
        std::fs::write(dir.path().join("build.log"), "log").unwrap();
        self.open_dir(dir).await
    }

    pub async fn open_dir(&self, dir: TempDir) -> Fixture {
        let r: OpenResult = self
            .client
            .request::<Open>(OpenParams {
                path: dir.path().to_str().unwrap().to_string(),
            })
            .await
            .expect("workspace/open");
        Fixture {
            client: self.client.clone(),
            id: r.workspace_id,
            root: PathBuf::from(r.root),
            name: r.name,
            _dir: dir,
        }
    }
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let dest = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            std::fs::create_dir_all(&dest).unwrap();
            copy_dir(&entry.path(), &dest);
        } else {
            std::fs::copy(entry.path(), &dest).unwrap();
        }
    }
}

// ------------------------------------------------------------ fixture

/// An open workspace backed by a temp dir.
pub struct Fixture {
    pub client: Client,
    pub id: WorkspaceId,
    pub root: PathBuf,
    pub name: String,
    pub _dir: TempDir,
}

impl Fixture {
    pub fn path(&self, rel: &str) -> PathBuf {
        self.root.join(rel)
    }

    pub async fn list(&self, dir: &str) -> Result<Vec<axiom_protocol::fs::DirEntry>, RpcError> {
        self.client
            .request::<List>(ListParams {
                workspace_id: self.id.clone(),
                path: wp(dir),
            })
            .await
            .map(|r| r.entries)
    }

    pub async fn read(&self, path: &str) -> Result<ReadFileResult, RpcError> {
        self.client
            .request::<ReadFile>(ReadFileParams {
                workspace_id: self.id.clone(),
                path: wp(path),
            })
            .await
    }

    pub async fn write(
        &self,
        path: &str,
        text: &str,
        expected: Option<&FileVersion>,
        overwrite: bool,
    ) -> Result<FileVersion, RpcError> {
        self.client
            .request::<WriteFile>(WriteFileParams {
                workspace_id: self.id.clone(),
                path: wp(path),
                text: text.to_string(),
                expected_version: expected.cloned(),
                overwrite,
            })
            .await
            .map(|r| r.version)
    }
}

pub fn names(entries: &[axiom_protocol::fs::DirEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.name.as_str()).collect()
}

pub fn entry<'a>(
    entries: &'a [axiom_protocol::fs::DirEntry],
    name: &str,
) -> &'a axiom_protocol::fs::DirEntry {
    entries
        .iter()
        .find(|e| e.name == name)
        .unwrap_or_else(|| panic!("no entry {name} in {:?}", names(entries)))
}

pub fn is_dir(e: &axiom_protocol::fs::DirEntry) -> bool {
    e.kind == EntryKind::Directory
}

// ---------------------------------------------------------- raw session

/// Line-level access to a daemon, for tests about wire order and framing.
pub struct RawSession {
    pub child: Child,
    pub stdin: Option<ChildStdin>,
    pub lines: tokio::io::Lines<BufReader<ChildStdout>>,
    _data: TempDir,
}

impl RawSession {
    pub async fn spawn() -> RawSession {
        let data = TempDir::new().unwrap();
        let mut child = axiomd()
            .env("AXIOM_DATA_DIR", data.path())
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let stdin = child.stdin.take();
        let lines = BufReader::new(child.stdout.take().unwrap()).lines();
        RawSession {
            child,
            stdin,
            lines,
            _data: data,
        }
    }

    pub async fn send(&mut self, line: &str) {
        let s = self.stdin.as_mut().expect("stdin closed");
        s.write_all(line.as_bytes()).await.unwrap();
        s.write_all(b"\n").await.unwrap();
        s.flush().await.unwrap();
    }

    pub async fn next(&mut self) -> Value {
        let line = tokio::time::timeout(Duration::from_secs(10), self.lines.next_line())
            .await
            .expect("timed out waiting for a line")
            .unwrap()
            .expect("daemon closed stdout");
        serde_json::from_str(&line).unwrap_or_else(|e| panic!("bad line {line}: {e}"))
    }

    pub async fn initialize(&mut self) {
        self.send(
            r#"{"jsonrpc":"2.0","id":0,"method":"initialize","params":{"protocolVersion":"0.1","client":{"name":"t","version":"0"}}}"#,
        )
        .await;
        let v = self.next().await;
        assert_eq!(v["id"], 0);
    }

    pub fn close_stdin(&mut self) {
        self.stdin = None;
    }
}
