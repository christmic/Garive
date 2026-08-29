package com.garive.eng.kt.core

import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class MemoryContextTest {
    private val document: JsonObject by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(
            root.resolve("spec/fixtures/agent/memory-context-derive-v1.json").readText(),
        ).jsonObject
    }

    @Test
    fun `Kotlin consumes every Memory context case`() {
        val cases = document.getValue("cases").jsonArray
        assertEquals(5, cases.size)
        cases.forEach { element ->
            val case = element.jsonObject
            val request = ContextRequest(
                "session", "turn",
                if (case.text("purpose") == "inference") ContextPurpose.INFERENCE else ContextPurpose.GOVERNANCE,
                null, 5uL, 8, case.text("max_bytes").toInt(),
            )
            val batches = case.getValue("batches").jsonArray.map { batch(it.jsonPrimitive.content) }
            when (val result = deriveContextWithMemory(request, emptyList(), batches)) {
                is MemoryContextResult.Failure -> assertEquals(case.text("status"), result.error.code, case.text("name"))
                is MemoryContextResult.Success -> {
                    assertEquals("ok", case.text("status"), case.text("name"))
                    assertEquals(case.positions("retained"), result.surface.retainedRefs.map { it.position })
                    assertEquals(case.positions("dropped"), result.surface.droppedRefs.map { it.position })
                    assertEquals(case.positions("filtered"), result.surface.filteredRefs.map { it.position })
                    assertTrue(result.surface.items.all { it is ContextItem.Input && it.kind == CandidateKind.MEMORY })
                }
            }
        }
    }

    private fun batch(name: String): MemoryRecallContextBatch {
        val shape = when (name) {
            "menu" -> Shape(3uL, MemoryRecallProduct.MENU, MemoryContextState.ACTIVE, null)
            "archived-menu" -> Shape(3uL, MemoryRecallProduct.MENU, MemoryContextState.ARCHIVED, null)
            "detail" -> Shape(4uL, MemoryRecallProduct.DETAIL, MemoryContextState.ACTIVE, "Use metric units.")
            "detail-2" -> Shape(5uL, MemoryRecallProduct.DETAIL, MemoryContextState.COLD, "Use metric units.")
            else -> error("unknown batch")
        }
        return MemoryRecallContextBatch(
            FactRef("session", shape.position), "fact-${shape.position}", "a".repeat(64),
            "selection-${shape.position}", "b".repeat(64), "user", shape.product,
            "baseline-v1", 2uL, false,
            listOf(
                MemoryContextItem(
                    "record-${shape.position}", "revision-1", "semantic", "preference",
                    "user_declared", shape.state, "unit preference",
                    "dd407b2b50d5735761059db743e2d628f0f6b17585ec025089e82380986dcff9",
                    17uL, shape.content,
                ),
            ),
        )
    }

    private data class Shape(
        val position: ULong,
        val product: MemoryRecallProduct,
        val state: MemoryContextState,
        val content: String?,
    )
}

private fun JsonObject.text(name: String): String = getValue(name).jsonPrimitive.content
private fun JsonObject.positions(name: String): List<ULong> =
    getValue(name).jsonArray.map { it.jsonPrimitive.content.toULong() }
