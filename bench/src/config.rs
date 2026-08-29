use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};

use crate::{unique_json::unique_json, BenchError, BenchErrorCode};

/// Parses duplicate-free explicit JSON and returns its RFC 8785 SHA-256 digest.
pub fn parse_explicit_config<T: DeserializeOwned>(bytes: &[u8]) -> Result<(T, String), BenchError> {
    let value = unique_json(bytes).map_err(|_| invalid())?;
    let canonical = serde_jcs::to_vec(&value).map_err(|_| invalid())?;
    let config = serde_json::from_value(value).map_err(|_| invalid())?;
    Ok((config, format!("{:x}", Sha256::digest(canonical))))
}

fn invalid() -> BenchError {
    BenchError::from_port(BenchErrorCode::InvalidConfiguration)
}
