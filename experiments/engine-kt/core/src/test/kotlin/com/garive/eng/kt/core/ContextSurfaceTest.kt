package com.garive.eng.kt.core

import com.garive.eng.kt.llm.ModelInputContent
import com.garive.eng.kt.llm.ModelInputItem
import com.garive.eng.kt.llm.ModelRole
import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class ContextSurfaceTest {
    private val document: JsonObject by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(root.resolve("spec/fixtures/agent/context-surface.json").readText()).jsonObject
    }

    @Test
    fun `Kotlin consumes every context case`() {
        val cases = document.getValue("cases").jsonArray
        assertEquals(12, cases.size)
        cases.forEach { element ->
            val case = element.jsonObject
            when (val result = deriveContext(request(case.obj("request")), candidates(case.getValue("candidates").jsonArray.map { it.jsonObject }))) {
                is ContextDerivationResult.Failure -> assertEquals(case.obj("expected").text("status"), result.error.code, case.text("name"))
                is ContextDerivationResult.Success -> assertSurface(case, result.surface)
            }
        }
    }

    private fun assertSurface(case: JsonObject, surface: ContextSurface) {
        val expected = case.obj("expected")
        val request = request(case.obj("request"))
        assertEquals("ok", expected.text("status"), case.text("name"))
        assertEquals(request.purpose, surface.purpose, case.text("name"))
        assertEquals((request.afterPosition ?: 0uL) + 1uL, surface.fromPosition, case.text("name"))
        assertEquals(request.throughPosition, surface.throughPosition, case.text("name"))
        assertEquals(expected.positions("retained"), surface.retainedRefs.map { it.position }, case.text("name"))
        assertEquals(expected.positions("dropped"), surface.droppedRefs.map { it.position }, case.text("name"))
        assertEquals(expected.positions("filtered"), surface.filteredRefs.map { it.position }, case.text("name"))
        assertEquals(expected.getValue("items").jsonArray.map { it.jsonPrimitive.content }, surface.items.map(::render), case.text("name"))
        assertEquals(expected.number("item_count").toInt(), surface.itemCount, case.text("name"))
        assertEquals(expected.number("bytes").toInt(), surface.utf8Bytes, case.text("name"))
    }

    private fun request(value: JsonObject) = ContextRequest(
        sessionId = "session-1",
        turnId = "turn-1",
        purpose = purpose(value.text("purpose")),
        afterPosition = value["after"]?.jsonPrimitive?.content?.takeUnless { it == "null" }?.toULong(),
        throughPosition = value.number("through"),
        maxItems = value.number("max_items").toInt(),
        maxUtf8Bytes = value.number("max_bytes").toInt(),
    )

    private fun candidates(values: List<JsonObject>) = values.map { value ->
        ContextCandidate(
            factRef = FactRef(value["session"]?.jsonPrimitive?.content ?: "session-1", value.number("position")),
            kind = candidateKind(value["kind"]?.jsonPrimitive?.content ?: "user-input"),
            retention = enumValueOf(value.text("retention").uppercase()),
            visibility = visibility(value.text("visibility")),
            items = value.getValue("items").jsonArray.map {
                ModelInputItem.Message(ModelRole.USER, listOf(ModelInputContent.Text(it.jsonPrimitive.content)))
            },
        )
    }

    private fun visibility(value: String): Visibility = when {
        value == "visible" -> Visibility.Visible
        value == "redacted" -> Visibility.Redacted
        value.startsWith("purpose:") -> Visibility.Purposes(setOf(purpose(value.substringAfter(':'))))
        else -> error("unknown visibility $value")
    }

    private fun purpose(value: String): ContextPurpose = when (value) {
        "inference" -> ContextPurpose.INFERENCE
        "governance" -> ContextPurpose.GOVERNANCE
        "tool-preparation" -> ContextPurpose.TOOL_PREPARATION
        "summarization" -> ContextPurpose.SUMMARIZATION
        else -> error("unknown purpose $value")
    }

    private fun candidateKind(value: String): CandidateKind = when (value) {
        "instruction" -> CandidateKind.INSTRUCTION
        "user-input" -> CandidateKind.USER_INPUT
        "model-output" -> CandidateKind.MODEL_OUTPUT
        "tool-observation" -> CandidateKind.TOOL_OBSERVATION
        "approval" -> CandidateKind.APPROVAL
        "summary" -> CandidateKind.SUMMARY
        "system-notice" -> CandidateKind.SYSTEM_NOTICE
        else -> error("unknown candidate kind $value")
    }

    private fun render(value: ContextItem): String = when (value) {
        is ContextItem.RedactedItem -> "redacted"
        is ContextItem.Input -> when (val content = (value.item as ModelInputItem.Message).content.first()) {
            is ModelInputContent.Text -> "text:${content.text}"
            else -> error("unexpected media fixture")
        }
    }

    private fun JsonObject.text(key: String) = getValue(key).jsonPrimitive.content
    private fun JsonObject.number(key: String) = text(key).toULong()
    private fun JsonObject.obj(key: String) = getValue(key).jsonObject
    private fun JsonObject.positions(key: String) = getValue(key).jsonArray.map { it.jsonPrimitive.content.toULong() }
}
