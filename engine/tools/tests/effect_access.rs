use garive_tools::{
    AccessMode, AccessNamespace, AccessPolicyEntry, InvocationAccessSet, ResourceAccess,
    ToolAccessPolicyV1,
};

#[test]
fn canonical_accesses_sort_and_preserve_exact_keys() {
    let accesses = InvocationAccessSet::new([
        ResourceAccess::new(AccessNamespace::Runtime, "catalog", AccessMode::Read).unwrap(),
        ResourceAccess::new(AccessNamespace::Filesystem, "src/lib.rs", AccessMode::Read).unwrap(),
    ])
    .unwrap();

    assert_eq!(accesses.values()[0].resource_key(), "src/lib.rs");
    assert_eq!(accesses.values()[1].resource_key(), "catalog");
}

#[test]
fn invalid_and_duplicate_exact_resources_fail_closed() {
    assert!(
        ResourceAccess::new(AccessNamespace::Filesystem, "../secret", AccessMode::Read).is_err()
    );
    assert!(
        ResourceAccess::new(AccessNamespace::Filesystem, "src\\secret", AccessMode::Read).is_err()
    );
    assert!(ResourceAccess::new(
        AccessNamespace::Network,
        "HTTPS://example.com:443",
        AccessMode::Read
    )
    .is_err());
    assert!(ResourceAccess::new(
        AccessNamespace::Network,
        "https://example.com:443/path",
        AccessMode::Read
    )
    .is_err());

    let access =
        ResourceAccess::new(AccessNamespace::Filesystem, "src/lib.rs", AccessMode::Read).unwrap();
    assert!(InvocationAccessSet::new([access.clone(), access]).is_err());
}

#[test]
fn policy_coverage_is_namespace_specific_and_segment_aware() {
    let policy = ToolAccessPolicyV1::new(
        "policy-v1",
        [AccessPolicyEntry::new("src", [AccessMode::Read]).unwrap()],
        [],
        [AccessPolicyEntry::new("https://example.com:443", [AccessMode::Read]).unwrap()],
        [],
        4,
        4096,
    )
    .unwrap();

    let allowed = InvocationAccessSet::new([
        ResourceAccess::new(
            AccessNamespace::Filesystem,
            "src/domain/lib.rs",
            AccessMode::Read,
        )
        .unwrap(),
        ResourceAccess::new(
            AccessNamespace::Network,
            "https://example.com:443",
            AccessMode::Read,
        )
        .unwrap(),
    ])
    .unwrap();
    assert!(policy.covers(&allowed));

    let sibling = InvocationAccessSet::new([ResourceAccess::new(
        AccessNamespace::Filesystem,
        "src-old/lib.rs",
        AccessMode::Read,
    )
    .unwrap()])
    .unwrap();
    assert!(!policy.covers(&sibling));
}
