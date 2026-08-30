package com.garive.eng.kt.ledger

import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class RuntimeFactPayloadsTest {
    private val document by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(root.resolve("spec/fixtures/ledger/runtime-facts-v1.json").readText()).jsonObject
    }

    @Test
    fun `every C6 payload fixture is applied at its declared version`() {
        val cases = document.getValue("valid_cases").jsonArray
        assertEquals(78, cases.size)
        cases.forEach { case ->
            val value = case.jsonObject
            val schema = value["schema_version"]?.jsonPrimitive?.content?.toUInt() ?: 1u
            val expected = if (value.optional("expected_disposition") == "applied_v2") {
                RuntimeFactDisposition.APPLIED_V2
            } else {
                RuntimeFactDisposition.APPLIED_V1
            }
            assertEquals(
                LedgerResult.Success(expected),
                validateRuntimeFact(fact(value, schema)),
                value.text("kind"),
            )
        }
    }

    @Test
    fun `exact fields types and envelopes fail closed for every kind`() {
        document.getValue("valid_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val schema = case["schema_version"]?.jsonPrimitive?.content?.toUInt() ?: 1u
            val original = fact(case, schema)
            val payload = case.getValue("payload").jsonObject
            val first = payload.keys.first()
            assertInvalid(original.withPayload(JsonObject(payload - first)), "missing ${case.text("kind")}")
            assertInvalid(
                original.withPayload(JsonObject(payload + ("extra" to JsonPrimitive(true)))),
                "extra ${case.text("kind")}",
            )
            assertInvalid(
                original.withPayload(JsonObject(payload + (first to JsonNull))),
                "type ${case.text("kind")}",
            )
            val missingIdentity = when {
                original.toolInvocationId != null -> original.copy(toolInvocationId = null)
                original.modelRequestId != null -> original.copy(modelRequestId = null)
                original.executionId != null -> original.copy(executionId = null)
                original.turnId != null -> original.copy(turnId = null)
                else -> original.copy(turnId = TurnId.of("forbidden"))
            }
            assertInvalid(missingIdentity, "identity ${case.text("kind")}")
        }
    }

    @Test
    fun `malformed digests and inline mismatches are rejected`() {
        var count = 0
        document.getValue("valid_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val (payload, changed) = corruptFirstDigest(case.getValue("payload"))
            if (changed) {
                count += 1
                val schema = case["schema_version"]?.jsonPrimitive?.content?.toUInt() ?: 1u
                assertInvalid(fact(case, schema).withPayload(payload), case.text("kind"))
            }
        }
        assertEquals(61, count)
    }

    @Test
    fun `memory revision classification binds authority receipt shape`() {
        val case = document.getValue("valid_cases").jsonArray.map(JsonElement::jsonObject)
            .first { it.text("kind") == "memory.revision_classified" }
        val payload = case.getValue("payload").jsonObject
        val receipt = "a".repeat(64)
        assertInvalid(fact(case).withPayload(JsonObject(payload +
            ("authority_receipt_digest" to JsonPrimitive(receipt)))), "agent receipt")
        val user = JsonObject(payload + ("authority" to JsonPrimitive("user_declared")))
        assertInvalid(fact(case).withPayload(user), "missing user receipt")
        assertEquals(LedgerResult.Success(RuntimeFactDisposition.APPLIED_V1),
            validateRuntimeFact(fact(case).withPayload(JsonObject(user +
                ("authority_receipt_digest" to JsonPrimitive(receipt))))))
    }

    @Test
    fun `unknown kinds and newer schemas remain opaque`() {
        val newer = document.getValue("unknown_schema").jsonObject
        assertEquals(
            LedgerResult.Success(RuntimeFactDisposition.OPAQUE),
            validateRuntimeFact(fact(newer, newer.number("schema_version").toUInt())),
        )
        assertEquals(
            LedgerResult.Success(RuntimeFactDisposition.OPAQUE),
            validateRuntimeFact(fact(document.getValue("valid_cases").jsonArray.first().jsonObject).copy(kind = FactKind.of("future.runtime_fact"))),
        )
    }

    private fun fact(value: JsonObject, schemaVersion: UInt = 1u): FactDraft {
        val payload = assertIs<CanonicalPayloadResult.Success>(
            CanonicalPayload.fromValue(value.getValue("payload")),
        ).payload
        return FactDraft(
            FactId.of("fact"),
            value.optional("turn")?.let(TurnId::of),
            value.optional("execution")?.let(ExecutionId::of),
            value.optional("request")?.let(ModelRequestId::of),
            value.optional("tool")?.let(ToolInvocationId::of),
            FactKind.of(value.text("kind")),
            schemaVersion,
            payload,
            "2026-08-29T00:00:00Z",
        )
    }

    private fun FactDraft.withPayload(value: JsonElement): FactDraft = copy(
        payload = assertIs<CanonicalPayloadResult.Success>(CanonicalPayload.fromValue(value)).payload,
    )

    private fun assertInvalid(fact: FactDraft, message: String) {
        assertEquals(LedgerResult.Failure(LedgerError.InvalidFact), validateRuntimeFact(fact), message)
    }

    private fun corruptFirstDigest(value: JsonElement): Pair<JsonElement, Boolean> = when (value) {
        is JsonObject -> {
            val output = value.toMutableMap()
            val direct = value.keys.firstOrNull { it == "digest" || it.endsWith("_digest") }
            if (direct != null) {
                output[direct] = JsonPrimitive("ABC")
                JsonObject(output) to true
            } else {
                var changed = false
                for ((key, child) in value) {
                    if (!changed) {
                        val (replacement, replaced) = corruptFirstDigest(child)
                        output[key] = replacement
                        changed = replaced
                    }
                }
                JsonObject(output) to changed
            }
        }
        is JsonArray -> {
            var changed = false
            JsonArray(value.map { child ->
                if (changed) child else corruptFirstDigest(child).let { (replacement, replaced) ->
                    changed = replaced
                    replacement
                }
            }) to changed
        }
        else -> value to false
    }
}

private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
private fun JsonObject.optional(key: String): String? = get(key)?.jsonPrimitive?.content
private fun JsonObject.number(key: String): Int = getValue(key).jsonPrimitive.content.toInt()
