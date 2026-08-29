use garive_llm::ModelInputItem;

/// Immutable identity and canonical configuration binding for one counter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TokenCounterDescriptor {
    /// Stable counter implementation identity.
    pub counter_id: String,
    /// Exact implementation/model vocabulary revision.
    pub counter_revision: String,
    /// SHA-256 of canonical non-secret counter configuration.
    pub config_digest: String,
    /// Whether this exact counter may produce publication evidence.
    pub publishable: bool,
}

impl TokenCounterDescriptor {
    /// Validates bounded identities and a lowercase SHA-256 digest.
    pub fn new(
        counter_id: impl Into<String>,
        counter_revision: impl Into<String>,
        config_digest: impl Into<String>,
        publishable: bool,
    ) -> Option<Self> {
        let value = Self {
            counter_id: counter_id.into(),
            counter_revision: counter_revision.into(),
            config_digest: config_digest.into(),
            publishable,
        };
        let identity = |text: &str| !text.is_empty() && text.len() <= 256;
        if !identity(&value.counter_id)
            || !identity(&value.counter_revision)
            || value.config_digest.len() != 64
            || !value
                .config_digest
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        {
            None
        } else {
            Some(value)
        }
    }
}

/// Injected exact token-counting boundary; implementations own no Agent state.
pub trait TokenCounter {
    /// Returns the frozen counter/configuration binding.
    fn descriptor(&self) -> &TokenCounterDescriptor;

    /// Counts the exact assembled provider-neutral model input.
    fn count_input_tokens(&self, items: &[ModelInputItem]) -> Result<u64, TokenCounterFailure>;
}

/// Content-free counter dependency failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TokenCounterFailure;
