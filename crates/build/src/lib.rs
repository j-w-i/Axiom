//! `axiom-build`: provider tasks (build, run, test…) run through `axiom-exec` inside the
//! project's environment, with their output parsed into normalised diagnostics
//! (ADR-0014; M2-BUILD-001).
//!
//! - [`parse`]: pure parsers for `cargo_json`, `gcc_text`, `regex` and `sarif`.
//! - [`run`]: task listing, the process spec, and the driver that batches diagnostics.

pub mod parse;
pub mod run;

pub use run::{drive, prepare, tasks, BuildError, BuildEvent, Counts, Prepared};
