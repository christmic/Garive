package com.garive.eng.kt.multiagent

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

class DelegationFixtureTest {
    private val root = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/multi-agent-delegation-v1.json").readText(),
    ).jsonObject

    @Test
    fun `shared digest failures and exact settlement match Rust`() {
        val intent = intent()
        assertEquals(root.intent.string("expected_intent_digest"), assertSuccess(intent.intentDigest()))
        val original = allowance(2uL, intent.budget)
        val authorization = assertSuccess(authorizeDelegation(intent, "grant-1", "authority-1", 0uL, 0uL, original))
        val completedFixture = root.getValue("completed_result").jsonObject
        val completed = assertSuccess(
            completeDelegationResult(
                intent,
                context(
                    DelegationUsage(TokenUsageEvidence.Known(10uL), TokenUsageEvidence.Known(5uL)),
                    DelegationConsumption(1uL, 2uL, 4uL, 1_200uL),
                ),
                ContentBinding.fromInline(completedFixture.getValue("content").jsonObject.string("inline_utf8")),
                completedFixture.getValue("content").jsonObject.string("inline_utf8"),
                emptyList(),
            ),
        )
        assertEquals(10uL, completed.settlement.charged.inputTokens)
        assertEquals(90uL, completed.settlement.released.inputTokens)
        assertEquals(
            completedFixture.string("expected_result_digest"),
            assertSuccess(completed.resultBinding()).digest,
        )
        val released = assertSuccess(releaseDelegationBudget(authorization.remaining, completed.settlement, original))
        assertEquals(original.remainingInputTokens - 10uL, released.remainingInputTokens)
        assertEquals(
            root.getValue("failure_codes").jsonArray.map { it.jsonPrimitive.content },
            DelegationErrorCode.entries.map(DelegationErrorCode::wireName),
        )
    }

    @Test
    fun `depth concurrency budget and schema bounds fail closed`() {
        val intent = intent()
        val exact = allowance(1uL, intent.budget)
        assertFailure(DelegationErrorCode.DEPTH_EXCEEDED, authorizeDelegation(intent, "grant", "authority", 2uL, 0uL, exact))
        assertFailure(DelegationErrorCode.CONCURRENCY_EXCEEDED, authorizeDelegation(intent, "grant", "authority", 0uL, 1uL, exact))
        assertFailure(DelegationErrorCode.BUDGET_EXHAUSTED, authorizeDelegation(intent, "grant", "authority", 0uL, 0uL, allowance(0uL, intent.budget)))
        assertFailure(
            DelegationErrorCode.RESULT_SCHEMA_MISMATCH,
            completeDelegationResult(
                intent,
                context(
                    DelegationUsage(TokenUsageEvidence.Unknown, TokenUsageEvidence.Unknown),
                    DelegationConsumption(1uL, 1uL, 1uL, 1uL),
                ),
                ContentBinding.fromInline("[]"), "[]", emptyList(),
            ),
        )
    }

    @Test
    fun `settlement never creates budget and checked release overflow fails`() {
        val reservation = intent().budget
        for (executions in 1uL..reservation.maxChildExecutions) {
            for (iterations in 0uL..reservation.maxIterations) {
                val settlement = assertSuccess(
                    settleDelegationBudget(
                        reservation, DelegationConsumption(1uL, executions, iterations, iterations * 100uL),
                        DelegationUsage(TokenUsageEvidence.Known(iterations), TokenUsageEvidence.Unknown),
                    ),
                )
                assertEquals(reservation.maxChildExecutions, settlement.charged.childExecutions + settlement.released.childExecutions)
                assertEquals(reservation.maxIterations, settlement.charged.iterations + settlement.released.iterations)
                assertEquals(reservation.maxInputTokens, settlement.charged.inputTokens + settlement.released.inputTokens)
                assertEquals(reservation.maxOutputTokens, settlement.charged.outputTokens)
            }
        }
        val maximum = DelegationAllowance(ULong.MAX_VALUE, 0uL, 0uL, 0uL, 0uL, 0uL, 1uL, 1uL, 1uL, 1uL, 1uL, 1uL)
        val zero = BudgetAmounts(0uL, 0uL, 0uL, 0uL, 0uL, 0uL)
        assertFailure(
            DelegationErrorCode.BUDGET_OVERFLOW,
            releaseDelegationBudget(maximum, DelegationBudgetSettlement(zero, zero.copy(childTurns = 1uL)), maximum),
        )
    }

    private fun intent(): DelegationIntent {
        val value = root.intent
        val child = value.getValue("child_requirement").jsonObject
        val objective = value.getValue("objective").jsonObject
        val schema = value.getValue("result_schema").jsonObject
        val evidence = value.getValue("input_evidence").jsonArray.single().jsonObject
        return assertSuccess(
            DelegationIntent.create(
                value.string("delegation_id"), value.string("parent_agent_instance_id"),
                value.string("parent_turn_id"), value.string("parent_execution_id"),
                assertSuccess(ChildRequirement.definition(child.string("definition_id"), child.string("definition_revision"))),
                ContentBinding.fromInline(objective.string("inline_utf8")),
                listOf(
                    assertSuccess(
                        FactReference.create(
                            evidence.string("session_id"), evidence.ulong("position"),
                            evidence.string("fact_id"), evidence.string("payload_digest"),
                        ),
                    ),
                ),
                ContentBinding.fromInline(schema.string("inline_utf8")), budget(value.getValue("budget").jsonObject),
                CancellationPolicy.CANCEL_WITH_PARENT, value.ulong("through_position"),
            ),
        )
    }

    private fun budget(value: JsonObject) = DelegationBudget(
        value.ulong("max_child_turns"), value.ulong("max_child_executions"), value.ulong("max_iterations"),
        value.ulong("max_input_tokens"), value.ulong("max_output_tokens"), value.ulong("deadline_budget_ms"),
        value.ulong("max_depth"), value.ulong("max_objective_bytes"), value.ulong("max_input_evidence"),
        value.ulong("max_result_schema_bytes"), value.ulong("max_result_bytes"), value.ulong("max_result_evidence"),
    )

    private fun allowance(multiplier: ULong, budget: DelegationBudget) = DelegationAllowance(
        budget.maxChildTurns * multiplier, budget.maxChildExecutions * multiplier,
        budget.maxIterations * multiplier, budget.maxInputTokens * multiplier,
        budget.maxOutputTokens * multiplier, budget.deadlineBudgetMs * multiplier,
        budget.maxDepth, budget.maxObjectiveBytes, budget.maxInputEvidence,
        budget.maxResultSchemaBytes, budget.maxResultBytes, budget.maxResultEvidence,
    )

    private fun context(usage: DelegationUsage, consumption: DelegationConsumption) = DelegationResultContext(
        "result-1", "delegation-1", "grant-1", "child-agent", "child-turn", "c".repeat(64), usage, consumption,
    )

    private val JsonObject.intent: JsonObject get() = getValue("intent").jsonObject
    private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content
    private fun JsonObject.ulong(key: String): ULong = string(key).toULong()
    private fun <T> assertSuccess(result: DelegationContractResult<T>): T = assertIs<DelegationContractResult.Success<T>>(result).value
    private fun assertFailure(expected: DelegationErrorCode, result: DelegationContractResult<*>): Unit =
        assertEquals(expected, assertIs<DelegationContractResult.Failure>(result).code)
}
