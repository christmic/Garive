package com.garive.eng.kt.host

import com.garive.eng.kt.ledger.CanonicalPayload
import com.garive.eng.kt.ledger.CanonicalPayloadResult
import com.garive.eng.kt.ledger.ExecutionId
import com.garive.eng.kt.ledger.FactDraft
import com.garive.eng.kt.ledger.FactId
import com.garive.eng.kt.ledger.FactKind
import com.garive.eng.kt.ledger.ModelRequestId
import com.garive.eng.kt.ledger.SessionId
import com.garive.eng.kt.ledger.TurnId
import com.garive.eng.kt.postgres.PostgresConfig
import com.garive.eng.kt.postgres.PostgresLedger
import io.zonky.test.db.postgres.embedded.EmbeddedPostgres
import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class PostgresRecoveryHostTest {
    @Test
    fun `real PostgreSQL host restarts only child-safe lost execution`() {
        EmbeddedPostgres.start().use { postgres ->
            val dataSource = postgres.postgresDatabase
            val jdbcUrl = dataSource.connection.use { it.metaData.url }
            val ledger = PostgresLedger.open(PostgresConfig(jdbcUrl, "postgres", "postgres"))
            val session = SessionId.of("recovery-session")
            val turn = TurnId.of("recovery-turn")
            val lost = ExecutionId.of("lost-execution")
            ledger.commit(
                session,
                0u,
                listOf(
                    draft("session", "session.opened"),
                    draft("turn", "turn.started", turn),
                    draft("execution", "execution.started", turn, lost),
                    draft("iteration", "execution.iteration_started", turn, lost),
                ),
            )
            val host = PostgresRecoveryHost(ledger)
            val result = host.restartLostExecution(
                PostgresRecoveryRequest(
                    session,
                    turn,
                    "recovery-1",
                    ExecutionId.of("replacement-execution"),
                    3u,
                    "2026-08-29T00:00:01Z",
                ),
            )
            assertEquals(listOf(5uL, 6uL), result.positions)
            val reopened = PostgresLedger.open(PostgresConfig(jdbcUrl, "postgres", "postgres"))
            val snapshot = reopened.loadTurn(turn)
            assertEquals(
                listOf("execution.abandoned", "execution.started"),
                snapshot.facts.takeLast(2).map { it.kind.value },
            )
            assertEquals(
                "1",
                snapshot.facts.last().payload.objectValue().getValue("completed_iterations").jsonPrimitive.content,
            )

            val unsafeSession = SessionId.of("unsafe-session")
            val unsafeTurn = TurnId.of("unsafe-turn")
            val unsafeExecution = ExecutionId.of("unsafe-execution")
            ledger.commit(
                unsafeSession,
                0u,
                listOf(
                    draft("unsafe-session-fact", "session.opened"),
                    draft("unsafe-turn-fact", "turn.started", unsafeTurn),
                    draft("unsafe-execution-fact", "execution.started", unsafeTurn, unsafeExecution),
                    draft("unsafe-prepared", "model.prepared", unsafeTurn, unsafeExecution, "unsafe-request"),
                    draft("unsafe-started", "model.started", unsafeTurn, unsafeExecution, "unsafe-request"),
                ),
            )
            val rejected = assertFailsWith<PostgresRecoveryRejected> {
                host.restartLostExecution(
                    PostgresRecoveryRequest(
                        unsafeSession,
                        unsafeTurn,
                        "unsafe-recovery",
                        ExecutionId.of("must-not-start"),
                        3u,
                        "2026-08-29T00:00:02Z",
                    ),
                )
            }
            assertEquals(RuntimeRecoveryAction.CLASSIFY_MODEL_UNCERTAIN, rejected.action)
            assertEquals(4, ledger.loadTurn(unsafeTurn).facts.size)
        }
    }

    private fun draft(
        id: String,
        kind: String,
        turn: TurnId? = null,
        execution: ExecutionId? = null,
        request: String? = null,
    ) = FactDraft(
        FactId.of(id),
        turn,
        execution,
        request?.let(ModelRequestId::of),
        null,
        FactKind.of(kind),
        1u,
        runtimePayload(kind),
        "2026-08-29T00:00:00Z",
    )

    private fun runtimePayload(kind: String): CanonicalPayload {
        val root = Path.of(System.getProperty("garive.repo.root"))
        val cases = Json.parseToJsonElement(
            root.resolve("spec/fixtures/ledger/runtime-facts-v1.json").readText(),
        ).jsonObject.getValue("valid_cases").jsonArray
        val value = cases.firstOrNull {
            it.jsonObject.getValue("kind").jsonPrimitive.content == kind
        }?.jsonObject?.getValue("payload") ?: JsonObject(emptyMap())
        return (CanonicalPayload.fromValue(value) as CanonicalPayloadResult.Success).payload
    }
}

private fun CanonicalPayload.objectValue(): JsonObject = Json.parseToJsonElement(json).jsonObject
