package com.garive.eng.kt.observability

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

public class ObservabilityFixtureTest {
    private val root: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/observability-v1.json").readText(),
    ).jsonObject

    @Test
    public fun `fixture is exact catalogue and enum vocabulary`() {
        val fixtureEnums = root.getValue("enum_values").jsonObject.mapValues { (_, values) ->
            values.jsonArray.map { it.jsonPrimitive.content }
        }
        assertEquals(fixtureEnums, SignalCatalogue.enumValues)
        val fixtureSchemas = root.getValue("catalogue").jsonArray.associate { raw ->
            val entry = raw.jsonObject
            val name = entry.string("name")
            name to SignalSchema(
                name,
                entry.getValue("attributes").jsonObject.mapValues { it.value.jsonPrimitive.content }.toSortedMap(),
                entry.getValue("measurements").jsonObject.mapValues { unit(it.value.jsonPrimitive.content) }.toSortedMap(),
                redaction(entry.string("minimum_redaction")),
            )
        }.toSortedMap()
        assertEquals(fixtureSchemas, SignalCatalogue.schemas)
    }

    @Test
    public fun `valid signals share Rust canonical digest`() {
        for (entry in root.getValue("valid_signals").jsonArray.map(JsonElement::jsonObject)) {
            val binding = assertSuccess(signal(entry.getValue("signal").jsonObject).binding())
            entry["expected_digest"]?.jsonPrimitive?.content?.takeIf(String::isNotEmpty)?.let {
                assertEquals(it, binding.digest)
            }
        }
    }

    @Test
    public fun `fixture mutations and failure catalogue are stable`() {
        for (case in root.getValue("invalid_cases").jsonArray.map(JsonElement::jsonObject)) {
            val mutation = case.string("mutation")
            val result = signalResult(mutate(mutation))
            assertEquals(case.string("expected"), assertIs<AgentSignalResult.Failure>(result).code.wireName, mutation)
        }
        assertEquals(
            root.getValue("portable_failure_codes").jsonArray.map { it.jsonPrimitive.content },
            AgentSignalErrorCode.entries.map(AgentSignalErrorCode::wireName),
        )
    }

    @Test
    public fun `high cardinality and secret labels fail closed`() {
        for (name in listOf("credential", "endpoint", "prompt", "provider_model", "raw_error", "response", "session_id", "status", "turn_id")) {
            val value = terminal().replace(
                "attributes",
                JsonArray(listOf(attribute(name, JsonObject(mapOf("kind" to JsonPrimitive("bool"), "value" to JsonPrimitive(true)))))),
            )
            assertEquals(
                AgentSignalErrorCode.ATTRIBUTE_NOT_ALLOWED,
                assertIs<AgentSignalResult.Failure>(signalResult(value)).code,
            )
        }
    }

    private fun signal(value: JsonObject): AgentSignal = assertSuccess(signalResult(value))

    private fun signalResult(value: JsonObject): AgentSignalResult<AgentSignal> {
        val correlation = value.getValue("correlation").jsonObject
        return AgentSignal.create(
            value.string("signal_name"), value.string("schema_version").toUInt(), value.string("observed_at_utc"),
            Severity.entries.single { it.wireName == value.string("severity") },
            Correlation(
                correlation.optional("trace_id"), correlation.optional("span_id"), correlation.optional("parent_span_id"),
                correlation.optional("session_id"), correlation.optional("turn_id"), correlation.optional("execution_id"),
                correlation.optional("model_request_id"), correlation.optional("tool_invocation_id"),
                correlation["durable_position"]?.jsonPrimitive?.content?.toULong(),
            ),
            value.getValue("attributes").jsonArray.map { raw ->
                val item = raw.jsonObject
                val encoded = item.getValue("value").jsonObject
                Attribute(
                    item.string("name"),
                    when (encoded.string("kind")) {
                        "string" -> AttributeValue.StringValue(encoded.string("value"))
                        "bool" -> AttributeValue.BoolValue(encoded.getValue("value").jsonPrimitive.boolean)
                        "integer" -> AttributeValue.IntegerValue(encoded.getValue("value").jsonPrimitive.long)
                        else -> error("unknown attribute kind")
                    },
                )
            },
            value.getValue("measurements").jsonArray.map { raw ->
                val item = raw.jsonObject
                val encoded = item.getValue("value").jsonObject
                Measurement(
                    item.string("name"),
                    when (encoded.string("kind")) {
                        "known" -> MeasurementValue.Known(encoded.string("value").toULong())
                        "unknown" -> MeasurementValue.Unknown
                        else -> error("unknown measurement kind")
                    },
                    unit(item.string("unit")),
                )
            },
            redaction(value.string("redaction_class")),
        )
    }

    private fun mutate(mutation: String): JsonObject {
        var value = when (mutation) {
            "total_bytes_unknown" -> Json.parseToJsonElement(
                """{"signal_name":"agent.context.derived","schema_version":1,"observed_at_utc":"2026-08-29T00:00:00Z","severity":"info","correlation":{},"attributes":[],"measurements":[{"name":"total_bytes","value":{"kind":"unknown"},"unit":"bytes"}],"redaction_class":"operational"}""",
            ).jsonObject
            "interaction_operational" -> root.getValue("valid_signals").jsonArray[1].jsonObject.getValue("signal").jsonObject
            else -> terminal()
        }
        value = when (mutation) {
            "unknown_signal" -> value.replace("signal_name", JsonPrimitive("agent.unknown"))
            "attribute_name_raw_error" -> value.replace("attributes", JsonArray(listOf(attribute("raw_error", stringValue("failed")))))
            "attribute_name_session_id" -> value.replace("attributes", JsonArray(listOf(attribute("session_id", stringValue("session")))))
            "attribute_count_9" -> value.replace(
                "attributes",
                JsonArray((0..8).map { attribute("a$it", JsonObject(mapOf("kind" to JsonPrimitive("bool"), "value" to JsonPrimitive(true)))) }),
            )
            "input_tokens_bytes" -> value.replace(
                "measurements",
                JsonArray(value.getValue("measurements").jsonArray.mapIndexed { index, item ->
                    if (index == 1) item.jsonObject.replace("unit", JsonPrimitive("bytes")) else item
                }),
            )
            "total_bytes_unknown" -> value
            "interaction_operational" -> value.replace("redaction_class", JsonPrimitive("operational"))
            "trace_uppercase" -> value.replace(
                "correlation",
                value.getValue("correlation").jsonObject.replace("trace_id", JsonPrimitive("A".repeat(32))),
            )
            "attributes_unsorted" -> value.replace("attributes", JsonArray(value.getValue("attributes").jsonArray.reversed()))
            else -> error("unknown mutation $mutation")
        }
        return value
    }

    private fun terminal(): JsonObject = root.getValue("valid_signals").jsonArray[0].jsonObject.getValue("signal").jsonObject
    private fun attribute(name: String, value: JsonObject): JsonObject = JsonObject(mapOf("name" to JsonPrimitive(name), "value" to value))
    private fun stringValue(value: String): JsonObject = JsonObject(mapOf("kind" to JsonPrimitive("string"), "value" to JsonPrimitive(value)))
    private fun JsonObject.replace(name: String, value: JsonElement): JsonObject = JsonObject(toMutableMap().apply { put(name, value) })
    private fun JsonObject.string(name: String): String = getValue(name).jsonPrimitive.content
    private fun JsonObject.optional(name: String): String? = get(name)?.jsonPrimitive?.content
    private fun unit(value: String): MeasurementUnit = MeasurementUnit.entries.single { it.wireName == value }
    private fun redaction(value: String): RedactionClass = RedactionClass.entries.single { it.wireName == value }
    private fun <T> assertSuccess(value: AgentSignalResult<T>): T = assertIs<AgentSignalResult.Success<T>>(value).value
}
