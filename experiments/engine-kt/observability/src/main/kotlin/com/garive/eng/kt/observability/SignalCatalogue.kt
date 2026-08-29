package com.garive.eng.kt.observability

/** Immutable schema for one admitted signal. */
public data class SignalSchema(
    public val name: String,
    public val attributes: Map<String, String>,
    public val measurements: Map<String, MeasurementUnit>,
    public val minimumRedaction: RedactionClass,
)

/** Exact portable v1 catalogue. */
public object SignalCatalogue {
    /** Exact sorted enum category values. */
    public val enumValues: Map<String, List<String>> = mapOf(
        "outcome" to listOf("completed", "denied", "failed", "interrupted", "rejected", "started", "stopped", "unavailable", "uncertain"),
        "reason" to listOf("authority_denied", "budget_exhausted", "cancelled", "corrupt_recovery_state", "deadline", "durability_failure", "invalid_input", "invalid_model_output", "invariant_violation", "iteration_limit", "port_failure", "required_capability_unavailable", "resource_unavailable", "sink_backpressured", "sink_unavailable", "token_limit"),
        "phase" to listOf("authorized", "completed", "dispatched", "prepared", "requested", "started", "terminal"),
        "classification" to listOf("approval", "external_input", "idempotent", "never_replay", "operator_reconciliation", "policy", "pressure", "sampling", "serialization", "shutdown", "sink"),
        "recovery_action" to listOf("abandon_and_restart", "classify_effect_uncertain", "classify_model_uncertain", "fail_recovery_bound", "recover_receipt_terminal"),
        "capability_class" to listOf("idempotent", "read_only", "side_effecting"),
        "protocol_family" to listOf("compatible", "messages", "responses"),
        "disposition" to listOf("accepted", "authorized", "committed", "conflict", "denied", "failed", "reclaimed", "rejected", "replayed"),
    )

    private fun a(vararg pairs: Pair<String, String>): Map<String, String> = sortedMapOf(*pairs)
    private fun m(vararg pairs: Pair<String, MeasurementUnit>): Map<String, MeasurementUnit> = sortedMapOf(*pairs)
    private fun schema(
        name: String,
        attributes: Map<String, String>,
        measurements: Map<String, MeasurementUnit>,
        redaction: RedactionClass = RedactionClass.OPERATIONAL,
    ): SignalSchema = SignalSchema(name, attributes, measurements, redaction)

    /** Exact v1 schemas by canonical signal name. */
    public val schemas: Map<String, SignalSchema> = listOf(
        schema("agent.context.derived", a("digest_present" to "bool", "replayed" to "bool"), m("input_tokens" to MeasurementUnit.TOKENS, "item_count" to MeasurementUnit.COUNT, "total_bytes" to MeasurementUnit.BYTES)),
        schema("agent.delegation.requested", a("disposition" to "disposition", "replayed" to "bool"), m("input_tokens" to MeasurementUnit.TOKENS, "output_tokens" to MeasurementUnit.TOKENS)),
        schema("agent.delegation.terminal", a("outcome" to "outcome", "reason" to "reason", "replayed" to "bool", "success" to "bool"), m("elapsed_ms" to MeasurementUnit.MILLISECONDS, "input_tokens" to MeasurementUnit.TOKENS, "output_tokens" to MeasurementUnit.TOKENS)),
        schema("agent.effect.prepared", a("capability_class" to "capability_class", "classification" to "classification", "replayed" to "bool"), m("attempt_count" to MeasurementUnit.COUNT)),
        schema("agent.effect.terminal", a("classification" to "classification", "outcome" to "outcome", "reason" to "reason", "replayed" to "bool", "success" to "bool"), m("elapsed_ms" to MeasurementUnit.MILLISECONDS)),
        schema("agent.execution.started", a("recovery_action" to "recovery_action", "replayed" to "bool"), m("completed_iterations" to MeasurementUnit.COUNT)),
        schema("agent.execution.terminal", a("outcome" to "outcome", "reason" to "reason", "replayed" to "bool", "success" to "bool"), m("completed_iterations" to MeasurementUnit.COUNT, "elapsed_ms" to MeasurementUnit.MILLISECONDS, "input_tokens" to MeasurementUnit.TOKENS, "output_tokens" to MeasurementUnit.TOKENS)),
        schema("agent.host.command", a("disposition" to "disposition", "replayed" to "bool", "success" to "bool"), m("elapsed_ms" to MeasurementUnit.MILLISECONDS)),
        schema("agent.host.event_page", a("replayed" to "bool", "success" to "bool"), m("item_count" to MeasurementUnit.COUNT, "total_bytes" to MeasurementUnit.BYTES)),
        schema("agent.interaction.required", a("classification" to "classification", "replayed" to "bool"), m("item_count" to MeasurementUnit.COUNT), RedactionClass.RESTRICTED),
        schema("agent.iteration.started", a("replayed" to "bool"), m("iteration_count" to MeasurementUnit.COUNT)),
        schema("agent.model.attempt", a("phase" to "phase", "protocol_family" to "protocol_family", "replayed" to "bool"), m("attempt_count" to MeasurementUnit.COUNT, "elapsed_ms" to MeasurementUnit.MILLISECONDS)),
        schema("agent.model.terminal", a("outcome" to "outcome", "protocol_family" to "protocol_family", "reason" to "reason", "replayed" to "bool", "success" to "bool"), m("elapsed_ms" to MeasurementUnit.MILLISECONDS, "input_tokens" to MeasurementUnit.TOKENS, "output_tokens" to MeasurementUnit.TOKENS)),
        schema("agent.recovery.classified", a("recovery_action" to "recovery_action", "replayed" to "bool"), m("attempt_count" to MeasurementUnit.COUNT)),
        schema("agent.scheduler.claim", a("disposition" to "disposition", "replayed" to "bool"), m("elapsed_ms" to MeasurementUnit.MILLISECONDS, "occurrence_count" to MeasurementUnit.COUNT)),
        schema("agent.scheduler.dispatch", a("disposition" to "disposition", "replayed" to "bool", "success" to "bool"), m("elapsed_ms" to MeasurementUnit.MILLISECONDS)),
        schema("agent.telemetry.dropped", a("classification" to "classification"), m("dropped_bytes" to MeasurementUnit.BYTES, "dropped_count" to MeasurementUnit.COUNT)),
    ).associateBy(SignalSchema::name).toSortedMap()
}
