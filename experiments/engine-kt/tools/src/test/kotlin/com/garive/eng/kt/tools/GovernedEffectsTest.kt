package com.garive.eng.kt.tools

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.io.path.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class GovernedEffectsTest {
    private val fixture: JsonObject by lazy {
        val root = Path(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(
            root.resolve("spec/fixtures/agent/governed-effects.json").readText(),
        ).jsonObject
    }

    private fun <T> toolSuccess(result: ToolContractResult<T>): T =
        (result as ToolContractResult.Success).value

    private fun <T> valueSuccess(result: GovernedValueResult<T>): T =
        (result as GovernedValueResult.Success).value

    private fun requirements(value: JsonObject): ExecutionRequirements =
        toolSuccess(
            ExecutionRequirements.create(
                value.getValue("capabilities").jsonArray.map { item ->
                    ExecutionCapability.entries.single { it.wireName == item.jsonPrimitive.content }
                },
                value.getValue("max_duration_ms").jsonPrimitive.content.toLong(),
                value.getValue("max_output_bytes").jsonPrimitive.content.toLong(),
            ),
        )

    private fun prepared(): PreparedToolCall {
        val expected = fixture.getValue("prepared_call").jsonObject
        val definition = toolSuccess(
            ToolDefinition.create(
                expected.getValue("tool_name").jsonPrimitive.content,
                expected.getValue("tool_revision").jsonPrimitive.content,
                "Read one admitted file.",
                Json.parseToJsonElement("""{"${'$'}schema":"https://json-schema.org/draft/2020-12/schema","type":"object","properties":{"path":{"type":"string","minLength":1}},"required":["path"],"additionalProperties":false}"""),
                requirements(expected.getValue("requirements").jsonObject),
                ReplayClass.READ_ONLY,
            ),
        )
        val call = toolSuccess(
            toolSuccess(ToolCatalog.create(listOf(definition))).prepare(
                ToolIntent(
                    expected.getValue("model_call_id").jsonPrimitive.content,
                    expected.getValue("tool_name").jsonPrimitive.content,
                    expected.getValue("normalized_arguments").jsonPrimitive.content,
                ),
            ),
        )
        assertEquals(expected.getValue("input_digest").jsonPrimitive.content, call.inputDigest)
        return call
    }

    private fun grant(name: String): InvocationGrant {
        val value = fixture.getValue("grants").jsonObject.getValue(name).jsonObject
        return InvocationGrant(
            valueSuccess(GrantId.create(value.getValue("grant_id").jsonPrimitive.content)),
            valueSuccess(ToolInvocationId.create(value.getValue("invocation_id").jsonPrimitive.content)),
            value.getValue("prepared_digest").jsonPrimitive.content,
            value.getValue("tool_name").jsonPrimitive.content,
            value.getValue("tool_revision").jsonPrimitive.content,
            requirements(value.getValue("granted_requirements").jsonObject),
            value.getValue("constraints_digest").jsonPrimitive.content,
            value.getValue("authority_revision").jsonPrimitive.content,
        )
    }

    private fun interaction(): InteractionRequest {
        val value = fixture.getValue("interaction").jsonObject
        return InteractionRequest(
            valueSuccess(InteractionId.create(value.getValue("interaction_id").jsonPrimitive.content)),
            valueSuccess(ToolInvocationId.create(value.getValue("invocation_id").jsonPrimitive.content)),
            value.getValue("prepared_digest").jsonPrimitive.content,
            InteractionKind.APPROVAL,
            value.getValue("prompt"),
            value.getValue("response_schema"),
            value.getValue("expiry_policy").jsonPrimitive.content,
        )
    }

    private fun receipt(name: String): EffectReceipt {
        val value = fixture.getValue("receipts").jsonObject.getValue(name).jsonObject
        return EffectReceipt(
            valueSuccess(ReceiptId.create(value.getValue("receipt_id").jsonPrimitive.content)),
            valueSuccess(ToolInvocationId.create(value.getValue("invocation_id").jsonPrimitive.content)),
            value.getValue("prepared_digest").jsonPrimitive.content,
            valueSuccess(GrantId.create(value.getValue("grant_id").jsonPrimitive.content)),
            value.getValue("executor_id").jsonPrimitive.content,
            value.getValue("executor_revision").jsonPrimitive.content,
            TerminalClassification.entries.single {
                it.name.lowercase() == value.getValue("terminal_classification").jsonPrimitive.content
            },
            value.getValue("result_digest").jsonPrimitive.content,
        )
    }

    private fun actionName(action: GovernedAction): String = when (action) {
        GovernedAction.Authorize -> "authorize"
        is GovernedAction.Dispatch -> "dispatch"
        is GovernedAction.Observation -> "observation_${action.observation.modelEnvelope().getValue("status").jsonPrimitive.content}"
        is GovernedAction.Suspend -> when (action.requirement) {
            is SuspensionRequirement.Interaction -> "suspend_approval"
            is SuspensionRequirement.OperatorReconciliation -> "suspend_reconciliation"
        }
        is GovernedAction.Fail -> "fail_${action.code.name.lowercase()}"
        GovernedAction.None -> "none"
    }

    private fun apply(reducer: GovernedEffect, operation: JsonObject): GovernedAction =
        when (operation.getValue("kind").jsonPrimitive.content) {
            "approve" -> reducer.applyAuthorization(AuthorizationVerdict.Approve(grant(operation.getValue("grant").jsonPrimitive.content)))
            "deny" -> reducer.applyAuthorization(AuthorizationVerdict.Deny(operation.getValue("code").jsonPrimitive.content, operation["details"]?.jsonPrimitive?.content))
            "replacement_required" -> reducer.applyAuthorization(AuthorizationVerdict.ReplacementRequired)
            "interaction_required" -> reducer.applyAuthorization(AuthorizationVerdict.InteractionRequired(interaction()))
            "interaction_resolved" -> interaction().let { reducer.applyInteraction(InteractionResolution.Resolved(it.interactionId, it.invocationId, it.preparedDigest, operation.getValue("response"))) }
            "interaction_cancelled" -> interaction().let { reducer.applyInteraction(InteractionResolution.Cancelled(it.interactionId, it.invocationId, it.preparedDigest)) }
            "started" -> reducer.applyExecution(ExecutionFact.Started(valueSuccess(DispatchAttemptId.create(operation.getValue("dispatch_attempt_id").jsonPrimitive.content))))
            "completed" -> reducer.applyExecution(ExecutionFact.Completed(operation["receipt"]?.jsonPrimitive?.content?.let(::receipt), operation.getValue("content"), operation.getValue("truncated").jsonPrimitive.content.toBoolean()))
            "failed" -> reducer.applyExecution(ExecutionFact.Failed(operation["receipt"]?.jsonPrimitive?.content?.let(::receipt), operation.getValue("code").jsonPrimitive.content, operation["details"]?.jsonPrimitive?.content, operation["partial"]))
            "uncertain" -> reducer.applyExecution(ExecutionFact.Uncertain(operation.getValue("evidence").jsonPrimitive.content))
            "unsupported" -> reducer.applyExecution(ExecutionFact.Unsupported(operation.getValue("requirement").jsonPrimitive.content))
            else -> error("unknown operation: ${operation.getValue("kind")}")
        }

    @Test
    fun sharedGovernedScenariosMatch() {
        fixture.getValue("scenarios").jsonArray.forEach { element ->
            val case = element.jsonObject
            val start = GovernedEffect.start(
                valueSuccess(ToolInvocationId.create(fixture.getValue("invocation_id").jsonPrimitive.content)),
                prepared(),
            )
            val reducer = start.first
            val actions = mutableListOf(start.second)
            case.getValue("operations").jsonArray.forEach { actions += apply(reducer, it.jsonObject) }
            val expected = case.getValue("expected").jsonObject
            assertEquals(
                expected.getValue("actions").jsonArray.map { it.jsonPrimitive.content },
                actions.map(::actionName),
                case.getValue("name").jsonPrimitive.content,
            )
            assertEquals(
                expected.getValue("execution_command_count").jsonPrimitive.content.toInt(),
                actions.count { it is GovernedAction.Dispatch },
            )
            assertEquals(expected.getValue("final_state").jsonPrimitive.content, reducer.state.name.lowercase())
            expected["observation"]?.let { expectedObservation ->
                val actual = actions.filterIsInstance<GovernedAction.Observation>().first().observation.modelEnvelope()
                assertEquals(expectedObservation, actual)
            }
        }
    }

    @Test
    fun governanceIdentitiesRejectEmptyValues() {
        assertTrue(ToolInvocationId.create("") is GovernedValueResult.Failure)
        assertTrue(InteractionId.create("") is GovernedValueResult.Failure)
        assertTrue(GrantId.create("") is GovernedValueResult.Failure)
        assertTrue(ReceiptId.create("") is GovernedValueResult.Failure)
        assertTrue(DispatchAttemptId.create("") is GovernedValueResult.Failure)
    }

    @Test
    fun sharedPreparationFailuresReduceSafely() {
        fixture.getValue("preparation_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val input = case.getValue("intent").jsonObject
            val intent = ToolIntent(
                input.getValue("model_call_id").jsonPrimitive.content,
                input.getValue("tool_name").jsonPrimitive.content,
                input.getValue("arguments_json").jsonPrimitive.content,
            )
            val error = (
                toolSuccess(
                    ToolCatalog.create(
                        listOf(
                            toolSuccess(
                                ToolDefinition.create(
                                    "read_file",
                                    "1",
                                    "Read one admitted file.",
                                    Json.parseToJsonElement("""{"type":"object","properties":{"path":{"type":"string","minLength":1}},"required":["path"],"additionalProperties":false}"""),
                                    toolSuccess(ExecutionRequirements.create(listOf(ExecutionCapability.FILESYSTEM_READ), 5000, 4096)),
                                    ReplayClass.READ_ONLY,
                                ),
                            ),
                        ),
                    ),
                ).prepare(intent) as ToolContractResult.Failure
            ).error
            when (val result = reducePreparationFailure(intent, error)) {
                is GovernedToolResult.Observation -> {
                    val feedback = (result.feedback as ToolFeedback.PreparationRejected).feedback
                    assertEquals(error.code, feedback.code)
                    assertEquals(
                        case.getValue("expected").jsonObject.getValue("failure_paths").jsonArray.map { it.jsonPrimitive.content },
                        feedback.failurePaths,
                    )
                }
                is GovernedToolResult.Fail -> assertEquals(GovernedFailureCode.INVALID_MODEL_OUTPUT, result.code)
                is GovernedToolResult.Suspend -> error("preparation cannot suspend")
            }
        }
    }
}
