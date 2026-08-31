//! Built-in immutable Desktop Knowledge source composition.

use std::sync::Arc;

use garive_knowledge::{
    CitationScheme, KnowledgeQueryMode, KnowledgeSourceDescriptor, KnowledgeSourceKind,
    KnowledgeTrustClass,
};
use garive_runtime::{
    LocalKnowledgeSystemBinding, StaticKnowledgeConnector, StaticKnowledgeDocument,
    SystemKnowledgeConnectorClock,
};

use crate::{
    desktop_agent::{
        DESKTOP_KNOWLEDGE_CAPABILITY_NAME, DESKTOP_KNOWLEDGE_CAPABILITY_REVISION,
        DESKTOP_KNOWLEDGE_DESCRIPTOR_DIGEST,
    },
    system_configuration::KnowledgeDocument,
    DesktopConfigurationError,
};

pub(crate) const CONNECTOR_ID: &str = "desktop.static-system-guide.v1";
pub(crate) const SOURCE_ID: &str = "desktop-system-guide";
pub(crate) const SOURCE_REVISION: &str = "desktop.system-guide.v1";
pub(crate) const SOURCE_SNAPSHOT_DIGEST: &str =
    "c090bf1c5f47526b4c8c466c4de68d9a78e23b3f39de99edcad96628a3ac0a23";

pub(crate) fn build_binding(
    config: &KnowledgeDocument,
) -> Result<LocalKnowledgeSystemBinding, DesktopConfigurationError> {
    if config.connector_id != CONNECTOR_ID
        || config.source_id != SOURCE_ID
        || config.source_revision != SOURCE_REVISION
        || config.source_snapshot_digest != SOURCE_SNAPSHOT_DIGEST
        || config.request_policy_revision != garive_runtime::KEYWORD_CURRENT_INPUT_REVISION
    {
        return Err(DesktopConfigurationError::UnknownProfile);
    }
    let source = KnowledgeSourceDescriptor::new(
        SOURCE_ID,
        SOURCE_REVISION,
        KnowledgeSourceKind::Documentation,
        "garive.desktop.system-guide",
        KnowledgeTrustClass::Curated,
        vec![KnowledgeQueryMode::Keyword],
        "86f69e175d738f4d11fda2a0e72bb78c974cea10e893061f7e955e96861e3209",
        CitationScheme::RecordKey,
        "becc05a3d5e61d0198b08736b94c8247d9a18a102c094c5df8730eee4ff69332",
    )
    .map_err(|_| DesktopConfigurationError::ConstructionFailure)?;
    let documents = [
        (
            "agent-definition",
            "Agent definition",
            "An Agent is an immutable resolved snapshot of instructions, model roles, tools, Memory, Knowledge, governance, context policy and runtime limits.",
        ),
        (
            "durable-execution",
            "Durable execution",
            "Runtime commits execution, Memory and Knowledge lifecycle facts to the ledger before crossing model or connector boundaries, then recovers from durable state.",
        ),
        (
            "system-boundaries",
            "System boundaries",
            "Desktop initializes and persists explicit system configuration. Runtime executes capabilities. Connectors receive construction parameters and do not discover environment configuration.",
        ),
    ]
    .into_iter()
    .map(|(id, title, content)| {
        StaticKnowledgeDocument::new(
            id,
            Some(title.to_owned()),
            content,
            config.max_document_bytes,
        )
        .map_err(|_| DesktopConfigurationError::ConstructionFailure)
    })
    .collect::<Result<Vec<_>, _>>()?;
    let connector = StaticKnowledgeConnector::new(
        source.clone(),
        SOURCE_SNAPSHOT_DIGEST,
        documents,
        config.max_documents,
        config.max_total_document_bytes,
        Arc::new(SystemKnowledgeConnectorClock),
    )
    .map_err(|_| DesktopConfigurationError::ConstructionFailure)?;
    LocalKnowledgeSystemBinding::new(
        DESKTOP_KNOWLEDGE_CAPABILITY_NAME,
        DESKTOP_KNOWLEDGE_CAPABILITY_REVISION,
        DESKTOP_KNOWLEDGE_DESCRIPTOR_DIGEST,
        source,
        &config.request_policy_revision,
        config.max_chunks,
        config.max_total_bytes,
        config.deadline_budget_ms,
        Arc::new(connector),
    )
    .map_err(|_| DesktopConfigurationError::ConstructionFailure)
}
