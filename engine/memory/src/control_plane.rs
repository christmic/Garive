//! Pure M2 Markdown entry parsing and canonical projection.

use sha2::{Digest, Sha256};

use crate::{HypothesisState, MemoryAuthority, MemoryScopeClass, MemoryType};

const HEADER: &str = "---\n";
const BASE_KEYS: [&str; 7] = [
    "schema_version",
    "memory_id",
    "revision",
    "authority",
    "kind",
    "scope",
    "lifecycle",
];

/// Explicit bounds for one M2 entry document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryDocumentLimits {
    /// Maximum normalized document bytes.
    pub max_document_bytes: usize,
    /// Maximum normalized content bytes.
    pub max_content_bytes: usize,
    /// Maximum identity bytes.
    pub max_id_bytes: usize,
}

impl MemoryDocumentLimits {
    /// Rejects zero bounds and constructs document limits.
    pub const fn new(
        max_document_bytes: usize,
        max_content_bytes: usize,
        max_id_bytes: usize,
    ) -> Result<Self, MemoryControlError> {
        if max_document_bytes == 0 || max_content_bytes == 0 || max_id_bytes == 0 {
            Err(MemoryControlError::InvalidLimits)
        } else {
            Ok(Self {
                max_document_bytes,
                max_content_bytes,
                max_id_bytes,
            })
        }
    }
}

/// Stable M2 document failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryControlError {
    /// A caller supplied a zero bound.
    InvalidLimits,
    /// Input exceeded a declared byte bound.
    BoundExceeded,
    /// Encoding, line endings, front matter, or content shape is invalid.
    InvalidDocument,
    /// A token names an unsupported enum value.
    UnsupportedValue,
}

impl MemoryControlError {
    /// Returns the M2 stable failure family.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::InvalidLimits | Self::InvalidDocument | Self::UnsupportedValue => {
                "memory_snapshot_invalid"
            }
            Self::BoundExceeded => "memory_control_bound_exceeded",
        }
    }
}

/// One normalized, user-auditable M2 Memory entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryControlDocument {
    memory_id: String,
    revision: u64,
    authority: MemoryAuthority,
    memory_type: MemoryType,
    scope: MemoryScopeClass,
    lifecycle: HypothesisState,
    erase: bool,
    content: String,
}

impl MemoryControlDocument {
    /// Returns the stable logical Memory identity.
    pub fn memory_id(&self) -> &str {
        &self.memory_id
    }

    /// Returns the optimistic entry revision.
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the declared provenance authority.
    pub const fn authority(&self) -> MemoryAuthority {
        self.authority
    }

    /// Returns the cognitive Memory type.
    pub const fn memory_type(&self) -> MemoryType {
        self.memory_type
    }

    /// Returns the portable scope class.
    pub const fn scope(&self) -> MemoryScopeClass {
        self.scope
    }

    /// Returns the hypothesis lifecycle view.
    pub const fn lifecycle(&self) -> HypothesisState {
        self.lifecycle
    }

    /// Returns whether the user explicitly requested erasure.
    pub const fn erase_requested(&self) -> bool {
        self.erase
    }

    /// Returns normalized content with exactly one final LF.
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Returns lowercase SHA-256 over normalized content bytes.
    pub fn content_digest(&self) -> String {
        hex_sha256(self.content.as_bytes())
    }

    /// Renders the unique canonical M2 Markdown representation.
    pub fn render(&self) -> String {
        let mut output = format!(
            "---\nschema_version: 1\nmemory_id: {}\nrevision: {}\nauthority: {}\nkind: {}\nscope: {}\nlifecycle: {}\n",
            self.memory_id,
            self.revision,
            authority_name(self.authority),
            type_name(self.memory_type),
            scope_name(self.scope),
            lifecycle_name(self.lifecycle),
        );
        if self.erase {
            output.push_str("erase: true\n");
        }
        output.push_str("---\n");
        output.push_str(&self.content);
        output
    }
}

/// Parses strict M2 front matter and normalizes CRLF/content termination.
pub fn parse_memory_document(
    bytes: &[u8],
    limits: MemoryDocumentLimits,
) -> Result<MemoryControlDocument, MemoryControlError> {
    if bytes.len() > limits.max_document_bytes {
        return Err(MemoryControlError::BoundExceeded);
    }
    let raw = std::str::from_utf8(bytes).map_err(|_| MemoryControlError::InvalidDocument)?;
    if raw.contains('\r') && raw.replace("\r\n", "\n").contains('\r') {
        return Err(MemoryControlError::InvalidDocument);
    }
    let normalized = raw.replace("\r\n", "\n");
    let rest = normalized
        .strip_prefix(HEADER)
        .ok_or(MemoryControlError::InvalidDocument)?;
    let (front, content) = rest
        .split_once("\n---\n")
        .ok_or(MemoryControlError::InvalidDocument)?;
    let lines: Vec<_> = front.lines().collect();
    if lines.len() != BASE_KEYS.len() && lines.len() != BASE_KEYS.len() + 1 {
        return Err(MemoryControlError::InvalidDocument);
    }
    let mut values = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let (key, value) = line
            .split_once(": ")
            .ok_or(MemoryControlError::InvalidDocument)?;
        let expected = if index < BASE_KEYS.len() {
            BASE_KEYS[index]
        } else {
            "erase"
        };
        if key != expected || !valid_token(value) {
            return Err(MemoryControlError::InvalidDocument);
        }
        values.push(value);
    }
    if values[0] != "1" {
        return Err(MemoryControlError::UnsupportedValue);
    }
    let memory_id = values[1].to_owned();
    if memory_id.len() > limits.max_id_bytes {
        return Err(MemoryControlError::BoundExceeded);
    }
    let revision = values[2]
        .parse::<u64>()
        .ok()
        .filter(|value| *value > 0)
        .ok_or(MemoryControlError::InvalidDocument)?;
    let authority = parse_authority(values[3])?;
    let memory_type = parse_type(values[4])?;
    let scope = parse_scope(values[5])?;
    let lifecycle = parse_lifecycle(values[6])?;
    let erase = values.get(7).is_some_and(|value| *value == "true");
    if values.get(7).is_some() && !erase {
        return Err(MemoryControlError::InvalidDocument);
    }
    let content = format!("{}\n", content.trim_end_matches('\n'));
    if content == "\n" || content.len() > limits.max_content_bytes {
        return Err(if content.len() > limits.max_content_bytes {
            MemoryControlError::BoundExceeded
        } else {
            MemoryControlError::InvalidDocument
        });
    }
    let document = MemoryControlDocument {
        memory_id,
        revision,
        authority,
        memory_type,
        scope,
        lifecycle,
        erase,
        content,
    };
    if document.render().len() > limits.max_document_bytes {
        return Err(MemoryControlError::BoundExceeded);
    }
    Ok(document)
}

fn valid_token(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn parse_authority(value: &str) -> Result<MemoryAuthority, MemoryControlError> {
    match value {
        "user_declared" => Ok(MemoryAuthority::UserDeclared),
        "agent_learned" => Ok(MemoryAuthority::AgentLearned),
        "organisation_published" => Ok(MemoryAuthority::OrganisationPublished),
        _ => Err(MemoryControlError::UnsupportedValue),
    }
}

fn parse_type(value: &str) -> Result<MemoryType, MemoryControlError> {
    match value {
        "semantic" => Ok(MemoryType::Semantic),
        "episodic" => Ok(MemoryType::Episodic),
        "lesson" => Ok(MemoryType::Lesson),
        "procedural" => Ok(MemoryType::Procedural),
        _ => Err(MemoryControlError::UnsupportedValue),
    }
}

fn parse_scope(value: &str) -> Result<MemoryScopeClass, MemoryControlError> {
    match value {
        "session" => Ok(MemoryScopeClass::Session),
        "agent_instance" => Ok(MemoryScopeClass::AgentInstance),
        "user" => Ok(MemoryScopeClass::User),
        "project" => Ok(MemoryScopeClass::Project),
        "platform" => Ok(MemoryScopeClass::Platform),
        _ => Err(MemoryControlError::UnsupportedValue),
    }
}

fn parse_lifecycle(value: &str) -> Result<HypothesisState, MemoryControlError> {
    match value {
        "candidate" => Ok(HypothesisState::Candidate),
        "active" => Ok(HypothesisState::Active),
        "cold" => Ok(HypothesisState::Cold),
        "archived" => Ok(HypothesisState::Archived),
        "promoted" => Ok(HypothesisState::Promoted),
        _ => Err(MemoryControlError::UnsupportedValue),
    }
}

fn authority_name(value: MemoryAuthority) -> &'static str {
    match value {
        MemoryAuthority::UserDeclared => "user_declared",
        MemoryAuthority::AgentLearned => "agent_learned",
        MemoryAuthority::OrganisationPublished => "organisation_published",
    }
}

fn type_name(value: MemoryType) -> &'static str {
    match value {
        MemoryType::Semantic => "semantic",
        MemoryType::Episodic => "episodic",
        MemoryType::Lesson => "lesson",
        MemoryType::Procedural => "procedural",
    }
}

fn scope_name(value: MemoryScopeClass) -> &'static str {
    match value {
        MemoryScopeClass::Session => "session",
        MemoryScopeClass::AgentInstance => "agent_instance",
        MemoryScopeClass::User => "user",
        MemoryScopeClass::Project => "project",
        MemoryScopeClass::Platform => "platform",
    }
}

fn lifecycle_name(value: HypothesisState) -> &'static str {
    match value {
        HypothesisState::Candidate => "candidate",
        HypothesisState::Active => "active",
        HypothesisState::Cold => "cold",
        HypothesisState::Archived => "archived",
        HypothesisState::Promoted => "promoted",
    }
}

fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
