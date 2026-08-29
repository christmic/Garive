use std::collections::BTreeSet;

use garive_eval::EvaluationCaseId;
use serde::Deserialize;

use crate::{unique_json::unique_json, BenchError, BenchErrorCode};

/// Exact official public dataset admitted by one B0 run.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SweDataset {
    /// Public SWE-bench Lite test split.
    Lite,
    /// Public SWE-bench Verified test split.
    Verified,
}

impl SweDataset {
    /// Returns the exact official dataset identity used by the harness.
    pub const fn official_name(self) -> &'static str {
        match self {
            Self::Lite => "SWE-bench/SWE-bench_Lite",
            Self::Verified => "SWE-bench/SWE-bench_Verified",
        }
    }
}

/// Explicit bounds for one official JSONL case source.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaseLoadLimits {
    /// Maximum number of source records.
    pub max_cases: usize,
    /// Maximum bytes in the complete JSONL source.
    pub max_document_bytes: usize,
    /// Maximum bytes in one JSON record.
    pub max_line_bytes: usize,
    /// Maximum bytes in one problem statement.
    pub max_problem_bytes: usize,
    /// Maximum test identities in either official group.
    pub max_tests_per_group: usize,
}

impl CaseLoadLimits {
    fn validate(self) -> Result<Self, BenchError> {
        if self.max_cases == 0
            || self.max_document_bytes == 0
            || self.max_line_bytes == 0
            || self.max_problem_bytes == 0
            || self.max_tests_per_group == 0
        {
            Err(BenchError::new(BenchErrorCode::InvalidLimits))
        } else {
            Ok(self)
        }
    }
}

/// Validated public SWE-bench case without gold patch or hidden hints.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SweCase {
    /// Official instance identity.
    pub instance_id: EvaluationCaseId,
    /// Exact `owner/repository` identity.
    pub repository: String,
    /// Exact 40-hex repository base commit.
    pub base_commit: String,
    /// Public issue statement supplied to the Agent intake adapter.
    pub problem_statement: String,
    /// Official repository version label.
    pub version: String,
    /// Tests required to change from failing to passing.
    pub fail_to_pass: Vec<String>,
    /// Regression tests required to remain passing.
    pub pass_to_pass: Vec<String>,
}

/// Parses an explicit official JSONL export under fail-closed limits.
pub fn parse_cases(bytes: &[u8], limits: CaseLoadLimits) -> Result<Vec<SweCase>, BenchError> {
    let limits = limits.validate()?;
    if bytes.len() > limits.max_document_bytes {
        return Err(BenchError::new(BenchErrorCode::DocumentTooLarge));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| BenchError::new(BenchErrorCode::InvalidCaseDocument))?;
    let mut cases = Vec::new();
    let mut identities = BTreeSet::new();
    for line in text.lines() {
        if line.is_empty() {
            return Err(BenchError::new(BenchErrorCode::InvalidCaseDocument));
        }
        if line.len() > limits.max_line_bytes {
            return Err(BenchError::new(BenchErrorCode::LineTooLarge));
        }
        if cases.len() == limits.max_cases {
            return Err(BenchError::new(BenchErrorCode::TooManyCases));
        }
        let raw: RawCase = serde_json::from_value(unique_json(line.as_bytes())?)
            .map_err(|_| BenchError::new(BenchErrorCode::InvalidCaseDocument))?;
        let case = validate_case(raw, limits)?;
        if !identities.insert(case.instance_id.clone()) {
            return Err(BenchError::new(BenchErrorCode::DuplicateCase));
        }
        cases.push(case);
    }
    if cases.is_empty() {
        return Err(BenchError::new(BenchErrorCode::InvalidCaseDocument));
    }
    Ok(cases)
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RawCase {
    instance_id: String,
    repo: String,
    base_commit: String,
    problem_statement: String,
    version: String,
    #[serde(rename = "FAIL_TO_PASS")]
    fail_to_pass: Vec<String>,
    #[serde(rename = "PASS_TO_PASS")]
    pass_to_pass: Vec<String>,
}

fn validate_case(raw: RawCase, limits: CaseLoadLimits) -> Result<SweCase, BenchError> {
    if raw.instance_id.is_empty()
        || raw.instance_id.len() > 256
        || !raw
            .instance_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
        || !valid_repository(&raw.repo)
        || raw.base_commit.len() != 40
        || !raw.base_commit.bytes().all(|byte| byte.is_ascii_hexdigit())
        || raw.problem_statement.is_empty()
        || raw.problem_statement.len() > limits.max_problem_bytes
        || raw.version.is_empty()
        || raw.version.len() > 64
    {
        return Err(BenchError::new(BenchErrorCode::InvalidCase));
    }
    validate_tests(
        &raw.fail_to_pass,
        &raw.pass_to_pass,
        limits.max_tests_per_group,
    )?;
    let instance_id = EvaluationCaseId::new(raw.instance_id)
        .map_err(|_| BenchError::new(BenchErrorCode::InvalidCase))?;
    Ok(SweCase {
        instance_id,
        repository: raw.repo,
        base_commit: raw.base_commit.to_ascii_lowercase(),
        problem_statement: raw.problem_statement,
        version: raw.version,
        fail_to_pass: raw.fail_to_pass,
        pass_to_pass: raw.pass_to_pass,
    })
}

fn valid_repository(value: &str) -> bool {
    let mut components = value.split('/');
    let valid_component = |component: &str| {
        !component.is_empty()
            && !matches!(component, "." | "..")
            && component.len() <= 128
            && component
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    };
    matches!(components.next(), Some(owner) if valid_component(owner))
        && matches!(components.next(), Some(repository) if valid_component(repository))
        && components.next().is_none()
}

fn validate_tests(fail: &[String], pass: &[String], maximum: usize) -> Result<(), BenchError> {
    if fail.is_empty() || fail.len() > maximum || pass.len() > maximum {
        return Err(BenchError::new(BenchErrorCode::InvalidTestSet));
    }
    let mut identities = BTreeSet::new();
    for identity in fail.iter().chain(pass) {
        if identity.is_empty() || identity.len() > 1_024 || !identities.insert(identity) {
            return Err(BenchError::new(BenchErrorCode::InvalidTestSet));
        }
    }
    Ok(())
}
