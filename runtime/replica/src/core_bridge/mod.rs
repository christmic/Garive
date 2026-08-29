//! Durable mapping between one disposable Core execution and Runtime facts.

mod encoding;
mod terminal;

pub use terminal::{plan_core_terminal, CoreTerminalContext};
