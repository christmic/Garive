package com.garive.eng.kt.core

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

class SharedFixtureTest {
    private val document: JsonObject by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(
            root.resolve("spec/fixtures/agent/execution-control.json").readText(),
        ).jsonObject
    }

    @Test
    fun `Kotlin consumes every execution control case`() {
        val cases = document.getValue("cases").jsonArray
        assertEquals(5, cases.size, "fixture coverage changed; review both runners")
        cases.forEach { element -> runCase(element.jsonObject) }
    }

    private fun runCase(case: JsonObject) {
        val name = case.getValue("name").jsonPrimitive.content
        val input = case.getValue("input").jsonObject
        val expected = case.getValue("expected").jsonObject
        val create = {
            ExecutionControl.create(
                TurnId.of(input.text("turn_id")),
                ExecutionId.of(input.text("execution_id")),
                input.text("completed").toUInt(),
                ExecutionLimits(input.text("maximum").toUInt()),
            )
        }
        if ("construction_error" in expected) {
            assertFailsWith<ControlException.CursorBeyondLimit>(name) { create() }
            return
        }
        val control = create()
        val actual = case.getValue("operations").jsonArray.map { operation ->
            renderOperation(control, operation.jsonPrimitive.content)
        }
        assertEquals(expected.getValue("results").jsonArray.map { it.jsonPrimitive.content }, actual, name)
        assertEquals(expected.text("completed").toUInt(), control.completedIterations, name)
        assertEquals(expected.text("status"), renderStatus(control.status), name)
    }

    private fun renderOperation(control: ExecutionControl, operation: String): String =
        try {
            when {
                operation == "begin" -> when (val result = control.beginIteration()) {
                    is BeginIteration.Started -> "started:${result.iteration}"
                    BeginIteration.IterationLimitReached -> "iteration-limit"
                }
                operation.startsWith("close:") -> {
                    val kind = enumValueOf<ExecutionOutcomeKind>(operation.substringAfter(':').uppercase())
                    control.close(kind)
                    "closed:${operation.substringAfter(':')}"
                }
                else -> error("unknown fixture operation $operation")
            }
        } catch (_: ControlException.AlreadyClosed) {
            "error:already-closed"
        }

    private fun renderStatus(status: ExecutionStatus): String = when (status) {
        ExecutionStatus.Active -> "active"
        is ExecutionStatus.Closed -> "closed:${status.kind.name.lowercase()}"
    }

    private fun JsonObject.text(key: String) = getValue(key).jsonPrimitive.content
}
