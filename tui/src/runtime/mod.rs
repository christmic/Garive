mod app;
mod clipboard;
mod controller;
#[allow(dead_code)]
mod effects;
mod external_editor;
mod host;
#[allow(dead_code)]
mod host_effects;
mod terminal;
mod terminal_appearance;
mod terminal_events;

pub use app::run;
pub(crate) use terminal::{
    SystemTerminal, TerminalError, TerminalGuard, TerminalOptions, TerminalReconfiguration,
};
pub(crate) use terminal_appearance::TerminalTheme;
