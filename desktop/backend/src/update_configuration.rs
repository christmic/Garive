//! Fail-closed admission for the public, non-secret Tauri updater configuration.

use std::collections::HashSet;

use serde_json::Value;
use url::{Host, Url};

const MAX_UPDATE_ENDPOINTS: usize = 2;
const MAX_UPDATE_ENDPOINT_BYTES: usize = 2_048;
const MAX_UPDATE_PUBLIC_KEY_BYTES: usize = 16 * 1_024;

/// Returns whether a Tauri updater plugin value is safe to expose as installed.
pub fn desktop_updater_configured(value: Option<&Value>) -> bool {
    let Some(config) = value.and_then(Value::as_object) else {
        return false;
    };
    let Some(public_key) = config.get("pubkey").and_then(Value::as_str) else {
        return false;
    };
    if public_key.trim().is_empty() || public_key.len() > MAX_UPDATE_PUBLIC_KEY_BYTES {
        return false;
    }
    for key in [
        "dangerousInsecureTransportProtocol",
        "dangerousAcceptInvalidCerts",
        "dangerousAcceptInvalidHostnames",
    ] {
        if !matches!(config.get(key), None | Some(Value::Bool(false))) {
            return false;
        }
    }
    let Some(endpoints) = config.get("endpoints").and_then(Value::as_array) else {
        return false;
    };
    if endpoints.is_empty() || endpoints.len() > MAX_UPDATE_ENDPOINTS {
        return false;
    }
    let mut distinct = HashSet::with_capacity(endpoints.len());
    endpoints.iter().all(|value| {
        let Some(raw) = value.as_str() else {
            return false;
        };
        if raw.len() > MAX_UPDATE_ENDPOINT_BYTES || !distinct.insert(raw) {
            return false;
        }
        let Ok(endpoint) = Url::parse(raw) else {
            return false;
        };
        endpoint.scheme() == "https"
            && endpoint.username().is_empty()
            && endpoint.password().is_none()
            && endpoint.fragment().is_none()
            && matches!(endpoint.host(), Some(Host::Domain(domain))
                if !domain.eq_ignore_ascii_case("localhost")
                    && !domain.to_ascii_lowercase().ends_with(".localhost"))
    })
}
