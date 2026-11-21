//! The Axiom daemon. Handlers are thin: deserialize → call one service
//! function → map the error → serialize. No logic (governance rule 4).

mod app;
mod handlers;
mod router;
pub mod settings;
mod tools;

pub use app::App;
pub use router::{build_router, NOT_YET_SERVED};
