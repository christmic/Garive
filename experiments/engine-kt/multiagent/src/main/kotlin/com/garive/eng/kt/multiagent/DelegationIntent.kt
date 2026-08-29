package com.garive.eng.kt.multiagent

import com.garive.eng.kt.tools.validatePortableValueSchema
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

private const val INTENT_CONTRACT: String = "garive.delegation-intent"

/** Exact existing child or immutable definition-revision requirement. */
public sealed interface ChildRequirement {
    public data class Existing(public val childAgentInstanceId: String) : ChildRequirement
    public data class Definition(public val definitionId: String, public val definitionRevision: String) : ChildRequirement

    public companion object {
        /** Validates an existing child identity. */
        public fun existing(childAgentInstanceId: String): DelegationContractResult<ChildRequirement> =
            if (validId(childAgentInstanceId)) success(Existing(childAgentInstanceId))
            else failure(DelegationErrorCode.INVALID_DELEGATION)

        /** Validates one exact child definition revision. */
        public fun definition(definitionId: String, definitionRevision: String): DelegationContractResult<ChildRequirement> =
            if (validId(definitionId) && validId(definitionRevision)) success(Definition(definitionId, definitionRevision))
            else failure(DelegationErrorCode.INVALID_DELEGATION)
    }
}

/** Parent cancellation behavior for a started child. */
public enum class CancellationPolicy(public val wireName: String) {
    INDEPENDENT("independent"), CANCEL_WITH_PARENT("cancel_with_parent"),
}

/** Canonical inline delegation intent binding. */
public data class DelegationIntentBinding(public val digest: String, public val inlineUtf8: String)

/** Immutable bounded parent-to-child request semantics. */
public class DelegationIntent private constructor(
    public val delegationId: String,
    public val parentAgentInstanceId: String,
    public val parentTurnId: String,
    public val parentExecutionId: String,
    public val childRequirement: ChildRequirement,
    public val objective: ContentBinding,
    public val inputEvidence: List<FactReference>,
    public val resultSchema: ContentBinding,
    public val budget: DelegationBudget,
    public val cancellationPolicy: CancellationPolicy,
    public val throughPosition: ULong,
) {
    /** Computes RFC 8785 canonical JSON and SHA-256. */
    public fun intentBinding(): DelegationContractResult<DelegationIntentBinding> = runCatching {
        val bytes = JsonCanonicalizer(intentJson().toString()).encodedUTF8
        DelegationIntentBinding(sha256(bytes), bytes.decodeToString())
    }.fold(::success) { failure(DelegationErrorCode.INVALID_DELEGATION) }

    /** Returns the portable intent digest. */
    public fun intentDigest(): DelegationContractResult<String> = when (val result = intentBinding()) {
        is DelegationContractResult.Success -> success(result.value.digest)
        is DelegationContractResult.Failure -> result
    }

    public companion object {
        /** Validates all identities, bounds, evidence coordinates, and result schema. */
        @Suppress("LongParameterList")
        public fun create(
            delegationId: String, parentAgentInstanceId: String, parentTurnId: String,
            parentExecutionId: String, childRequirement: ChildRequirement,
            objective: ContentBinding, inputEvidence: List<FactReference>,
            resultSchema: ContentBinding, budget: DelegationBudget,
            cancellationPolicy: CancellationPolicy, throughPosition: ULong,
        ): DelegationContractResult<DelegationIntent> {
            if (budget.validate() is DelegationContractResult.Failure ||
                listOf(delegationId, parentAgentInstanceId, parentTurnId, parentExecutionId).any { !validId(it) } ||
                objective.inlineUtf8?.encodeToByteArray()?.size?.toULong()?.let { it > budget.maxObjectiveBytes } == true ||
                inputEvidence.size.toULong() > budget.maxInputEvidence ||
                inputEvidence.any { it.position > throughPosition } || inputEvidence.distinct().size != inputEvidence.size ||
                !validResultSchema(resultSchema, budget.maxResultSchemaBytes)
            ) return failure(DelegationErrorCode.INVALID_DELEGATION)
            return success(
                DelegationIntent(
                    delegationId, parentAgentInstanceId, parentTurnId, parentExecutionId,
                    childRequirement, objective, inputEvidence.toList(), resultSchema, budget,
                    cancellationPolicy, throughPosition,
                ),
            )
        }
    }

    @OptIn(ExperimentalSerializationApi::class)
    private fun intentJson(): JsonObject = JsonObject(
        mapOf(
            "contract" to JsonPrimitive(INTENT_CONTRACT), "version" to JsonPrimitive(1),
            "parent_agent_instance_id" to JsonPrimitive(parentAgentInstanceId),
            "parent_turn_id" to JsonPrimitive(parentTurnId), "parent_execution_id" to JsonPrimitive(parentExecutionId),
            "child_requirement" to childJson(childRequirement), "objective" to bindingJson(objective),
            "input_evidence" to kotlinx.serialization.json.JsonArray(inputEvidence.map(::evidenceJson)),
            "result_schema" to bindingJson(resultSchema), "budget" to budgetJson(budget),
            "cancellation_policy" to JsonPrimitive(cancellationPolicy.wireName),
            "through_position" to JsonPrimitive(throughPosition),
        ),
    )
}

private fun validResultSchema(binding: ContentBinding, maxBytes: ULong): Boolean {
    val text = binding.inlineUtf8 ?: return false
    if (text.encodeToByteArray().size.toULong() > maxBytes) return false
    return runCatching {
        val parsed = Json.parseToJsonElement(text)
        JsonCanonicalizer(parsed.toString()).encodedString == text && validatePortableValueSchema(parsed)
    }.getOrDefault(false)
}

private fun childJson(value: ChildRequirement): JsonObject = when (value) {
    is ChildRequirement.Existing -> JsonObject(mapOf("kind" to JsonPrimitive("existing"), "child_agent_instance_id" to JsonPrimitive(value.childAgentInstanceId)))
    is ChildRequirement.Definition -> JsonObject(mapOf("kind" to JsonPrimitive("definition"), "definition_id" to JsonPrimitive(value.definitionId), "definition_revision" to JsonPrimitive(value.definitionRevision)))
}

private fun bindingJson(value: ContentBinding): JsonObject = JsonObject(buildMap {
    put("digest", JsonPrimitive(value.digest))
    value.inlineUtf8?.let { put("inline_utf8", JsonPrimitive(it)) }
    value.reference?.let { put("reference", JsonPrimitive(it)) }
})

@OptIn(ExperimentalSerializationApi::class)
private fun evidenceJson(value: FactReference): JsonElement = JsonObject(
    mapOf("session_id" to JsonPrimitive(value.sessionId), "position" to JsonPrimitive(value.position), "fact_id" to JsonPrimitive(value.factId), "payload_digest" to JsonPrimitive(value.payloadDigest)),
)

@OptIn(ExperimentalSerializationApi::class)
private fun budgetJson(value: DelegationBudget): JsonObject = JsonObject(
    mapOf(
        "max_child_turns" to JsonPrimitive(value.maxChildTurns), "max_child_executions" to JsonPrimitive(value.maxChildExecutions),
        "max_iterations" to JsonPrimitive(value.maxIterations), "max_input_tokens" to JsonPrimitive(value.maxInputTokens),
        "max_output_tokens" to JsonPrimitive(value.maxOutputTokens), "deadline_budget_ms" to JsonPrimitive(value.deadlineBudgetMs),
        "max_depth" to JsonPrimitive(value.maxDepth), "max_objective_bytes" to JsonPrimitive(value.maxObjectiveBytes),
        "max_input_evidence" to JsonPrimitive(value.maxInputEvidence), "max_result_schema_bytes" to JsonPrimitive(value.maxResultSchemaBytes),
        "max_result_bytes" to JsonPrimitive(value.maxResultBytes), "max_result_evidence" to JsonPrimitive(value.maxResultEvidence),
    ),
)
