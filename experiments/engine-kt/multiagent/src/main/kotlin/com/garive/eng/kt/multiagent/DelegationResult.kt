package com.garive.eng.kt.multiagent

import com.garive.eng.kt.tools.validatePortableValue
import kotlinx.serialization.json.Json
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

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
) {
    /** Returns RFC 8785 canonical JSON binding every governed result field. */
    public fun resultBinding(): DelegationContractResult<ContentBinding> = runCatching {
        val bytes = JsonCanonicalizer(resultJson().toString()).encodedUTF8
        ContentBinding.fromInline(bytes.decodeToString())
    }.fold(::success) { failure(DelegationErrorCode.INVALID_DELEGATION) }

    @OptIn(ExperimentalSerializationApi::class)
    private fun resultJson(): JsonObject = JsonObject(
        mapOf(
            "contract" to JsonPrimitive("garive.delegation-result"), "version" to JsonPrimitive(1),
            "result_id" to JsonPrimitive(context.resultId), "delegation_id" to JsonPrimitive(context.delegationId),
            "grant_id" to JsonPrimitive(context.grantId), "child_agent_instance_id" to JsonPrimitive(context.childAgentInstanceId),
            "child_turn_id" to JsonPrimitive(context.childTurnId), "child_snapshot_digest" to JsonPrimitive(context.childSnapshotDigest),
            "outcome" to outcomeJson(outcome), "usage" to usageJson(context.usage),
            "consumption" to JsonObject(
                mapOf(
                    "child_turns" to JsonPrimitive(context.consumption.childTurns),
                    "child_executions" to JsonPrimitive(context.consumption.childExecutions),
                    "completed_iterations" to JsonPrimitive(context.consumption.completedIterations),
                    "elapsed_ms" to JsonPrimitive(context.consumption.elapsedMs),
                ),
            ),
        ),
    )
}

@OptIn(ExperimentalSerializationApi::class)
private fun outcomeJson(value: DelegationOutcome): JsonObject = when (value) {
    is DelegationOutcome.Completed -> JsonObject(
        mapOf(
            "kind" to JsonPrimitive("completed"),
            "content" to JsonObject(buildMap {
                put("digest", JsonPrimitive(value.content.digest))
                value.content.inlineUtf8?.let { put("inline_utf8", JsonPrimitive(it)) }
                value.content.reference?.let { put("reference", JsonPrimitive(it)) }
            }),
            "evidence" to kotlinx.serialization.json.JsonArray(value.evidence.map {
                JsonObject(mapOf("session_id" to JsonPrimitive(it.sessionId), "position" to JsonPrimitive(it.position), "fact_id" to JsonPrimitive(it.factId), "payload_digest" to JsonPrimitive(it.payloadDigest)))
            }),
        ),
    )
    is DelegationOutcome.Stopped -> JsonObject(mapOf("kind" to JsonPrimitive("stopped"), "reason" to JsonPrimitive(value.reason.wireName)))
    is DelegationOutcome.Failed -> JsonObject(mapOf("kind" to JsonPrimitive("failed"), "reason" to JsonPrimitive(value.reason.wireName)))
}

private fun usageJson(value: DelegationUsage): JsonObject = JsonObject(
    mapOf("input_tokens" to tokenJson(value.inputTokens), "output_tokens" to tokenJson(value.outputTokens)),
)
@OptIn(ExperimentalSerializationApi::class)
private fun tokenJson(value: TokenUsageEvidence): JsonObject = when (value) {
    is TokenUsageEvidence.Known -> JsonObject(mapOf("kind" to JsonPrimitive("known"), "value" to JsonPrimitive(value.value)))
    TokenUsageEvidence.Unknown -> JsonObject(mapOf("kind" to JsonPrimitive("unknown")))
}

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
