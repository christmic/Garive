package com.garive.eng.kt.plan

import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

public class PlanFixtureTest {
    private val fixture by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(root.resolve("spec/fixtures/agent/plan-lifecycle-v1.json").readText()).jsonObject
    }

    @Test
    public fun `canonical definition and lifecycle match Rust`() {
        val definition = definition()
        assertEquals(
            fixture.getValue("definition").jsonObject.getValue("canonical_json").jsonPrimitive.content,
            definition.canonicalJson().value(),
        )
        assertEquals(
            fixture.getValue("definition").jsonObject.getValue("digest").jsonPrimitive.content,
            definition.digest().value(),
        )
        var snapshot = PlanSnapshot.create(definition)
        fixture.getValue("valid_lifecycle").jsonArray.forEach { element ->
            val step = element.jsonObject
            val transition = when (step.getValue("transition").jsonPrimitive.content) {
                "adopt" -> PlanTransition.Adopt
                "claim" -> PlanTransition.Claim(id(step.getValue("step_id").jsonPrimitive.content))
                "start" -> PlanTransition.Start(id(step.getValue("step_id").jsonPrimitive.content))
                "complete_step" -> PlanTransition.CompleteStep(id(step.getValue("step_id").jsonPrimitive.content))
                "complete" -> PlanTransition.Complete(
                    step.getValue("criteria_complete").jsonPrimitive.content.toBooleanStrict(),
                )
                else -> error("unknown fixture transition")
            }
            snapshot = snapshot.apply(transition).value()
            assertEquals(step.getValue("plan_state").jsonPrimitive.content, snapshot.state.wireName)
            assertEquals(
                step.getValue("ready").jsonArray.map { it.jsonPrimitive.content },
                snapshot.readySteps().map(PlanStepId::value),
            )
            assertEquals(step.getValue("total_attempts").jsonPrimitive.content.toInt(), snapshot.totalAttempts)
        }
    }

    @Test
    public fun `cycles unknown dependencies and missing scope fail closed`() {
        val cycle = listOf(
            step("a", listOf("b"), listOf("accepted")),
            step("b", listOf("a"), listOf("artifact")),
        )
        assertEquals(PlanErrorCode.PLAN_CYCLE, assertIs<PlanResult.Failure>(create(cycle)).error.code)
        assertEquals(
            PlanErrorCode.PLAN_INVALID,
            assertIs<PlanResult.Failure>(create(listOf(step("a", listOf("missing"), criteria())))).error.code,
        )
        val unavailable = PlanStepV1.create(
            id("a"),
            "Unavailable",
            emptyList(),
            criteria(),
            listOf(PlanCapabilityReference.create("browser", "native-v1").value()),
            listOf(digest('d')),
            1,
        ).value()
        assertEquals(
            PlanErrorCode.PLAN_INVALID,
            assertIs<PlanResult.Failure>(create(listOf(unavailable))).error.code,
        )
    }

    private fun definition(): PlanDefinitionV1 = create(
        listOf(
            step("prepare", emptyList(), listOf("accepted")),
            step("deliver", listOf("prepare"), listOf("artifact")),
        ),
    ).value()

    private fun create(steps: List<PlanStepV1>): PlanResult<PlanDefinitionV1> = PlanDefinitionV1.create(
        PlanId.create("plan-1").value(),
        1,
        "goal-1",
        2,
        digest('a'),
        digest('b'),
        digest('c'),
        "safety-v1",
        steps,
        PlanBoundsV1.create(4, 2, 6, 10_000, 60_000).value(),
        criteria().toSet(),
        emptySet(),
        setOf(capability()),
    )

    private fun step(id: String, dependencies: List<String>, criteria: List<String>): PlanStepV1 =
        PlanStepV1.create(
            id(id),
            "Complete $id",
            dependencies.map(::id),
            criteria,
            listOf(capability()),
            listOf(digest('d')),
            2,
        ).value()

    private fun id(value: String): PlanStepId = PlanStepId.create(value).value()
    private fun capability(): PlanCapabilityReference =
        PlanCapabilityReference.create("tools", "catalogue-v1").value()
    private fun criteria(): List<String> = listOf("accepted", "artifact")
    private fun digest(character: Char): String = character.toString().repeat(64)
}

private fun <T> PlanResult<T>.value(): T = assertIs<PlanResult.Success<T>>(this).value
