//! Normalised build/tool diagnostics. Produced by diagnostic parsers in the
//! build engine (M2) and consumed by the Problems view, agents and the
//! verification pipeline. Frozen for M2 (M2-ARCH-001): carried by `build/diagnostics`.
//! A diagnostic located outside the workspace (a dependency's source, a toolchain file)
//! has no `location`; its text stays in `message`/`rendered`.

use crate::WorkspacePath;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostic {
    pub severity: Severity,
    /// One-line message without location prefix.
    pub message: String,
    /// Tool-specific code, e.g. `E0308`, `-Wunused-variable`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
    /// Producing tool, e.g. `rustc`, `clang`, `tsc`.
    pub source: String,
    /// Primary location. Absent for diagnostics not tied to a file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<Location>,
    /// Secondary locations ("defined here", notes).
    #[serde(default)]
    pub related: Vec<RelatedInfo>,
    /// The tool's own human-readable rendering, if it provides one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendered: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
    Hint,
}

/// A span in a workspace file. Lines and columns are 1-based; columns count
/// Unicode scalar values. `end` is exclusive and may be absent (point).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct Location {
    pub file: WorkspacePath,
    pub line: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_line: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_column: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct RelatedInfo {
    pub message: String,
    pub location: Location,
}
