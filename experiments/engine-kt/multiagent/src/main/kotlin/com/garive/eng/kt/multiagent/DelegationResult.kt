package com.garive.eng.kt.multiagent

import com.garive.eng.kt.tools.validatePortableValue
import kotlinx.serialization.json.Json

/** Stable child terminal reason admitted into a bounded parent observation. */
public enum class ChildTerminalReason(public val wireName: String) {
    ITERATION_LIMIT("iteration_limit"), TOKEN_LIMIT("token_limit"), DEADLINE("deadline"),
    CANCELLED("cancelled"), RESOURCE_UNAVAILABLE("resource_unavailable"), INVALID_INPUT("invalid_input"),
    INVALID_MODEL_OUTPUT("invalid_model_output"), REQUIRED_CAPABILITY_UNAVAILABLE("required_capability_unavailable"),
    PORT_FAILURE("port_failure"), INVARIANT_VIOLATION("invariant_violation"),
    DURABILITY_FAILURE("durability_failure"), CORRUPT_RECOVERY_STATE("corrupt_recovery_state"),
}

/** Non-completion terminal category of the child Turn. */
public enum class TerminalOutcomeKind { STOPPED, FAILED }

/** Portable child evidence exposed to the parent. */
public sealed interface DelegationOutcome {
    public data class Completed(public val content: ContentBinding, public val evidence: List<FactReference>) : DelegationOutcome
    public data class Stopped(public val reason: ChildTerminalReason) : DelegationOutcome
    public data class Failed(public val reason: ChildTerminalReason) : DelegationOutcome
}

/** Exact child/result identities and terminal accounting evidence. */
public data class DelegationResultContext(
    public val resultId: String, public val delegationId: String, public val grantId: String,
    public val childAgentInstanceId: String, public val childTurnId: String,
    public val childSnapshotDigest: String, public val usage: DelegationUsage,
    public val consumption: DelegationConsumption,
)

/** Validated terminal child result and conservative budget settlement. */
public data class DelegationResult(
    public val context: DelegationResultContext,
    public val outcome: DelegationOutcome,
    public val settlement: DelegationBudgetSettlement,
)

/** Validates bounded completed content against the frozen portable result schema. */
public fun completeDelegationResult(
    intent: DelegationIntent, context: DelegationResultContext, content: ContentBinding,
    resolvedContentUtf8: String, evidence: List<FactReference>,
): DelegationContractResult<DelegationResult> {
    val settlement = when (val value = validateContext(intent, context)) {
        is DelegationContractResult.Success -> value.value
        is DelegationContractResult.Failure -> return value
    }
    if (sha256(resolvedContentUtf8.encodeToByteArray()) != content.digest ||
        resolvedContentUtf8.encodeToByteArray().size.toULong() > intent.budget.maxResultBytes ||
        evidence.size.toULong() > intent.budget.maxResultEvidence || evidence.distinct().size != evidence.size
    ) return failure(DelegationErrorCode.INVALID_DELEGATION)
    val valid = runCatching {
        val schema = Json.parseToJsonElement(requireNotNull(intent.resultSchema.inlineUtf8))
        val result = Json.parseToJsonElement(resolvedContentUtf8)
        validatePortableValue(schema, result)
    }.getOrDefault(false)
    if (!valid) return failure(DelegationErrorCode.RESULT_SCHEMA_MISMATCH)
    return success(DelegationResult(context, DelegationOutcome.Completed(content, evidence.toList()), settlement))
}

/** Validates a stopped or failed child terminal without inventing content. */
public fun terminalDelegationResult(
    intent: DelegationIntent, context: DelegationResultContext,
    kind: TerminalOutcomeKind, reason: ChildTerminalReason,
): DelegationContractResult<DelegationResult> {
    val settlement = when (val value = validateContext(intent, context)) {
        is DelegationContractResult.Success -> value.value
        is DelegationContractResult.Failure -> return value
    }
    val outcome = when (kind) {
        TerminalOutcomeKind.STOPPED -> DelegationOutcome.Stopped(reason)
        TerminalOutcomeKind.FAILED -> DelegationOutcome.Failed(reason)
    }
    return success(DelegationResult(context, outcome, settlement))
}

private fun validateContext(
    intent: DelegationIntent,
    context: DelegationResultContext,
): DelegationContractResult<DelegationBudgetSettlement> {
    if (context.delegationId != intent.delegationId ||
        listOf(context.resultId, context.grantId, context.childAgentInstanceId, context.childTurnId).any { !validId(it) } ||
        !validDigest(context.childSnapshotDigest)
    ) return failure(DelegationErrorCode.INVALID_DELEGATION)
    return settleDelegationBudget(intent.budget, context.consumption, context.usage)
}
