//! What an agent may do: capabilities, rules, requests and grants. Nothing here does anything; the
//! crates that do (`axiom-spawn`, `axiom-tools`) ask first and take what they get as a type.
//!
//! Authority is a set of capabilities that is only ever narrowed (SI-1). A rule chooses between
//! what the capabilities already allow; it never adds to them. A person's answer is the only thing
//! that widens anything (SI-2), and only what was shown to them.

pub mod audit;
pub mod caps;
pub mod paths;
pub mod profile;
pub mod request;
pub mod rules;
pub mod shell;

pub use caps::{Budget, Caps, Host, Net, Reach, Tools};
pub use paths::{BadPath, Place};
pub use profile::{BadProfile, Keep, Profile};
pub use request::{Answer, Approval, Ask, NotAnswered, Requests, Unsandboxed};
pub use rules::{Access, Action, Decision, Policy, Verdict};
