use std::{fs, num::NonZeroU32, path::PathBuf};

use garive_core::{
    BeginIteration, ControlError, ExecutionControl, ExecutionId, ExecutionLimits,
    ExecutionOutcomeKind, ExecutionStatus, TurnId,
};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/execution-control.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn outcome_kind(value: &str) -> ExecutionOutcomeKind {
    match value {
        "completed" => ExecutionOutcomeKind::Completed,
        "suspended" => ExecutionOutcomeKind::Suspended,
        "stopped" => ExecutionOutcomeKind::Stopped,
        "failed" => ExecutionOutcomeKind::Failed,
        other => panic!("unknown fixture outcome kind: {other}"),
    }
}

fn status(value: ExecutionStatus) -> String {
    match value {
        ExecutionStatus::Active => "active".into(),
        ExecutionStatus::Closed(kind) => format!(
            "closed:{}",
            match kind {
                ExecutionOutcomeKind::Completed => "completed",
                ExecutionOutcomeKind::Suspended => "suspended",
                ExecutionOutcomeKind::Stopped => "stopped",
                ExecutionOutcomeKind::Failed => "failed",
            }
        ),
    }
}

#[test]
fn rust_consumes_every_execution_control_case() {
    let document = fixture();
    assert_eq!(document["schema_version"], 1);
    assert_eq!(document["contract"], "execution-control");
    let cases = document["cases"].as_array().unwrap();
    assert_eq!(
        cases.len(),
        5,
        "fixture coverage changed; review both runners"
    );

    for case in cases {
        let name = case["name"].as_str().unwrap();
        let input = &case["input"];
        let result = ExecutionControl::new(
            TurnId::try_from(input["turn_id"].as_str().unwrap()).unwrap(),
            ExecutionId::try_from(input["execution_id"].as_str().unwrap()).unwrap(),
            input["completed"].as_u64().unwrap() as u32,
            ExecutionLimits::new(
                NonZeroU32::new(input["maximum"].as_u64().unwrap() as u32).unwrap(),
            ),
        );

        if case["expected"]["construction_error"].is_string() {
            assert!(
                matches!(result, Err(ControlError::CursorBeyondLimit { .. })),
                "{name}"
            );
            continue;
        }

        let mut control = result.unwrap();
        let mut actual = Vec::new();
        for operation in case["operations"].as_array().unwrap() {
            let operation = operation.as_str().unwrap();
            let rendered = if operation == "begin" {
                match control.begin_iteration() {
                    Ok(BeginIteration::Started { iteration }) => {
                        format!("started:{}", iteration.get())
                    }
                    Ok(BeginIteration::IterationLimitReached) => "iteration-limit".into(),
                    Err(ControlError::AlreadyClosed) => "error:already-closed".into(),
                    Err(error) => panic!("{name}: unexpected begin error: {error}"),
                }
            } else if let Some(kind) = operation.strip_prefix("close:") {
                match control.close(outcome_kind(kind)) {
                    Ok(()) => format!("closed:{kind}"),
                    Err(ControlError::AlreadyClosed) => "error:already-closed".into(),
                    Err(error) => panic!("{name}: unexpected close error: {error}"),
                }
            } else {
                panic!("{name}: unknown operation {operation}");
            };
            actual.push(rendered);
        }

        let expected = &case["expected"];
        let expected_results: Vec<_> = expected["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        assert_eq!(actual, expected_results, "{name}");
        assert_eq!(
            control.completed_iterations(),
            expected["completed"].as_u64().unwrap() as u32,
            "{name}"
        );
        assert_eq!(status(control.status()), expected["status"], "{name}");
    }
}
