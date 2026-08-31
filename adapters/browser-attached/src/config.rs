//! Explicit Native Messaging construction values.

const MAX_NATIVE_MESSAGE_BYTES: usize = 1_048_576;
const MAX_EXTENSION_ORIGIN_BYTES: usize = 256;

/// Hard bounds applied before any Native Messaging JSON parse or write.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AttachedLimits {
    max_frame_bytes: usize,
}

impl AttachedLimits {
    /// Constructs one non-zero frame limit no larger than Garive's 1 MiB cap.
    pub fn new(max_frame_bytes: usize) -> Result<Self, AttachedConfigError> {
        if !(1..=MAX_NATIVE_MESSAGE_BYTES).contains(&max_frame_bytes) {
            return Err(AttachedConfigError::InvalidLimit);
        }
        Ok(Self { max_frame_bytes })
    }

    /// Returns the exact accepted frame ceiling.
    pub const fn max_frame_bytes(self) -> usize {
        self.max_frame_bytes
    }
}

/// Complete explicit construction for one browser-started native host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AttachedConfig {
    expected_extension_origin: String,
    limits: AttachedLimits,
}

impl AttachedConfig {
    /// Constructs a host boundary for one exact Chrome extension origin.
    pub fn new(
        expected_extension_origin: impl Into<String>,
        limits: AttachedLimits,
    ) -> Result<Self, AttachedConfigError> {
        let expected_extension_origin = expected_extension_origin.into();
        validate_extension_origin(&expected_extension_origin)?;
        Ok(Self {
            expected_extension_origin,
            limits,
        })
    }

    /// Verifies Chrome's explicit native-host caller argument.
    pub fn admit_caller(&self, caller_origin: &str) -> Result<(), AttachedConfigError> {
        if caller_origin == self.expected_extension_origin {
            Ok(())
        } else {
            Err(AttachedConfigError::CallerDenied)
        }
    }

    /// Returns the exact admitted extension origin.
    pub fn expected_extension_origin(&self) -> &str {
        &self.expected_extension_origin
    }

    /// Returns the immutable message bounds.
    pub const fn limits(&self) -> AttachedLimits {
        self.limits
    }
}

fn validate_extension_origin(value: &str) -> Result<(), AttachedConfigError> {
    let Some(id) = value
        .strip_prefix("chrome-extension://")
        .and_then(|value| value.strip_suffix('/'))
    else {
        return Err(AttachedConfigError::InvalidExtensionOrigin);
    };
    if value.len() > MAX_EXTENSION_ORIGIN_BYTES
        || id.len() != 32
        || !id.bytes().all(|byte| matches!(byte, b'a'..=b'p'))
    {
        return Err(AttachedConfigError::InvalidExtensionOrigin);
    }
    Ok(())
}

/// Stable explicit-construction failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AttachedConfigError {
    /// A configured hard bound is zero or above the frozen cap.
    InvalidLimit,
    /// The configured origin is not one exact Chrome extension origin.
    InvalidExtensionOrigin,
    /// Chrome's caller origin differs from the configured extension.
    CallerDenied,
}
