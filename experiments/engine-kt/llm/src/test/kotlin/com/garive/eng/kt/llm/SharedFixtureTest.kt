package com.garive.eng.kt.llm

import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.time.Duration.Companion.seconds
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class SharedFixtureTest {
    private val document: JsonObject by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(root.resolve("spec/fixtures/agent/model-outcome.json").readText()).jsonObject
    }

    @Test
    fun `Kotlin consumes every model outcome case`() {
        val cases = document.getValue("cases").jsonArray
        assertEquals(7, cases.size, "fixture coverage changed; review both runners")
        cases.forEach { runCase(it.jsonObject) }
    }

    private fun runCase(case: JsonObject) {
        val name = case.text("name")
        val usage = usage(case.getValue("usage").jsonObject)
        val input = case.getValue("outcome").jsonObject
        val items = items(input.getValue("item_kinds").jsonArray.map { it.jsonPrimitive.content })
        val outcome = when (input.text("envelope")) {
            "completed" -> InvokeOutcome.Completed(items, usage, ModelStopReason.EndTurn)
            "rejected" -> InvokeOutcome.Rejected(
                enumValueOf(input.text("reason").replace('-', '_').uppercase()),
                "fixture",
            )
            "interrupted" -> InvokeOutcome.Interrupted(
                enumValueOf(input.text("reason").replace('-', '_').uppercase()),
                items,
                usage,
            )
            "unavailable" -> InvokeOutcome.Unavailable(
                enumValueOf(input.text("reason").replace('-', '_').uppercase()),
                1.seconds,
            )
            else -> error("$name: unknown outcome envelope")
        }
        val expected = case.getValue("expected").jsonObject
        assertEquals(expected.text("total"), renderTotal(usage.totalTokens()), name)
        assertEquals(expected.text("kind"), outcome.kind.name.lowercase(), name)
        assertEquals(expected.text("success").toBoolean(), outcome.isSuccess, name)
        assertEquals(expected.text("partial").toBoolean(), outcome.isPartial, name)
        val actualItems = when (outcome) {
            is InvokeOutcome.Completed -> outcome.items
            is InvokeOutcome.Interrupted -> outcome.partialItems
            else -> emptyList()
        }
        assertEquals(items, actualItems, name)
    }

    private fun usage(value: JsonObject) = ModelUsage(
        inputTokens = count(value.text("input")),
        outputTokens = count(value.text("output")),
        cacheReadTokens = value["cache_read"]?.jsonPrimitive?.content?.let(::count),
        cacheWriteTokens = value["cache_write"]?.jsonPrimitive?.content?.let(::count),
        source = enumValueOf(value.text("source").replace('-', '_').uppercase()),
    )

    private fun count(value: String): TokenCount =
        if (value == "unknown") TokenCount.Unknown else TokenCount.Known(value.toULong())

    private fun items(kinds: List<String>): List<ModelItem> = kinds.map { kind ->
        when (kind) {
            "text" -> ModelItem.Text("text")
            "reasoning" -> ModelItem.Reasoning(ReasoningContent.OpaqueReference("reasoning"))
            "tool-intent" -> ModelItem.ToolIntent("call", "tool", "{}")
            "tool-observation" -> ModelItem.ToolObservation("call", "{}")
            "media-reference" -> ModelItem.MediaReference(MediaKind.Image, "media")
            else -> error("unknown item kind $kind")
        }
    }

    private fun renderTotal(value: UsageTotal): String = when (value) {
        is UsageTotal.Known -> "known:${value.value}"
        UsageTotal.Unknown -> "unknown"
        UsageTotal.Overflow -> "overflow"
    }

    private fun JsonObject.text(key: String) = getValue(key).jsonPrimitive.content
}
