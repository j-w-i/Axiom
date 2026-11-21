//! The running UI of a preview, for clients (protocol 0.5, milestone M5; ADR-0019 §3): the
//! Inspector and the agent's `ui.*` tools see the daemon's normalised model (sources resolved to
//! workspace paths, candidates ranked); the adapter's own `inspect/*` protocol stays between the
//! daemon and the page. Types: `crate::inspection`. Normative prose:
//! `docs/protocols/inspection.md`.

use crate::inspection::{
    HitTestResult, MutationReason, NodeId, Snapshot, SurfaceId, UiNode, VerificationReport,
};
use crate::preview::PreviewId;
use crate::{method, notification};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

method!(
    /// Turns inspect mode on or off in the page: on, hovering highlights the element under the
    /// pointer and a click selects it (and is not passed to the app). Errors: `not_found`,
    /// `adapter_unavailable` (no page connected).
    InspectMode, "ui/inspectMode", InspectModeParams => crate::lifecycle::Empty
);
method!(
    /// The page's node tree, normalised. Errors: `not_found`, `adapter_unavailable`, `timeout`.
    TakeSnapshot, "ui/snapshot", SnapshotParams => Snapshot
);
method!(
    /// The node at a point and its ancestors, with source candidates. Errors: `not_found` (also
    /// for empty space), `adapter_unavailable`.
    HitTest, "ui/hitTest", HitTestParams => HitTestResult
);
method!(
    /// One node with its computed properties and source candidates. Errors: `not_found`,
    /// `adapter_unavailable`.
    NodeDetails, "ui/nodeDetails", NodeParams => UiNode
);
method!(
    /// Shows (`node` set) or hides the page's highlight overlay. Errors: `not_found`,
    /// `adapter_unavailable`.
    Highlight, "ui/highlight", HighlightParams => crate::lifecycle::Empty
);
method!(
    /// Evaluates the stated assertions once the page is quiet (no mutation for 150 ms, at most
    /// `timeoutMs`), re-identifying their nodes by stable key, and adds the daemon's own checks
    /// (`buildClean`, `runtimeClean`, `nodeReidentified`; pixel checks are `skipped` in M5).
    /// Errors: `not_found`, `adapter_unavailable`, `invalid_params` (no assertion).
    Verify, "ui/verify", VerifyParams => VerificationReport
);
notification!(
    /// Daemon to client. The person selected an element in the page (inspect mode): the node
    /// and its ancestors, nearest first, with source candidates.
    Selected,
    "ui/selected",
    SelectedParams
);
notification!(
    /// Daemon to client. The element under the pointer in inspect mode (`node` absent: none).
    Hovered,
    "ui/hovered",
    HoveredParams
);
notification!(
    /// Daemon to client. The page changed (reload, hot reload, layout); snapshots are stale.
    DidMutate,
    "ui/didMutate",
    DidMutateParams
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct InspectModeParams {
    pub preview_id: PreviewId,
    pub on: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotParams {
    pub preview_id: PreviewId,
    /// Maximum number of nodes. Default 5000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_nodes: Option<u32>,
    #[serde(default)]
    pub include_invisible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HitTestParams {
    pub preview_id: PreviewId,
    pub surface: SurfaceId,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeParams {
    pub preview_id: PreviewId,
    pub node: NodeId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HighlightParams {
    pub preview_id: PreviewId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
}

/// Which element an assertion is about. After a reload node ids are gone, so the stable key
/// (`<file>:<line>:<column>#<instance>`) is what finds it again; `node` is used only while the
/// page has not changed.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct NodeTarget {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
}

/// A measurement of a node's bounds, in logical pixels of its surface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
    Width,
    Height,
    CenterX,
    CenterY,
}

/// `eq` holds within 0.5 logical px.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum Comparison {
    Eq,
    Lt,
    Le,
    Gt,
    Ge,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct EdgeRef {
    pub node: NodeTarget,
    pub edge: Edge,
}

/// What the agent states, before patching, would show success.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum Assertion {
    /// `node.edge op (other.edge + offset)`, or `node.edge op value`.
    #[serde(rename_all = "camelCase")]
    Geometry {
        node: NodeTarget,
        edge: Edge,
        op: Comparison,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        value: Option<f64>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        other: Option<EdgeRef>,
        #[serde(default)]
        offset: f64,
    },
    /// A computed property (namespaced, e.g. `css.padding-left`) equals `equals`.
    #[serde(rename_all = "camelCase")]
    Property {
        node: NodeTarget,
        name: String,
        equals: Value,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerifyParams {
    pub preview_id: PreviewId,
    pub assertions: Vec<Assertion>,
    /// The element the change was about (re-identification is reported for it).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<NodeTarget>,
    /// How long to wait for the page to settle. Default 5000, at most 30000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
    /// Runtime errors and build errors are counted from this time (ms since the epoch);
    /// default: the previous `ui/verify` or the adapter's connection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SelectedParams {
    pub preview_id: PreviewId,
    /// `chain[0]` is the selected node; the rest its ancestors.
    pub chain: Vec<UiNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HoveredParams {
    pub preview_id: PreviewId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<UiNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DidMutateParams {
    pub preview_id: PreviewId,
    pub reason: MutationReason,
}
