use std::{collections::BTreeSet, fs, path::PathBuf};

use serde::Deserialize;

const MAX_FIXTURE_BYTES: u64 = 64 * 1_024;
const MAX_CASES_PER_FAMILY: usize = 64;
const MAX_STEPS_PER_CASE: usize = 16;
const MAX_TEXT_BYTES: usize = 4_096;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductFixture {
    schema_version: u32,
    redaction_canaries: Vec<String>,
    bootstrap_cases: Vec<Case>,
    navigation_cases: Vec<Case>,
    conversation_cases: Vec<Case>,
    command_cases: Vec<Case>,
    follow_cases: Vec<Case>,
    suspension_cases: Vec<Case>,
    activity_cases: Vec<Case>,
    editor_cases: Vec<Case>,
    persistence_cases: Vec<Case>,
    failure_cases: Vec<Case>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    name: String,
    initial: ModelState,
    steps: Vec<Step>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Step {
    action: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    position: Option<u64>,
    #[serde(default)]
    input: Option<String>,
    expected: ExpectedState,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ModelState {
    state: String,
    #[serde(default)]
    session_id: Option<String>,
    #[serde(default)]
    turn_id: Option<String>,
    position: u64,
    draft: String,
    #[serde(default)]
    pending_command_id: Option<String>,
}

#[derive(Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
struct ExpectedState {
    state: String,
    effect: String,
    position: u64,
    #[serde(default)]
    notice_code: Option<String>,
    draft: String,
}

#[test]
fn product_fixture_is_strict_bounded_complete_and_content_safe() {
    let (bytes, fixture) = load_fixture();
    assert!(bytes.len() as u64 <= MAX_FIXTURE_BYTES);
    assert_eq!(fixture.schema_version, 1);
    assert!(!fixture.redaction_canaries.is_empty());
    assert!(fixture
        .redaction_canaries
        .iter()
        .all(|value| !value.is_empty() && value.len() <= 256));

    let mut names = BTreeSet::new();
    for family in fixture.families() {
        assert!(!family.is_empty());
        assert!(family.len() <= MAX_CASES_PER_FAMILY);
        for case in family {
            assert!(names.insert(case.name.as_str()), "duplicate case name");
            assert!(valid_name(&case.name));
            validate_model(&case.initial);
            assert!(!case.steps.is_empty());
            assert!(case.steps.len() <= MAX_STEPS_PER_CASE);
            for step in &case.steps {
                assert!(valid_name(&step.action));
                step.session_id.as_deref().map(validate_id);
                assert!(step.position.is_none_or(|value| value > 0));
                assert!(step
                    .input
                    .as_ref()
                    .is_none_or(|value| { value.len() <= MAX_TEXT_BYTES && safe_text(value) }));
                validate_expected(&step.expected, &fixture.redaction_canaries);
            }
        }
    }
}

#[test]
fn product_fixture_rejects_unknown_fields_and_duplicate_case_names() {
    let (bytes, _) = load_fixture();
    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<ProductFixture>(value).is_err());

    let mut value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["failure_cases"][0]["name"] = value["bootstrap_cases"][0]["name"].clone();
    let fixture: ProductFixture = serde_json::from_value(value).unwrap();
    let mut names = BTreeSet::new();
    assert!(fixture
        .families()
        .flatten()
        .any(|case| !names.insert(case.name.as_str())));
}

impl ProductFixture {
    fn families(&self) -> impl Iterator<Item = &[Case]> {
        [
            self.bootstrap_cases.as_slice(),
            self.navigation_cases.as_slice(),
            self.conversation_cases.as_slice(),
            self.command_cases.as_slice(),
            self.follow_cases.as_slice(),
            self.suspension_cases.as_slice(),
            self.activity_cases.as_slice(),
            self.editor_cases.as_slice(),
            self.persistence_cases.as_slice(),
            self.failure_cases.as_slice(),
        ]
        .into_iter()
    }
}

fn load_fixture() -> (Vec<u8>, ProductFixture) {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../spec/fixtures/tui/tui-product-v1.json");
    let metadata = fs::metadata(&path).unwrap();
    assert!(metadata.len() <= MAX_FIXTURE_BYTES);
    let bytes = fs::read(path).unwrap();
    let fixture = serde_json::from_slice(&bytes).unwrap();
    (bytes, fixture)
}

fn validate_model(value: &ModelState) {
    assert!(valid_state(&value.state));
    assert!(value.position <= 9_007_199_254_740_991);
    value.session_id.as_deref().map(validate_id);
    value.turn_id.as_deref().map(validate_id);
    value.pending_command_id.as_deref().map(validate_id);
    assert!(value.draft.len() <= MAX_TEXT_BYTES && safe_text(&value.draft));
}

fn validate_expected(value: &ExpectedState, canaries: &[String]) {
    assert!(valid_state(&value.state));
    assert!(valid_name(&value.effect));
    assert!(value.notice_code.as_deref().is_none_or(valid_name));
    assert!(value.draft.len() <= MAX_TEXT_BYTES && safe_text(&value.draft));
    let encoded = serde_json::to_string(value).unwrap();
    assert!(canaries.iter().all(|canary| !encoded.contains(canary)));
}

fn validate_id(value: &str) {
    assert!(!value.is_empty() && value.len() <= 128 && valid_name(value));
}

fn valid_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

fn valid_state(value: &str) -> bool {
    matches!(
        value,
        "loading"
            | "idle"
            | "submitting"
            | "running"
            | "suspended"
            | "continuing"
            | "command_unknown"
            | "disconnected"
    )
}

fn safe_text(value: &str) -> bool {
    value.chars().all(|character| {
        character == '\n'
            || (!character.is_control()
                && !matches!(character, '\u{202a}'..='\u{202e}' | '\u{2066}'..='\u{2069}'))
    })
}
