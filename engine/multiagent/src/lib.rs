//! Agent delegation semantics and neutral child-execution ports.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

mod intent;
mod values;

pub use intent::{CancellationPolicy, ChildRequirement, DelegationIntent, DelegationIntentBinding};
pub use values::{
    ContentBinding, DelegationBudget, DelegationError, DelegationErrorCode, FactReference,
};
