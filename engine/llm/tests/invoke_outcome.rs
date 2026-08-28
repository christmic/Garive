use std::time::Duration;

use garive_llm::{
    Completed, InvokeOutcome, InvokeOutcomeKind, ModelUsage, OverflowEvidence, PartialOutput,
};

fn usage() -> ModelUsage {
    ModelUsage {
        input_tokens: 10,
        output_tokens: 2,
        cache_read_tokens: 4,
        cache_write_tokens: 1,
    }
}

#[test]
fn every_variant_has_a_distinct_kind() {
    let partial = || PartialOutput {
        text: "prefix".into(),
        usage: usage(),
    };
    let outcomes = [
        InvokeOutcome::Completed(Completed {
            text: "done".into(),
            usage: usage(),
        }),
        InvokeOutcome::Overflow(OverflowEvidence {
            normalized_input_tokens: Some(100),
            accepted_limit_tokens: Some(90),
        }),
        InvokeOutcome::OutputTruncated(partial()),
        InvokeOutcome::RateBudgetExhausted {
            retry_after: Some(Duration::from_secs(1)),
        },
        InvokeOutcome::PartialCancelled(partial()),
        InvokeOutcome::AuthFailure {
            reason: "reauthentication required".into(),
        },
        InvokeOutcome::ContentViolation {
            reason: "policy category".into(),
            violated_field: Some("input".into()),
        },
        InvokeOutcome::ModelUnavailable {
            model_id: "primary".into(),
        },
        InvokeOutcome::CircuitBreakerOpen {
            target: "pool-a".into(),
        },
    ];
    let expected = [
        InvokeOutcomeKind::Completed,
        InvokeOutcomeKind::Overflow,
        InvokeOutcomeKind::OutputTruncated,
        InvokeOutcomeKind::RateBudgetExhausted,
        InvokeOutcomeKind::PartialCancelled,
        InvokeOutcomeKind::AuthFailure,
        InvokeOutcomeKind::ContentViolation,
        InvokeOutcomeKind::ModelUnavailable,
        InvokeOutcomeKind::CircuitBreakerOpen,
    ];

    assert_eq!(outcomes.each_ref().map(InvokeOutcome::kind), expected);
    assert!(outcomes[0].is_completed());
    assert!(outcomes[1..].iter().all(|outcome| !outcome.is_completed()));
}

#[test]
fn partial_output_is_not_completed() {
    let outcome = InvokeOutcome::OutputTruncated(PartialOutput {
        text: "valid prefix".into(),
        usage: usage(),
    });

    assert!(!outcome.is_completed());
    let InvokeOutcome::OutputTruncated(partial) = outcome else {
        panic!("expected truncated output");
    };
    assert_eq!(partial.text, "valid prefix");
    assert_eq!(partial.usage, usage());
}

#[test]
fn usage_total_is_checked_and_does_not_double_count_cache_breakdowns() {
    assert_eq!(usage().total_tokens(), Some(12));
    assert_eq!(
        ModelUsage {
            input_tokens: u64::MAX,
            output_tokens: 1,
            ..ModelUsage::default()
        }
        .total_tokens(),
        None
    );
}
