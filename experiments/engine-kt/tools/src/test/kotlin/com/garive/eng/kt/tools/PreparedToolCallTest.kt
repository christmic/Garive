package com.garive.eng.kt.tools

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertNotEquals
import kotlin.test.fail

class PreparedToolCallTest {
    private val fixture: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/prepared-tool-call.json").readText(),
    ).jsonObject

    private fun capability(value: String): ExecutionCapability = when (value) {
        "filesystem_read" -> ExecutionCapability.FILESYSTEM_READ
        "filesystem_write" -> ExecutionCapability.FILESYSTEM_WRITE
        "process" -> ExecutionCapability.PROCESS
        "network" -> ExecutionCapability.NETWORK
        else -> fail("unknown fixture capability: $value")
    }

    private fun replayClass(value: String): ReplayClass = when (value) {
        "read_only" -> ReplayClass.READ_ONLY
        "idempotent" -> ReplayClass.IDEMPOTENT
        "receipt_recoverable" -> ReplayClass.RECEIPT_RECOVERABLE
        "never_replay" -> ReplayClass.NEVER_REPLAY
        else -> fail("unknown fixture replay class: $value")
    }

    private fun <T> ToolContractResult<T>.value(): T = when (this) {
        is ToolContractResult.Success -> value
        is ToolContractResult.Failure -> fail("unexpected tool error: $error")
    }

    private fun definition(value: JsonElement): ToolDefinition {
        val objectValue = value.jsonObject
        val requirements = objectValue.getValue("requirements").jsonObject
        return ToolDefinition.create(
            name = objectValue.getValue("name").jsonPrimitive.content,
            revision = objectValue.getValue("revision").jsonPrimitive.content,
            description = objectValue.getValue("description").jsonPrimitive.content,
            inputSchema = objectValue.getValue("input_schema"),
            requirements = ExecutionRequirements.create(
                capabilities = requirements.getValue("capabilities").jsonArray.map {
                    capability(it.jsonPrimitive.content)
                },
                maxDurationMs = requirements.getValue("max_duration_ms").jsonPrimitive.content.toLong(),
                maxOutputBytes = requirements.getValue("max_output_bytes").jsonPrimitive.content.toLong(),
            ).value(),
            replayClass = replayClass(objectValue.getValue("replay_class").jsonPrimitive.content),
        ).value()
    }

    private fun code(value: String): PreparationErrorCode = PreparationErrorCode.entries.single {
        it.wireName == value
    }

    @Test
    fun `shared preparation cases match`() {
        val catalog = ToolCatalog.create(fixture.getValue("definitions").jsonArray.map(::definition)).value()
        fixture.getValue("cases").jsonArray.forEach { caseValue ->
            val case = caseValue.jsonObject
            val input = case.getValue("input").jsonObject
            val expected = case.getValue("expected").jsonObject
            val result = catalog.prepare(
                ToolIntent(
                    modelCallId = input.getValue("model_call_id").jsonPrimitive.content,
                    toolName = input.getValue("tool_name").jsonPrimitive.content,
                    argumentsJson = input.getValue("arguments_json").jsonPrimitive.content,
                ),
            )
            if (expected.getValue("status").jsonPrimitive.content == "prepared") {
                val prepared = result.value()
                assertEquals(expected.getValue("normalized_arguments").jsonPrimitive.content, prepared.normalizedArguments, case["name"].toString())
                assertEquals(expected.getValue("input_digest").jsonPrimitive.content, prepared.inputDigest, case["name"].toString())
            } else {
                val failure = result as? ToolContractResult.Failure ?: fail("expected failure: ${case["name"]}")
                assertEquals(code(expected.getValue("code").jsonPrimitive.content), failure.error.code)
                expected["instance_path"]?.let {
                    val schemaFailure = failure.error.failures.first()
                    assertEquals(it.jsonPrimitive.content, schemaFailure.instancePath)
                    assertEquals(expected.getValue("schema_path").jsonPrimitive.content, schemaFailure.schemaPath)
                    assertEquals(expected.getValue("keyword").jsonPrimitive.content, schemaFailure.keyword)
                }
            }
        }
    }

    @Test
    fun `invalid definitions fail before catalog use`() {
        fixture.getValue("invalid_definitions").jsonArray.forEach { caseValue ->
            val case = caseValue.jsonObject
            val result = ToolDefinition.create(
                name = case.getValue("name").jsonPrimitive.content,
                revision = "1",
                description = "invalid fixture definition",
                inputSchema = case.getValue("schema"),
                requirements = ExecutionRequirements.create(listOf(ExecutionCapability.FILESYSTEM_READ), 1, 1).value(),
                replayClass = ReplayClass.READ_ONLY,
            )
            val failure = result as? ToolContractResult.Failure ?: fail("expected invalid definition")
            assertEquals(code(case.getValue("expected_code").jsonPrimitive.content), failure.error.code)
        }
    }

    @Test
    fun `duplicate names and invalid requirements fail`() {
        val first = definition(fixture.getValue("definitions").jsonArray.first())
        val duplicate = ToolCatalog.create(listOf(first, first)) as ToolContractResult.Failure
        assertEquals(PreparationErrorCode.INVALID_TOOL_DEFINITION, duplicate.error.code)
        val invalid = ExecutionRequirements.create(emptyList(), 0, 1) as ToolContractResult.Failure
        assertEquals(PreparationErrorCode.INVALID_TOOL_DEFINITION, invalid.error.code)
    }

    @Test
    fun `executable meaning changes digest`() {
        val base = fixture.getValue("definitions").jsonArray.first().jsonObject
        val intent = ToolIntent("call", "read_file", "{\"path\":\"a\"}")
        fun digest(value: JsonObject): String = ToolCatalog.create(listOf(definition(value)))
            .value()
            .prepare(intent)
            .value()
            .inputDigest
        fun changed(name: String, value: JsonElement): JsonObject = JsonObject(base.toMutableMap().apply { put(name, value) })

        val original = digest(base)
        assertNotEquals(original, digest(changed("revision", JsonPrimitive("2"))))

        val requirements = JsonObject(base.getValue("requirements").jsonObject.toMutableMap().apply {
            put("max_output_bytes", JsonPrimitive(8192))
        })
        assertNotEquals(original, digest(changed("requirements", requirements)))
        assertNotEquals(original, digest(changed("replay_class", JsonPrimitive("never_replay"))))

        val arguments = ToolCatalog.create(fixture.getValue("definitions").jsonArray.map(::definition))
            .value()
            .prepare(ToolIntent("call", "read_file", "{\"path\":\"b\"}"))
            .value()
        assertNotEquals(original, arguments.inputDigest)
    }
}
