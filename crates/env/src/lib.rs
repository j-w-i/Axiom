//! `axiom-env`: toolchain providers (ADR-0006, ADR-0014). Normative prose:
//! `docs/protocols/providers.md`.
//!
//! - [`manifest`]: `provider.toml` parsing and validation beyond serde.
//! - [`registry`]: built-in, user and workspace providers, override order and trust.
//! - [`detect`]: which providers apply to a workspace and which of their tools are present.

pub mod detect;
pub mod manifest;
pub mod registry;

pub use detect::{detect, HostPaths};
pub use registry::{Builtin, Registry, Sources};
