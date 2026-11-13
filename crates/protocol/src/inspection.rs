//! Framework-independent UI inspection model (the Visual Intelligence
//! contract). Normative prose: `docs/protocols/inspection.md`.
//!
//! DRAFT 0.1: validated by experiments E2 (QML) and E3 (React/DOM); frozen
//! when milestone M5 starts. Not served by the 0.1 daemon.
//!
//! Coordinates: every rectangle is in *logical pixels* of its [`Surface`],
//! origin at the surface's top-left, y down, after all transforms and
//! scrolling (i.e. where the node is actually painted). Screenshot pixels =
//! logical pixels x `Surface::device_pixel_ratio`.

use crate::diagnostics::Location;
use crate::lifecycle::PeerInfo;
use crate::{method, notification};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Node identity within one adapter connection. Valid until the node is
/// destroyed; never reused within a connection. Not stable across restarts.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct NodeId(pub String);

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct SurfaceId(pub String);

/// What an adapter can do. Sent once in `inspect/hello`. Consumers must
/// degrade gracefully on every `false`/`None`; nothing here is mandatory
/// except `tree`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Capabilities {
    /// Framework identifier: `dom`, `react`, `qml`, `flutter`, ...
    pub framework: String,
    /// Detected framework/runtime version, if known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub framework_version: Option<String>,
    pub adapter: PeerInfo,
    /// Level 2: can produce a node tree with bounds.
    pub tree: bool,
    /// Can map a point to the top-most node itself (otherwise the daemon
    /// hit-tests against the last snapshot's bounds).
    pub hit_test: bool,
    pub properties: bool,
    pub accessibility: bool,
    /// Level 3: best precision of source locations this adapter can report.
    pub source: SourcePrecision,
    /// Can draw a highlight overlay on a node in the running app.
    pub highlight: bool,
    /// Can capture pixels itself. If false the preview host captures.
    pub screenshot: bool,
    /// Emits `inspect/didMutate`.
    pub mutations: bool,
}

#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, JsonSchema,
)]
#[serde(rename_all = "camelCase")]
pub enum SourcePrecision {
    None,
    File,
    Line,
    LineColumn,
}

/// A top-level rendering target: a window, a browser page, an iframe, or an
/// embedded foreign surface (e.g. a web view inside a native window).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Surface {
    pub id: SurfaceId,
    pub kind: SurfaceKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    /// Visible size in logical pixels.
    pub width: f64,
    pub height: f64,
    pub device_pixel_ratio: f64,
    /// Scroll offset of the surface's root scroller, logical pixels.
    #[serde(default)]
    pub scroll_x: f64,
    #[serde(default)]
    pub scroll_y: f64,
    /// For nested surfaces: the node in the parent surface that hosts this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host_node: Option<NodeId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum SurfaceKind {
    Window,
    Page,
    Frame,
    Embedded,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// One element of the running UI.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UiNode {
    pub id: NodeId,
    pub surface: SurfaceId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<NodeId>,
    /// Children in paint order (last is top-most).
    #[serde(default)]
    pub children: Vec<NodeId>,
    /// Framework type as the developer wrote it where possible: `button`,
    /// `BalanceCard`, `QQuickRectangle`.
    pub type_name: String,
    pub kind: NodeKind,
    /// Developer-assigned identifier if any: DOM `id`, `data-testid`,
    /// QML `objectName`/`id`, accessibility identifier.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Own rendered text, truncated to 200 characters.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    /// Axis-aligned bounding box. Absent for nodes that render nothing
    /// (pure components, `display: contents`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bounds: Option<Rect>,
    pub visibility: Visibility,
    /// Candidate source locations, best first. Empty when unknown. More than
    /// one entry means the mapping is ambiguous or multi-faceted; see
    /// [`SourceRelation`].
    #[serde(default)]
    pub sources: Vec<SourceCandidate>,
    /// Identity intended to survive a rebuild/reload of unchanged source: see
    /// `docs/protocols/inspection.md#stable-keys`. Absent if the adapter
    /// cannot compute one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stable_key: Option<String>,
    /// Set when several live nodes share this node's primary source
    /// location (list items, repeated components).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub instance: Option<Instance>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub accessibility: Option<Accessibility>,
    /// Layout-relevant runtime properties, framework vocabulary preserved
    /// (CSS computed style subset, QML properties...). Only present in
    /// `inspect/nodeDetails` results.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub properties: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum NodeKind {
    /// Renders pixels itself (DOM element, QQuickItem, View).
    Element,
    /// Logical grouping with no pixels of its own (React component).
    Component,
    Text,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Visibility {
    /// The framework considers the node visible (not hidden, non-zero size,
    /// opacity > 0, attached).
    pub visible: bool,
    /// Part of the node lies outside the surface or an ancestor's clip.
    #[serde(default)]
    pub clipped: bool,
    /// Whether another node paints over this one's centre point.
    /// `None` = not determined.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub occluded: Option<bool>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct Instance {
    /// 0-based position among live nodes with the same primary source.
    pub index: u32,
    pub count: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Accessibility {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SourceCandidate {
    pub location: Location,
    pub relation: SourceRelation,
    pub method: MappingMethod,
    /// 0.0..=1.0. Must not exceed `method.max_confidence()`.
    pub confidence: f32,
    /// Location is in generated or third-party code (not a file the user
    /// would edit). Such candidates are never auto-selected for patching.
    #[serde(default)]
    pub generated: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum SourceRelation {
    /// Where this element is written (the JSX tag, the QML object literal).
    Instantiation,
    /// Where the component type of this element is defined.
    Definition,
    /// The component that rendered this element (owner).
    Owner,
    /// A style rule affecting this element.
    Style,
}

/// How a source location was obtained. Ordered from most to least reliable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum MappingMethod {
    /// Stamped at build time by Axiom instrumentation (E3: `data-axiom-src`).
    BuildInstrumentation,
    /// Recorded by the framework's own compiler/runtime (E2: QQmlData).
    RuntimeMetadata,
    /// Derived through a source map from a generated location.
    SourceMap,
    /// Matched by name/text/structure search in the workspace.
    Heuristic,
    /// Inferred by a model from pixels. Always requires confirmation.
    VisualInference,
}

impl MappingMethod {
    /// Upper bound adapters and the daemon must respect.
    pub const fn max_confidence(self) -> f32 {
        match self {
            Self::BuildInstrumentation | Self::RuntimeMetadata => 1.0,
            Self::SourceMap => 0.9,
            Self::Heuristic => 0.6,
            Self::VisualInference => 0.4,
        }
    }
}

// ---- Adapter protocol (daemon is the client, the runtime adapter the server) ----

method!(
    /// First request on an adapter connection.
    Hello, "inspect/hello", HelloParams => Capabilities
);
method!(
    /// Captures the node tree. Errors: `too_large` is never returned; instead
    /// the adapter truncates depth-first and sets `truncated`.
    TakeSnapshot, "inspect/snapshot", SnapshotParams => Snapshot
);
method!(
    /// Top-most node at a point plus its ancestor chain (nearest first).
    /// Errors: `not_found` if nothing is at the point, `unsupported`.
    HitTest, "inspect/hitTest", HitTestParams => HitTestResult
);
method!(
    /// Full detail (including `properties`) for one node. Errors: `not_found`
    /// if the node no longer exists.
    NodeDetails, "inspect/nodeDetails", NodeRef => UiNode
);
method!(
    /// Shows (`node` set) or clears (`node` absent) the highlight overlay.
    Highlight, "inspect/highlight", HighlightParams => crate::lifecycle::Empty
);
notification!(
    /// Adapter to daemon: the tree changed; previous snapshots are stale.
    DidMutate,
    "inspect/didMutate",
    DidMutateParams
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct HelloParams {
    pub protocol_version: String,
    /// Session token issued by the daemon when the preview was launched.
    pub token: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SnapshotParams {
    /// Limit to the subtree under this node; whole UI if absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub root: Option<NodeId>,
    /// Maximum number of nodes to return. Default 5000.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_nodes: Option<u32>,
    /// Include nodes the framework considers invisible. Default false.
    #[serde(default)]
    pub include_invisible: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Snapshot {
    /// Milliseconds since the Unix epoch, adapter clock.
    pub captured_ms: u64,
    pub surfaces: Vec<Surface>,
    /// Flat list, parents before children.
    pub nodes: Vec<UiNode>,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HitTestParams {
    pub surface: SurfaceId,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HitTestResult {
    /// `chain[0]` is the hit node; the rest are its ancestors up to the root.
    pub chain: Vec<UiNode>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct NodeRef {
    pub node: NodeId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct HighlightParams {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node: Option<NodeId>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct DidMutateParams {
    /// Coarse reason, for diagnostics only.
    pub reason: MutationReason,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum MutationReason {
    Tree,
    Layout,
    Navigation,
    HotReload,
}

// ---- Verification (daemon-side result model) ----

/// Outcome of the before/after comparison in the closed-loop workflow.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct VerificationReport {
    pub verdict: Verdict,
    pub checks: Vec<Check>,
    /// Bounding box of changed pixels (logical px), if screenshots differ.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_region: Option<Rect>,
    /// Fraction of pixels that differ, 0.0..=1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub changed_fraction: Option<f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Verdict {
    /// Every check passed and at least one check directly tested the
    /// requested outcome.
    Verified,
    /// At least one check failed.
    Failed,
    /// Nothing failed, but the requested outcome could not be tested
    /// objectively (e.g. the selected node could not be re-identified).
    Inconclusive,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Check {
    pub kind: CheckKind,
    pub passed: bool,
    /// Human-readable one-liner: what was expected and what was observed.
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expected: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actual: Option<Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum CheckKind {
    /// The build/reload finished without new error diagnostics.
    BuildClean,
    /// The app produced no new runtime errors after reload.
    RuntimeClean,
    /// The selected node was found again via its stable key.
    NodeReidentified,
    /// A geometric assertion on node bounds (the agent states these up
    /// front, e.g. "left edge of A equals left edge of B").
    Geometry,
    /// Pixels changed inside the expected region.
    PixelsChangedInRegion,
    /// Pixels did not change outside the expected region.
    PixelsStableElsewhere,
    /// A property/computed-style assertion.
    Property,
}
