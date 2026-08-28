package com.garive.runtime.server.postgres

import com.garive.runtime.server.ledger.CanonicalPayload
import com.garive.runtime.server.ledger.CanonicalPayloadResult
import com.garive.runtime.server.ledger.CommitDisposition
import com.garive.runtime.server.ledger.ExecutionId
import com.garive.runtime.server.ledger.FactDraft
import com.garive.runtime.server.ledger.FactId
import com.garive.runtime.server.ledger.FactKind
import com.garive.runtime.server.ledger.LedgerError
import com.garive.runtime.server.ledger.ModelRequestId
import com.garive.runtime.server.ledger.SessionId
import com.garive.runtime.server.ledger.TurnId
import io.zonky.test.db.postgres.embedded.EmbeddedPostgres
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

class PostgresLedgerIntegrationTest {
    @Test
    fun `real PostgreSQL preserves atomic ledger recovery`() {
        EmbeddedPostgres.start().use { postgres ->
            val dataSource = postgres.postgresDatabase
            val jdbcUrl = dataSource.connection.use { connection ->
                val majorVersion = connection.metaData.databaseMajorVersion
                assertTrue(majorVersion >= 14, "real PostgreSQL is required")
                connection.metaData.url
            }
            val config = PostgresConfig(jdbcUrl, "postgres", "postgres")
            val session = SessionId.of("session")
            val first = initialFacts()
            val ledger = PostgresLedger.open(config)

            val committed = ledger.commit(session, 0u, first)
            assertEquals(CommitDisposition.COMMITTED, committed.disposition)
            assertEquals((1uL..5uL).toList(), committed.positions)
            assertEquals(listOf("r1"), ledger.listUncertainModelRequests(session).map { it.value })

            val reopened = PostgresLedger.open(config)
            assertEquals(1uL, reopened.sessionVersion(session))
            val facts = reopened.readFacts(session, 0u, 5u)
            assertEquals(5, facts.size)
            assertEquals("""{"a":"蟹","z":[2,1]}""", facts.first().payload.json)

            val replay = reopened.commit(session, 0u, initialFacts())
            assertEquals(CommitDisposition.REPLAYED, replay.disposition)
            assertEquals(1uL, replay.sessionVersion)
            assertDomain(LedgerError.ConcurrentModification) {
                reopened.commit(session, 0u, listOf(draft("f6", "privacy.redacted")))
            }

            val collision = initialFacts().first().copy(
                payload = payload("""{"changed":true}"""),
            )
            assertDomain(LedgerError.IdempotencyCollision) {
                reopened.commit(session, 1u, listOf(collision))
            }
            assertDomain(LedgerError.InvalidTransition) {
                reopened.commit(
                    session,
                    1u,
                    listOf(
                        draft("f7", "execution.completed", "t1", "e1"),
                        draft("f8", "execution.failed", "t1", "e1"),
                    ),
                )
            }
            assertEquals(1uL, PostgresLedger.open(config).sessionVersion(session))
            assertEquals(5, PostgresLedger.open(config).readFacts(session, 0u, 5u).size)

            dataSource.connection.use { connection ->
                connection.createStatement().use {
                    it.executeUpdate(
                        "UPDATE ledger_facts SET payload_sha256 = " +
                            "'0000000000000000000000000000000000000000000000000000000000000000' " +
                            "WHERE fact_id = 'f2'",
                    )
                }
            }
            assertFailsWith<PostgresLedgerError.Corrupt> {
                PostgresLedger.open(config).sessionVersion(session)
            }
        }
    }

    private fun initialFacts() = listOf(
        draft("f1", "session.opened", payload = payload("""{"z":[2,1],"a":"蟹"}""")),
        draft("f2", "turn.started", "t1"),
        draft("f3", "execution.started", "t1", "e1"),
        draft("f4", "model.prepared", "t1", "e1", "r1"),
        draft("f5", "model.started", "t1", "e1", "r1"),
    )

    private fun draft(
        id: String,
        kind: String,
        turn: String? = null,
        execution: String? = null,
        request: String? = null,
        payload: CanonicalPayload = payload("{}"),
    ) = FactDraft(
        FactId.of(id),
        turn?.let(TurnId::of),
        execution?.let(ExecutionId::of),
        request?.let(ModelRequestId::of),
        null,
        FactKind.of(kind),
        1u,
        payload,
        "2026-08-29T00:00:00Z",
    )

    private fun payload(json: String): CanonicalPayload {
        val value = Json.parseToJsonElement(json)
        assertIs<JsonObject>(value)
        return assertIs<CanonicalPayloadResult.Success>(CanonicalPayload.fromValue(value)).payload
    }

    private fun assertDomain(expected: LedgerError, block: () -> Unit) {
        val error = assertFailsWith<PostgresLedgerError.Domain>(block = block)
        assertEquals(expected, error.error)
    }
}
