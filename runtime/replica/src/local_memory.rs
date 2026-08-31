//! Catalogue-bound preparation of production local Memory context.

use std::sync::Arc;

use garive_config::CapabilityKind;
use garive_core::{AgentEntry, ResumeInput};
use garive_knowledge::{
    complete_knowledge, Citation, CitationScheme, ContentBinding as KnowledgeContentBinding,
    FreshnessRequirement, KnowledgeEvidence, KnowledgeFreshness, KnowledgeQueryMode,
    KnowledgeRequest, KnowledgeSourceDescriptor, KnowledgeTrustClass,
};
use garive_memory::{
    ContentBinding, MemoryDocumentLimits, MemoryPurpose, MemoryQuery, MemoryScope, MemoryScore,
};
use serde_json::json;
use sha2::{Digest, Sha256};

use crate::{
    authorize_memory_query, plan_memory_retrieval, LocalCapabilityPreparationFactory,
    LocalCapabilityPreparationInput, LocalWorkerError, MemoryAccessGrant, MemoryControlGrant,
    MemoryPrefix, MemoryRepositoryError, MemoryRetrievalContext, PreparedAgentCapabilities,
    PreparedKnowledgeCapability, RecoveredKnowledgeContext, RuntimeAgentCatalogue, SqliteLedger,
};

/// Portable M0 contract version supported by local production composition.
pub const LOCAL_MEMORY_CONTRACT_VERSION: u64 = 1;
/// First bounded product retrieval policy for explicit user declarations.
pub const USER_DECLARED_PUSH_REVISION: &str = "user-declared-push-v1";
/// Portable K0 contract version supported by local production composition.
pub const LOCAL_KNOWLEDGE_CONTRACT_VERSION: u64 = 1;
/// First automatic K0 request policy over trusted current input.
pub const KEYWORD_CURRENT_INPUT_REVISION: &str = "keyword-current-input-v1";

/// Explicit Runtime-owned binding for one snapshot-admitted Memory capability.
#[derive(Clone)]
pub struct LocalMemorySystemBinding {
    capability_name: String,
    exact_revision: String,
    descriptor_digest: String,
    namespace_id: String,
    retriever_revision: String,
    source_policy_revision: String,
    control_grant: MemoryControlGrant,
    allowed_scopes: Vec<MemoryScope>,
    document_limits: MemoryDocumentLimits,
    max_results: u32,
    max_total_bytes: u64,
    max_repository_records: usize,
    max_repository_facts: usize,
}

/// Explicit Runtime-owned binding for one snapshot-admitted Knowledge source.
pub struct LocalKnowledgeSystemBinding {
    capability_name: String,
    exact_revision: String,
    descriptor_digest: String,
    source: KnowledgeSourceDescriptor,
    request_policy_revision: String,
    max_chunks: u32,
    max_total_bytes: u64,
    deadline_budget_ms: u64,
    connector: Arc<dyn crate::KnowledgeConnector>,
}

impl LocalKnowledgeSystemBinding {
    /// Constructs one exact source binding without ambient configuration.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        capability_name: impl Into<String>,
        exact_revision: impl Into<String>,
        descriptor_digest: impl Into<String>,
        source: KnowledgeSourceDescriptor,
        request_policy_revision: impl Into<String>,
        max_chunks: u32,
        max_total_bytes: u64,
        deadline_budget_ms: u64,
        connector: Arc<dyn crate::KnowledgeConnector>,
    ) -> Result<Self, LocalWorkerError> {
        let value = Self {
            capability_name: capability_name.into(),
            exact_revision: exact_revision.into(),
            descriptor_digest: descriptor_digest.into(),
            source,
            request_policy_revision: request_policy_revision.into(),
            max_chunks,
            max_total_bytes,
            deadline_budget_ms,
            connector,
        };
        if !valid_text(&value.capability_name)
            || !valid_text(&value.exact_revision)
            || !valid_digest(&value.descriptor_digest)
            || value.request_policy_revision != KEYWORD_CURRENT_INPUT_REVISION
            || value.max_chunks == 0
            || value.max_total_bytes == 0
            || value.deadline_budget_ms == 0
        {
            return Err(LocalWorkerError::InvalidComposition);
        }
        Ok(value)
    }
}

impl LocalMemorySystemBinding {
    /// Constructs a complete binding without environment or filesystem discovery.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        capability_name: impl Into<String>,
        exact_revision: impl Into<String>,
        descriptor_digest: impl Into<String>,
        namespace_id: impl Into<String>,
        retriever_revision: impl Into<String>,
        source_policy_revision: impl Into<String>,
        control_grant: MemoryControlGrant,
        allowed_scopes: Vec<MemoryScope>,
        document_limits: MemoryDocumentLimits,
        max_results: u32,
        max_total_bytes: u64,
        max_repository_records: usize,
        max_repository_facts: usize,
    ) -> Result<Self, LocalWorkerError> {
        let value = Self {
            capability_name: capability_name.into(),
            exact_revision: exact_revision.into(),
            descriptor_digest: descriptor_digest.into(),
            namespace_id: namespace_id.into(),
            retriever_revision: retriever_revision.into(),
            source_policy_revision: source_policy_revision.into(),
            control_grant,
            allowed_scopes,
            document_limits,
            max_results,
            max_total_bytes,
            max_repository_records,
            max_repository_facts,
        };
        if !valid_text(&value.capability_name)
            || !valid_text(&value.exact_revision)
            || !valid_digest(&value.descriptor_digest)
            || !valid_text(&value.namespace_id)
            || !valid_text(&value.retriever_revision)
            || value.source_policy_revision != USER_DECLARED_PUSH_REVISION
            || value.allowed_scopes.is_empty()
            || !value
                .allowed_scopes
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            || value.max_results == 0
            || value.max_total_bytes == 0
            || value.max_repository_records == 0
            || value.max_repository_facts == 0
        {
            return Err(LocalWorkerError::InvalidComposition);
        }
        Ok(value)
    }
}

/// Resolves D0 descriptors and prepares exact local capability inputs.
pub struct CatalogueCapabilityPreparationFactory {
    catalogue: Arc<RuntimeAgentCatalogue>,
    memory: Option<LocalMemorySystemBinding>,
    knowledge: Option<LocalKnowledgeSystemBinding>,
}

impl CatalogueCapabilityPreparationFactory {
    /// Constructs preparation from an immutable catalogue and explicit bindings.
    pub fn new(
        catalogue: Arc<RuntimeAgentCatalogue>,
        memory: Option<LocalMemorySystemBinding>,
    ) -> Self {
        Self {
            catalogue,
            memory,
            knowledge: None,
        }
    }

    /// Installs one explicit Knowledge source binding.
    pub fn with_knowledge(mut self, knowledge: LocalKnowledgeSystemBinding) -> Self {
        self.knowledge = Some(knowledge);
        self
    }
}

impl LocalCapabilityPreparationFactory for CatalogueCapabilityPreparationFactory {
    fn prepare(
        &self,
        ledger: &SqliteLedger,
        input: LocalCapabilityPreparationInput<'_>,
    ) -> Result<PreparedAgentCapabilities, LocalWorkerError> {
        let installation = self
            .catalogue
            .resolve(
                &input.committed.definition_id,
                &input.committed.definition_revision,
                &input.committed.snapshot_digest,
            )
            .map_err(|_| LocalWorkerError::CapabilityBindingMismatch)?;
        let memory_descriptors: Vec<_> = installation
            .snapshot()
            .capabilities()
            .descriptors
            .iter()
            .filter(|descriptor| descriptor.kind == CapabilityKind::Memory)
            .collect();
        let mut prepared = PreparedAgentCapabilities::default();
        match memory_descriptors.as_slice() {
            [] => {}
            [descriptor] => {
                let binding = self
                    .memory
                    .as_ref()
                    .ok_or(LocalWorkerError::CapabilityBindingMissing)?;
                verify_descriptor(
                    descriptor,
                    &binding.capability_name,
                    &binding.exact_revision,
                    LOCAL_MEMORY_CONTRACT_VERSION,
                    &binding.descriptor_digest,
                )?;
                prepared = prepare_memory(ledger, input, binding)?;
            }
            _ => return Err(LocalWorkerError::CapabilityBindingMismatch),
        }
        let knowledge_descriptors: Vec<_> = installation
            .snapshot()
            .capabilities()
            .descriptors
            .iter()
            .filter(|descriptor| descriptor.kind == CapabilityKind::Knowledge)
            .collect();
        match knowledge_descriptors.as_slice() {
            [] => {}
            [descriptor] => {
                let binding = self
                    .knowledge
                    .as_ref()
                    .ok_or(LocalWorkerError::CapabilityBindingMissing)?;
                verify_descriptor(
                    descriptor,
                    &binding.capability_name,
                    &binding.exact_revision,
                    LOCAL_KNOWLEDGE_CONTRACT_VERSION,
                    &binding.descriptor_digest,
                )?;
                let (retrieval, recovered) = prepare_knowledge(ledger, input, binding)?;
                prepared.knowledge_retrieval = retrieval;
                prepared.recovered_knowledge = recovered;
            }
            _ => return Err(LocalWorkerError::CapabilityBindingMismatch),
        }
        Ok(prepared)
    }
}

fn verify_descriptor(
    descriptor: &garive_config::CapabilityDescriptor,
    name: &str,
    revision: &str,
    contract_version: u64,
    digest: &str,
) -> Result<(), LocalWorkerError> {
    if descriptor.name == name
        && descriptor.exact_revision == revision
        && descriptor.contract_version == contract_version
        && descriptor.descriptor_digest == digest
    {
        Ok(())
    } else {
        Err(LocalWorkerError::CapabilityBindingMismatch)
    }
}

fn prepare_knowledge(
    ledger: &SqliteLedger,
    input: LocalCapabilityPreparationInput<'_>,
    binding: &LocalKnowledgeSystemBinding,
) -> Result<
    (
        Option<PreparedKnowledgeCapability>,
        Option<RecoveredKnowledgeContext>,
    ),
    LocalWorkerError,
> {
    let query = KnowledgeContentBinding::from_inline(entry_content(&input.request.entry));
    let identity = knowledge_identity(input.committed, binding, query.digest())?;
    let request_id = format!("knowledge-request-{identity}");
    let request = existing_knowledge_request(ledger, input, binding, &request_id, query.digest())?
        .map_or_else(
            || {
                KnowledgeRequest::new(
                    &request_id,
                    binding.source.source_id(),
                    binding.source.source_revision(),
                    KnowledgeQueryMode::Keyword,
                    query,
                    Vec::new(),
                    input.committed.committed_position,
                    binding.max_chunks,
                    binding.max_total_bytes,
                    binding.deadline_budget_ms,
                    FreshnessRequirement::CachedAllowed,
                )
            },
            Ok,
        )
        .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    let grant = crate::KnowledgeAccessGrant::new(
        binding.source.source_id(),
        binding.source.source_revision(),
    )
    .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    if let Some(recovered) = recover_completed_knowledge(ledger, input, binding, &request)? {
        return Ok((None, Some(recovered)));
    }
    let prepared = PreparedKnowledgeCapability::new(
        binding.source.clone(),
        request,
        grant,
        format!("knowledge-dispatch-{identity}"),
        binding.connector.clone(),
    )
    .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    Ok((Some(prepared), None))
}

fn existing_knowledge_request(
    ledger: &SqliteLedger,
    input: LocalCapabilityPreparationInput<'_>,
    binding: &LocalKnowledgeSystemBinding,
    request_id: &str,
    query_digest: &str,
) -> Result<Option<KnowledgeRequest>, LocalWorkerError> {
    let snapshot = ledger
        .load_turn(&input.committed.turn_id)
        .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    let mut matches = Vec::new();
    for fact in snapshot.facts.iter().filter(|fact| {
        fact.execution_id.as_ref() == Some(&input.committed.execution_id)
            && fact.kind.as_str() == "knowledge.requested"
    }) {
        let payload = serde_json::from_str::<serde_json::Value>(fact.payload.as_json())
            .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
        if payload
            .get("request_id")
            .and_then(serde_json::Value::as_str)
            == Some(request_id)
        {
            matches.push(payload);
        }
    }
    let ([] | [_]) = matches.as_slice() else {
        return Err(LocalWorkerError::KnowledgePreparationFailed);
    };
    let Some(payload) = matches.first() else {
        return Ok(None);
    };
    let query = payload
        .get("query")
        .and_then(serde_json::Value::as_object)
        .ok_or(LocalWorkerError::KnowledgePreparationFailed)?;
    let query = KnowledgeContentBinding::inline(
        query
            .get("digest")
            .and_then(serde_json::Value::as_str)
            .ok_or(LocalWorkerError::KnowledgePreparationFailed)?,
        query
            .get("inline_utf8")
            .and_then(serde_json::Value::as_str)
            .ok_or(LocalWorkerError::KnowledgePreparationFailed)?,
    )
    .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    let empty_filters = payload
        .pointer("/filters/inline_utf8")
        .and_then(serde_json::Value::as_str)
        == Some("[]");
    if query.digest() != query_digest
        || payload.get("source_id").and_then(serde_json::Value::as_str)
            != Some(binding.source.source_id())
        || payload
            .get("source_revision")
            .and_then(serde_json::Value::as_str)
            != Some(binding.source.source_revision())
        || payload.get("mode").and_then(serde_json::Value::as_str) != Some("keyword")
        || payload
            .get("freshness_kind")
            .and_then(serde_json::Value::as_str)
            != Some("cached_allowed")
        || !empty_filters
    {
        return Err(LocalWorkerError::KnowledgePreparationFailed);
    }
    let request = KnowledgeRequest::new(
        request_id,
        binding.source.source_id(),
        binding.source.source_revision(),
        KnowledgeQueryMode::Keyword,
        query,
        Vec::new(),
        number(payload, "through_position")?,
        u32::try_from(number(payload, "max_chunks")?)
            .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?,
        number(payload, "max_total_bytes")?,
        number(payload, "deadline_budget_ms")?,
        FreshnessRequirement::CachedAllowed,
    )
    .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    if payload
        .get("request_digest")
        .and_then(serde_json::Value::as_str)
        != Some(
            &request
                .request_digest()
                .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?,
        )
    {
        return Err(LocalWorkerError::KnowledgePreparationFailed);
    }
    Ok(Some(request))
}

fn recover_completed_knowledge(
    ledger: &SqliteLedger,
    input: LocalCapabilityPreparationInput<'_>,
    binding: &LocalKnowledgeSystemBinding,
    request: &KnowledgeRequest,
) -> Result<Option<RecoveredKnowledgeContext>, LocalWorkerError> {
    let snapshot = ledger
        .load_turn(&input.committed.turn_id)
        .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    let has_requested = snapshot.facts.iter().any(|fact| {
        fact.execution_id.as_ref() == Some(&input.committed.execution_id)
            && fact.kind.as_str() == "knowledge.requested"
            && serde_json::from_str::<serde_json::Value>(fact.payload.as_json())
                .ok()
                .and_then(|payload| {
                    payload
                        .get("request_id")
                        .and_then(serde_json::Value::as_str)
                        .map(|value| value == request.request_id())
                })
                == Some(true)
    });
    if !has_requested {
        return Ok(None);
    }
    let context = crate::KnowledgeRecoveryContext {
        session_id: input.committed.session_id.clone(),
        turn_id: input.committed.turn_id.clone(),
        execution_id: input.committed.execution_id.clone(),
        through_position: snapshot.through_position,
        request_id: request.request_id().to_owned(),
    };
    let action = crate::derive_knowledge_recovery(ledger, &context)
        .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    let crate::KnowledgeRecoveryAction::ReturnTerminal {
        request_digest,
        terminal_position,
        request_through_position,
        completed: true,
    } = action
    else {
        return Ok(None);
    };
    if request_digest
        != request
            .request_digest()
            .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?
        || request_through_position != request.through_position()
    {
        return Err(LocalWorkerError::KnowledgePreparationFailed);
    }
    let terminal = snapshot
        .facts
        .iter()
        .find(|fact| fact.position == terminal_position)
        .ok_or(LocalWorkerError::KnowledgePreparationFailed)?;
    let payload: serde_json::Value = serde_json::from_str(terminal.payload.as_json())
        .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    let bindings = payload
        .get("evidence")
        .and_then(serde_json::Value::as_array)
        .ok_or(LocalWorkerError::KnowledgePreparationFailed)?;
    let evidence = bindings
        .iter()
        .map(|value| recover_evidence(value, &binding.source))
        .collect::<Result<Vec<_>, _>>()?;
    let completed = complete_knowledge(request, &binding.source, evidence, true)
        .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    let items = completed
        .evidence
        .iter()
        .map(crate::core_bridge::knowledge_input)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    Ok(Some(RecoveredKnowledgeContext {
        terminal_position,
        items,
    }))
}

fn recover_evidence(
    value: &serde_json::Value,
    source: &KnowledgeSourceDescriptor,
) -> Result<KnowledgeEvidence, LocalWorkerError> {
    let text = |name: &str| {
        value
            .get(name)
            .and_then(serde_json::Value::as_str)
            .ok_or(LocalWorkerError::KnowledgePreparationFailed)
    };
    let content_value = value
        .get("content")
        .and_then(serde_json::Value::as_object)
        .ok_or(LocalWorkerError::KnowledgePreparationFailed)?;
    let content = KnowledgeContentBinding::inline(
        content_value
            .get("digest")
            .and_then(serde_json::Value::as_str)
            .ok_or(LocalWorkerError::KnowledgePreparationFailed)?,
        content_value
            .get("inline_utf8")
            .and_then(serde_json::Value::as_str)
            .ok_or(LocalWorkerError::KnowledgePreparationFailed)?,
    )
    .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    let citation = Citation::new(
        citation_scheme(text("citation_kind")?)?,
        text("citation_locator")?,
        optional_text(value, "citation_title")?,
        optional_text(value, "canonical_uri")?,
        text("citation_content_digest")?,
    )
    .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    KnowledgeEvidence::new(
        text("evidence_id")?,
        source.source_id(),
        source.source_revision(),
        optional_text(value, "source_snapshot_digest")?,
        content,
        number(value, "content_byte_length")?,
        citation,
        text("retrieved_at_utc")?,
        freshness(text("freshness")?)?,
        trust(text("trust_class")?)?,
        u16::try_from(number(value, "rank_basis_points")?)
            .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?,
    )
    .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)
}

fn optional_text(
    value: &serde_json::Value,
    name: &str,
) -> Result<Option<String>, LocalWorkerError> {
    value
        .get(name)
        .map(|item| {
            item.as_str()
                .map(str::to_owned)
                .ok_or(LocalWorkerError::KnowledgePreparationFailed)
        })
        .transpose()
}

fn number(value: &serde_json::Value, name: &str) -> Result<u64, LocalWorkerError> {
    value
        .get(name)
        .and_then(serde_json::Value::as_u64)
        .ok_or(LocalWorkerError::KnowledgePreparationFailed)
}

fn citation_scheme(value: &str) -> Result<CitationScheme, LocalWorkerError> {
    match value {
        "uri_fragment" => Ok(CitationScheme::UriFragment),
        "document_offset" => Ok(CitationScheme::DocumentOffset),
        "record_key" => Ok(CitationScheme::RecordKey),
        "opaque_locator" => Ok(CitationScheme::OpaqueLocator),
        _ => Err(LocalWorkerError::KnowledgePreparationFailed),
    }
}

fn freshness(value: &str) -> Result<KnowledgeFreshness, LocalWorkerError> {
    match value {
        "fresh" => Ok(KnowledgeFreshness::Fresh),
        "cached" => Ok(KnowledgeFreshness::Cached),
        "stale" => Ok(KnowledgeFreshness::Stale),
        _ => Err(LocalWorkerError::KnowledgePreparationFailed),
    }
}

fn trust(value: &str) -> Result<KnowledgeTrustClass, LocalWorkerError> {
    match value {
        "curated" => Ok(KnowledgeTrustClass::Curated),
        "first_party" => Ok(KnowledgeTrustClass::FirstParty),
        "third_party" => Ok(KnowledgeTrustClass::ThirdParty),
        "untrusted" => Ok(KnowledgeTrustClass::Untrusted),
        _ => Err(LocalWorkerError::KnowledgePreparationFailed),
    }
}

fn knowledge_identity(
    committed: &crate::CommittedTurn,
    binding: &LocalKnowledgeSystemBinding,
    query_digest: &str,
) -> Result<String, LocalWorkerError> {
    let bytes = serde_jcs::to_vec(&json!({
        "contract": "garive.local-knowledge-preparation",
        "version": 1,
        "execution_id": committed.execution_id.as_str(),
        "source_id": binding.source.source_id(),
        "source_revision": binding.source.source_revision(),
        "request_policy_revision": binding.request_policy_revision,
        "query_digest": query_digest,
    }))
    .map_err(|_| LocalWorkerError::KnowledgePreparationFailed)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn prepare_memory(
    ledger: &SqliteLedger,
    input: LocalCapabilityPreparationInput<'_>,
    binding: &LocalMemorySystemBinding,
) -> Result<PreparedAgentCapabilities, LocalWorkerError> {
    let repository = ledger
        .read_memory_context_repository(
            &binding.control_grant,
            &binding.namespace_id,
            binding.document_limits,
            binding.max_repository_records,
            binding.max_repository_facts,
        )
        .map_err(map_repository_error)?;
    let (repository_revision, records) = repository
        .map(|snapshot| (snapshot.repository_revision, snapshot.records))
        .unwrap_or_default();
    let scores = records
        .iter()
        .map(|record| {
            let bytes = record
                .content()
                .inline_utf8()
                .and_then(|content| u64::try_from(content.len()).ok())
                .ok_or(LocalWorkerError::MemoryPreparationFailed)?;
            MemoryScore::new(record.record_id(), record.revision_id(), 10_000, bytes)
                .map_err(|_| LocalWorkerError::MemoryPreparationFailed)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let query_binding = ContentBinding::from_inline(entry_content(&input.request.entry));
    let query_id = query_id(
        input.committed,
        repository_revision,
        &records,
        &query_binding,
    )?;
    let query = MemoryQuery::new(
        query_id,
        &binding.namespace_id,
        binding.allowed_scopes.clone(),
        MemoryPurpose::Context,
        &binding.retriever_revision,
        query_binding,
        input.committed.committed_position,
        input.recorded_at,
        binding.max_results,
        binding.max_total_bytes,
        false,
        None,
    )
    .map_err(|_| LocalWorkerError::MemoryPreparationFailed)?;
    let grant = MemoryAccessGrant::new(
        &binding.namespace_id,
        binding.allowed_scopes.clone(),
        vec![MemoryPrefix {
            session_id: input.committed.session_id.clone(),
            through_position: input.committed.committed_position,
        }],
        None,
    )
    .map_err(|_| LocalWorkerError::MemoryPreparationFailed)?;
    authorize_memory_query(&grant, &query)
        .map_err(|_| LocalWorkerError::MemoryPreparationFailed)?;
    let planned = plan_memory_retrieval(
        &MemoryRetrievalContext {
            turn_id: input.committed.turn_id.clone(),
            execution_id: input.committed.execution_id.clone(),
            recorded_at: input.recorded_at.to_owned(),
        },
        &records,
        &scores,
        &query,
    )
    .map_err(|_| LocalWorkerError::MemoryPreparationFailed)?;
    Ok(PreparedAgentCapabilities {
        memory_retrieval: Some(planned),
        ..PreparedAgentCapabilities::default()
    })
}

fn entry_content(entry: &AgentEntry) -> String {
    match entry {
        AgentEntry::Start { trusted_input } => trusted_input.clone(),
        AgentEntry::Continue { resume_input } => match resume_input {
            ResumeInput::ExternalInput(value) => value.clone(),
            ResumeInput::Reconciliation(value) => value.clone(),
            ResumeInput::ResourceReady => "resource_ready".to_owned(),
        },
    }
}

fn query_id(
    committed: &crate::CommittedTurn,
    repository_revision: u64,
    records: &[garive_memory::MemoryRecord],
    query: &ContentBinding,
) -> Result<String, LocalWorkerError> {
    let preimage = json!({
        "contract": "garive.local-memory-preparation",
        "version": 1,
        "execution_id": committed.execution_id.as_str(),
        "repository_revision": repository_revision,
        "records": records,
        "query_digest": query.digest(),
    });
    let bytes =
        serde_jcs::to_vec(&preimage).map_err(|_| LocalWorkerError::MemoryPreparationFailed)?;
    Ok(format!("memory-query-{:x}", Sha256::digest(bytes)))
}

fn map_repository_error(error: MemoryRepositoryError) -> LocalWorkerError {
    match error {
        MemoryRepositoryError::Corrupt => LocalWorkerError::MemoryRepositoryCorrupt,
        MemoryRepositoryError::BoundExceeded => LocalWorkerError::MemoryRepositoryBoundExceeded,
        MemoryRepositoryError::Unavailable
        | MemoryRepositoryError::Stale
        | MemoryRepositoryError::Unauthorized => LocalWorkerError::MemoryPreparationFailed,
    }
}

fn valid_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= 128 && value.trim() == value
}

fn valid_digest(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
