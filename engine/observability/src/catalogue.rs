use crate::{AttributeValue, MeasurementUnit, RedactionClass};

/// Exact v1 signal names in canonical order.
pub const SIGNAL_NAMES: &[&str] = &[
    "agent.context.derived",
    "agent.delegation.requested",
    "agent.delegation.terminal",
    "agent.effect.prepared",
    "agent.effect.terminal",
    "agent.execution.started",
    "agent.execution.terminal",
    "agent.host.command",
    "agent.host.event_page",
    "agent.interaction.required",
    "agent.iteration.started",
    "agent.model.attempt",
    "agent.model.terminal",
    "agent.recovery.classified",
    "agent.scheduler.claim",
    "agent.scheduler.dispatch",
    "agent.telemetry.dropped",
];

/// Immutable schema for one admitted signal.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SignalSchema {
    /// Exact signal name.
    pub name: &'static str,
    /// Allowed attribute key to type-category pairs.
    pub attributes: &'static [(&'static str, &'static str)],
    /// Allowed measurement name and exact unit pairs.
    pub measurements: &'static [(&'static str, MeasurementUnit)],
    /// Weakest source redaction class admitted.
    pub minimum_redaction: RedactionClass,
}

const START_A: &[(&str, &str)] = &[("recovery_action", "recovery_action"), ("replayed", "bool")];
const TERMINAL_A: &[(&str, &str)] = &[
    ("outcome", "outcome"),
    ("reason", "reason"),
    ("replayed", "bool"),
    ("success", "bool"),
];
const REPLAY_A: &[(&str, &str)] = &[("replayed", "bool")];
const CONTEXT_A: &[(&str, &str)] = &[("digest_present", "bool"), ("replayed", "bool")];
const MODEL_A: &[(&str, &str)] = &[
    ("phase", "phase"),
    ("protocol_family", "protocol_family"),
    ("replayed", "bool"),
];
const MODEL_T_A: &[(&str, &str)] = &[
    ("outcome", "outcome"),
    ("protocol_family", "protocol_family"),
    ("reason", "reason"),
    ("replayed", "bool"),
    ("success", "bool"),
];
const EFFECT_A: &[(&str, &str)] = &[
    ("capability_class", "capability_class"),
    ("classification", "classification"),
    ("replayed", "bool"),
];
const EFFECT_T_A: &[(&str, &str)] = &[
    ("classification", "classification"),
    ("outcome", "outcome"),
    ("reason", "reason"),
    ("replayed", "bool"),
    ("success", "bool"),
];
const CLASS_A: &[(&str, &str)] = &[("classification", "classification"), ("replayed", "bool")];
const RECOVERY_A: &[(&str, &str)] = &[("recovery_action", "recovery_action"), ("replayed", "bool")];
const COMMAND_A: &[(&str, &str)] = &[
    ("disposition", "disposition"),
    ("replayed", "bool"),
    ("success", "bool"),
];
const PAGE_A: &[(&str, &str)] = &[("replayed", "bool"), ("success", "bool")];
const CLAIM_A: &[(&str, &str)] = &[("disposition", "disposition"), ("replayed", "bool")];
const DROP_A: &[(&str, &str)] = &[("classification", "classification")];
const ITER_M: &[(&str, MeasurementUnit)] = &[("iteration_count", MeasurementUnit::Count)];
const START_M: &[(&str, MeasurementUnit)] = &[("completed_iterations", MeasurementUnit::Count)];
const TERM_M: &[(&str, MeasurementUnit)] = &[
    ("completed_iterations", MeasurementUnit::Count),
    ("elapsed_ms", MeasurementUnit::Milliseconds),
    ("input_tokens", MeasurementUnit::Tokens),
    ("output_tokens", MeasurementUnit::Tokens),
];
const CONTEXT_M: &[(&str, MeasurementUnit)] = &[
    ("input_tokens", MeasurementUnit::Tokens),
    ("item_count", MeasurementUnit::Count),
    ("total_bytes", MeasurementUnit::Bytes),
];
const ATTEMPT_M: &[(&str, MeasurementUnit)] = &[
    ("attempt_count", MeasurementUnit::Count),
    ("elapsed_ms", MeasurementUnit::Milliseconds),
];
const MODEL_T_M: &[(&str, MeasurementUnit)] = &[
    ("elapsed_ms", MeasurementUnit::Milliseconds),
    ("input_tokens", MeasurementUnit::Tokens),
    ("output_tokens", MeasurementUnit::Tokens),
];
const ONE_ATTEMPT: &[(&str, MeasurementUnit)] = &[("attempt_count", MeasurementUnit::Count)];
const ELAPSED: &[(&str, MeasurementUnit)] = &[("elapsed_ms", MeasurementUnit::Milliseconds)];
const ITEMS: &[(&str, MeasurementUnit)] = &[("item_count", MeasurementUnit::Count)];
const PAGE_M: &[(&str, MeasurementUnit)] = &[
    ("item_count", MeasurementUnit::Count),
    ("total_bytes", MeasurementUnit::Bytes),
];
const CLAIM_M: &[(&str, MeasurementUnit)] = &[
    ("elapsed_ms", MeasurementUnit::Milliseconds),
    ("occurrence_count", MeasurementUnit::Count),
];
const TOKENS: &[(&str, MeasurementUnit)] = &[
    ("input_tokens", MeasurementUnit::Tokens),
    ("output_tokens", MeasurementUnit::Tokens),
];
const DELEG_T: &[(&str, MeasurementUnit)] = &[
    ("elapsed_ms", MeasurementUnit::Milliseconds),
    ("input_tokens", MeasurementUnit::Tokens),
    ("output_tokens", MeasurementUnit::Tokens),
];
const DROP_M: &[(&str, MeasurementUnit)] = &[
    ("dropped_bytes", MeasurementUnit::Bytes),
    ("dropped_count", MeasurementUnit::Count),
];

/// Returns the exact immutable v1 schema, or `None` for an unknown name.
pub fn signal_schema(name: &str) -> Option<SignalSchema> {
    let (attributes, measurements, minimum_redaction) = match name {
        "agent.execution.started" => (START_A, START_M, RedactionClass::Operational),
        "agent.execution.terminal" => (TERMINAL_A, TERM_M, RedactionClass::Operational),
        "agent.iteration.started" => (REPLAY_A, ITER_M, RedactionClass::Operational),
        "agent.context.derived" => (CONTEXT_A, CONTEXT_M, RedactionClass::Operational),
        "agent.model.attempt" => (MODEL_A, ATTEMPT_M, RedactionClass::Operational),
        "agent.model.terminal" => (MODEL_T_A, MODEL_T_M, RedactionClass::Operational),
        "agent.effect.prepared" => (EFFECT_A, ONE_ATTEMPT, RedactionClass::Operational),
        "agent.effect.terminal" => (EFFECT_T_A, ELAPSED, RedactionClass::Operational),
        "agent.interaction.required" => (CLASS_A, ITEMS, RedactionClass::Restricted),
        "agent.recovery.classified" => (RECOVERY_A, ONE_ATTEMPT, RedactionClass::Operational),
        "agent.host.command" => (COMMAND_A, ELAPSED, RedactionClass::Operational),
        "agent.host.event_page" => (PAGE_A, PAGE_M, RedactionClass::Operational),
        "agent.scheduler.claim" => (CLAIM_A, CLAIM_M, RedactionClass::Operational),
        "agent.scheduler.dispatch" => (COMMAND_A, ELAPSED, RedactionClass::Operational),
        "agent.delegation.requested" => (CLAIM_A, TOKENS, RedactionClass::Operational),
        "agent.delegation.terminal" => (TERMINAL_A, DELEG_T, RedactionClass::Operational),
        "agent.telemetry.dropped" => (DROP_A, DROP_M, RedactionClass::Operational),
        _ => return None,
    };
    Some(SignalSchema {
        name: SIGNAL_NAMES.iter().copied().find(|item| *item == name)?,
        attributes,
        measurements,
        minimum_redaction,
    })
}

pub(crate) fn attribute_valid(category: &str, value: &AttributeValue) -> bool {
    match (category, value) {
        ("bool", AttributeValue::Bool { .. }) => true,
        (_, AttributeValue::String { value }) => {
            attribute_enum_values(category).contains(&value.as_str())
        }
        _ => false,
    }
}
/// Returns the exact sorted v1 values for an enum category.
pub fn attribute_enum_values(category: &str) -> &'static [&'static str] {
    match category {
        "outcome" => &[
            "completed",
            "denied",
            "failed",
            "interrupted",
            "rejected",
            "started",
            "stopped",
            "unavailable",
            "uncertain",
        ],
        "reason" => &[
            "authority_denied",
            "budget_exhausted",
            "cancelled",
            "corrupt_recovery_state",
            "deadline",
            "durability_failure",
            "invalid_input",
            "invalid_model_output",
            "invariant_violation",
            "iteration_limit",
            "port_failure",
            "required_capability_unavailable",
            "resource_unavailable",
            "sink_backpressured",
            "sink_unavailable",
            "token_limit",
        ],
        "phase" => &[
            "authorized",
            "completed",
            "dispatched",
            "prepared",
            "requested",
            "started",
            "terminal",
        ],
        "classification" => &[
            "approval",
            "external_input",
            "idempotent",
            "never_replay",
            "operator_reconciliation",
            "policy",
            "pressure",
            "sampling",
            "serialization",
            "shutdown",
            "sink",
        ],
        "recovery_action" => &[
            "abandon_and_restart",
            "classify_effect_uncertain",
            "classify_model_uncertain",
            "fail_recovery_bound",
            "recover_receipt_terminal",
        ],
        "capability_class" => &["idempotent", "read_only", "side_effecting"],
        "protocol_family" => &["compatible", "messages", "responses"],
        "disposition" => &[
            "accepted",
            "authorized",
            "committed",
            "conflict",
            "denied",
            "failed",
            "reclaimed",
            "rejected",
            "replayed",
        ],
        _ => &[],
    }
}
