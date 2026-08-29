use std::{future::Future, pin::Pin, sync::Arc};

use garive_core::{AttributedKnowledge, KnowledgeCitationAttribution};
use garive_knowledge::{
    complete_knowledge, CitationScheme, KnowledgeErrorCode, KnowledgeEvidence, KnowledgeFreshness,
    KnowledgeRequest, KnowledgeSourceDescriptor, KnowledgeTrustClass,
};

use super::execution::CommitCoordinator;
use super::{
    plan_knowledge_completed, plan_knowledge_dispatched, plan_knowledge_failed,
    plan_knowledge_requested, KnowledgeFailurePhase, KnowledgeFailureReason,
    KnowledgeLifecycleContext, PreparedKnowledgeRequest,
};
use crate::{DurableExecutionError, RuntimeCommandError};

/// Boxed provider-neutral connector future owned by the configured connector.
pub type KnowledgeConnectorFuture<'a> =
    Pin<Box<dyn Future<Output = KnowledgeConnectorOutcome> + Send + 'a>>;

/// Configured connector boundary; implementations receive no ambient Garive configuration.
pub trait KnowledgeConnector: Send + Sync {
    /// Retrieves evidence for one exact source/request pair.
    fn retrieve<'a>(
        &'a self,
        source: &'a KnowledgeSourceDescriptor,
        request: &'a KnowledgeRequest,
    ) -> KnowledgeConnectorFuture<'a>;
}

/// Sanitized terminal response from a configured Knowledge connector.
pub enum KnowledgeConnectorOutcome {
    /// Returned evidence and whether connector order is semantic.
    Completed {
        /// Exact candidate evidence; Runtime validates and bounds it.
        evidence: Vec<KnowledgeEvidence>,
        /// Whether connector order must be preserved.
        connector_order_stable: bool,
    },
    /// A requested filter is unsupported.
    FilterUnsupported,
    /// The requested freshness cannot be supplied.
    FreshnessUnavailable,
    /// The connector is temporarily unavailable.
    Unavailable {
        /// Optional non-zero retry hint.
        retry_after_ms: Option<u64>,
    },
    /// The connector rejected the request.
    Rejected,
    /// Dispatch has no trustworthy terminal result.
    Uncertain,
}

/// Frozen source authorization supplied explicitly by Host composition.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct KnowledgeAccessGrant {
    source_id: String,
    source_revision: String,
}

impl KnowledgeAccessGrant {
    /// Creates an exact source/revision grant without ambient configuration.
    pub fn new(
        source_id: impl Into<String>,
        source_revision: impl Into<String>,
    ) -> Result<Self, RuntimeCommandError> {
        let value = Self {
            source_id: source_id.into(),
            source_revision: source_revision.into(),
        };
        if value.source_id.is_empty() || value.source_revision.is_empty() {
            Err(RuntimeCommandError::InvalidCommand)
        } else {
            Ok(value)
        }
    }

    fn allows(&self, request: &KnowledgeRequest) -> bool {
        self.source_id == request.source_id() && self.source_revision == request.source_revision()
    }
}

/// Exact K0 work supplied to Runtime before Core invocation.
pub struct PreparedKnowledgeCapability {
    source: KnowledgeSourceDescriptor,
    request: KnowledgeRequest,
    grant: KnowledgeAccessGrant,
    dispatch_attempt_id: String,
    connector: Arc<dyn KnowledgeConnector>,
}

impl PreparedKnowledgeCapability {
    /// Freezes source, request, authority, attempt and configured connector.
    pub fn new(
        source: KnowledgeSourceDescriptor,
        request: KnowledgeRequest,
        grant: KnowledgeAccessGrant,
        dispatch_attempt_id: impl Into<String>,
        connector: Arc<dyn KnowledgeConnector>,
    ) -> Result<Self, RuntimeCommandError> {
        let value = Self {
            source,
            request,
            grant,
            dispatch_attempt_id: dispatch_attempt_id.into(),
            connector,
        };
        if value.dispatch_attempt_id.is_empty() {
            Err(RuntimeCommandError::InvalidCommand)
        } else {
            Ok(value)
        }
    }
}

pub(super) async fn execute_knowledge_capability(
    coordinator: &mut CommitCoordinator<'_>,
    context: &KnowledgeLifecycleContext,
    capability: PreparedKnowledgeCapability,
) -> Result<Vec<AttributedKnowledge>, DurableExecutionError> {
    let prepared = plan_knowledge_requested(context, &capability.request)
        .map_err(DurableExecutionError::Command)?;
    coordinator.commit(vec![prepared.fact.clone()])?;
    if !capability.grant.allows(&capability.request) {
        return commit_failure(
            coordinator,
            context,
            &prepared,
            KnowledgeFailurePhase::PreDispatch,
            KnowledgeFailureReason::SourceDenied,
            None,
        );
    }
    if let Err(error) = capability.request.validate_source(&capability.source) {
        return commit_failure(
            coordinator,
            context,
            &prepared,
            KnowledgeFailurePhase::PreDispatch,
            failure_reason(error.code()),
            None,
        );
    }
    let dispatched = plan_knowledge_dispatched(context, &prepared, &capability.dispatch_attempt_id)
        .map_err(DurableExecutionError::Command)?;
    coordinator.commit(vec![dispatched])?;
    match capability
        .connector
        .retrieve(&capability.source, &capability.request)
        .await
    {
        KnowledgeConnectorOutcome::Completed {
            evidence,
            connector_order_stable,
        } => {
            if let Err(error) = complete_knowledge(
                &capability.request,
                &capability.source,
                evidence.clone(),
                connector_order_stable,
            ) {
                return commit_failure(
                    coordinator,
                    context,
                    &prepared,
                    KnowledgeFailurePhase::ResponseValidation,
                    failure_reason(error.code()),
                    None,
                );
            }
            let planned = plan_knowledge_completed(
                context,
                &prepared,
                &capability.source,
                &capability.request,
                evidence,
                connector_order_stable,
            )
            .map_err(DurableExecutionError::Command)?;
            let attributed = planned
                .completed
                .evidence
                .iter()
                .map(attributed_knowledge)
                .collect::<Result<Vec<_>, _>>()?;
            coordinator.commit(vec![planned.fact])?;
            Ok(attributed)
        }
        KnowledgeConnectorOutcome::FilterUnsupported => commit_failure(
            coordinator,
            context,
            &prepared,
            KnowledgeFailurePhase::ResponseValidation,
            KnowledgeFailureReason::FilterUnsupported,
            None,
        ),
        KnowledgeConnectorOutcome::FreshnessUnavailable => commit_failure(
            coordinator,
            context,
            &prepared,
            KnowledgeFailurePhase::ResponseValidation,
            KnowledgeFailureReason::FreshnessUnavailable,
            None,
        ),
        KnowledgeConnectorOutcome::Unavailable { retry_after_ms } => {
            let (reason, retry) = if retry_after_ms == Some(0) {
                (KnowledgeFailureReason::InvalidQuery, None)
            } else {
                (KnowledgeFailureReason::Unavailable, retry_after_ms)
            };
            commit_failure(
                coordinator,
                context,
                &prepared,
                KnowledgeFailurePhase::ResponseValidation,
                reason,
                retry,
            )
        }
        KnowledgeConnectorOutcome::Rejected => commit_failure(
            coordinator,
            context,
            &prepared,
            KnowledgeFailurePhase::ResponseValidation,
            KnowledgeFailureReason::Rejected,
            None,
        ),
        KnowledgeConnectorOutcome::Uncertain => commit_failure(
            coordinator,
            context,
            &prepared,
            KnowledgeFailurePhase::Dispatched,
            KnowledgeFailureReason::Uncertain,
            None,
        ),
    }
}

fn commit_failure(
    coordinator: &mut CommitCoordinator<'_>,
    context: &KnowledgeLifecycleContext,
    prepared: &PreparedKnowledgeRequest,
    phase: KnowledgeFailurePhase,
    reason: KnowledgeFailureReason,
    retry_after_ms: Option<u64>,
) -> Result<Vec<AttributedKnowledge>, DurableExecutionError> {
    let fact = plan_knowledge_failed(context, prepared, phase, reason, retry_after_ms)
        .map_err(DurableExecutionError::Command)?;
    coordinator.commit(vec![fact])?;
    Ok(Vec::new())
}

fn failure_reason(code: KnowledgeErrorCode) -> KnowledgeFailureReason {
    match code {
        KnowledgeErrorCode::InvalidQuery => KnowledgeFailureReason::InvalidQuery,
        KnowledgeErrorCode::SourceNotFound => KnowledgeFailureReason::SourceNotFound,
        KnowledgeErrorCode::SourceRevisionMismatch => {
            KnowledgeFailureReason::SourceRevisionMismatch
        }
        KnowledgeErrorCode::SourceDenied => KnowledgeFailureReason::SourceDenied,
        KnowledgeErrorCode::FilterUnsupported => KnowledgeFailureReason::FilterUnsupported,
        KnowledgeErrorCode::FreshnessUnavailable => KnowledgeFailureReason::FreshnessUnavailable,
        KnowledgeErrorCode::ConnectorUnavailable => KnowledgeFailureReason::Unavailable,
        KnowledgeErrorCode::ConnectorRejected => KnowledgeFailureReason::Rejected,
        KnowledgeErrorCode::RetrievalUncertain => KnowledgeFailureReason::Uncertain,
        KnowledgeErrorCode::CitationInvalid => KnowledgeFailureReason::CitationInvalid,
        KnowledgeErrorCode::ContentDigestMismatch => KnowledgeFailureReason::ContentDigestMismatch,
        KnowledgeErrorCode::LimitExceeded => KnowledgeFailureReason::LimitExceeded,
        KnowledgeErrorCode::DurabilityFailure => KnowledgeFailureReason::DurabilityFailure,
        KnowledgeErrorCode::CorruptKnowledgeState => KnowledgeFailureReason::CorruptKnowledgeState,
    }
}

fn attributed_knowledge(
    value: &KnowledgeEvidence,
) -> Result<AttributedKnowledge, DurableExecutionError> {
    let content_utf8 = value
        .content()
        .inline_utf8()
        .ok_or(DurableExecutionError::Command(
            RuntimeCommandError::InvalidCommand,
        ))?;
    Ok(AttributedKnowledge {
        source_id: value.source_id().into(),
        source_revision: value.source_revision().into(),
        evidence_id: value.evidence_id().into(),
        source_snapshot_digest: value.source_snapshot_digest().map(Into::into),
        content_digest: value.content().digest().into(),
        content_utf8: content_utf8.into(),
        content_byte_length: value.content_byte_length(),
        citation: KnowledgeCitationAttribution {
            locator_kind: citation_kind(value.citation().locator_kind()).into(),
            locator: value.citation().locator().into(),
            title: value.citation().title().map(Into::into),
            canonical_uri: value.citation().canonical_uri().map(Into::into),
            content_digest: value.citation().content_digest().into(),
        },
        retrieved_at_utc: value.retrieved_at_utc().into(),
        freshness: freshness(value.freshness()).into(),
        trust_class: trust(value.trust_class()).into(),
        rank_basis_points: value.rank_basis_points(),
    })
}

const fn citation_kind(value: CitationScheme) -> &'static str {
    match value {
        CitationScheme::UriFragment => "uri_fragment",
        CitationScheme::DocumentOffset => "document_offset",
        CitationScheme::RecordKey => "record_key",
        CitationScheme::OpaqueLocator => "opaque_locator",
    }
}
const fn freshness(value: KnowledgeFreshness) -> &'static str {
    match value {
        KnowledgeFreshness::Fresh => "fresh",
        KnowledgeFreshness::Cached => "cached",
        KnowledgeFreshness::Stale => "stale",
    }
}
const fn trust(value: KnowledgeTrustClass) -> &'static str {
    match value {
        KnowledgeTrustClass::Curated => "curated",
        KnowledgeTrustClass::FirstParty => "first_party",
        KnowledgeTrustClass::ThirdParty => "third_party",
        KnowledgeTrustClass::Untrusted => "untrusted",
    }
}
