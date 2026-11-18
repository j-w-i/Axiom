//! `axiom-env`: toolchain providers (ADR-0006, ADR-0014). Normative prose:
//! `docs/protocols/providers.md`.
//!
//! - [`manifest`]: `provider.toml` parsing and validation beyond serde.
//! - [`registry`]: built-in, user and workspace providers, override order and trust.
//! - [`detect`]: which providers apply to a workspace and which of their tools are present.
//! - [`install`]: install plans, managed archive installs, uninstall; [`fetch`] (HTTPS) and
//!   [`archive`] (safe extraction) beneath it.

pub mod archive;
pub mod detect;
pub mod error;
pub mod fetch;
pub mod install;
pub mod manifest;
pub mod registry;

pub use detect::{detect, HostPaths};
pub use error::EnvError;
pub use fetch::{Http, HttpConfig};
pub use install::Installer;
pub use registry::{Builtin, Registry, Sources};
