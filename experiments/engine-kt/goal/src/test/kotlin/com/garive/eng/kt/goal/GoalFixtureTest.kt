package com.garive.eng.kt.goal

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long

class GoalFixtureTest {
    private val fixture: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/goal-lifecycle-v1.json").readText(),
    ).jsonObject
    private val definition: GoalDefinitionV1 = definition(fixture.getValue("definition").jsonObject)
    private val evidence: GoalEvidenceV1 = evidence(fixture.getValue("evidence").jsonObject)

    @Test
    fun canonicalDefinitionAndValidLifecycleMatchRust() {
        assertEquals(1, fixture.getValue("schema_version").jsonPrimitive.int)
        assertEquals(
            fixture.getValue("definition").jsonObject.getValue("definition_digest").jsonPrimitive.content,
            definition.digest().value(),
        )
        var snapshot = GoalSnapshot.create(definition)
        fixture.getValue("valid_sequence").jsonArray.forEach { element ->
            val step = element.jsonObject
            val transition = when (step.getValue("operation").jsonPrimitive.content) {
                "activate" -> GoalTransition.Activate
                "suspend" -> GoalTransition.Suspend(step.getValue("reason").jsonPrimitive.content)
                "succeed" -> GoalTransition.Succeed(listOf(evidence))
                else -> error("unknown fixture transition")
            }
            snapshot = snapshot.apply(
                step.getValue("expected_revision").jsonPrimitive.long,
                transition,
            ).value()
            assertEquals(step.getValue("revision").jsonPrimitive.long, snapshot.revision)
            assertEquals(step.getValue("state").jsonPrimitive.content, snapshot.state.wireName)
        }
    }

    @Test
    fun everySharedInvalidCaseFailsWithTheExactCode() {
        fixture.getValue("invalid_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val start = when (case.getValue("start").jsonPrimitive.content) {
                "draft" -> GoalSnapshot.create(definition)
                "active" -> GoalSnapshot.create(definition).apply(1, GoalTransition.Activate).value()
                "succeeded" -> GoalSnapshot.create(definition)
                    .apply(1, GoalTransition.Activate).value()
                    .apply(2, GoalTransition.Succeed(listOf(evidence))).value()
                else -> error("unknown fixture start")
            }
            val transition = when (case.getValue("operation").jsonPrimitive.content) {
                "activate" -> GoalTransition.Activate
                "succeed" -> GoalTransition.Succeed(listOf(evidence))
                "succeed_empty" -> GoalTransition.Succeed(emptyList())
                else -> error("unknown fixture operation")
            }
            val failure = assertIs<GoalResult.Failure>(
                start.apply(case.getValue("expected_revision").jsonPrimitive.long, transition),
                case.getValue("name").jsonPrimitive.content,
            )
            assertEquals(case.getValue("expected_code").jsonPrimitive.content, failure.error.code.wireName)
        }
    }

    @Test
    fun childGoalCanOnlyNarrowItsParentGrant() {
        val child = GoalDefinitionV1.create(
            GoalId.create("child").value(),
            "Child objective",
            definition.criteria,
            GoalScopeV1.create(null, listOf("workspace-1")).value(),
            GoalBoundsV1.create(2, 3, 1, 5_000, 30_000).value(),
            definition.goalId,
            definition.capabilityReferences,
        ).value()
        assertIs<GoalResult.Success<Unit>>(child.validateChildOf(definition))

        val wider = GoalDefinitionV1.create(
            GoalId.create("wider").value(),
            "Wider objective",
            definition.criteria,
            GoalScopeV1.create("session-2", listOf("workspace-1")).value(),
            GoalBoundsV1.create(3, 4, 2, null, 60_000).value(),
            definition.goalId,
            definition.capabilityReferences,
        ).value()
        assertEquals(
            GoalErrorCode.GOAL_SCOPE_EXCEEDED,
            assertIs<GoalResult.Failure>(wider.validateChildOf(definition)).error.code,
        )
    }

    @Test
    fun goalTextReferencesAndCollectionsAreByteBounded() {
        assertIs<GoalResult.Success<GoalId>>(GoalId.create("g".repeat(256)))
        assertEquals(
            GoalErrorCode.GOAL_INVALID,
            assertIs<GoalResult.Failure>(GoalId.create("g".repeat(257))).error.code,
        )
        assertIs<GoalResult.Failure>(GoalCapabilityReference.create("capability", "r".repeat(513)))
        assertIs<GoalResult.Failure>(
            GoalEvidenceV1.create(
                GoalEvidenceId.create("evidence").value(),
                GoalCriterionId.create("criterion").value(),
                GoalEvidenceKind.ARTIFACT,
                "r".repeat(513),
                "b".repeat(64),
                1,
            ),
        )
        assertIs<GoalResult.Failure>(
            GoalDefinitionV1.create(
                GoalId.create("large-objective").value(),
                "界".repeat(5_462),
                definition.criteria,
                definition.scope,
                definition.bounds,
                null,
                definition.capabilityReferences,
            ),
        )
        assertIs<GoalResult.Failure>(
            GoalScopeV1.create("session-1", (0..256).map { "workspace-$it" }),
        )
    }

    private fun definition(value: JsonObject): GoalDefinitionV1 {
        val criterion = value.getValue("criteria").jsonArray.single().jsonObject
        val scope = value.getValue("scope").jsonObject
        val bounds = value.getValue("bounds").jsonObject
        return GoalDefinitionV1.create(
            GoalId.create(value.getValue("goal_id").jsonPrimitive.content).value(),
            value.getValue("objective").jsonPrimitive.content,
            listOf(
                GoalCriterion.UserAcceptance(
                    GoalCriterionId.create(criterion.getValue("criterion_id").jsonPrimitive.content).value(),
                    criterion.getValue("response_schema_digest").jsonPrimitive.content,
                ),
            ),
            GoalScopeV1.create(
                scope.getValue("session_id").jsonPrimitive.contentOrNull,
                scope.getValue("workspace_capability_ids").jsonArray.map { it.jsonPrimitive.content },
            ).value(),
            GoalBoundsV1.create(
                bounds.getValue("max_attempts").jsonPrimitive.int,
                bounds.getValue("max_plan_revisions").jsonPrimitive.int,
                bounds.getValue("max_child_goals").jsonPrimitive.int,
                bounds.getValue("token_budget").jsonPrimitive.contentOrNull?.toLong(),
                bounds.getValue("duration_budget_ms").jsonPrimitive.contentOrNull?.toLong(),
            ).value(),
            value.getValue("parent_goal_id").jsonPrimitive.contentOrNull?.let { GoalId.create(it).value() },
            value.getValue("capability_references").jsonArray.map {
                val reference = it.jsonObject
                GoalCapabilityReference.create(
                    reference.getValue("name").jsonPrimitive.content,
                    reference.getValue("exact_revision").jsonPrimitive.content,
                ).value()
            },
        ).value()
    }

    private fun evidence(value: JsonObject): GoalEvidenceV1 = GoalEvidenceV1.create(
        GoalEvidenceId.create(value.getValue("evidence_id").jsonPrimitive.content).value(),
        GoalCriterionId.create(value.getValue("criterion_id").jsonPrimitive.content).value(),
        GoalEvidenceKind.USER_ACCEPTANCE,
        value.getValue("durable_reference").jsonPrimitive.content,
        value.getValue("evidence_digest").jsonPrimitive.content,
        value.getValue("observed_at_commit_version").jsonPrimitive.long,
    ).value()
}

private fun <T> GoalResult<T>.value(): T = assertIs<GoalResult.Success<T>>(this).value
