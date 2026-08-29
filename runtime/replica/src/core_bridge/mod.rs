//! Durable mapping between one disposable Core execution and Runtime facts.

mod encoding;
mod model_lifecycle;
mod terminal;

pub use encoding::canonical_model_request_digest;
pub use model_lifecycle::{
    plan_model_prepared, plan_model_started, plan_model_terminal, plan_model_uncertain,
    ModelLifecycleContext, PreparedModelRequest, RuntimeModelUncertainReason,
};
pub use terminal::{plan_core_terminal, CoreTerminalContext};
