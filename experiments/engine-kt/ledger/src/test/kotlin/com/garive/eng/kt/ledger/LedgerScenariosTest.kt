package com.garive.eng.kt.ledger

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

class LedgerScenariosTest {
    private val document by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(root.resolve("spec/fixtures/ledger/ledger-scenarios.json").readText()).jsonObject
    }

    @Test
    fun `Kotlin consumes every ledger scenario`() {
        val cases = document.getValue("cases").jsonArray
        assertEquals(15, cases.size, "fixture coverage changed; review both runners")
        cases.forEach { runCase(it.jsonObject) }
    }

    @Test
    fun `canonical payload is cross-language stable`() {
        val value = Json.parseToJsonElement("""{"z":[2,1],"a":"蟹","escaped":"\n"}""")
        val payload = assertIs<CanonicalPayloadResult.Success>(CanonicalPayload.fromValue(value)).payload
        assertEquals("""{"a":"蟹","escaped":"\n","z":[2,1]}""", payload.json)
        assertEquals(64, payload.sha256.length)
        assertEquals(
            CanonicalPayloadError.NON_CANONICAL,
            assertIs<CanonicalPayloadResult.Failure>(
                CanonicalPayload.fromStoredJson("{ \"z\": [2, 1], \"a\": \"蟹\", \"escaped\": \"\\n\" }", payload.sha256),
            ).error,
        )
        assertEquals(
            CanonicalPayloadError.DIGEST_MISMATCH,
            assertIs<CanonicalPayloadResult.Failure>(CanonicalPayload.fromStoredJson(payload.json, "00")).error,
        )
        assertIs<CanonicalPayloadResult.Failure>(CanonicalPayload.fromValue(Json.parseToJsonElement("1.5")))
        assertEquals(LedgerError.InvalidFact, draft(Json.parseToJsonElement("""{"id":"time","kind":"future.opaque"}""").jsonObject).copy(recordedAt = "today").validate())
    }

    private fun runCase(case: JsonObject) {
        val sessionId = SessionId.of("session")
        val ledger = LedgerState()
        val results = case.getValue("operations").jsonArray.map { operationElement ->
            val operation = operationElement.jsonObject
            when {
                "commit" in operation -> {
                    val commit = operation.getValue("commit").jsonObject
                    renderCommit(
                        ledger.commit(
                            sessionId,
                            commit.number("expected").toULong(),
                            commit.getValue("facts").jsonArray.map { draft(it.jsonObject) },
                        ),
                    )
                }
                "read" in operation -> {
                    val read = operation.getValue("read").jsonObject
                    renderRead(
                        ledger.readFacts(
                            sessionId,
                            read.number("after").toULong(),
                            read.number("through").toULong(),
                        ),
                    )
                }
                "verify_corrupt" in operation -> {
                    val position = operation.getValue("verify_corrupt").jsonObject.number("position").toULong()
                    val fact = requireNotNull(ledger.factAt(sessionId, position))
                    val corrupt = fact.copy(payload = fact.payload.withDigestForCorruptionTest("00"))
                    corrupt.verify()?.let { "error:${it.code}" } ?: "ok"
                }
                else -> error("unknown operation in ${case.text("name")}")
            }
        }
        val expected = case.getValue("expected").jsonObject
        assertEquals(expected.getValue("results").jsonArray.map { it.jsonPrimitive.content }, results, case.text("name"))
        assertEquals(expected.number("version").toULong(), ledger.sessionVersion(sessionId), case.text("name"))
        assertEquals(expected.number("fact_count"), ledger.factCount(sessionId), case.text("name"))
        val uncertain = assertIs<LedgerResult.Success<List<ModelRequestId>>>(
            ledger.listUncertainModelRequests(sessionId),
        ).value.map { it.value }
        assertEquals(expected.getValue("uncertain").jsonArray.map { it.jsonPrimitive.content }, uncertain, case.text("name"))
        val uncertainTools = assertIs<LedgerResult.Success<List<ToolInvocationId>>>(
            ledger.listUncertainToolInvocations(sessionId),
        ).value.map { it.value }
        assertEquals(
            expected["uncertain_tools"]?.jsonArray?.map { it.jsonPrimitive.content } ?: emptyList(),
            uncertainTools,
            case.text("name"),
        )
    }

    private fun draft(value: JsonObject): FactDraft {
        val basePayload = (value["payload"] ?: runtimePayload(value.text("kind"))).jsonObject
        val overrides = value["payload_overrides"]?.jsonObject ?: JsonObject(emptyMap())
        val payloadValue: JsonElement = JsonObject(basePayload + overrides)
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

    private fun renderCommit(result: LedgerResult<CommitResult>) = when (result) {
        is LedgerResult.Failure -> "error:${result.error.code}"
        is LedgerResult.Success -> {
            val value = result.value
            val disposition = value.disposition.name.lowercase()
            "$disposition:${value.sessionVersion}:${value.positions.first()}-${value.positions.last()}"
        }
    }

    private fun renderRead(result: LedgerResult<List<DurableFact>>) = when (result) {
        is LedgerResult.Failure -> "error:${result.error.code}"
        is LedgerResult.Success -> "read:${result.value.joinToString(",") { it.kind.value }}"
    }

    private fun JsonObject.text(key: String) = getValue(key).jsonPrimitive.content
    private fun JsonObject.optional(key: String) = get(key)?.jsonPrimitive?.contentOrNull
    private fun JsonObject.number(key: String) = text(key).toInt()
}
