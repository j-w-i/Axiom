//! Contract tests for the protocol 0.4 additions (ADR-0018, M4-ARCH-001). Same rule as
//! `contract.rs`: a failure means the wire changed; follow `governance.md`, do not edit the JSON.

use axiom_protocol::envelope::{ErrorKind, RpcError};
use axiom_protocol::method::{Method, Notification};
use axiom_protocol::{git, lsp, registry, terminal, workspace, PROTOCOL_VERSION};
use serde_json::{json, Value};

fn round_trip<T: serde::Serialize + serde::de::DeserializeOwned + PartialEq + std::fmt::Debug>(
    v: Value,
) -> T {
    let t: T = serde_json::from_value(v.clone()).expect("deserializes");
    assert_eq!(
        serde_json::to_value(&t).unwrap(),
        v,
        "re-serializes identically"
    );
    t
}

#[test]
fn protocol_version_is_0_4_or_later() {
    let (major, minor) = PROTOCOL_VERSION.split_once('.').expect("MAJOR.MINOR");
    assert_eq!(major, "0");
    assert!(minor.parse::<u32>().unwrap() >= 4, "{PROTOCOL_VERSION}");
}

#[test]
fn m4_methods_and_notifications_are_registered() {
    let methods = [
        (lsp::Servers::NAME, "lsp/servers"),
        (lsp::Request::NAME, "lsp/request"),
        (lsp::Notify::NAME, "lsp/notify"),
        (lsp::Restart::NAME, "lsp/restart"),
        (terminal::Create::NAME, "terminal/create"),
        (terminal::Write::NAME, "terminal/write"),
        (terminal::Resize::NAME, "terminal/resize"),
        (terminal::Close::NAME, "terminal/close"),
        (terminal::List::NAME, "terminal/list"),
        (git::Status::NAME, "git/status"),
        (git::Show::NAME, "git/show"),
        (workspace::Search::NAME, "workspace/search"),
    ];
    let names = registry::method_names();
    for (name, wire) in methods {
        assert_eq!(name, wire);
        assert!(names.contains(&name), "{name} not registered");
    }
    let mut g = schemars::SchemaGenerator::default();
    let notes: Vec<_> = registry::notifications(&mut g)
        .into_iter()
        .map(|n| n.name)
        .collect();
    for (name, wire) in [
        (lsp::LspNotification::NAME, "lsp/notification"),
        (lsp::ServerStateChanged::NAME, "lsp/serverState"),
        (terminal::Output::NAME, "terminal/output"),
        (terminal::Exited::NAME, "terminal/exited"),
        (git::DidChange::NAME, "git/didChange"),
    ] {
        assert_eq!(name, wire);
        assert!(notes.contains(&name), "{name} not registered");
    }
}

#[test]
fn m4_error_codes() {
    let cases = [
        (ErrorKind::LspError, -32026, "lsp_error"),
        (ErrorKind::ServerUnavailable, -32027, "server_unavailable"),
        (ErrorKind::GitUnavailable, -32028, "git_unavailable"),
    ];
    for (kind, code, name) in cases {
        assert_eq!(kind.code(), code);
        let v = serde_json::to_value(RpcError::new(kind, "x")).unwrap();
        assert_eq!(v["code"], code);
        assert_eq!(v["data"]["kind"], name);
        let back: RpcError = serde_json::from_value(v).unwrap();
        assert_eq!(back.data.kind, kind);
    }
}

#[test]
fn lsp_shapes() {
    let s: lsp::ServersResult = round_trip(json!({"servers":[
        {"serverId":"rust-analyzer","providerId":"rust","languages":["rust"],"state":"ready",
         "capabilities":{"hoverProvider":true}},
        {"serverId":"typescript-language-server","providerId":"node","languages":["typescript","javascript"],
         "state":"missing","detail":"typescript-language-server was not found.",
         "installHint":"npm install -g typescript-language-server typescript"}
    ]}));
    assert_eq!(s.servers[1].state, lsp::ServerState::Missing);
    let future: lsp::ServerState = serde_json::from_value(json!("paused")).unwrap();
    assert_eq!(future, lsp::ServerState::Unknown);
    round_trip::<lsp::RequestParams>(json!({"workspaceId":"w1","serverId":"rust-analyzer",
        "method":"textDocument/hover","params":{"textDocument":{"uri":"file:///p/src/main.rs"},"position":{"line":1,"character":4}}}));
    round_trip::<lsp::RequestResult>(json!({"result":null}));
    round_trip::<lsp::NotifyParams>(
        json!({"workspaceId":"w1","serverId":"rust-analyzer","method":"textDocument/didClose","params":{"textDocument":{"uri":"file:///p/a.rs"}}}),
    );
    round_trip::<lsp::LspNotificationParams>(json!({"workspaceId":"w1","serverId":"rust-analyzer",
        "method":"textDocument/publishDiagnostics","params":{"uri":"file:///p/src/main.rs","diagnostics":[]}}));
    round_trip::<lsp::ServerStateParams>(json!({"workspaceId":"w1","server":
        {"serverId":"rust-analyzer","providerId":"rust","languages":["rust"],"state":"failed","detail":"It crashed 3 times."}}));
}

#[test]
fn terminal_shapes() {
    round_trip::<terminal::CreateParams>(json!({"workspaceId":"w1","cols":80,"rows":24}));
    round_trip::<terminal::CreateParams>(
        json!({"workspaceId":"w1","argv":["sh","-i"],"cols":120,"rows":30}),
    );
    round_trip::<terminal::TerminalInfo>(
        json!({"terminalId":"t1","workspaceId":"w1","title":"zsh","pid":4242,"running":true}),
    );
    round_trip::<terminal::WriteParams>(json!({"terminalId":"t1","data":"ls\r"}));
    round_trip::<terminal::ResizeParams>(json!({"terminalId":"t1","cols":100,"rows":40}));
    let l: terminal::ListResult =
        round_trip(json!({"terminals":[{"terminalId":"t1","workspaceId":"w1",
        "title":"zsh","running":false,"exitCode":0,"backlog":"$ ls\r\n","seq":3}]}));
    assert_eq!(l.terminals[0].info.exit_code, Some(0));
    round_trip::<terminal::OutputParams>(json!({"terminalId":"t1","seq":4,"data":"π\r\n"}));
    round_trip::<terminal::ExitedParams>(json!({"terminalId":"t1","code":2}));
    round_trip::<terminal::ExitedParams>(json!({"terminalId":"t1"}));
}

#[test]
fn git_shapes() {
    let s: git::GitStatus = round_trip(json!({"repo":true,"branch":"main","head":"5493f5e",
        "upstream":"origin/main","ahead":1,"behind":0,"files":[
        {"path":"src/a.rs","index":"unmodified","worktree":"modified"},
        {"path":"src/b.rs","index":"renamed","worktree":"unmodified","origPath":"src/old.rs"},
        {"path":"new.txt","index":"untracked","worktree":"untracked"}
    ]}));
    assert_eq!(s.files[1].index, git::GitFileState::Renamed);
    round_trip::<git::GitStatus>(
        json!({"repo":false,"reason":"Not a Git repository.","ahead":0,"behind":0,"files":[]}),
    );
    round_trip::<git::ShowParams>(json!({"workspaceId":"w1","path":"src/a.rs","rev":"head"}));
    round_trip::<git::ShowResult>(json!({"text":"fn a() {}\n","binary":false}));
    round_trip::<git::ShowResult>(json!({"binary":true}));
}

#[test]
fn search_shapes() {
    let p: workspace::SearchParams =
        serde_json::from_value(json!({"workspaceId":"w1","query":"fn main"})).unwrap();
    assert_eq!(p.mode, workspace::SearchMode::Text);
    assert!(!p.regex && !p.case_sensitive && !p.whole_word);
    round_trip::<workspace::SearchParams>(json!({"workspaceId":"w1","query":"a.c","mode":"text",
        "regex":true,"caseSensitive":true,"wholeWord":false,"include":"src/**","exclude":"target/**","limit":100}));
    round_trip::<workspace::SearchResult>(
        json!({"matches":[{"path":"src/main.rs","line":1,"column":1,
        "endColumn":8,"preview":"fn main() {","previewColumn":1}],"files":[],"truncated":false,"filesSearched":3}),
    );
    round_trip::<workspace::SearchResult>(
        json!({"matches":[],"files":["src/main.rs","Cargo.toml"],"truncated":true,"filesSearched":2}),
    );
}
