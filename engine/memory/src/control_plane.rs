//! Pure M2 snapshot document values and parsing.

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use sha2::{Digest, Sha256};

use crate::{
    HypothesisState, MemoryAuthority, MemoryKind, MemoryScopeClass, MemorySensitivity, MemoryType,
};

const HEADER: &str = "---\n";
const BASE_KEYS: [&str; 9] = [
    "schema_version",
    "record_ref",
    "authority",
    "memory_type",
    "memory_role",
    "scope",
    "scope_owner_b64",
    "lifecycle",
    "sensitivity",
];

/// Explicit bounds for one M2 entry document.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct MemoryDocumentLimits {
    /// Maximum normalized document bytes.
    pub max_document_bytes: usize,
    /// Maximum normalized content bytes.
    pub max_content_bytes: usize,
    /// Maximum decoded identity bytes.
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

/// Stable M2 validation or planning failure.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MemoryControlError {
    /// A caller supplied a zero bound.
    InvalidLimits,
    /// Input exceeded a declared bound.
    BoundExceeded,
    /// Snapshot encoding, shape, identity, or digest is invalid.
    InvalidSnapshot,
    /// An edit attempts to change Engine- or authority-owned state.
    ForbiddenChange,
    /// The repository or affected record no longer matches the snapshot.
    StaleSnapshot,
}

impl MemoryControlError {
    /// Returns the stable M2 failure family.
    pub const fn wire_name(self) -> &'static str {
        match self {
            Self::InvalidLimits | Self::InvalidSnapshot => "memory_snapshot_invalid",
            Self::BoundExceeded => "memory_control_bound_exceeded",
            Self::ForbiddenChange => "memory_import_forbidden_change",
            Self::StaleSnapshot => "stale_memory_snapshot",
        }
    }
}

/// Exact existing M0 identity or bounded new-entry correlation token.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum MemoryRecordRef {
    /// Existing immutable record and revision identities.
    Existing {
        /// Exact M0 record identity.
        record_id: String,
        /// Exact M0 revision identity.
        revision_id: String,
    },
    /// User-created document awaiting Runtime identity allocation.
    New {
        /// Package-local import correlation token.
        draft_token: String,
    },
}

impl MemoryRecordRef {
    /// Returns the exact existing record identity, when present.
    pub fn record_id(&self) -> Option<&str> {
        match self {
            Self::Existing { record_id, .. } => Some(record_id),
            Self::New { .. } => None,
        }
    }
    /// Returns the exact existing revision identity, when present.
    pub fn revision_id(&self) -> Option<&str> {
        match self {
            Self::Existing { revision_id, .. } => Some(revision_id),
            Self::New { .. } => None,
        }
    }
    /// Returns the new-entry draft token, when present.
    pub fn draft_token(&self) -> Option<&str> {
        match self {
            Self::New { draft_token } => Some(draft_token),
            Self::Existing { .. } => None,
        }
    }
    fn render(&self) -> String {
        match self {
            Self::Existing {
                record_id,
                revision_id,
            } => format!(
                "existing.{}.{}",
                URL_SAFE_NO_PAD.encode(record_id),
                URL_SAFE_NO_PAD.encode(revision_id),
            ),
            Self::New { draft_token } => format!("new.{draft_token}"),
        }
    }
}

/// One normalized, user-auditable M2 Memory entry.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MemoryControlDocument {
    record_ref: MemoryRecordRef,
    authority: MemoryAuthority,
    memory_type: MemoryType,
    memory_role: MemoryKind,
    scope: MemoryScopeClass,
    scope_owner_id: String,
    lifecycle: HypothesisState,
    sensitivity: MemorySensitivity,
    erase: bool,
    content: String,
}

impl MemoryControlDocument {
    /// Builds one canonical current document from admitted repository fields.
    #[allow(clippy::too_many_arguments)]
    pub fn from_repository_record(
        record_id: impl Into<String>,
        revision_id: impl Into<String>,
        authority: MemoryAuthority,
        memory_type: MemoryType,
        memory_role: MemoryKind,
        scope: MemoryScopeClass,
        scope_owner_id: impl Into<String>,
        lifecycle: HypothesisState,
        sensitivity: MemorySensitivity,
        content: impl Into<String>,
        limits: MemoryDocumentLimits,
    ) -> Result<Self, MemoryControlError> {
        let record_id = record_id.into();
        let revision_id = revision_id.into();
        let scope_owner_id = scope_owner_id.into();
        let raw_content = content.into();
        if !valid_decoded_identity(&record_id, limits.max_id_bytes)
            || !valid_decoded_identity(&revision_id, limits.max_id_bytes)
            || !valid_decoded_identity(&scope_owner_id, limits.max_id_bytes)
            || (raw_content.contains('\r') && raw_content.replace("\r\n", "\n").contains('\r'))
        {
            return Err(MemoryControlError::InvalidSnapshot);
        }
        let normalized = raw_content.replace("\r\n", "\n");
        let content = format!("{}\n", normalized.trim_end_matches('\n'));
        if content == "\n" {
            return Err(MemoryControlError::InvalidSnapshot);
        }
        if content.len() > limits.max_content_bytes {
            return Err(MemoryControlError::BoundExceeded);
        }
        let document = Self {
            record_ref: MemoryRecordRef::Existing {
                record_id,
                revision_id,
            },
            authority,
            memory_type,
            memory_role,
            scope,
            scope_owner_id,
            lifecycle,
            sensitivity,
            erase: false,
            content,
        };
        if document.render().len() > limits.max_document_bytes {
            Err(MemoryControlError::BoundExceeded)
        } else {
            Ok(document)
        }
    }

    /// Returns the exact existing identity or new-entry token.
    pub const fn record_ref(&self) -> &MemoryRecordRef {
        &self.record_ref
    }
    /// Returns the declared provenance authority.
    pub const fn authority(&self) -> MemoryAuthority {
        self.authority
    }
    /// Returns the cognitive Memory type.
    pub const fn memory_type(&self) -> MemoryType {
        self.memory_type
    }
    /// Returns the preserved M0 content role.
    pub const fn memory_role(&self) -> MemoryKind {
        self.memory_role
    }
    /// Returns the portable scope class.
    pub const fn scope(&self) -> MemoryScopeClass {
        self.scope
    }
    /// Returns the exact decoded scope-owner identity.
    pub fn scope_owner_id(&self) -> &str {
        &self.scope_owner_id
    }
    /// Returns the hypothesis lifecycle view.
    pub const fn lifecycle(&self) -> HypothesisState {
        self.lifecycle
    }
    /// Returns the sensitivity class.
    pub const fn sensitivity(&self) -> MemorySensitivity {
        self.sensitivity
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
    /// Returns lowercase SHA-256 over canonical Markdown bytes.
    pub fn document_digest(&self) -> String {
        hex_sha256(self.render().as_bytes())
    }
    /// Renders the unique canonical M2 Markdown representation.
    pub fn render(&self) -> String {
        let mut output = format!(
            "---\nschema_version: 1\nrecord_ref: {}\nauthority: {}\nmemory_type: {}\nmemory_role: {}\nscope: {}\nscope_owner_b64: {}\nlifecycle: {}\nsensitivity: {}\n",
            self.record_ref.render(), authority_name(self.authority), type_name(self.memory_type),
            role_name(self.memory_role), scope_name(self.scope), URL_SAFE_NO_PAD.encode(&self.scope_owner_id),
            lifecycle_name(self.lifecycle), sensitivity_name(self.sensitivity),
        );
        if self.erase {
            output.push_str("erase: true\n");
        }
        output.push_str("---\n");
        output.push_str(&self.content);
        output
    }

    /// Rebinds a prepared new or superseding document to its committed M0 identity.
    pub fn bind_existing_identity(
        &self,
        record_id: impl Into<String>,
        revision_id: impl Into<String>,
        max_id_bytes: usize,
    ) -> Result<Self, MemoryControlError> {
        let record_id = record_id.into();
        let revision_id = revision_id.into();
        if !valid_decoded_identity(&record_id, max_id_bytes)
            || !valid_decoded_identity(&revision_id, max_id_bytes)
        {
            return Err(MemoryControlError::InvalidSnapshot);
        }
        let mut bound = self.clone();
        bound.record_ref = MemoryRecordRef::Existing {
            record_id,
            revision_id,
        };
        bound.erase = false;
        Ok(bound)
    }

    /// Returns the same immutable revision rendered at a later admitted lifecycle state.
    pub fn with_lifecycle(&self, lifecycle: HypothesisState) -> Self {
        let mut updated = self.clone();
        updated.lifecycle = lifecycle;
        updated.erase = false;
        updated
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
    let raw = std::str::from_utf8(bytes).map_err(|_| MemoryControlError::InvalidSnapshot)?;
    if raw.contains('\r') && raw.replace("\r\n", "\n").contains('\r') {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    let normalized = raw.replace("\r\n", "\n");
    let rest = normalized
        .strip_prefix(HEADER)
        .ok_or(MemoryControlError::InvalidSnapshot)?;
    let (front, raw_content) = rest
        .split_once("\n---\n")
        .ok_or(MemoryControlError::InvalidSnapshot)?;
    if raw_content.contains("\n---") {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    let lines: Vec<_> = front.lines().collect();
    if lines.len() != BASE_KEYS.len() && lines.len() != BASE_KEYS.len() + 1 {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    let mut values = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        let (key, value) = line
            .split_once(": ")
            .ok_or(MemoryControlError::InvalidSnapshot)?;
        let expected = BASE_KEYS.get(index).copied().unwrap_or("erase");
        if key != expected || !valid_token(value, 512) {
            return Err(MemoryControlError::InvalidSnapshot);
        }
        values.push(value);
    }
    if values[0] != "1" {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    let record_ref = parse_record_ref(values[1], limits.max_id_bytes)?;
    let authority = parse_authority(values[2])?;
    let memory_type = parse_type(values[3])?;
    let memory_role = parse_role(values[4])?;
    let scope = parse_scope(values[5])?;
    let scope_owner_id = decode_identity(values[6], limits.max_id_bytes)?;
    let lifecycle = parse_lifecycle(values[7])?;
    let sensitivity = parse_sensitivity(values[8])?;
    let erase = values.get(9).is_some_and(|value| *value == "true");
    if values.get(9).is_some() && !erase {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    let content = format!("{}\n", raw_content.trim_end_matches('\n'));
    if content == "\n" || content.len() > limits.max_content_bytes {
        return Err(if content.len() > limits.max_content_bytes {
            MemoryControlError::BoundExceeded
        } else {
            MemoryControlError::InvalidSnapshot
        });
    }
    let document = MemoryControlDocument {
        record_ref,
        authority,
        memory_type,
        memory_role,
        scope,
        scope_owner_id,
        lifecycle,
        sensitivity,
        erase,
        content,
    };
    if document.render().len() > limits.max_document_bytes {
        return Err(MemoryControlError::BoundExceeded);
    }
    Ok(document)
}

fn parse_record_ref(
    value: &str,
    max_id_bytes: usize,
) -> Result<MemoryRecordRef, MemoryControlError> {
    if let Some(value) = value.strip_prefix("existing.") {
        let (record, revision) = value
            .split_once('.')
            .ok_or(MemoryControlError::InvalidSnapshot)?;
        if revision.contains('.') {
            return Err(MemoryControlError::InvalidSnapshot);
        }
        Ok(MemoryRecordRef::Existing {
            record_id: decode_identity(record, max_id_bytes)?,
            revision_id: decode_identity(revision, max_id_bytes)?,
        })
    } else if let Some(draft_token) = value.strip_prefix("new.") {
        if !valid_token(draft_token, 64) {
            return Err(MemoryControlError::InvalidSnapshot);
        }
        Ok(MemoryRecordRef::New {
            draft_token: draft_token.to_owned(),
        })
    } else {
        Err(MemoryControlError::InvalidSnapshot)
    }
}

fn decode_identity(value: &str, max_bytes: usize) -> Result<String, MemoryControlError> {
    let bytes = URL_SAFE_NO_PAD
        .decode(value)
        .map_err(|_| MemoryControlError::InvalidSnapshot)?;
    let decoded = String::from_utf8(bytes).map_err(|_| MemoryControlError::InvalidSnapshot)?;
    if !valid_decoded_identity(&decoded, max_bytes) || URL_SAFE_NO_PAD.encode(&decoded) != value {
        return Err(MemoryControlError::InvalidSnapshot);
    }
    Ok(decoded)
}

fn valid_decoded_identity(value: &str, max_bytes: usize) -> bool {
    max_bytes > 0 && !value.is_empty() && value.len() <= max_bytes && value.trim() == value
}

fn valid_token(value: &str, max: usize) -> bool {
    !value.is_empty()
        && value.len() <= max
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
}

fn parse_authority(value: &str) -> Result<MemoryAuthority, MemoryControlError> {
    match value {
        "user_declared" => Ok(MemoryAuthority::UserDeclared),
        "agent_learned" => Ok(MemoryAuthority::AgentLearned),
        "organisation_published" => Ok(MemoryAuthority::OrganisationPublished),
        _ => Err(MemoryControlError::InvalidSnapshot),
    }
}
fn parse_type(value: &str) -> Result<MemoryType, MemoryControlError> {
    match value {
        "semantic" => Ok(MemoryType::Semantic),
        "episodic" => Ok(MemoryType::Episodic),
        "lesson" => Ok(MemoryType::Lesson),
        "procedural" => Ok(MemoryType::Procedural),
        _ => Err(MemoryControlError::InvalidSnapshot),
    }
}
fn parse_role(value: &str) -> Result<MemoryKind, MemoryControlError> {
    match value {
        "preference" => Ok(MemoryKind::Preference),
        "constraint" => Ok(MemoryKind::Constraint),
        "decision" => Ok(MemoryKind::Decision),
        "learned_fact" => Ok(MemoryKind::LearnedFact),
        "summary" => Ok(MemoryKind::Summary),
        _ => Err(MemoryControlError::InvalidSnapshot),
    }
}
fn parse_scope(value: &str) -> Result<MemoryScopeClass, MemoryControlError> {
    match value {
        "session" => Ok(MemoryScopeClass::Session),
        "agent_instance" => Ok(MemoryScopeClass::AgentInstance),
        "user" => Ok(MemoryScopeClass::User),
        "project" => Ok(MemoryScopeClass::Project),
        "platform" => Ok(MemoryScopeClass::Platform),
        _ => Err(MemoryControlError::InvalidSnapshot),
    }
}
fn parse_lifecycle(value: &str) -> Result<HypothesisState, MemoryControlError> {
    match value {
        "candidate" => Ok(HypothesisState::Candidate),
        "active" => Ok(HypothesisState::Active),
        "cold" => Ok(HypothesisState::Cold),
        "archived" => Ok(HypothesisState::Archived),
        "promoted" => Ok(HypothesisState::Promoted),
        _ => Err(MemoryControlError::InvalidSnapshot),
    }
}
fn parse_sensitivity(value: &str) -> Result<MemorySensitivity, MemoryControlError> {
    match value {
        "ordinary" => Ok(MemorySensitivity::Ordinary),
        "restricted" => Ok(MemorySensitivity::Restricted),
        _ => Err(MemoryControlError::InvalidSnapshot),
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
fn role_name(value: MemoryKind) -> &'static str {
    match value {
        MemoryKind::Preference => "preference",
        MemoryKind::Constraint => "constraint",
        MemoryKind::Decision => "decision",
        MemoryKind::LearnedFact => "learned_fact",
        MemoryKind::Summary => "summary",
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
fn sensitivity_name(value: MemorySensitivity) -> &'static str {
    match value {
        MemorySensitivity::Ordinary => "ordinary",
        MemorySensitivity::Restricted => "restricted",
    }
}

pub(crate) fn hex_sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
