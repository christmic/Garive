package com.garive.eng.kt.ledger

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonObject

class LedgerTransitionMatrixTest {
    private fun fact(id: String, kind: String): FactDraft {
        val lifecycle = kind == "tool.preparation_rejected" || kind.startsWith("turn.") || kind.startsWith("execution.") ||
            kind.startsWith("model.") || kind.startsWith("effect.") || kind.startsWith("knowledge.")
        val payload = assertIs<CanonicalPayloadResult.Success>(
            CanonicalPayload.fromValue(runtimePayload(kind)),
        ).payload
        return FactDraft(
            FactId.of(id),
            if (lifecycle) TurnId.of("turn") else null,
            if (kind == "tool.preparation_rejected" || kind.startsWith("execution.") || kind.startsWith("model.") || kind.startsWith("effect.") || kind.startsWith("knowledge.")) {
                ExecutionId.of("execution")
            } else {
                null
            },
            if (kind == "tool.preparation_rejected" || kind.startsWith("model.")) ModelRequestId.of("request") else null,
            if (kind.startsWith("effect.")) ToolInvocationId.of("tool") else null,
            FactKind.of(kind),
            1u,
            payload,
            "2026-08-29T00:00:00Z",
        )
    }

    private fun commitKinds(kinds: List<String>): LedgerResult<CommitResult> = LedgerState().commit(
        SessionId.of("session"),
        0u,
        kinds.mapIndexed { index, kind -> fact("fact-$index", kind) },
    )

    private fun assertValid(vararg kinds: String) {
        val result = assertIs<LedgerResult.Success<CommitResult>>(commitKinds(kinds.toList()))
        assertEquals(CommitDisposition.COMMITTED, result.value.disposition)
    }

    private fun assertTransitionError(vararg kinds: String) {
        val result = assertIs<LedgerResult.Failure>(commitKinds(kinds.toList()))
        assertEquals(LedgerError.InvalidTransition, result.error)
    }

    @Test
    fun `every turn and execution terminal is admitted once`() {
        assertValid(
            "session.opened", "turn.started", "execution.started",
            "execution.iteration_started", "execution.completed",
        )
        assertTransitionError(
            "session.opened", "turn.started", "execution.started",
            "execution.iteration_started", "execution.iteration_started",
        )
        listOf("turn.completed", "turn.stopped", "turn.failed").forEach { terminal ->
            val executionTerminal = terminal.replaceFirst("turn.", "execution.")
            assertValid(
                "session.opened", "turn.started", "execution.started", executionTerminal, terminal,
            )
            assertTransitionError(
                "session.opened", "turn.started", "execution.started", executionTerminal,
                terminal, terminal,
            )
        }
        assertValid(
            "session.opened",
            "turn.started",
            "execution.started",
            "execution.completed",
            "turn.completed",
            "session.closed",
        )
        listOf(
            "execution.abandoned",
            "execution.completed",
            "execution.suspended",
            "execution.stopped",
            "execution.failed",
        ).forEach { terminal ->
            assertValid("session.opened", "turn.started", "execution.started", terminal)
            assertTransitionError(
                "session.opened",
                "turn.started",
                "execution.started",
                terminal,
                terminal,
            )
        }
    }

    @Test
    fun `C6 control facts require exact lifecycle owners`() {
        assertValid(
            "session.opened", "turn.started", "execution.started",
            "execution.abandoned",
        )
        assertTransitionError(
            "session.opened", "turn.started", "execution.started",
            "model.prepared", "model.started", "tool.preparation_rejected",
        )
        assertValid(
            "session.opened", "turn.started", "execution.started",
            "effect.prepared", "effect.denied", "effect.observation", "execution.completed",
        )
        assertTransitionError(
            "session.opened", "turn.started", "execution.started",
            "effect.prepared", "effect.observation",
        )
        assertTransitionError(
            "session.opened", "turn.started", "turn.completed", "turn.cancel_requested",
        )
    }

    @Test
    fun `every model terminal requires prepared started and same execution`() {
        listOf(
            "model.completed",
            "model.rejected",
            "model.interrupted",
            "model.unavailable",
            "model.uncertain",
        ).forEach { terminal ->
            assertValid(
                "session.opened",
                "turn.started",
                "execution.started",
                "model.prepared",
                "model.started",
                terminal,
                "execution.completed",
            )
            assertTransitionError(
                "session.opened",
                "turn.started",
                "execution.started",
                "model.prepared",
                terminal,
            )
        }

        val ledger = LedgerState()
        val secondExecution = fact("e2", "execution.started").copy(executionId = ExecutionId.of("execution-2"))
        val wrongOwner = fact("wrong-owner", "model.started").copy(executionId = secondExecution.executionId)
        val result = ledger.commit(
            SessionId.of("session"),
            0u,
            listOf(
                fact("open", "session.opened"),
                fact("turn", "turn.started"),
                fact("e1", "execution.started"),
                secondExecution,
                fact("prepared", "model.prepared"),
                wrongOwner,
            ),
        )
        assertEquals(LedgerError.InvalidTransition, assertIs<LedgerResult.Failure>(result).error)
    }

    @Test
    fun `every effect terminal and receipt path is explicit`() {
        assertValid(
            "session.opened",
            "turn.started",
            "execution.started",
            "effect.prepared",
            "effect.authorized",
            "effect.failed",
            "execution.completed",
        )
        listOf(
            listOf("effect.prepared", "effect.started"),
            listOf("effect.prepared", "effect.authorized", "effect.started"),
        ).forEach { prefix ->
            listOf("effect.failed", "effect.uncertain").forEach { terminal ->
                assertValid(
                    *(listOf("session.opened", "turn.started", "execution.started") +
                        prefix + terminal + "execution.completed").toTypedArray(),
                )
            }
        }
        assertValid(
            "session.opened",
            "turn.started",
            "execution.started",
            "effect.prepared",
            "effect.started",
            "effect.receipt",
            "effect.completed",
            "execution.completed",
        )
        listOf(
            listOf("effect.prepared", "effect.denied"),
            listOf("effect.prepared", "effect.authorized", "effect.denied"),
        ).forEach { prefix ->
            assertValid(
                *(listOf("session.opened", "turn.started", "execution.started") + prefix).toTypedArray(),
            )
        }
        assertTransitionError(
            "session.opened",
            "turn.started",
            "execution.started",
            "effect.prepared",
            "effect.completed",
        )
        assertValid(
            "session.opened",
            "turn.started",
            "execution.started",
            "effect.prepared",
            "effect.started",
            "effect.uncertain",
            "execution.suspended",
            "turn.suspended",
            "effect.reconciled",
            "effect.observation",
        )
        assertTransitionError(
            "session.opened",
            "turn.started",
            "execution.started",
            "effect.prepared",
            "effect.started",
            "effect.uncertain",
            "effect.observation",
        )
    }

    @Test
    fun `every Knowledge terminal requires requested and exact dispatch state`() {
        assertValid(
            "session.opened", "turn.started", "execution.started", "knowledge.requested",
            "knowledge.dispatched", "knowledge.completed", "execution.completed",
        )
        assertTransitionError(
            "session.opened", "turn.started", "execution.started", "knowledge.completed",
        )
        assertTransitionError(
            "session.opened", "turn.started", "execution.started", "knowledge.requested",
            "knowledge.completed",
        )
        assertTransitionError(
            "session.opened", "turn.started", "execution.started", "knowledge.requested",
            "knowledge.dispatched", "knowledge.completed", "knowledge.completed",
        )
        assertTransitionError(
            "session.opened", "turn.started", "execution.started", "knowledge.requested",
            "execution.completed",
        )
    }

    @Test
    fun `scheduler claims ranges terminals and updates are exact`() {
        assertValid(
            "session.opened", "schedule.created", "schedule.claimed",
            "schedule.fired", "schedule.skipped", "schedule.cancelled",
        )
        val exhaustedPayload = Json.parseToJsonElement(
            """{"last_handled_ordinal":4,"revision_id":"revision-1","schedule_id":"schedule-1"}""",
        )
        val exhausted = fact("exhausted", "schedule.exhausted").copy(
            payload = assertIs<CanonicalPayloadResult.Success>(
                CanonicalPayload.fromValue(exhaustedPayload),
            ).payload,
        )
        assertIs<LedgerResult.Success<CommitResult>>(
            LedgerState().commit(
                SessionId.of("exhausted-session"),
                0u,
                listOf(
                    fact("exhausted-open", "session.opened"),
                    fact("exhausted-create", "schedule.created"),
                    fact("exhausted-claim", "schedule.claimed"),
                    fact("exhausted-fired", "schedule.fired"),
                    fact("exhausted-skipped", "schedule.skipped"),
                    exhausted,
                    fact("exhausted-close", "session.closed"),
                ),
            ),
        )
        assertTransitionError("session.opened", "schedule.claimed")
        assertTransitionError("session.opened", "schedule.created", "schedule.fired")
        assertTransitionError(
            "session.opened", "schedule.created", "schedule.claimed", "schedule.cancelled",
        )
        assertTransitionError(
            "session.opened", "schedule.created", "schedule.claimed",
            "schedule.fired", "schedule.fired",
        )
        val failDuePayload = Json.parseToJsonElement(
            """{"occurrence_id":"occurrence-1","ordinal":1,"reason":"misfire_limit_exceeded","revision_id":"revision-1","schedule_id":"schedule-1"}""",
        )
        val failDue = fact("fail-due", "schedule.failed").copy(
            payload = assertIs<CanonicalPayloadResult.Success>(
                CanonicalPayload.fromValue(failDuePayload),
            ).payload,
        )
        assertIs<LedgerResult.Success<CommitResult>>(
            LedgerState().commit(
                SessionId.of("failed-session"),
                0u,
                listOf(
                    fact("failed-open", "session.opened"),
                    fact("failed-create", "schedule.created"),
                    failDue,
                ),
            ),
        )

        val supersededPayload = Json.parseToJsonElement(
            """{"command_id":"supersede","expected_revision_id":"revision-1","reason":"superseded","schedule_id":"schedule-1"}""",
        )
        val superseded = fact("supersede", "schedule.cancelled").copy(
            payload = assertIs<CanonicalPayloadResult.Success>(
                CanonicalPayload.fromValue(supersededPayload),
            ).payload,
        )
        val nextPayload = runtimePayload("schedule.created").jsonObject.toMutableMap().also {
            it["revision_id"] = kotlinx.serialization.json.JsonPrimitive("revision-2")
        }
        val next = fact("create-next", "schedule.created").copy(
            payload = assertIs<CanonicalPayloadResult.Success>(
                CanonicalPayload.fromValue(JsonObject(nextPayload)),
            ).payload,
        )
        val session = SessionId.of("session")
        val ledger = LedgerState()
        assertEquals(
            CommitDisposition.COMMITTED,
            assertIs<LedgerResult.Success<CommitResult>>(
                ledger.commit(
                    session,
                    0u,
                    listOf(
                        fact("open", "session.opened"),
                        fact("create", "schedule.created"),
                        superseded,
                        next,
                    ),
                ),
            ).value.disposition,
        )
        val invalid = LedgerState().commit(
            session,
            0u,
            listOf(
                fact("open", "session.opened"),
                fact("create", "schedule.created"),
                superseded,
            ),
        )
        assertEquals(LedgerError.InvalidTransition, assertIs<LedgerResult.Failure>(invalid).error)
    }

    @Test
    fun `parents cannot close before active or recovery pending children`() {
        assertTransitionError(
            "session.opened",
            "turn.started",
            "execution.started",
            "turn.completed",
        )
        listOf("model.started", "effect.started", "effect.receipt").forEach { pending ->
            val prepared = if (pending.startsWith("model")) "model.prepared" else "effect.prepared"
            val kinds = mutableListOf("session.opened", "turn.started", "execution.started", prepared)
            if (pending == "effect.receipt") kinds += "effect.started"
            kinds += listOf(pending, "execution.completed")
            assertTransitionError(*kinds.toTypedArray())
        }
        assertTransitionError("session.opened", "turn.started", "session.closed")
        assertTransitionError(
            "session.opened",
            "turn.started",
            "execution.started",
            "session.closed",
        )
    }

    @Test
    fun `commit validation and idempotency fail closed without partial state`() {
        val session = SessionId.of("session")
        val ledger = LedgerState()
        assertEquals(
            LedgerError.EmptyBatch,
            assertIs<LedgerResult.Failure>(ledger.commit(session, 0u, emptyList())).error,
        )
        val duplicate = fact("duplicate", "session.opened")
        assertEquals(
            LedgerError.InvalidFact,
            assertIs<LedgerResult.Failure>(ledger.commit(session, 0u, listOf(duplicate, duplicate))).error,
        )
        assertEquals(0, ledger.factCount(session))

        assertIs<LedgerResult.Success<CommitResult>>(
            ledger.commit(session, 0u, listOf(fact("open", "session.opened"))),
        )
        val incomplete = ledger.commit(
            session,
            1u,
            listOf(fact("open", "session.opened"), fact("new", "privacy.redacted")),
        )
        assertEquals(LedgerError.IncompleteReplay, assertIs<LedgerResult.Failure>(incomplete).error)
        assertEquals(1u, ledger.sessionVersion(session))
        assertEquals(1, ledger.factCount(session))
    }

    @Test
    fun `lifecycle identities cannot be reowned by another session`() {
        listOf("turn.started", "execution.started", "model.prepared", "effect.prepared").forEach { sharedKind ->
            val first = SessionId.of("first")
            val second = SessionId.of("second")
            val ledger = LedgerState()
            val prefix = listOf("session.opened", "turn.started", "execution.started")
                .takeWhile { it != sharedKind } + sharedKind
            assertIs<LedgerResult.Success<CommitResult>>(
                ledger.commit(first, 0u, prefix.mapIndexed { index, kind -> fact("first-$index", kind) }),
            )

            val result = ledger.commit(
                second,
                0u,
                listOf(fact("second-open", "session.opened"), fact("reowned", sharedKind)),
            )
            assertEquals(LedgerError.InvalidTransition, assertIs<LedgerResult.Failure>(result).error, sharedKind)
            assertEquals(0, ledger.factCount(second), sharedKind)
        }
    }

    @Test
    fun `read and recovery queries cover ranges filters and missing references`() {
        val session = SessionId.of("session")
        val ledger = LedgerState()
        assertIs<LedgerResult.Success<CommitResult>>(
            ledger.commit(
                session,
                0u,
                listOf(
                    fact("open", "session.opened"),
                    fact("turn", "turn.started"),
                    fact("execution", "execution.started"),
                    fact("model", "model.prepared"),
                    fact("effect", "effect.prepared"),
                ),
            ),
        )
        assertEquals(
            LedgerError.InvalidReadRange,
            assertIs<LedgerResult.Failure>(ledger.readFacts(session, 0u, 0u)).error,
        )
        assertEquals(
            LedgerError.InvalidReadRange,
            assertIs<LedgerResult.Failure>(ledger.readFacts(session, 3u, 3u)).error,
        )
        val missing = SessionId.of("missing")
        assertEquals(
            LedgerError.MissingReference,
            assertIs<LedgerResult.Failure>(ledger.readFacts(missing, 0u, 1u)).error,
        )

        val filtered = assertIs<LedgerResult.Success<List<DurableFact>>>(
            ledger.readFacts(session, 0u, 5u, setOf(FactKind.of("turn.started"))),
        ).value
        assertEquals(listOf(2uL), filtered.map { it.position })
        val snapshot = assertIs<LedgerResult.Success<TurnSnapshot>>(
            ledger.loadTurn(TurnId.of("turn")),
        ).value
        assertEquals(1u, snapshot.sessionVersion)
        assertEquals(5u, snapshot.throughPosition)
        assertEquals(4, snapshot.facts.size)
        assertEquals(1, ledger.findModelRequest(ModelRequestId.of("request")).size)
        assertEquals(1, ledger.findToolInvocation(ToolInvocationId.of("tool")).size)
        assertEquals(
            LedgerError.MissingReference,
            assertIs<LedgerResult.Failure>(ledger.loadTurn(TurnId.of("missing"))).error,
        )
        assertEquals(
            LedgerError.MissingReference,
            assertIs<LedgerResult.Failure>(ledger.listUncertainModelRequests(missing)).error,
        )
        assertEquals(
            LedgerError.MissingReference,
            assertIs<LedgerResult.Failure>(ledger.listUncertainToolInvocations(missing)).error,
        )
    }

    @Test
    fun `envelope identity and canonical payload boundaries are typed`() {
        listOf<(String) -> Any>(
            SessionId::of,
            TurnId::of,
            ExecutionId::of,
            FactId::of,
            ModelRequestId::of,
            ToolInvocationId::of,
            FactKind::of,
        ).forEach { constructor -> assertFailsWith<IllegalArgumentException> { constructor("") } }

        assertEquals(
            CanonicalPayloadError.INVALID_JSON,
            assertIs<CanonicalPayloadResult.Failure>(CanonicalPayload.fromStoredJson("{", "00")).error,
        )
        assertEquals(
            CanonicalPayloadError.NON_CANONICAL,
            assertIs<CanonicalPayloadResult.Failure>(
                CanonicalPayload.fromStoredJson("{\"b\":1,\"a\":2}", "00"),
            ).error,
        )
        assertEquals(
            CanonicalPayloadError.DIGEST_MISMATCH,
            assertIs<CanonicalPayloadResult.Failure>(CanonicalPayload.fromStoredJson("{}", "00")).error,
        )
        assertEquals(
            CanonicalPayloadError.UNSUPPORTED_NUMBER,
            assertIs<CanonicalPayloadResult.Failure>(
                CanonicalPayload.fromValue(Json.parseToJsonElement("1.5")),
            ).error,
        )

        val invalid = fact("invalid", "session.opened")
        assertEquals(LedgerError.InvalidFact, invalid.copy(schemaVersion = 0u).validate())
        assertEquals(LedgerError.InvalidFact, invalid.copy(recordedAt = "not-a-time").validate())
    }
}
