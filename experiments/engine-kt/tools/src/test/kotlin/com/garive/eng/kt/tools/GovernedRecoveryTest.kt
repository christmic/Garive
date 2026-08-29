package com.garive.eng.kt.tools

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.io.path.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals

class GovernedRecoveryTest {
    @Test
    fun sharedRecoveryMatrixNeverInfersSafeReplay() {
        val root = Path(System.getProperty("garive.repo.root"))
        val fixture = Json.parseToJsonElement(
            root.resolve("spec/fixtures/agent/governed-effects.json").readText(),
        ).jsonObject
        fixture.getValue("recovery_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val position = RecoveryPosition.entries.single {
                it.name.lowercase() == case.getValue("position").jsonPrimitive.content
            }
            val replay = ReplayClass.entries.single {
                it.wireName == case.getValue("replay_class").jsonPrimitive.content
            }
            val expected = RecoveryDecision.entries.single {
                it.name.lowercase() == case.getValue("expected").jsonPrimitive.content
            }
            assertEquals(
                expected,
                recoverEffect(
                    position,
                    replay,
                    case.getValue("executor_proves_replay").jsonPrimitive.content.toBoolean(),
                ),
                case.getValue("name").jsonPrimitive.content,
            )
        }
    }
}
