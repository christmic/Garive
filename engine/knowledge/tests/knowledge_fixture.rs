use garive_knowledge::{
    complete_knowledge, Citation, CitationScheme, ContentBinding, FreshnessRequirement,
    KnowledgeErrorCode, KnowledgeEvidence, KnowledgeFilter, KnowledgeFilterOperator,
    KnowledgeFilterValue, KnowledgeFreshness, KnowledgeQueryMode, KnowledgeRequest,
    KnowledgeSourceDescriptor, KnowledgeSourceKind, KnowledgeTrustClass,
};
use serde_json::Value;

fn fixture() -> Value {
    serde_json::from_str(include_str!(
        "../../../spec/fixtures/agent/knowledge-retrieval-v1.json"
    ))
    .unwrap()
}
fn text(value: &Value, key: &str) -> String {
    value[key].as_str().unwrap().into()
}
fn content(value: &Value) -> ContentBinding {
    ContentBinding::inline(text(value, "digest"), text(value, "inline_utf8")).unwrap()
}
fn source(root: &Value) -> KnowledgeSourceDescriptor {
    let value = &root["source"];
    KnowledgeSourceDescriptor::new(
        text(value, "source_id"),
        text(value, "source_revision"),
        KnowledgeSourceKind::Documentation,
        text(value, "content_domain"),
        KnowledgeTrustClass::Curated,
        vec![KnowledgeQueryMode::Keyword, KnowledgeQueryMode::Semantic],
        text(value, "freshness_policy_digest"),
        CitationScheme::UriFragment,
        text(value, "capability_metadata_digest"),
    )
    .unwrap()
}
fn filters(root: &Value) -> Vec<KnowledgeFilter> {
    root["request"]["filters"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            let operator = match value["operator"].as_str().unwrap() {
                "equal" => KnowledgeFilterOperator::Equal,
                "greater_than_or_equal" => KnowledgeFilterOperator::GreaterThanOrEqual,
                other => panic!("unknown operator {other}"),
            };
            let filter_value = if let Some(number) = value["value"].as_i64() {
                KnowledgeFilterValue::Integer(number)
            } else {
                KnowledgeFilterValue::String(text(value, "value"))
            };
            KnowledgeFilter::new(text(value, "field"), operator, filter_value).unwrap()
        })
        .collect()
}
fn request(root: &Value, freshness: FreshnessRequirement) -> KnowledgeRequest {
    let value = &root["request"];
    KnowledgeRequest::new(
        text(value, "request_id"),
        text(value, "source_id"),
        text(value, "source_revision"),
        KnowledgeQueryMode::Keyword,
        content(&value["query"]),
        filters(root),
        value["through_position"].as_u64().unwrap(),
        value["max_chunks"].as_u64().unwrap() as u32,
        value["max_total_bytes"].as_u64().unwrap(),
        value["deadline_budget_ms"].as_u64().unwrap(),
        freshness,
    )
    .unwrap()
}
fn evidence(
    root: &Value,
    freshness: KnowledgeFreshness,
    snapshot: Option<String>,
) -> Vec<KnowledgeEvidence> {
    root["evidence"]
        .as_array()
        .unwrap()
        .iter()
        .map(|value| {
            let content = content(&value["content"]);
            KnowledgeEvidence::new(
                text(value, "evidence_id"),
                "docs",
                "1",
                snapshot.clone(),
                content.clone(),
                value["content_byte_length"].as_u64().unwrap(),
                Citation::new(
                    CitationScheme::UriFragment,
                    text(value, "locator"),
                    None,
                    Some(format!(
                        "https://example.test/{}",
                        text(value, "evidence_id")
                    )),
                    content.digest(),
                )
                .unwrap(),
                "2026-08-29T00:00:00Z",
                freshness,
                KnowledgeTrustClass::Curated,
                value["rank_basis_points"].as_u64().unwrap() as u16,
            )
            .unwrap()
        })
        .collect()
}

#[test]
fn request_digest_and_filter_binding_are_frozen() {
    let root = fixture();
    let ordinary = request(&root, FreshnessRequirement::CachedAllowed);
    assert_eq!(
        ordinary.request_digest().unwrap(),
        root["request"]["expected_request_digest"]
    );
    assert_eq!(
        ordinary.filters_binding().unwrap().digest(),
        root["request"]["expected_filters_digest"]
    );
    ordinary.validate_source(&source(&root)).unwrap();
    let duplicate = KnowledgeFilter::new(
        "year",
        KnowledgeFilterOperator::Equal,
        KnowledgeFilterValue::Integer(2025),
    )
    .unwrap();
    let mut duplicated = filters(&root);
    duplicated.push(duplicate);
    assert_eq!(
        KnowledgeRequest::new(
            "bad",
            "docs",
            "1",
            KnowledgeQueryMode::Keyword,
            ContentBinding::from_inline("x"),
            duplicated,
            1,
            1,
            1,
            1,
            FreshnessRequirement::CachedAllowed
        )
        .unwrap_err()
        .code(),
        KnowledgeErrorCode::InvalidQuery,
    );
}

#[test]
fn ordering_freshness_and_bounds_follow_shared_vectors() {
    let root = fixture();
    let ordinary = request(&root, FreshnessRequirement::CachedAllowed);
    for case in root["ordering_cases"].as_array().unwrap() {
        let completed = complete_knowledge(
            &ordinary,
            &source(&root),
            evidence(&root, KnowledgeFreshness::Fresh, None),
            case["connector_order_stable"].as_bool().unwrap(),
        )
        .unwrap();
        let ids: Vec<_> = completed
            .evidence
            .iter()
            .map(|value| value.evidence_id())
            .collect();
        let expected: Vec<_> = case["expected_ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert_eq!(ids, expected, "{}", case["name"]);
        assert_eq!(completed.truncated, case["truncated"]);
    }
    let revalidate = request(&root, FreshnessRequirement::Revalidate);
    assert_eq!(
        complete_knowledge(
            &revalidate,
            &source(&root),
            evidence(&root, KnowledgeFreshness::Stale, None),
            false
        )
        .unwrap_err()
        .code(),
        KnowledgeErrorCode::InvalidQuery,
    );
    let exact = request(
        &root,
        FreshnessRequirement::ExactSnapshot {
            snapshot_digest: "c".repeat(64),
        },
    );
    assert!(complete_knowledge(
        &exact,
        &source(&root),
        evidence(&root, KnowledgeFreshness::Cached, Some("c".repeat(64))),
        false
    )
    .is_ok());
    assert_eq!(
        complete_knowledge(
            &exact,
            &source(&root),
            evidence(&root, KnowledgeFreshness::Cached, Some("d".repeat(64))),
            false
        )
        .unwrap_err()
        .code(),
        KnowledgeErrorCode::InvalidQuery,
    );
}
