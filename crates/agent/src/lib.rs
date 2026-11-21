//! The agent runtime (ADR-0007, ADR-0015; contract `docs/protocols/agents.md`): model providers,
//! and (in later M3 tasks) the tool registry, permission gate, checkpoints, sessions and loop.
//! Depends only on `protocol` and `store`: tools reach the platform services through the
//! implementations `axiomd` registers.

pub mod credentials;
pub mod model;
pub mod providers;
pub mod tools;
