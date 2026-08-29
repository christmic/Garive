package com.garive.eng.kt.ledger

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.put
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

class DelegationTransitionTest {
    private val session = SessionId.of("session")

    @Test
    fun `delegation boundaries and observed continuation are atomic`() {
        val ledger = LedgerState()
        establishAuthorized(ledger)
        val missingStart = LedgerState().also(::establishAuthorized)
        assertFailure(missingStart.commit(session, 3uL, childStartBatch(false)))
        assertSuccess(ledger.commit(session, 3uL, childStartBatch(true)))

        val terminal = childTerminalBatch(true)
        val missingTerminal = LedgerState().also(::establishAuthorized)
        assertSuccess(missingTerminal.commit(session, 3uL, childStartBatch(true)))
        assertFailure(missingTerminal.commit(session, 4uL, childTerminalBatch(false)))
        assertSuccess(ledger.commit(session, 4uL, terminal))
        assertFailure(ledger.commit(session, 5uL, listOf(resultInput("premature"))))
        assertSuccess(ledger.commit(session, 5uL, listOf(fixture("observed", "delegation.observed", "turn", "execution"))))
        assertSuccess(ledger.commit(session, 6uL, listOf(resultInput("result-input"), continuedTurn())))
    }

    private fun establishAuthorized(ledger: LedgerState) {
        assertSuccess(
            ledger.commit(
                session, 0uL,
                listOf(
                    fixture("open", "session.opened", null, null),
                    fixture("parent", "turn.started", "turn", null),
                    fixture("parent-execution", "execution.started", "turn", "execution"),
                ),
            ),
        )
        val requested = mutate("delegation.requested", "parent_agent_instance_id" to JsonPrimitive("agent"))
        assertSuccess(ledger.commit(session, 1uL, listOf(fact("requested", "delegation.requested", "turn", "execution", requested))))
        assertSuccess(ledger.commit(session, 2uL, listOf(fixture("authorized", "delegation.authorized", "turn", "execution"))))
    }

    private fun childStartBatch(includeBinding: Boolean): List<FactDraft> = buildList {
        add(
            fact(
                "execution-suspended", "execution.suspended", "turn", "execution",
                mutate(
                    "execution.suspended", "suspension_id" to JsonPrimitive("delegation-suspension-1"),
                    "reason" to JsonPrimitive("delegation_pending"),
                ),
            ),
        )
        add(
            fact(
                "turn-suspended", "turn.suspended", "turn", null,
                mutate(
                    "turn.suspended", "suspension_id" to JsonPrimitive("delegation-suspension-1"),
                    "reason" to JsonPrimitive("delegation_pending"),
                ),
            ),
        )
        add(
            fact(
                "child-start", "turn.started", "child-turn", null,
                mutate(
                    "turn.started", "agent_instance_id" to JsonPrimitive("child-agent"),
                    "snapshot_digest" to JsonPrimitive("c".repeat(64)),
                ),
            ),
        )
        if (includeBinding) add(fixture("child-bound", "delegation.child_started", "turn", "execution"))
    }

    private fun childTerminalBatch(includeBinding: Boolean): List<FactDraft> = buildList {
        add(
            fact(
                "child-execution", "execution.started", "child-turn", "child-execution",
                buildJsonObject {
                    put("snapshot_digest", "c".repeat(64)); put("through_position", 0)
                    put("completed_iterations", 0); put("limits", buildJsonObject { put("max_iterations", 1) })
                    put("recovery_ordinal", 0)
                },
            ),
        )
        add(fixture("child-execution-done", "execution.completed", "child-turn", "child-execution"))
        add(
            fact(
                "child-turn-done", "turn.completed", "child-turn", null,
                mutate("turn.completed", "execution_id" to JsonPrimitive("child-execution")),
            ),
        )
        if (includeBinding) add(fixture("delegation-terminal", "delegation.child_terminal", "turn", "execution"))
    }

    private fun resultInput(id: String): FactDraft = fact(
        id, "turn.input", "turn", null,
        buildJsonObject {
            put("input_kind", "delegation_result")
            put("content", buildJsonObject {
                put("digest", "67de0204b4a2e4f3302cc45d68feac2346d1b3697f36d0f90d4ba9f6fd65e815")
                put("reference", "fixture:delegation-result-1")
            })
            put("suspension_id", "delegation-suspension-1")
        },
    )

    private fun continuedTurn(): FactDraft = fact(
        "continued", "turn.started", "turn", null,
        buildJsonObject {
            put("command_id", "continue"); put("kind", "continue"); put("agent_instance_id", "agent")
            put("definition_id", "definition"); put("definition_revision", "revision")
            put("snapshot_digest", EMPTY_DIGEST); put("trusted_input_digest", EMPTY_DIGEST)
            put("prior_suspension_id", "delegation-suspension-1"); put("expected_session_version", 6)
        },
    )

    private fun mutate(kind: String, vararg changes: Pair<String, JsonElement>): JsonObject =
        JsonObject(runtimePayload(kind).jsonObject.toMutableMap().apply { changes.forEach { (key, value) -> put(key, value) } })

    private fun fixture(id: String, kind: String, turn: String?, execution: String?): FactDraft =
        fact(id, kind, turn, execution, runtimePayload(kind))

    private fun fact(id: String, kind: String, turn: String?, execution: String?, payload: JsonElement): FactDraft = FactDraft(
        FactId.of(id), turn?.let(TurnId::of), execution?.let(ExecutionId::of), null, null,
        FactKind.of(kind), 1u, assertIs<CanonicalPayloadResult.Success>(CanonicalPayload.fromValue(payload)).payload,
        "2026-08-29T00:00:00Z",
    )

    private fun assertSuccess(value: LedgerResult<CommitResult>) { assertIs<LedgerResult.Success<CommitResult>>(value) }
    private fun assertFailure(value: LedgerResult<CommitResult>) { assertEquals(LedgerError.InvalidTransition, assertIs<LedgerResult.Failure>(value).error) }

    private companion object {
        const val EMPTY_DIGEST = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    }
}
