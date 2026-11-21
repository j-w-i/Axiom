//! The agent runtime (ADR-0007, ADR-0015; contract `docs/protocols/agents.md`): model providers,
//! and (in later M3 tasks) the tool registry, permission gate, checkpoints, sessions and loop.
//! Depends only on `protocol` and `store`: tools reach the platform services through the
//! implementations `axiomd` registers.

pub mod credentials;
pub mod gate;
pub mod model;
pub mod providers;
pub mod tools;

/// A stable key for a workspace root, used to name its sessions, checkpoints and audit log
/// under the data dir: the first 16 hex digits of the SHA-256 of the root path.
pub fn workspace_key(root: &std::path::Path) -> String {
    use sha2::Digest;
    let d = sha2::Sha256::digest(root.to_string_lossy().as_bytes());
    hex::encode(&d[..8])
}
