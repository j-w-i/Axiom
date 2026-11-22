//! Enumerates every method and notification so tooling (schema export,
//! `initialize.methods`, contract tests) cannot drift from the definitions.

use crate::method::{Method, Notification};
use schemars::{JsonSchema, Schema, SchemaGenerator};

/// One wire method with the schemas of its payloads.
pub struct MethodSpec {
    pub name: &'static str,
    pub params: Schema,
    pub result: Schema,
}

/// One wire notification with the schema of its payload.
pub struct NotificationSpec {
    pub name: &'static str,
    pub params: Schema,
}

fn m<M: Method>(g: &mut SchemaGenerator) -> MethodSpec
where
    M::Params: JsonSchema,
    M::Result: JsonSchema,
{
    MethodSpec {
        name: M::NAME,
        params: g.subschema_for::<M::Params>(),
        result: g.subschema_for::<M::Result>(),
    }
}

fn n<N: Notification>(g: &mut SchemaGenerator) -> NotificationSpec
where
    N::Params: JsonSchema,
{
    NotificationSpec {
        name: N::NAME,
        params: g.subschema_for::<N::Params>(),
    }
}

/// Every request method in protocol 0.6 (0.1 and the M2–M5 and 0.6 additions). Add new methods
/// here.
pub fn methods(g: &mut SchemaGenerator) -> Vec<MethodSpec> {
    use crate::{agent, build, env, fs, lifecycle, settings, task, workspace};
    vec![
        m::<lifecycle::Initialize>(g),
        m::<lifecycle::Shutdown>(g),
        m::<workspace::Open>(g),
        m::<workspace::Close>(g),
        m::<workspace::Recent>(g),
        m::<workspace::GetState>(g),
        m::<workspace::SetState>(g),
        m::<fs::List>(g),
        m::<fs::ReadFile>(g),
        m::<fs::WriteFile>(g),
        m::<fs::Watch>(g),
        m::<task::Start>(g),
        m::<task::Cancel>(g),
        m::<task::ListTasks>(g),
        m::<settings::Get>(g),
        m::<settings::Set>(g),
        m::<env::Providers>(g),
        m::<env::Detect>(g),
        m::<env::Versions>(g),
        m::<env::PlanInstall>(g),
        m::<env::Install>(g),
        m::<env::Uninstall>(g),
        m::<env::Resolve>(g),
        m::<env::Trust>(g),
        m::<build::Tasks>(g),
        m::<build::Run>(g),
        m::<agent::Models>(g),
        m::<agent::ConfigureProvider>(g),
        m::<agent::SessionCreate>(g),
        m::<agent::SessionList>(g),
        m::<agent::SessionOpen>(g),
        m::<agent::SessionRename>(g),
        m::<agent::SessionDelete>(g),
        m::<agent::Image>(g),
        m::<agent::Send>(g),
        m::<agent::Cancel>(g),
        m::<agent::Approve>(g),
        m::<agent::InstallAgent>(g),
        m::<agent::Checkpoints>(g),
        m::<agent::Restore>(g),
        m::<agent::Tools>(g),
        m::<agent::ToolCall>(g),
        m::<agent::GetPolicy>(g),
        m::<agent::SetPolicy>(g),
        // 0.4 (ADR-0018)
        m::<crate::lsp::Servers>(g),
        m::<crate::lsp::Request>(g),
        m::<crate::lsp::Notify>(g),
        m::<crate::lsp::Restart>(g),
        m::<crate::terminal::Create>(g),
        m::<crate::terminal::Write>(g),
        m::<crate::terminal::Resize>(g),
        m::<crate::terminal::Close>(g),
        m::<crate::terminal::List>(g),
        m::<crate::git::Status>(g),
        m::<crate::git::Show>(g),
        m::<crate::workspace::Search>(g),
        // 0.5 (ADR-0019).
        m::<crate::preview::Start>(g),
        m::<crate::preview::Stop>(g),
        m::<crate::preview::List>(g),
        m::<crate::ui::InspectMode>(g),
        m::<crate::ui::TakeSnapshot>(g),
        m::<crate::ui::HitTest>(g),
        m::<crate::ui::NodeDetails>(g),
        m::<crate::ui::Highlight>(g),
        m::<crate::ui::Verify>(g),
        // 0.6 (audit AUD-022, CR-AUD-022-workspace-file-operations).
        m::<fs::CreateFile>(g),
        m::<fs::CreateDirectory>(g),
        m::<fs::Rename>(g),
        m::<fs::Trash>(g),
    ]
}

/// Every notification in protocol 0.6. Add new notifications here.
pub fn notifications(g: &mut SchemaGenerator) -> Vec<NotificationSpec> {
    use crate::{agent, build, env, fs, lifecycle, settings, task};
    vec![
        n::<lifecycle::Cancel>(g),
        n::<lifecycle::Log>(g),
        n::<fs::DidChange>(g),
        n::<task::Output>(g),
        n::<task::Exited>(g),
        n::<settings::DidChange>(g),
        n::<env::Progress>(g),
        n::<env::Installed>(g),
        n::<build::Diagnostics>(g),
        n::<agent::Event>(g),
        n::<agent::ApprovalRequested>(g),
        n::<agent::ApprovalResolved>(g),
        n::<agent::Reveal>(g),
        n::<crate::lsp::LspNotification>(g),
        n::<crate::lsp::ServerStateChanged>(g),
        n::<crate::terminal::Output>(g),
        n::<crate::terminal::Exited>(g),
        n::<crate::git::DidChange>(g),
        n::<crate::preview::State>(g),
        n::<crate::ui::Selected>(g),
        n::<crate::ui::Hovered>(g),
        n::<crate::ui::DidMutate>(g),
    ]
}

/// Wire names of all request methods, in declaration order.
pub fn method_names() -> Vec<&'static str> {
    methods(&mut SchemaGenerator::default())
        .into_iter()
        .map(|s| s.name)
        .collect()
}

/// The complete protocol as one JSON document (written to
/// `schemas/ipc/protocol.schema.json` by the `export_schema` example).
pub fn protocol_document() -> serde_json::Value {
    let mut g = SchemaGenerator::default();
    let methods: serde_json::Map<_, _> = methods(&mut g)
        .into_iter()
        .map(|s| {
            (
                s.name.to_string(),
                serde_json::json!({ "params": s.params, "result": s.result }),
            )
        })
        .collect();
    let notifications: serde_json::Map<_, _> = notifications(&mut g)
        .into_iter()
        .map(|s| {
            (
                s.name.to_string(),
                serde_json::json!({ "params": s.params }),
            )
        })
        .collect();
    let envelope = g.subschema_for::<crate::envelope::Message>();
    serde_json::json!({
        "$comment": "GENERATED by `cargo run -p axiom-protocol --example export_schema`. Do not edit.",
        "protocolVersion": crate::PROTOCOL_VERSION,
        "envelope": envelope,
        "methods": methods,
        "notifications": notifications,
        "$defs": g.take_definitions(true),
    })
}

/// Schema of `provider.toml` (frozen for M2) (written to `schemas/provider/manifest.schema.json`).
pub fn provider_manifest_document() -> serde_json::Value {
    serde_json::to_value(schemars::schema_for!(crate::provider::ProviderManifest))
        .expect("schema serializes")
}

/// The inspection adapter protocol and data model, frozen for M5 (written to
/// `schemas/inspection/inspection.schema.json`).
pub fn inspection_document() -> serde_json::Value {
    use crate::inspection as i;
    let mut g = SchemaGenerator::default();
    let methods: serde_json::Map<_, _> = [
        m::<i::Hello>(&mut g),
        m::<i::TakeSnapshot>(&mut g),
        m::<i::HitTest>(&mut g),
        m::<i::NodeDetails>(&mut g),
        m::<i::Highlight>(&mut g),
        m::<i::SetMode>(&mut g),
    ]
    .into_iter()
    .map(|s| {
        (
            s.name.to_string(),
            serde_json::json!({ "params": s.params, "result": s.result }),
        )
    })
    .collect();
    let notifications: serde_json::Map<_, _> = [
        n::<i::DidMutate>(&mut g),
        n::<i::AdapterHovered>(&mut g),
        n::<i::AdapterSelected>(&mut g),
        n::<i::RuntimeError>(&mut g),
    ]
    .into_iter()
    .map(|s| {
        (
            s.name.to_string(),
            serde_json::json!({ "params": s.params }),
        )
    })
    .collect();
    let report = g.subschema_for::<i::VerificationReport>();
    let diagnostic = g.subschema_for::<crate::diagnostics::Diagnostic>();
    serde_json::json!({
        "$comment": "GENERATED by `cargo run -p axiom-protocol --example export_schema`. Do not edit. FROZEN for M5.",
        "methods": methods,
        "notifications": notifications,
        "verificationReport": report,
        "diagnostic": diagnostic,
        "$defs": g.take_definitions(true),
    })
}
