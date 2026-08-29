use bench::{parse_cases, BenchErrorCode, CaseLoadLimits};

const SMOKE: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/cases/swe-bench-lite-smoke.jsonl"
));

#[test]
fn official_smoke_case_loads_without_gold_material() {
    let cases = parse_cases(SMOKE, limits()).unwrap();
    assert_eq!(cases.len(), 1);
    let case = &cases[0];
    assert_eq!(case.instance_id.as_str(), "astropy__astropy-12907");
    assert_eq!(case.repository, "astropy/astropy");
    assert_eq!(case.base_commit.len(), 40);
    assert_eq!(case.fail_to_pass.len(), 2);
    assert_eq!(case.pass_to_pass.len(), 13);
    let source = String::from_utf8(SMOKE.to_vec()).unwrap();
    assert!(!source.contains("\"patch\""));
    assert!(!source.contains("test_patch"));
    assert!(!source.contains("hints"));
}

#[test]
fn unknown_duplicate_gold_and_duplicate_case_fields_fail_closed() {
    let source = String::from_utf8(SMOKE.to_vec()).unwrap();
    for changed in [
        source.replace("\"version\":\"4.3\"", "\"version\":\"4.3\",\"future\":true"),
        source.replace(
            "\"version\":\"4.3\"",
            "\"version\":\"4.3\",\"version\":\"4.3\"",
        ),
        source.replace(
            "\"version\":\"4.3\"",
            "\"version\":\"4.3\",\"patch\":\"gold\"",
        ),
    ] {
        assert_eq!(
            parse_cases(changed.as_bytes(), limits())
                .unwrap_err()
                .code(),
            BenchErrorCode::InvalidCaseDocument
        );
    }
    let duplicated = format!("{source}{source}");
    assert_eq!(
        parse_cases(duplicated.as_bytes(), limits())
            .unwrap_err()
            .code(),
        BenchErrorCode::DuplicateCase
    );
}

#[test]
fn bounds_repository_commit_and_test_sets_fail_closed() {
    let source = String::from_utf8(SMOKE.to_vec()).unwrap();
    assert_eq!(
        parse_cases(
            SMOKE,
            CaseLoadLimits {
                max_cases: 0,
                ..limits()
            }
        )
        .unwrap_err()
        .code(),
        BenchErrorCode::InvalidLimits
    );
    assert_eq!(
        parse_cases(
            SMOKE,
            CaseLoadLimits {
                max_document_bytes: 1,
                ..limits()
            }
        )
        .unwrap_err()
        .code(),
        BenchErrorCode::DocumentTooLarge
    );
    let bad_repo = source.replace("astropy/astropy", "../astropy");
    assert_eq!(
        parse_cases(bad_repo.as_bytes(), limits())
            .unwrap_err()
            .code(),
        BenchErrorCode::InvalidCase
    );
    let bad_commit = source.replace("d16bfe05a744909de4b27f5875fe0d4ed41ce607", "not-a-commit");
    assert_eq!(
        parse_cases(bad_commit.as_bytes(), limits())
            .unwrap_err()
            .code(),
        BenchErrorCode::InvalidCase
    );
    let overlap = source.replace(
        "astropy/modeling/tests/test_separable.py::test_coord_matrix",
        "astropy/modeling/tests/test_separable.py::test_separable[compound_model6-result6]",
    );
    assert_eq!(
        parse_cases(overlap.as_bytes(), limits())
            .unwrap_err()
            .code(),
        BenchErrorCode::InvalidTestSet
    );
}

fn limits() -> CaseLoadLimits {
    CaseLoadLimits {
        max_cases: 16,
        max_document_bytes: 1_000_000,
        max_line_bytes: 1_000_000,
        max_problem_bytes: 100_000,
        max_tests_per_group: 1_000,
    }
}
