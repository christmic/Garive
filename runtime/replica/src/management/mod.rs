//! Runtime-management configuration domain: wire types, persistence, and digest helpers.
//!
//! The [`ManagementConfigStore`] reads and writes the singleton
//! `runtime_management_config` row introduced in SQLite schema v9. It is the
//! persistence side of the loopback `/v1/management/setup` HTTP surface that
//! bootstraps a headless Runtime without a SetupService round trip.

mod digest;
mod store;
mod types;
mod validator;

#[allow(unused_imports)]
pub use digest::{configuration_digest, receipt_digest, MANAGEMENT_CONFIG_CONTRACT};
pub use store::ManagementConfigStore;
pub use types::{
    ManagementCommitBody, ManagementConfigError, ManagementConfigReceipt, ManagementConfigState,
    MANAGEMENT_COMMIT_BODY_SCHEMA_VERSION,
};
pub use validator::{AllowAllValidator, ManagementValidator};
