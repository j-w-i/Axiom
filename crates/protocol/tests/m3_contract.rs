//! Contract tests for the protocol 0.3 additions (ADR-0015, M3-ARCH-001). Same rule as
//! `contract.rs`: a failure means the wire changed; follow `governance.md`, do not edit the JSON.

use axiom_protocol::agent::{self, *};
use axiom_protocol::envelope::{ErrorKind, RpcError};
use axiom_protocol::lifecycle::InitializeParams;
use axiom_protocol::method::{Method, Notification};
use axiom_protocol::{registry, PROTOCOL_VERSION};
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
fn protocol_version_is_0_3() {
    assert_eq!(PROTOCOL_VERSION, "0.3");
}

#[test]
fn m3_methods_and_notifications_are_registered() {
    let expected = [
        (agent::Models::NAME, "agent/models"),
        (agent::ConfigureProvider::NAME, "agent/configureProvider"),
        (agent::SessionCreate::NAME, "agent/sessionCreate"),
        (agent::SessionList::NAME, "agent/sessionList"),
        (agent::SessionOpen::NAME, "agent/sessionOpen"),
        (agent::Send::NAME, "agent/send"),
        (agent::Cancel::NAME, "agent/cancel"),
        (agent::Approve::NAME, "agent/approve"),
        (agent::Checkpoints::NAME, "agent/checkpoints"),
        (agent::Restore::NAME, "agent/restore"),
        (agent::Tools::NAME, "agent/tools"),
        (agent::ToolCall::NAME, "agent/toolCall"),
        (agent::GetPolicy::NAME, "agent/policy"),
        (agent::SessionRename::NAME, "agent/sessionRename"),
        (agent::SessionDelete::NAME, "agent/sessionDelete"),
        (agent::Image::NAME, "agent/image"),
        (agent::SetPolicy::NAME, "agent/setPolicy"),
    ];
    let names = registry::method_names();
    for (name, wire) in expected {
        assert_eq!(name, wire);
        assert!(names.contains(&name), "{name} not registered");
    }
    let mut g = schemars::SchemaGenerator::default();
    let notes: Vec<_> = registry::notifications(&mut g)
        .into_iter()
        .map(|n| n.name)
        .collect();
    for (name, wire) in [
        (agent::Event::NAME, "agent/event"),
        (agent::ApprovalRequested::NAME, "agent/approvalRequested"),
        (agent::ApprovalResolved::NAME, "agent/approvalResolved"),
        (agent::Reveal::NAME, "agent/reveal"),
    ] {
        assert_eq!(name, wire);
        assert!(notes.contains(&name), "{name} not registered");
    }
}

#[test]
fn m3_error_codes() {
    let cases = [
        (ErrorKind::ApprovalRequired, -32023, "approval_required"),
        (ErrorKind::CheckpointConflict, -32024, "checkpoint_conflict"),
        (ErrorKind::ModelError, -32025, "model_error"),
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
fn initialize_params_0_1_still_parse_and_0_3_fields_are_optional() {
    let old: InitializeParams = round_trip(
        json!({"protocolVersion":"0.1","client":{"name":"axiom-desktop","version":"0.1.0"}}),
    );
    assert!(old.auth_token.is_none() && old.capabilities.is_none());
    let new: InitializeParams = round_trip(json!({
        "protocolVersion":"0.3","client":{"name":"axiom","version":"0.1.0"},
        "authToken":"abc","capabilities":{"approvals":true}
    }));
    assert_eq!(new.auth_token.as_deref(), Some("abc"));
    assert!(new.capabilities.unwrap().approvals);
}

#[test]
fn provider_config_wire_shape() {
    let c: ProviderConfig = round_trip(json!({
        "id":"local","kind":"openaiChat","name":"Ollama","baseUrl":"http://127.0.0.1:11434/v1",
        "models":["qwen3:8b"],"capabilities":{"tools":"yes","contextWindow":32768}
    }));
    assert_eq!(c.kind, ProviderKind::OpenaiChat);
    round_trip::<ProviderConfig>(json!({"id":"a","kind":"anthropic","name":"Anthropic"}));
    round_trip::<ConfigureProviderParams>(json!({"id":"a","apiKey":"k","clearKey":true}));
    round_trip::<ModelsResult>(json!({"providers":[{
        "config":{"id":"a","kind":"anthropic","name":"Anthropic"},"hasKey":false,
        "models":[{"id":"m","displayName":"M","capabilities":{
            "tools":"yes","streamingToolArgs":"yes","parallelToolCalls":"unknown","images":"no"}}],
        "error":"Authentication failed."
    }]}));
}

#[test]
fn session_wire_shapes() {
    round_trip::<SessionCreateParams>(
        json!({"workspaceId":"w1","providerId":"a","model":"m","title":"Fix"}),
    );
    round_trip::<SessionInfo>(json!({
        "sessionId":"s1","title":"Fix","providerId":"a","model":"m",
        "createdMs":1,"updatedMs":2,"running":false,"pendingApprovals":0
    }));
    round_trip::<SendParams>(json!({
        "workspaceId":"w1","sessionId":"s1","text":"hi",
        "attachments":[{"path":"src/a.rs","lines":[3,9]}]
    }));
    round_trip::<RestoreParams>(
        json!({"workspaceId":"w1","sessionId":"s1","checkpoint":2,"skipConflicts":true}),
    );
    round_trip::<RestoreResult>(json!({
        "checkpoint":2,"undoCheckpoint":5,"restored":["src/a.rs"],
        "conflicts":[{"path":"src/b.rs","reason":"modifiedSinceAgent"}]
    }));
}

#[test]
fn event_wire_shapes() {
    let cases = [
        json!({"type":"user","text":"add a function"}),
        json!({"type":"textDelta","text":"Sure"}),
        json!({"type":"assistantText","text":"Sure."}),
        json!({"type":"toolCall","callId":"c1","tool":"fs.applyPatch","input":{"path":"a"},
               "effect":"writeWorkspace","targets":["src/a.rs"]}),
        json!({"type":"toolResult","callId":"c1","isError":false,"summary":"+3 −1",
               "output":{"version":"v2"},"truncated":true,"rule":"write_workspace → ask"}),
        json!({"type":"interrupted","callId":"c1"}),
        json!({"type":"checkpoint","checkpoint":{"number":1,"label":"before fs.applyPatch",
               "createdMs":5,"turn":1}}),
        json!({"type":"compaction","throughSeq":40,"summary":"…"}),
        json!({"type":"usage","inputTokens":100,"outputTokens":20,"cachedInputTokens":80}),
        json!({"type":"turnEnd","reason":"endTurn"}),
        json!({"type":"error","kind":"model_error","modelKind":"rateLimited","message":"Slow down."}),
        json!({"type":"approvalResolved","resolution":{"requestId":"r1","decision":"session"}}),
    ];
    for body in cases {
        round_trip::<AgentEvent>(json!({"sessionId":"s1","seq":3,"tsMs":9,"turn":1,"event":body}));
    }
}

#[test]
fn approval_and_policy_wire_shapes() {
    round_trip::<ApprovalRequest>(json!({
        "requestId":"r1","sessionId":"s1","principal":"session:s1","tool":"task.run",
        "effect":"execute","targets":["cargo build"],"argv":["cargo","build"],
        "checkpointPlanned":true,"canRemember":true
    }));
    round_trip::<ApproveParams>(json!({"requestId":"r1","decision":"once"}));
    round_trip::<ApprovalResolution>(json!({"requestId":"r1"}));
    let p: Policy = round_trip(json!({"default":"ask","rules":[
        {"effect":"read","decision":"allow"},
        {"effect":"execute","argvPrefix":["cargo","build"],"decision":"allow"},
        {"tool":"mcp.github.*","decision":"deny"},
        {"effect":"writeWorkspace","path":"src/**","decision":"ask"}
    ]}));
    assert_eq!(p.rules.len(), 4);
    round_trip::<ToolCallParams>(json!({
        "workspaceId":"w1","tool":"fs.read","input":{"path":"a"},"principal":"mcp:claude-code"
    }));
    round_trip::<ToolCallResult>(json!({"output":{},"text":"","isError":true}));
    round_trip::<RevealParams>(json!({"workspaceId":"w1","path":"a","line":3,"column":1}));
}

/// M3-ARCH-002 (ADR-0016, ADR-0017): workspace-free sessions, rename/delete, edit and regenerate,
/// images, ACP providers. Every shape accepted before still parses.
#[test]
fn chat_additions_wire_shapes() {
    // Without a workspace.
    round_trip::<SessionCreateParams>(json!({"providerId":"a","model":"m"}));
    round_trip::<SessionRef>(json!({"sessionId":"s1"}));
    round_trip::<SessionListParams>(json!({}));
    // The workspace-ref shape of 0.3 as frozen by ARCH-001 still parses as the list params.
    let p: SessionListParams = round_trip(json!({"workspaceId":"w1"}));
    assert_eq!(p.workspace_id.unwrap().0, "w1");
    round_trip::<SessionRenameParams>(json!({"sessionId":"s1","title":"Trip plan"}));
    round_trip::<SendParams>(json!({
        "sessionId":"s1","text":"again","fromTurn":3,
        "images":[{"mediaType":"image/png","data":"iVBORw0KGgo="}]
    }));
    round_trip::<ImageParams>(json!({"sessionId":"s1","sha256":"ab"}));
    round_trip::<ImageData>(json!({"mediaType":"image/png","data":"iVBORw0KGgo="}));
    for body in [
        json!({"type":"rewind","fromTurn":3}),
        json!({"type":"user","text":"look","images":[{"mediaType":"image/png","sha256":"ab","bytes":12}]}),
        json!({"type":"assistantText","text":"1. read","kind":"plan"}),
    ] {
        round_trip::<AgentEvent>(json!({"sessionId":"s1","seq":1,"tsMs":1,"turn":1,"event":body}));
    }
    let c: ProviderConfig = round_trip(json!({
        "id":"claude-code","kind":"acp","name":"Claude Code","command":["claude-agent-acp"],
        "env":{"NO_COLOR":"1"}
    }));
    assert_eq!(c.kind, ProviderKind::Acp);
}
