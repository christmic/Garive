use std::sync::Arc;

use garive_knowledge::{
    CitationScheme, ContentBinding, FreshnessRequirement, KnowledgeFilter, KnowledgeFilterOperator,
    KnowledgeFilterValue, KnowledgeQueryMode, KnowledgeRequest, KnowledgeSourceDescriptor,
    KnowledgeSourceKind, KnowledgeTrustClass,
};
use garive_runtime::{
    KnowledgeConnector, KnowledgeConnectorClock, KnowledgeConnectorOutcome,
    StaticKnowledgeConnector, StaticKnowledgeDocument, StaticKnowledgeError,
};

const RECORDED_AT: &str = "2026-08-31T12:00:00Z";

struct FixedClock;

impl KnowledgeConnectorClock for FixedClock {
    fn recorded_at(&self) -> String {
        RECORDED_AT.to_owned()
    }
}

fn source(revision: &str) -> KnowledgeSourceDescriptor {
    KnowledgeSourceDescriptor::new(
        "system-guide",
        revision,
        KnowledgeSourceKind::Documentation,
        "garive.system-guide",
        KnowledgeTrustClass::Curated,
        vec![KnowledgeQueryMode::Keyword],
        "a".repeat(64),
        CitationScheme::RecordKey,
        "b".repeat(64),
    )
    .expect("valid source")
}

fn document(id: &str, title: &str, content: &str) -> StaticKnowledgeDocument {
    StaticKnowledgeDocument::new(id, Some(title.to_owned()), content, 1_024)
        .expect("valid document")
}

fn connector() -> StaticKnowledgeConnector {
    StaticKnowledgeConnector::new(
        source("v1"),
        "c".repeat(64),
        vec![
            document("agent", "Agent", "Agent configuration and runtime loop"),
            document(
                "memory",
                "Memory",
                "Memory context and durable agent history",
            ),
            document("sandbox", "Sandbox", "Sandbox policy and process isolation"),
        ],
        8,
        4_096,
        Arc::new(FixedClock),
    )
    .expect("valid connector")
}

fn request(
    query: &str,
    freshness: FreshnessRequirement,
    filters: Vec<KnowledgeFilter>,
) -> KnowledgeRequest {
    KnowledgeRequest::new(
        "request-1",
        "system-guide",
        "v1",
        KnowledgeQueryMode::Keyword,
        ContentBinding::from_inline(query),
        filters,
        17,
        4,
        4_096,
        2_000,
        freshness,
    )
    .expect("valid request")
}

#[tokio::test]
async fn retrieves_attributed_evidence_in_canonical_document_order() {
    let source = source("v1");
    let outcome = connector()
        .retrieve(
            &source,
            &request("agent memory", FreshnessRequirement::Revalidate, vec![]),
        )
        .await;

    let KnowledgeConnectorOutcome::Completed {
        evidence,
        connector_order_stable,
    } = outcome
    else {
        panic!("expected completed retrieval");
    };
    assert!(!connector_order_stable);
    assert_eq!(evidence.len(), 2);
    assert_eq!(evidence[0].citation().locator(), "agent");
    assert_eq!(evidence[1].citation().locator(), "memory");
    assert_eq!(evidence[0].rank_basis_points(), 5_000);
    assert_eq!(evidence[1].rank_basis_points(), 10_000);
    assert!(evidence
        .iter()
        .all(|item| item.retrieved_at_utc() == RECORDED_AT));
    assert!(evidence
        .iter()
        .all(|item| item.source_snapshot_digest() == Some(&"c".repeat(64))));
}

#[tokio::test]
async fn rejects_filters_and_unavailable_exact_snapshots_explicitly() {
    let source = source("v1");
    let filter = KnowledgeFilter::new(
        "section",
        KnowledgeFilterOperator::Equal,
        KnowledgeFilterValue::String("runtime".to_owned()),
    )
    .expect("valid filter");
    assert!(matches!(
        connector()
            .retrieve(
                &source,
                &request("agent", FreshnessRequirement::CachedAllowed, vec![filter]),
            )
            .await,
        KnowledgeConnectorOutcome::FilterUnsupported
    ));
    assert!(matches!(
        connector()
            .retrieve(
                &source,
                &request(
                    "agent",
                    FreshnessRequirement::ExactSnapshot {
                        snapshot_digest: "d".repeat(64),
                    },
                    vec![],
                ),
            )
            .await,
        KnowledgeConnectorOutcome::FreshnessUnavailable
    ));
}

#[tokio::test]
async fn rejects_a_different_source_revision_before_dispatch_result_use() {
    let source = source("v2");
    assert!(matches!(
        connector()
            .retrieve(
                &source,
                &request("agent", FreshnessRequirement::CachedAllowed, vec![]),
            )
            .await,
        KnowledgeConnectorOutcome::Rejected
    ));
}

#[test]
fn constructor_enforces_canonical_identity_and_collection_bounds() {
    let duplicate = vec![
        document("agent", "One", "one"),
        document("agent", "Two", "two"),
    ];
    assert_eq!(
        StaticKnowledgeConnector::new(
            source("v1"),
            "c".repeat(64),
            duplicate,
            2,
            100,
            Arc::new(FixedClock),
        )
        .err(),
        Some(StaticKnowledgeError::InvalidConfiguration)
    );
    assert_eq!(
        StaticKnowledgeConnector::new(
            source("v1"),
            "c".repeat(64),
            vec![document("agent", "Agent", "four")],
            1,
            3,
            Arc::new(FixedClock),
        )
        .err(),
        Some(StaticKnowledgeError::InvalidConfiguration)
    );
}
