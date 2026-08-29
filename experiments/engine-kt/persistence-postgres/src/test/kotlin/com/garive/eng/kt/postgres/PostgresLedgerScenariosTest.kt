package com.garive.eng.kt.postgres

import com.garive.eng.kt.ledger.CanonicalPayload
import com.garive.eng.kt.ledger.CanonicalPayloadResult
import com.garive.eng.kt.ledger.CommitDisposition
import com.garive.eng.kt.ledger.ExecutionId
import com.garive.eng.kt.ledger.FactDraft
import com.garive.eng.kt.ledger.FactId
import com.garive.eng.kt.ledger.FactKind
import com.garive.eng.kt.ledger.ModelRequestId
import com.garive.eng.kt.ledger.SessionId
import com.garive.eng.kt.ledger.ToolInvocationId
import com.garive.eng.kt.ledger.TurnId
import io.zonky.test.db.postgres.embedded.EmbeddedPostgres
import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class PostgresLedgerScenariosTest {
    private val root = Path.of(System.getProperty("garive.repo.root"))
    private val scenarios = Json.parseToJsonElement(
        root.resolve("spec/fixtures/ledger/ledger-scenarios.json").readText(),
    ).jsonObject.getValue("cases").jsonArray
    private val payloads = Json.parseToJsonElement(
        root.resolve("spec/fixtures/ledger/runtime-facts-v1.json").readText(),
    ).jsonObject.getValue("valid_cases").jsonArray

    @Test
    fun `PostgreSQL replays every shared ledger scenario`() {
        assertEquals(15, scenarios.size)
        EmbeddedPostgres.start().use { postgres ->
            val dataSource = postgres.postgresDatabase
            val config = PostgresConfig(dataSource.connection.use { it.metaData.url }, "postgres", "postgres")
            scenarios.forEach { element ->
                val case = element.jsonObject
                val session = SessionId.of("session")
                val ledger = PostgresLedger.open(config)
                val results = case.getValue("operations").jsonArray.map { operationElement ->
                    val operation = operationElement.jsonObject
                    when {
                        "commit" in operation -> commit(ledger, session, operation.getValue("commit").jsonObject)
                        "read" in operation -> read(ledger, session, operation.getValue("read").jsonObject)
                        "verify_corrupt" in operation -> corrupt(dataSource, ledger, session)
                        else -> error("unknown operation")
                    }
                }
                val expected = case.getValue("expected").jsonObject
                assertEquals(
                    expected.getValue("results").jsonArray.map { it.jsonPrimitive.content },
                    results,
                    case.text("name"),
                )
                if (case.text("name") != "unknown-kind-preserved-and-corruption-rejected") {
                    assertEquals(expected.ulong("version"), ledger.sessionVersion(session), case.text("name"))
                    val count = dataSource.connection.use { connection ->
                        connection.createStatement().use { statement ->
                            statement.executeQuery("SELECT COUNT(*) FROM ledger_facts").use { rows ->
                                require(rows.next())
                                rows.getInt(1)
                            }
                        }
                    }
                    assertEquals(expected.int("fact_count"), count, case.text("name"))
                    assertEquals(
                        expected.getValue("uncertain").jsonArray.map { it.jsonPrimitive.content },
                        ledger.listUncertainModelRequests(session).map { it.value },
                        case.text("name"),
                    )
                    assertEquals(
                        expected["uncertain_tools"]?.jsonArray?.map { it.jsonPrimitive.content } ?: emptyList(),
                        ledger.listUncertainToolInvocations(session).map { it.value },
                        case.text("name"),
                    )
                }
                dataSource.connection.use { connection ->
                    connection.createStatement().use { it.execute("TRUNCATE ledger_facts, ledger_sessions") }
                }
            }
        }
    }

    private fun commit(ledger: PostgresLedger, session: SessionId, value: JsonObject): String = try {
        val result = ledger.commit(
            session,
            value.ulong("expected"),
            value.getValue("facts").jsonArray.map { draft(it.jsonObject) },
        )
        val disposition = when (result.disposition) {
            CommitDisposition.COMMITTED -> "committed"
            CommitDisposition.REPLAYED -> "replayed"
        }
        "$disposition:${result.sessionVersion}:${result.positions.first()}-${result.positions.last()}"
    } catch (error: PostgresLedgerError) {
        "error:${code(error)}"
    }

    private fun read(ledger: PostgresLedger, session: SessionId, value: JsonObject): String = try {
        "read:" + ledger.readFacts(session, value.ulong("after"), value.ulong("through"))
            .joinToString(",") { it.kind.value }
    } catch (error: PostgresLedgerError) {
        "error:${code(error)}"
    }

    private fun corrupt(
        dataSource: javax.sql.DataSource,
        ledger: PostgresLedger,
        session: SessionId,
    ): String {
        dataSource.connection.use { connection ->
            connection.createStatement().use {
                it.executeUpdate(
                    "UPDATE ledger_facts SET payload_sha256=" +
                        "'0000000000000000000000000000000000000000000000000000000000000000' " +
                        "WHERE fact_id='f2'",
                )
            }
        }
        return try {
            ledger.sessionVersion(session)
            "ok"
        } catch (error: PostgresLedgerError) {
            "error:${code(error)}"
        }
    }

    private fun draft(value: JsonObject): FactDraft {
        val base = (value["payload"] ?: runtimePayload(value.text("kind"))).jsonObject
        val overrides = value["payload_overrides"]?.jsonObject ?: JsonObject(emptyMap())
        val payloadValue: JsonElement = JsonObject(base + overrides)
        val payload = assertIs<CanonicalPayloadResult.Success>(CanonicalPayload.fromValue(payloadValue)).payload
        return FactDraft(
            FactId.of(value.text("id")),
            value.optional("turn")?.let(TurnId::of),
            value.optional("execution")?.let(ExecutionId::of),
            value.optional("request")?.let(ModelRequestId::of),
            value.optional("tool")?.let(ToolInvocationId::of),
            FactKind.of(value.text("kind")),
            1u,
            payload,
            "2026-08-29T00:00:00Z",
        )
    }

    private fun runtimePayload(kind: String): JsonElement = payloads.firstOrNull {
        it.jsonObject.text("kind") == kind
    }?.jsonObject?.getValue("payload") ?: JsonObject(emptyMap())

    private fun code(error: PostgresLedgerError): String = when (error) {
        is PostgresLedgerError.Domain -> error.error.code
        is PostgresLedgerError.Corrupt -> if ("DIGEST_MISMATCH" in error.detail) "digest-mismatch" else "corruption"
        else -> error.code
    }

    private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
    private fun JsonObject.optional(key: String): String? = get(key)?.jsonPrimitive?.contentOrNull
    private fun JsonObject.ulong(key: String): ULong = text(key).toULong()
    private fun JsonObject.int(key: String): Int = text(key).toInt()
}
