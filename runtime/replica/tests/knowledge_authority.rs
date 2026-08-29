use garive_knowledge::{
    ContentBinding, FreshnessRequirement, KnowledgeErrorCode, KnowledgeQueryMode, KnowledgeRequest,
};
use garive_runtime::KnowledgeAccessGrant;

#[test]
fn source_grants_are_exact_and_namespace_isolated() {
    let request = KnowledgeRequest::new(
        "request",
        "docs",
        "revision-1",
        KnowledgeQueryMode::Keyword,
        ContentBinding::from_inline("query"),
        vec![],
        7,
        1,
        64,
        1_000,
        FreshnessRequirement::CachedAllowed,
    )
    .unwrap();
    KnowledgeAccessGrant::new("docs", "revision-1")
        .unwrap()
        .authorize(&request)
        .unwrap();
    assert_eq!(
        KnowledgeAccessGrant::new("private-docs", "revision-1")
            .unwrap()
            .authorize(&request),
        Err(KnowledgeErrorCode::SourceDenied),
    );
    assert_eq!(
        KnowledgeAccessGrant::new("docs", "revision-2")
            .unwrap()
            .authorize(&request),
        Err(KnowledgeErrorCode::SourceDenied),
    );
}
