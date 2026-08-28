//! Durable-fact vocabulary and ledger ports; storage adapters live in Runtime.

#![forbid(unsafe_code)]

mod canonical;

pub use canonical::{CanonicalPayload, CanonicalPayloadError};
