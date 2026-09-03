//! Durable Agent metadata and directory-admission rules.

use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

/// Maximum UTF-8 bytes admitted from one Agent instruction file.
pub const MAX_AGENT_INSTRUCTION_BYTES: u64 = 256 * 1024;

/// Stable lifecycle of one registered Agent.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    /// Defined but unavailable to Sessions and Runs.
    Inactive,
    /// Admitted for new Session membership and Runs.
    Active,
    /// Administratively retired until explicitly reactivated.
    Archived,
}

/// Public metadata for one directory-backed Agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct RegisteredAgent {
    /// Exact public API version.
    pub api_version: &'static str,
    /// Stable immutable Agent identity.
    pub agent_id: String,
    /// Immutable Agent resource root and ordinary workspace.
    pub working_directory: PathBuf,
    /// Ordered read-only knowledge roots.
    pub readonly_knowledge_directories: Vec<PathBuf>,
    /// Optional single read/write knowledge root.
    pub writable_knowledge_directory: Option<PathBuf>,
    /// Current administrative lifecycle.
    pub status: AgentStatus,
}

/// Request for creating one inactive Agent.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CreateAgentRequest {
    /// Stable immutable Agent identity.
    pub agent_id: String,
    /// Immutable Agent resource root and ordinary workspace.
    pub working_directory: PathBuf,
    /// Ordered read-only knowledge roots.
    #[serde(default)]
    pub readonly_knowledge_directories: Vec<PathBuf>,
    /// Optional single read/write knowledge root.
    #[serde(default)]
    pub writable_knowledge_directory: Option<PathBuf>,
}

/// Atomic replacement for the mutable Agent knowledge binding.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct UpdateAgentKnowledgeRequest {
    /// Complete replacement ordered read-only roots.
    pub readonly_knowledge_directories: Vec<PathBuf>,
    /// Complete replacement optional read/write root; null clears it.
    pub writable_knowledge_directory: Option<PathBuf>,
}

/// Bounded stable Agent registry result.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RegisteredAgentPage {
    /// Exact public API version.
    pub api_version: &'static str,
    /// Agents in ascending identity order.
    pub agents: Vec<RegisteredAgent>,
}

/// Directory binding or persisted Agent metadata is invalid.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AgentRegistryValidationError {
    /// Agent identity does not satisfy the stable grammar.
    InvalidAgentId,
    /// A root is relative, missing, linked, inaccessible, or wrong kind.
    InvalidDirectory,
    /// Two authority roots overlap or resolve to the same directory.
    OverlappingDirectories,
    /// Required or optional Agent resources failed bounded validation.
    InvalidAgentResources,
}

/// Validates and canonicalizes all roots for a newly defined Agent.
pub fn validate_create_request(
    request: &CreateAgentRequest,
) -> Result<CreateAgentRequest, AgentRegistryValidationError> {
    if !valid_agent_id(&request.agent_id) {
        return Err(AgentRegistryValidationError::InvalidAgentId);
    }
    let working_directory = canonical_directory(&request.working_directory)?;
    let readonly_knowledge_directories = request
        .readonly_knowledge_directories
        .iter()
        .map(|path| canonical_directory(path))
        .collect::<Result<Vec<_>, _>>()?;
    let writable_knowledge_directory = request
        .writable_knowledge_directory
        .as_deref()
        .map(canonical_directory)
        .transpose()?;
    validate_distinct_roots(
        &working_directory,
        &readonly_knowledge_directories,
        writable_knowledge_directory.as_deref(),
    )?;
    Ok(CreateAgentRequest {
        agent_id: request.agent_id.clone(),
        working_directory,
        readonly_knowledge_directories,
        writable_knowledge_directory,
    })
}

/// Validates and canonicalizes a replacement knowledge binding.
pub fn validate_knowledge_update(
    working_directory: &Path,
    request: &UpdateAgentKnowledgeRequest,
) -> Result<UpdateAgentKnowledgeRequest, AgentRegistryValidationError> {
    let readonly_knowledge_directories = request
        .readonly_knowledge_directories
        .iter()
        .map(|path| canonical_directory(path))
        .collect::<Result<Vec<_>, _>>()?;
    let writable_knowledge_directory = request
        .writable_knowledge_directory
        .as_deref()
        .map(canonical_directory)
        .transpose()?;
    validate_distinct_roots(
        working_directory,
        &readonly_knowledge_directories,
        writable_knowledge_directory.as_deref(),
    )?;
    Ok(UpdateAgentKnowledgeRequest {
        readonly_knowledge_directories,
        writable_knowledge_directory,
    })
}

/// Revalidates all authorities and Agent instruction resources for activation or Run admission.
pub fn validate_active_binding(
    agent: &RegisteredAgent,
) -> Result<(), AgentRegistryValidationError> {
    let recreated = validate_create_request(&CreateAgentRequest {
        agent_id: agent.agent_id.clone(),
        working_directory: agent.working_directory.clone(),
        readonly_knowledge_directories: agent.readonly_knowledge_directories.clone(),
        writable_knowledge_directory: agent.writable_knowledge_directory.clone(),
    })?;
    if recreated.working_directory != agent.working_directory
        || recreated.readonly_knowledge_directories != agent.readonly_knowledge_directories
        || recreated.writable_knowledge_directory != agent.writable_knowledge_directory
    {
        return Err(AgentRegistryValidationError::InvalidDirectory);
    }
    require_writable(&agent.working_directory)?;
    if let Some(path) = &agent.writable_knowledge_directory {
        require_writable(path)?;
    }
    validate_instruction(&agent.working_directory.join("AGENT.md"), true)?;
    validate_instruction(&agent.working_directory.join("SOUL.md"), false)?;
    let skills = agent.working_directory.join("skills");
    if skills.exists() && (!skills.is_dir() || skills.symlink_metadata().is_err()) {
        return Err(AgentRegistryValidationError::InvalidAgentResources);
    }
    Ok(())
}

fn valid_agent_id(value: &str) -> bool {
    let bytes = value.as_bytes();
    (1..=64).contains(&bytes.len())
        && bytes[0].is_ascii_alphanumeric()
        && bytes.iter().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'.' | b'_' | b'-')
        })
}

fn canonical_directory(path: &Path) -> Result<PathBuf, AgentRegistryValidationError> {
    if !path.is_absolute() {
        return Err(AgentRegistryValidationError::InvalidDirectory);
    }
    let metadata =
        fs::symlink_metadata(path).map_err(|_| AgentRegistryValidationError::InvalidDirectory)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AgentRegistryValidationError::InvalidDirectory);
    }
    fs::read_dir(path).map_err(|_| AgentRegistryValidationError::InvalidDirectory)?;
    fs::canonicalize(path).map_err(|_| AgentRegistryValidationError::InvalidDirectory)
}

fn validate_distinct_roots(
    working: &Path,
    readonly: &[PathBuf],
    writable: Option<&Path>,
) -> Result<(), AgentRegistryValidationError> {
    let roots = std::iter::once(working)
        .chain(readonly.iter().map(PathBuf::as_path))
        .chain(writable)
        .collect::<Vec<_>>();
    let unique = roots.iter().copied().collect::<BTreeSet<_>>();
    if unique.len() != roots.len()
        || roots.iter().enumerate().any(|(index, left)| {
            roots
                .iter()
                .skip(index + 1)
                .any(|right| left.starts_with(right) || right.starts_with(left))
        })
    {
        return Err(AgentRegistryValidationError::OverlappingDirectories);
    }
    Ok(())
}

fn require_writable(path: &Path) -> Result<(), AgentRegistryValidationError> {
    let metadata =
        fs::metadata(path).map_err(|_| AgentRegistryValidationError::InvalidDirectory)?;
    if metadata.permissions().readonly() {
        return Err(AgentRegistryValidationError::InvalidDirectory);
    }
    Ok(())
}

fn validate_instruction(path: &Path, required: bool) -> Result<(), AgentRegistryValidationError> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if !required && error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(_) => return Err(AgentRegistryValidationError::InvalidAgentResources),
    };
    if metadata.file_type().is_symlink()
        || !metadata.is_file()
        || metadata.len() == 0
        || metadata.len() > MAX_AGENT_INSTRUCTION_BYTES
    {
        return Err(AgentRegistryValidationError::InvalidAgentResources);
    }
    let bytes = fs::read(path).map_err(|_| AgentRegistryValidationError::InvalidAgentResources)?;
    std::str::from_utf8(&bytes)
        .map(|_| ())
        .map_err(|_| AgentRegistryValidationError::InvalidAgentResources)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creation_canonicalizes_non_overlapping_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let working = temp.path().join("working");
        let readonly = temp.path().join("readonly");
        fs::create_dir(&working).expect("working");
        fs::create_dir(&readonly).expect("readonly");
        let request = CreateAgentRequest {
            agent_id: "agent.one".into(),
            working_directory: working.clone(),
            readonly_knowledge_directories: vec![readonly.clone()],
            writable_knowledge_directory: None,
        };
        let valid = validate_create_request(&request).expect("valid request");
        assert_eq!(valid.working_directory, fs::canonicalize(working).unwrap());
        assert_eq!(
            valid.readonly_knowledge_directories,
            vec![fs::canonicalize(readonly).unwrap()]
        );
    }

    #[test]
    fn activation_requires_agent_markdown() {
        let temp = tempfile::tempdir().expect("tempdir");
        let agent = RegisteredAgent {
            api_version: "v1",
            agent_id: "agent-one".into(),
            working_directory: fs::canonicalize(temp.path()).unwrap(),
            readonly_knowledge_directories: Vec::new(),
            writable_knowledge_directory: None,
            status: AgentStatus::Inactive,
        };
        assert_eq!(
            validate_active_binding(&agent),
            Err(AgentRegistryValidationError::InvalidAgentResources)
        );
        fs::write(temp.path().join("AGENT.md"), "# Agent\n").expect("instruction");
        validate_active_binding(&agent).expect("active binding");
    }

    #[test]
    fn creation_rejects_overlapping_authority_roots() {
        let temp = tempfile::tempdir().expect("tempdir");
        let nested = temp.path().join("knowledge");
        fs::create_dir(&nested).expect("nested");
        let request = CreateAgentRequest {
            agent_id: "agent-one".into(),
            working_directory: temp.path().to_owned(),
            readonly_knowledge_directories: vec![nested],
            writable_knowledge_directory: None,
        };
        assert_eq!(
            validate_create_request(&request),
            Err(AgentRegistryValidationError::OverlappingDirectories)
        );
    }
}
