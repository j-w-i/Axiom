//! The tool registry (`docs/protocols/agents.md` §Tools, §Execution pipeline). Tools are typed
//! operations with a declared effect; their implementations are registered by `axiomd` and call
//! the platform services. A call has two phases:
//!
//! 1. [`Registry::prepare`]: validate the input and resolve its targets (paths, argv, the diff a
//!    patch would make). Nothing is changed.
//! 2. [`Registry::execute`]: run it. This needs an [`Allowed`] token, which only the permission
//!    gate creates, so no path can run a tool without a gate decision.
//!
//! Results are bounded (`maxResultBytes`, default 32 KiB) with a `truncated` flag; errors are
//! structured (`kind`, one-sentence `message`, a `hint` for the model).

pub mod inputs;
pub mod patch;

use axiom_protocol::agent::{Effect, ToolInfo};
use axiom_protocol::{ErrorKind, WorkspaceId, WorkspacePath};
use futures_util::future::BoxFuture;
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub const DEFAULT_MAX_RESULT_BYTES: usize = 32 * 1024;

#[derive(Debug, Clone)]
pub struct ToolSpec {
    pub name: String,
    pub description: String,
    pub input_schema: Value,
    pub effect: Effect,
    pub max_result_bytes: usize,
}

/// Who is calling and where.
#[derive(Debug, Clone)]
pub struct ToolCtx {
    pub workspace_id: WorkspaceId,
    /// `session:<id>`, `mcp:<client>`, `cli`.
    pub principal: String,
    pub cancel: CancellationToken,
}

/// What a call will touch, for the gate and the approval card.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Targets {
    /// Workspace paths read or written.
    pub paths: Vec<WorkspacePath>,
    /// For `execute`: the argv.
    pub argv: Option<Vec<String>>,
    /// For edits: the unified diff that would be applied.
    pub preview: Option<String>,
    /// File versions the approval saw; execution refuses if a file changed since.
    pub versions: Vec<(WorkspacePath, Option<axiom_protocol::fs::FileVersion>)>,
    /// This call's effect when it is more than the tool's (an `fs.applyPatch` delete).
    pub effect: Option<Effect>,
}

impl Targets {
    /// Display strings: the paths, or the argv joined.
    pub fn display(&self) -> Vec<String> {
        let mut v: Vec<String> = self.paths.iter().map(|p| p.0.clone()).collect();
        if let Some(a) = &self.argv {
            v.push(a.join(" "));
        }
        v
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolOutput {
    /// The structured result (what clients render).
    pub output: Value,
    /// The result as text for the model.
    pub text: String,
    /// One line for a tool row.
    pub summary: String,
    pub truncated: bool,
}

impl ToolOutput {
    /// `output` as the model text too (pretty JSON).
    pub fn json(output: Value, summary: impl Into<String>) -> Self {
        let text = serde_json::to_string_pretty(&output).unwrap_or_default();
        Self {
            output,
            text,
            summary: summary.into(),
            truncated: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ToolError {
    pub kind: ErrorKind,
    /// One sentence.
    pub message: String,
    /// What the model should do differently.
    pub hint: Option<String>,
}

impl ToolError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            hint: None,
        }
    }
    pub fn hint(mut self, h: impl Into<String>) -> Self {
        self.hint = Some(h.into());
        self
    }
    pub fn invalid(message: impl Into<String>) -> Self {
        Self::new(ErrorKind::InvalidParams, message)
    }
    /// `{kind, message, hint}` with `kind` as its wire name.
    pub fn to_value(&self) -> Value {
        let kind = serde_json::to_value(self.kind).unwrap_or(json!("unknown"));
        let mut v = json!({"kind": kind, "message": self.message});
        if let Some(h) = &self.hint {
            v["hint"] = json!(h);
        }
        v
    }
    /// The error as a tool result (the model sees it; it is not a protocol error).
    pub fn into_output(self) -> ToolOutput {
        let mut text = format!("Error ({}): {}", self.kind_name(), self.message);
        if let Some(h) = &self.hint {
            text.push_str("\nHint: ");
            text.push_str(h);
        }
        ToolOutput {
            summary: self.message.clone(),
            output: self.to_value(),
            text,
            truncated: false,
        }
    }
    pub fn kind_name(&self) -> String {
        serde_json::to_value(self.kind)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| "unknown".into())
    }
}

/// Proof that the permission gate allowed this call. Only the gate can create one.
#[derive(Debug)]
pub struct Allowed {
    _private: (),
}

impl Allowed {
    /// Only the permission gate (`gate::Gate::check`) calls this.
    pub(crate) fn granted() -> Self {
        Self { _private: () }
    }

    /// TEST ONLY (feature `test-support`, enabled by dev-dependencies): run a tool without the
    /// gate, to test the tool itself.
    #[cfg(any(test, feature = "test-support"))]
    pub fn for_tests() -> Self {
        Self::granted()
    }
}

pub trait Tool: Send + Sync {
    fn spec(&self) -> &ToolSpec;
    /// Validates `input` and works out what it touches. Must not change anything.
    fn prepare<'a>(
        &'a self,
        ctx: &'a ToolCtx,
        input: &'a Value,
    ) -> BoxFuture<'a, Result<Targets, ToolError>>;
    /// Runs the call; `targets` are what `prepare` returned (and the gate approved).
    fn execute<'a>(
        &'a self,
        ctx: &'a ToolCtx,
        input: Value,
        targets: &'a Targets,
        allowed: Allowed,
    ) -> BoxFuture<'a, Result<ToolOutput, ToolError>>;
}

/// A validated call waiting for the gate.
pub struct Prepared {
    pub tool: Arc<dyn Tool>,
    pub name: String,
    pub effect: Effect,
    pub input: Value,
    pub targets: Targets,
}

#[derive(Default, Clone)]
pub struct Registry {
    tools: BTreeMap<String, Arc<dyn Tool>>,
    /// Tools for the daemon's own bridges (ACP file and terminal access): callable and gated
    /// like the others, never offered to a model or listed.
    hidden: std::collections::BTreeSet<String>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// # Panics
    /// If a tool with that name is already registered.
    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.spec().name.clone();
        assert!(
            self.tools.insert(name.clone(), tool).is_none(),
            "tool `{name}` registered twice"
        );
    }

    /// Registers a tool that is never listed or offered (see `hidden`).
    pub fn register_hidden(&mut self, tool: Arc<dyn Tool>) {
        let name = tool.spec().name.clone();
        self.register(tool);
        self.hidden.insert(name);
    }

    pub fn get(&self, name: &str) -> Option<&Arc<dyn Tool>> {
        self.tools.get(name)
    }

    pub fn names(&self) -> Vec<String> {
        self.visible().map(|t| t.spec().name.clone()).collect()
    }

    fn visible(&self) -> impl Iterator<Item = &Arc<dyn Tool>> {
        self.tools
            .iter()
            .filter(|(n, _)| !self.hidden.contains(*n))
            .map(|(_, t)| t)
    }

    /// For `agent/tools` and MCP `tools/list`.
    pub fn infos(&self) -> Vec<ToolInfo> {
        self.visible()
            .map(|t| {
                let s = t.spec();
                ToolInfo {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    input_schema: s.input_schema.clone(),
                    effect: s.effect,
                }
            })
            .collect()
    }

    /// For a model request.
    pub fn defs(&self) -> Vec<crate::model::ToolDef> {
        self.visible()
            .map(|t| {
                let s = t.spec();
                crate::model::ToolDef {
                    name: s.name.clone(),
                    description: s.description.clone(),
                    input_schema: s.input_schema.clone(),
                }
            })
            .collect()
    }

    /// Phase 1: an unknown tool or an invalid input is an error the model sees.
    pub async fn prepare(
        &self,
        ctx: &ToolCtx,
        name: &str,
        input: Value,
    ) -> Result<Prepared, ToolError> {
        let tool = self.tools.get(name).cloned().ok_or_else(|| {
            ToolError::new(ErrorKind::NotFound, format!("There is no tool `{name}`."))
                .hint(format!("Use one of: {}.", self.names().join(", ")))
        })?;
        let input = if input.is_null() { json!({}) } else { input };
        let targets = tool.prepare(ctx, &input).await?;
        Ok(Prepared {
            effect: targets.effect.unwrap_or(tool.spec().effect),
            name: name.to_string(),
            tool,
            input,
            targets,
        })
    }

    /// Phase 2, after the gate allowed it. The result is bounded.
    pub async fn execute(
        &self,
        ctx: &ToolCtx,
        prepared: Prepared,
        allowed: Allowed,
    ) -> Result<ToolOutput, ToolError> {
        let max = prepared.tool.spec().max_result_bytes;
        let out = prepared
            .tool
            .execute(ctx, prepared.input, &prepared.targets, allowed)
            .await?;
        Ok(bound(out, max))
    }
}

/// Deserialises a tool input, mapping failure to `invalid_params` with the reason as the hint.
pub fn parse<T: serde::de::DeserializeOwned>(input: &Value) -> Result<T, ToolError> {
    serde_json::from_value(input.clone()).map_err(|e| {
        ToolError::invalid("The tool input does not match its schema.")
            .hint(format!("{e}. Check the tool's input schema."))
    })
}

/// Cuts `text` to `max` bytes (on a char boundary) and the structured output to the same budget.
pub fn bound(mut out: ToolOutput, max: usize) -> ToolOutput {
    if out.text.len() > max {
        let mut cut = max;
        while !out.text.is_char_boundary(cut) {
            cut -= 1;
        }
        out.text.truncate(cut);
        out.text
            .push_str("\n[truncated: the result is larger than the tool's limit]");
        out.truncated = true;
    }
    let size = serde_json::to_vec(&out.output)
        .map(|v| v.len())
        .unwrap_or(0);
    if size > max * 2 {
        out.output = json!({"truncated": true, "text": out.text});
        out.truncated = true;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::FutureExt;

    struct Echo(ToolSpec);
    impl Tool for Echo {
        fn spec(&self) -> &ToolSpec {
            &self.0
        }
        fn prepare<'a>(
            &'a self,
            _ctx: &'a ToolCtx,
            input: &'a Value,
        ) -> BoxFuture<'a, Result<Targets, ToolError>> {
            async move {
                let i: inputs::ReadInput = parse(input)?;
                Ok(Targets {
                    paths: vec![i.path],
                    ..Default::default()
                })
            }
            .boxed()
        }
        fn execute<'a>(
            &'a self,
            _ctx: &'a ToolCtx,
            input: Value,
            _targets: &'a Targets,
            _allowed: Allowed,
        ) -> BoxFuture<'a, Result<ToolOutput, ToolError>> {
            async move {
                Ok(ToolOutput {
                    text: "x".repeat(100),
                    output: input,
                    summary: "ok".into(),
                    truncated: false,
                })
            }
            .boxed()
        }
    }

    fn reg() -> Registry {
        let mut r = Registry::new();
        r.register(Arc::new(Echo(ToolSpec {
            name: "fs.read".into(),
            description: "Read".into(),
            input_schema: inputs::schema_of::<inputs::ReadInput>(),
            effect: Effect::Read,
            max_result_bytes: 10,
        })));
        r
    }

    fn ctx() -> ToolCtx {
        ToolCtx {
            workspace_id: WorkspaceId("w".into()),
            principal: "test".into(),
            cancel: CancellationToken::new(),
        }
    }

    #[tokio::test]
    async fn tools_unknown_and_invalid_inputs_are_tool_errors_with_hints() {
        let r = reg();
        let e = r.prepare(&ctx(), "fs.nope", json!({})).await.err().unwrap();
        assert_eq!(e.kind, ErrorKind::NotFound);
        assert!(e.hint.unwrap().contains("fs.read"));
        let e = r
            .prepare(&ctx(), "fs.read", json!({"paht": "a"}))
            .await
            .err()
            .unwrap();
        assert_eq!(e.kind, ErrorKind::InvalidParams);
        assert!(e.hint.unwrap().contains("unknown field"));
        assert_eq!(
            e_value(),
            json!({"kind":"invalid_params","message":"m","hint":"h"})
        );
    }

    fn e_value() -> Value {
        ToolError::invalid("m").hint("h").to_value()
    }

    #[tokio::test]
    async fn tools_results_are_bounded() {
        let r = reg();
        let p = r
            .prepare(&ctx(), "fs.read", json!({"path": "a"}))
            .await
            .unwrap();
        assert_eq!(p.targets.paths, [WorkspacePath("a".into())]);
        let out = r.execute(&ctx(), p, Allowed::granted()).await.unwrap();
        assert!(out.truncated);
        assert!(out.text.starts_with("xxxxxxxxxx\n[truncated"));
    }

    #[test]
    fn tools_schemas_are_plain_objects() {
        let s = inputs::schema_of::<inputs::ApplyPatchInput>();
        assert_eq!(s["type"], "object");
        assert!(s.get("$schema").is_none());
        assert!(s["properties"]["edits"].is_object());
        assert_eq!(s["required"], json!(["path"]));
        let e = inputs::schema_of::<inputs::EmptyInput>();
        assert_eq!(e["properties"], json!({}));
    }
}
