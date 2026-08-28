use std::time::Duration;

use garive_llm::{
    InterruptionKind, InvokeOutcome, InvokeOutcomeKind, MediaKind, ModelItem, ModelStopReason,
    ModelUsage, ReasoningContent, RejectionKind, TokenCount, UnavailableKind, UsageSource,
    UsageTotal,
};

fn usage(input: TokenCount, output: TokenCount) -> ModelUsage {
    ModelUsage {
        input_tokens: input,
        output_tokens: output,
        cache_read_tokens: Some(TokenCount::Known(4)),
        cache_write_tokens: Some(TokenCount::Known(1)),
        source: UsageSource::ProviderReported,
    }
}

#[test]
fn usage_distinguishes_known_unknown_and_overflow() {
    assert_eq!(
        usage(TokenCount::Known(10), TokenCount::Known(2)).total_tokens(),
        UsageTotal::Known(12)
    );
    assert_eq!(
        usage(TokenCount::Unknown, TokenCount::Known(2)).total_tokens(),
        UsageTotal::Unknown
    );
    assert_eq!(
        usage(TokenCount::Known(u64::MAX), TokenCount::Known(1)).total_tokens(),
        UsageTotal::Overflow
    );
}

#[test]
fn ordered_items_cover_the_portable_model_surface() {
    let items = vec![
        ModelItem::Text { text: "one".into() },
        ModelItem::Refusal {
            text: "cannot comply".into(),
        },
        ModelItem::Reasoning {
            content: ReasoningContent::OpaqueReference("reasoning-1".into()),
        },
        ModelItem::ToolIntent {
            model_call_id: "call-1".into(),
            tool_name: "read".into(),
            arguments_json: r#"{"path":"a"}"#.into(),
        },
        ModelItem::ToolObservation {
            model_call_id: "call-1".into(),
            result_json: r#"{"text":"b"}"#.into(),
        },
        ModelItem::MediaReference {
            media_kind: MediaKind::Image,
            reference: "sha256:image".into(),
        },
    ];

    let outcome = InvokeOutcome::Completed {
        items: items.clone(),
        usage: usage(TokenCount::Known(1), TokenCount::Known(1)),
        stop_reason: ModelStopReason::EndTurn,
    };
    let InvokeOutcome::Completed { items: actual, .. } = outcome else {
        panic!("expected completed outcome");
    };
    assert_eq!(actual, items);
}

#[test]
fn fact_envelopes_do_not_encode_recovery_actions() {
    let usage = usage(TokenCount::Known(1), TokenCount::Unknown);
    let outcomes = [
        InvokeOutcome::Completed {
            items: vec![],
            usage,
            stop_reason: ModelStopReason::EndTurn,
        },
        InvokeOutcome::Rejected {
            kind: RejectionKind::ContextOverflow,
            sanitized_evidence: "limit".into(),
        },
        InvokeOutcome::Interrupted {
            kind: InterruptionKind::OutputLimit,
            partial_items: vec![ModelItem::Text {
                text: "prefix".into(),
            }],
            usage,
        },
        InvokeOutcome::Unavailable {
            kind: UnavailableKind::RateLimited,
            retry_after: Some(Duration::from_secs(1)),
        },
    ];

    assert_eq!(
        outcomes.each_ref().map(InvokeOutcome::kind),
        [
            InvokeOutcomeKind::Completed,
            InvokeOutcomeKind::Rejected,
            InvokeOutcomeKind::Interrupted,
            InvokeOutcomeKind::Unavailable,
        ]
    );
    assert!(outcomes[0].is_success());
    assert!(outcomes[2].is_partial());
    assert!(outcomes[1..].iter().all(|value| !value.is_success()));
    assert!(!outcomes[0].is_partial());
}

#[test]
fn all_reason_kinds_remain_distinct() {
    assert_ne!(RejectionKind::Authentication, RejectionKind::ContentPolicy);
    assert_ne!(InterruptionKind::Cancelled, InterruptionKind::Transport);
    assert_ne!(
        UnavailableKind::CircuitOpen,
        UnavailableKind::ModelUnavailable
    );
    assert_ne!(ModelStopReason::PauseTurn, ModelStopReason::Refusal);
}
