#![cfg(unix)]

use std::{collections::BTreeMap, path::PathBuf};

use garive_context_pressure::{CommandTokenCounter, CommandTokenCounterConfig, TokenCounter};
use garive_llm::{ModelInputContent, ModelInputItem, ModelRole};

#[test]
fn explicit_counter_clears_environment_and_parses_exact_response() {
    let script = r#"
        [ -z "${HOME+x}" ] || exit 8
        [ "$ONLY_VALUE" = "visible" ] || exit 9
        payload=$(cat)
        [ -n "$payload" ] || exit 10
        printf '{"schema_version":1,"input_tokens":17}'
    "#;
    let first = counter(script, 100, 128, 128);
    let second = counter(script, 100, 128, 128);
    assert_eq!(first.descriptor(), second.descriptor());
    assert!(!first.descriptor().publishable);
    assert_eq!(first.count_input_tokens(&items()).unwrap(), 17);
}

#[test]
fn exit_timeout_output_bounds_and_strict_json_fail_closed() {
    let failures = [
        ("exit 4", 100, 128, 128),
        ("while :; do :; done", 10, 128, 128),
        ("printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx'", 100, 8, 128),
        ("printf 'xxxxxxxxxxxxxxxxxxxxxxxxxxxxxxxx' >&2; printf '{\"schema_version\":1,\"input_tokens\":1}'", 100, 128, 8),
        ("printf '{\"schema_version\":1,\"input_tokens\":0}'", 100, 128, 128),
        ("printf '{\"schema_version\":1,\"input_tokens\":1,\"extra\":true}'", 100, 128, 128),
        ("printf '{\"schema_version\":1,\"schema_version\":1,\"input_tokens\":1}'", 100, 128, 128),
    ];
    for (script, timeout, stdout, stderr) in failures {
        assert!(counter(script, timeout, stdout, stderr)
            .count_input_tokens(&items())
            .is_err());
    }
}

fn counter(
    script: &str,
    timeout_ms: u64,
    max_stdout_bytes: usize,
    max_stderr_bytes: usize,
) -> CommandTokenCounter {
    CommandTokenCounter::new(CommandTokenCounterConfig {
        counter_id: "shell-fixture".into(),
        counter_revision: "v1".into(),
        publishable: false,
        executable: PathBuf::from("/bin/sh"),
        argv: vec!["-c".into(), script.into()],
        cwd: std::env::current_dir().unwrap(),
        environment: BTreeMap::from([("ONLY_VALUE".into(), "visible".into())]),
        timeout_ms,
        max_stdout_bytes,
        max_stderr_bytes,
    })
    .unwrap()
}

fn items() -> Vec<ModelInputItem> {
    vec![ModelInputItem::Message {
        role: ModelRole::User,
        content: vec![ModelInputContent::Text("count these tokens".into())],
    }]
}
