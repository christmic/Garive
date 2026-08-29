package com.garive.eng.kt.host

import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class RuntimeRecoveryTest {
    @Test
    fun `Kotlin consumes every frozen restart case`() {
        val root = Path.of(System.getProperty("garive.repo.root"))
        val cases = Json.parseToJsonElement(
            root.resolve("spec/fixtures/agent/durable-runtime-turn.json").readText(),
        ).jsonObject.getValue("recovery_cases").jsonArray
        assertEquals(10, cases.size)
        cases.forEach { element ->
            val case = element.jsonObject
            val snapshot = RuntimeRecoverySnapshot(
                ExecutionRecoveryPosition.valueOf(case.text("execution").uppercase()),
                ModelRecoveryPosition.valueOf(case.text("model").uppercase()),
                EffectRecoveryPosition.valueOf(case.text("effect").uppercase()),
                case.optionalNumber("recovery_ordinal") ?: 0uL,
                case.optionalNumber("max_recoveries") ?: 3uL,
            )
            assertEquals(
                RuntimeRecoveryAction.valueOf(case.text("expected").uppercase()),
                selectRuntimeRecovery(snapshot),
                case.text("name"),
            )
        }
    }
}

private fun kotlinx.serialization.json.JsonObject.text(key: String): String =
    getValue(key).jsonPrimitive.content

private fun kotlinx.serialization.json.JsonObject.optionalNumber(key: String): ULong? =
    get(key)?.jsonPrimitive?.content?.toULong()
