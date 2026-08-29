//! Durable mapping between one disposable Core execution and Runtime facts.

mod encoding;
mod terminal;

pub use encoding::canonical_model_request_digest;
pub use terminal::{plan_core_terminal, CoreTerminalContext};
