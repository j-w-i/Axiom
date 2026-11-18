//! Application-wide settings (not per-workspace). Values are JSON; keys are
//! dot-separated, e.g. `appearance.theme`. The set of known keys and their
//! defaults is defined by the daemon and documented in
//! `docs/protocols/ipc.md`; unknown keys are stored and returned unchanged.

use crate::lifecycle::Empty;
use crate::{method, notification};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

method!(
    /// Reads settings. Known keys that were never set return their default.
    Get, "settings/get", GetParams => GetResult
);
method!(
    /// Sets keys; a `null` value resets the key to its default. Durable
    /// before the response. Errors: `invalid_params` if a known key receives
    /// a value of the wrong type (details: `{ "key": string }`).
    Set, "settings/set", SetParams => Empty
);
notification!(
    /// Daemon to every connected client (including the one that made the
    /// change) after settings change.
    DidChange,
    "settings/didChange",
    GetResult
);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GetParams {
    /// Keys to read; empty means all known and stored keys.
    #[serde(default)]
    pub keys: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct GetResult {
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct SetParams {
    pub values: BTreeMap<String, Value>,
}
