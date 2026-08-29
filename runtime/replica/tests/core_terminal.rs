use std::{fs, path::PathBuf};

use garive_core::{
    AgentFailureReason, AgentOutcome, ExecutionReport, StopReason, SuspensionReason, UsageSummary,
};
use garive_ledger::{validate_runtime_fact, ExecutionId, RuntimeFactDisposition, TurnId};
use garive_llm::{ModelItem, TokenCount};
use garive_runtime::{plan_core_terminal, CoreTerminalContext, RuntimeCommandError};
use serde_json::Value;

fn fixture() -> Value {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../spec/fixtures/agent/durable-runtime-turn.json");
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn usage(input: TokenCount, output: TokenCount, estimated: bool) -> UsageSummary {
    UsageSummary {
        input_tokens: input,
        output_tokens: output,
        estimated,
    }
}

fn report(kind: &str) -> ExecutionReport {
    let usage = usage(TokenCount::Known(3), TokenCount::Unknown, false);
    let outcome = match kind {
        "completed" => AgentOutcome::Completed {
            response_items: vec![ModelItem::Text {
                text: "done".into(),
            }],
            usage,
        },
        "suspended" => AgentOutcome::Suspended {
            reason: SuspensionReason::PartialOutput,
            partial_items: vec![ModelItem::Text {
                text: "partial".into(),
            }],
            last_durable_position: 4,
            governed_binding: None,
        },
        "stopped" => AgentOutcome::Stopped {
            reason: StopReason::ResourceUnavailable,
        },
        "failed" => AgentOutcome::Failed {
            reason: AgentFailureReason::RequiredCapabilityUnavailable,
        },
        other => panic!("unknown outcome {other}"),
    };
    ExecutionReport {
        outcome,
        completed_iterations: 1,
        usage,
    }
}

fn context() -> CoreTerminalContext {
    CoreTerminalContext {
        turn_id: TurnId::try_from("turn").unwrap(),
        execution_id: ExecutionId::try_from("execution").unwrap(),
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

#[test]
fn every_frozen_core_outcome_maps_to_two_strict_terminal_facts() {
    let fixture = fixture();
    let cases = fixture["core_outcome_cases"].as_array().unwrap();
    assert_eq!(cases.len(), 4);
    for case in cases {
        let facts =
            plan_core_terminal(&context(), &report(case["outcome"].as_str().unwrap())).unwrap();
        let actual: Vec<_> = facts.iter().map(|fact| fact.kind.as_str()).collect();
        let expected: Vec<_> = case["facts"]
            .as_array()
            .unwrap()
            .iter()
            .map(|kind| kind.as_str().unwrap())
            .collect();
        assert_eq!(actual, expected, "{}", case["outcome"]);
        assert!(facts
            .iter()
            .all(|fact| { validate_runtime_fact(fact) == Ok(RuntimeFactDisposition::AppliedV1) }));
    }
}

#[test]
fn terminal_mapping_preserves_unknown_usage_and_content_integrity() {
    let facts = plan_core_terminal(&context(), &report("completed")).unwrap();
    let execution: Value = serde_json::from_str(facts[0].payload.as_json()).unwrap();
    assert_eq!(execution["usage"]["input_tokens"]["value"], 3);
    assert_eq!(execution["usage"]["output_tokens"]["kind"], "unknown");
    assert_eq!(execution["usage"]["source"], "provider_reported");
    assert_eq!(execution["completed_iterations"], 1);
    assert_eq!(
        execution["response"]["inline_utf8"],
        r#"[{"kind":"text","text":"done"}]"#
    );
}

#[test]
fn inconsistent_completed_usage_and_invalid_clock_fail_closed() {
    let mut inconsistent = report("completed");
    inconsistent.usage.estimated = true;
    assert_eq!(
        plan_core_terminal(&context(), &inconsistent),
        Err(RuntimeCommandError::InvariantViolation)
    );
    let mut context = context();
    context.recorded_at = "today".into();
    assert_eq!(
        plan_core_terminal(&context, &report("stopped")),
        Err(RuntimeCommandError::InvalidCommand)
    );
}
