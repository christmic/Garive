//! Reproducible C7-A corpus loading and context-pressure evidence tooling.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod command_counter;
mod corpus;
mod counter;
mod error;
mod http_exchange;
mod provider_counter;
mod runner;

pub use command_counter::{CommandTokenCounter, CommandTokenCounterConfig};
pub use corpus::{load_corpus, ContextPressureCase, ContextPressureCorpus};
pub use counter::{TokenCounter, TokenCounterDescriptor, TokenCounterFailure};
pub use error::{ContextPressureError, ContextPressureErrorCode};
pub use http_exchange::{ReqwestTokenCountExchangePort, TokenCountHttpLimits};
pub use provider_counter::{
    AnthropicProviderCounter, AnthropicProviderCounterConfig, TokenCountExchangePort,
};
pub use runner::{measure_context_pressure, ContextPressureRun};
