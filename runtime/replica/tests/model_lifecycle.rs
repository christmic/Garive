use std::time::Duration;

use garive_ledger::{validate_runtime_fact, ExecutionId, RuntimeFactDisposition, TurnId};
use garive_llm::{
    InterruptionKind, InvokeOutcome, ModelCapability, ModelInputContent, ModelInputItem, ModelItem,
    ModelOutputSettings, ModelRequest, ModelRequestId, ModelRole, ModelStopReason, ModelTargetId,
    ModelUsage, RejectionKind, TextMode, TokenCount, UnavailableKind, UsageSource,
};
use garive_runtime::{
    plan_model_prepared, plan_model_started, plan_model_terminal, plan_model_uncertain,
    ModelLifecycleContext, RuntimeModelUncertainReason,
};

fn context() -> ModelLifecycleContext {
    ModelLifecycleContext {
        turn_id: TurnId::try_from("turn").unwrap(),
        execution_id: ExecutionId::try_from("execution").unwrap(),
        deployment_id: "deployment".into(),
        recovery_policy_revision: "policy".into(),
        max_attempts: 2,
        recorded_at: "2026-08-29T00:00:00Z".into(),
    }
}

fn request() -> ModelRequest {
    ModelRequest {
        request_id: ModelRequestId::new("request"),
        target_id: ModelTargetId::new("target"),
        required_capabilities: vec![ModelCapability::Text],
        input_items: vec![ModelInputItem::Message {
            role: ModelRole::User,
            content: vec![ModelInputContent::Text("hello".into())],
        }],
        tools: vec![],
        output: ModelOutputSettings {
            max_output_tokens: Some(10),
            text_mode: TextMode::Plain,
            reasoning_visibility: false,
        },
        trace_metadata: vec![],
    }
}

fn usage() -> ModelUsage {
    ModelUsage {
        input_tokens: TokenCount::Known(2),
        output_tokens: TokenCount::Unknown,
        cache_read_tokens: Some(TokenCount::Known(1)),
        cache_write_tokens: None,
        source: UsageSource::ProviderReported,
    }
}

fn outcomes() -> Vec<InvokeOutcome> {
    vec![
        InvokeOutcome::Completed {
            items: vec![ModelItem::Text {
                text: "done".into(),
            }],
            usage: usage(),
            stop_reason: ModelStopReason::EndTurn,
        },
        InvokeOutcome::Rejected {
            kind: RejectionKind::ContentPolicy,
            sanitized_evidence: "policy".into(),
        },
        InvokeOutcome::Interrupted {
            kind: InterruptionKind::Transport,
            partial_items: vec![],
            usage: usage(),
        },
        InvokeOutcome::Unavailable {
            kind: UnavailableKind::RateLimited,
            retry_after: Some(Duration::from_millis(250)),
        },
    ]
}

#[test]
fn model_lifecycle_plans_every_strict_boundary_before_dispatch() {
    let context = context();
    let prepared = plan_model_prepared(&context, &request()).unwrap();
    let started = plan_model_started(&context, &prepared, "attempt-1").unwrap();
    assert_eq!(
        validate_runtime_fact(&prepared.fact),
        Ok(RuntimeFactDisposition::AppliedV1)
    );
    assert_eq!(
        validate_runtime_fact(&started),
        Ok(RuntimeFactDisposition::AppliedV1)
    );

    let expected = [
        "model.completed",
        "model.rejected",
        "model.interrupted",
        "model.unavailable",
    ];
    for (outcome, expected) in outcomes().iter().zip(expected) {
        let terminal = plan_model_terminal(&context, &prepared, outcome).unwrap();
        assert_eq!(terminal.kind.as_str(), expected);
        assert_eq!(
            validate_runtime_fact(&terminal),
            Ok(RuntimeFactDisposition::AppliedV1)
        );
    }
}

#[test]
fn missing_normalized_result_becomes_explicit_uncertainty() {
    let context = context();
    let prepared = plan_model_prepared(&context, &request()).unwrap();
    let uncertain = plan_model_uncertain(
        &context,
        &prepared,
        RuntimeModelUncertainReason::ProviderStateUnknown,
    )
    .unwrap();
    assert_eq!(uncertain.kind.as_str(), "model.uncertain");
    assert_eq!(
        validate_runtime_fact(&uncertain),
        Ok(RuntimeFactDisposition::AppliedV1)
    );
}
