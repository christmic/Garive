package com.garive.eng.kt.host

import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class CoreOutcomeMappingTest {
    @Test
    fun `Kotlin consumes every frozen Core outcome mapping`() {
        val root = Path.of(System.getProperty("garive.repo.root"))
        val cases = Json.parseToJsonElement(
            root.resolve("spec/fixtures/agent/durable-runtime-turn.json").readText(),
        ).jsonObject.getValue("core_outcome_cases").jsonArray
        assertEquals(4, cases.size)
        cases.forEach { element ->
            val case = element.jsonObject
            val actual = mapCoreOutcome(
                RuntimeCoreOutcomeKind.valueOf(case.getValue("outcome").jsonPrimitive.content.uppercase()),
            )
            assertEquals(
                case.getValue("facts").jsonArray.map { it.jsonPrimitive.content },
                actual.facts,
            )
            assertEquals(case.getValue("turn_state").jsonPrimitive.content, actual.turnState)
        }
    }
}
