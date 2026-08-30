package com.garive.eng.kt.plan

import java.security.MessageDigest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

/** Stable portable Plan failure classification. */
public enum class PlanErrorCode(public val wireName: String) {
    /** Identity, binding, bounds, step, or digest is malformed. */
    PLAN_INVALID("plan_invalid"),
    /** Dependency graph contains a cycle. */
    PLAN_CYCLE("plan_cycle"),
    /** Requested progress transition is not admitted. */
    PLAN_TRANSITION_INVALID("plan_transition_invalid"),
    /** Step is not currently ready to claim. */
    STEP_NOT_READY("step_not_ready"),
    /** A hard Plan or step bound was exhausted. */
    PLAN_BOUND_EXCEEDED("plan_bound_exceeded"),
}

/** Typed portable Plan failure. */
public data class PlanError(public val code: PlanErrorCode)

/** Typed success or stable Plan failure. */
public sealed interface PlanResult<out T> {
    /** Successful immutable value. */
    public data class Success<T>(public val value: T) : PlanResult<T>
    /** Stable contract failure. */
    public data class Failure(public val error: PlanError) : PlanResult<Nothing>
}

/** Non-empty opaque Plan identity. */
@ConsistentCopyVisibility
public data class PlanId private constructor(public val value: String) {
    public companion object {
        /** Validates and constructs a Plan identity. */
        public fun create(value: String): PlanResult<PlanId> =
            if (value.isEmpty()) failure(PlanErrorCode.PLAN_INVALID) else PlanResult.Success(PlanId(value))
    }
}

/** Non-empty opaque Plan step identity. */
@ConsistentCopyVisibility
public data class PlanStepId private constructor(public val value: String) : Comparable<PlanStepId> {
    public override fun compareTo(other: PlanStepId): Int = value.compareTo(other.value)
    public companion object {
        /** Validates and constructs a step identity. */
        public fun create(value: String): PlanResult<PlanStepId> =
            if (value.isEmpty()) failure(PlanErrorCode.PLAN_INVALID) else PlanResult.Success(PlanStepId(value))
    }
}

/** Exact capability revision required by one step. */
@ConsistentCopyVisibility
public data class PlanCapabilityReference private constructor(
    public val name: String,
    public val exactRevision: String,
) : Comparable<PlanCapabilityReference> {
    public override fun compareTo(other: PlanCapabilityReference): Int =
        compareValuesBy(this, other, PlanCapabilityReference::name, PlanCapabilityReference::exactRevision)
    public companion object {
        /** Validates one exact non-empty capability reference. */
        public fun create(name: String, exactRevision: String): PlanResult<PlanCapabilityReference> =
            if (name.isEmpty() || exactRevision.isEmpty()) failure(PlanErrorCode.PLAN_INVALID)
            else PlanResult.Success(PlanCapabilityReference(name, exactRevision))
    }
}

/** Explicit non-zero hard bounds for one Plan revision. */
@ConsistentCopyVisibility
public data class PlanBoundsV1 private constructor(
    public val maxSteps: Int,
    public val maxParallelReady: Int,
    public val maxTotalAttempts: Int,
    public val tokenBudget: Long?,
    public val durationBudgetMs: Long?,
) {
    public companion object {
        /** Validates mandatory and optional bounds as non-zero. */
        public fun create(
            maxSteps: Int,
            maxParallelReady: Int,
            maxTotalAttempts: Int,
            tokenBudget: Long?,
            durationBudgetMs: Long?,
        ): PlanResult<PlanBoundsV1> =
            if (maxSteps <= 0 || maxParallelReady <= 0 || maxParallelReady > maxSteps ||
                maxTotalAttempts <= 0 || tokenBudget != null && tokenBudget <= 0 ||
                durationBudgetMs != null && durationBudgetMs <= 0
            ) failure(PlanErrorCode.PLAN_INVALID)
            else PlanResult.Success(
                PlanBoundsV1(maxSteps, maxParallelReady, maxTotalAttempts, tokenBudget, durationBudgetMs),
            )
    }
}

/** One immutable executable node in declaration/tie-break order. */
public class PlanStepV1 private constructor(
    public val stepId: PlanStepId,
    public val objective: String,
    dependsOn: List<PlanStepId>,
    completionCriteria: List<String>,
    requiredCapabilities: List<PlanCapabilityReference>,
    inputBindings: List<String>,
    public val maxAttempts: Int,
) {
    /** Canonical unique direct dependencies. */
    public val dependsOn: List<PlanStepId> = dependsOn.toList()
    /** Canonical unique Goal criterion identities. */
    public val completionCriteria: List<String> = completionCriteria.toList()
    /** Canonical unique exact capabilities. */
    public val requiredCapabilities: List<PlanCapabilityReference> = requiredCapabilities.toList()
    /** Canonical unique content/fact digests. */
    public val inputBindings: List<String> = inputBindings.toList()

    public companion object {
        /** Validates one step and canonicalizes every set-valued binding. */
        @Suppress("LongParameterList")
        public fun create(
            stepId: PlanStepId,
            objective: String,
            dependsOn: List<PlanStepId>,
            completionCriteria: List<String>,
            requiredCapabilities: List<PlanCapabilityReference>,
            inputBindings: List<String>,
            maxAttempts: Int,
        ): PlanResult<PlanStepV1> {
            val dependencies = dependsOn.sorted()
            val criteria = completionCriteria.sorted()
            val capabilities = requiredCapabilities.sorted()
            val inputs = inputBindings.sorted()
            val valid = objective.isNotEmpty() && maxAttempts > 0 && criteria.isNotEmpty() &&
                stepId !in dependencies && unique(dependencies) && uniqueNonEmpty(criteria) &&
                unique(capabilities) && unique(inputs) && inputs.all(::validDigest)
            return if (!valid) failure(PlanErrorCode.PLAN_INVALID)
            else PlanResult.Success(
                PlanStepV1(stepId, objective, dependencies, criteria, capabilities, inputs, maxAttempts),
            )
        }
    }
}

/** Immutable canonical Plan revision content. */
public class PlanDefinitionV1 private constructor(
    public val planId: PlanId,
    public val planRevision: Long,
    public val goalId: String,
    public val goalRevision: Long,
    public val goalDefinitionDigest: String,
    public val agentSnapshotDigest: String,
    public val toolCatalogueDigest: String,
    public val safetyPolicyRevision: String,
    steps: List<PlanStepV1>,
    public val bounds: PlanBoundsV1,
) {
    /** Steps in semantic declaration/tie-break order. */
    public val steps: List<PlanStepV1> = steps.toList()

    /** Returns the exact RFC 8785 Plan definition document. */
    public fun canonicalJson(): PlanResult<String> = runCatching {
        JsonCanonicalizer(json().toString()).encodedString
    }.fold({ PlanResult.Success(it) }, { failure(PlanErrorCode.PLAN_INVALID) })

    /** Returns lowercase SHA-256 over the canonical definition. */
    public fun digest(): PlanResult<String> = when (val canonical = canonicalJson()) {
        is PlanResult.Failure -> canonical
        is PlanResult.Success -> PlanResult.Success(sha256(canonical.value))
    }

    internal fun step(stepId: PlanStepId): PlanStepV1? = steps.firstOrNull { it.stepId == stepId }

    private fun json(): JsonObject = JsonObject(
        mapOf(
            "contract" to JsonPrimitive("garive.plan-definition"),
            "version" to JsonPrimitive(1),
            "plan_id" to JsonPrimitive(planId.value),
            "plan_revision" to JsonPrimitive(planRevision),
            "goal_id" to JsonPrimitive(goalId),
            "goal_revision" to JsonPrimitive(goalRevision),
            "goal_definition_digest" to JsonPrimitive(goalDefinitionDigest),
            "agent_snapshot_digest" to JsonPrimitive(agentSnapshotDigest),
            "tool_catalogue_digest" to JsonPrimitive(toolCatalogueDigest),
            "safety_policy_revision" to JsonPrimitive(safetyPolicyRevision),
            "steps" to JsonArray(steps.map(::stepJson)),
            "bounds" to boundsJson(bounds),
        ),
    )

    public companion object {
        /** Validates frozen bindings, criterion/capability scope, and DAG topology. */
        @Suppress("LongParameterList")
        public fun create(
            planId: PlanId,
            planRevision: Long,
            goalId: String,
            goalRevision: Long,
            goalDefinitionDigest: String,
            agentSnapshotDigest: String,
            toolCatalogueDigest: String,
            safetyPolicyRevision: String,
            steps: List<PlanStepV1>,
            bounds: PlanBoundsV1,
            requiredGoalCriteria: Set<String>,
            alreadySatisfiedCriteria: Set<String>,
            availableCapabilities: Set<PlanCapabilityReference>,
        ): PlanResult<PlanDefinitionV1> {
            val ids = steps.map(PlanStepV1::stepId).toSet()
            val covered = steps.flatMap(PlanStepV1::completionCriteria).toSet() + alreadySatisfiedCriteria
            val valid = planRevision > 0 && goalId.isNotEmpty() && goalRevision > 0 &&
                validDigest(goalDefinitionDigest) && validDigest(agentSnapshotDigest) &&
                validDigest(toolCatalogueDigest) && safetyPolicyRevision.isNotEmpty() &&
                steps.isNotEmpty() && steps.size <= bounds.maxSteps && ids.size == steps.size &&
                requiredGoalCriteria.isNotEmpty() && alreadySatisfiedCriteria.all(requiredGoalCriteria::contains) &&
                requiredGoalCriteria.all(covered::contains) && steps.all { step ->
                    step.dependsOn.all(ids::contains) && step.completionCriteria.all(requiredGoalCriteria::contains) &&
                        step.requiredCapabilities.all(availableCapabilities::contains)
                }
            if (!valid) return failure(PlanErrorCode.PLAN_INVALID)
            if (cyclic(steps)) return failure(PlanErrorCode.PLAN_CYCLE)
            return PlanResult.Success(
                PlanDefinitionV1(
                    planId, planRevision, goalId, goalRevision, goalDefinitionDigest, agentSnapshotDigest,
                    toolCatalogueDigest, safetyPolicyRevision, steps, bounds,
                ),
            )
        }
    }
}

private fun cyclic(steps: List<PlanStepV1>): Boolean {
    val remaining = steps.associate { it.stepId to it.dependsOn.toMutableSet() }.toMutableMap()
    val completed = mutableSetOf<PlanStepId>()
    while (true) {
        val ready = steps.map(PlanStepV1::stepId).filter { id -> remaining[id]?.all(completed::contains) == true }
        if (ready.isEmpty()) break
        ready.forEach { id -> remaining.remove(id); completed.add(id) }
    }
    return remaining.isNotEmpty()
}

private fun stepJson(value: PlanStepV1): JsonObject = JsonObject(
    mapOf(
        "step_id" to JsonPrimitive(value.stepId.value),
        "objective" to JsonPrimitive(value.objective),
        "depends_on" to JsonArray(value.dependsOn.map { JsonPrimitive(it.value) }),
        "completion_criteria" to JsonArray(value.completionCriteria.map(::JsonPrimitive)),
        "required_capabilities" to JsonArray(value.requiredCapabilities.map(::capabilityJson)),
        "input_bindings" to JsonArray(value.inputBindings.map(::JsonPrimitive)),
        "max_attempts" to JsonPrimitive(value.maxAttempts),
    ),
)

private fun capabilityJson(value: PlanCapabilityReference): JsonObject = JsonObject(
    mapOf("name" to JsonPrimitive(value.name), "exact_revision" to JsonPrimitive(value.exactRevision)),
)

private fun boundsJson(value: PlanBoundsV1): JsonObject = JsonObject(
    mapOf(
        "max_steps" to JsonPrimitive(value.maxSteps),
        "max_parallel_ready" to JsonPrimitive(value.maxParallelReady),
        "max_total_attempts" to JsonPrimitive(value.maxTotalAttempts),
        "token_budget" to (value.tokenBudget?.let(::JsonPrimitive) ?: JsonNull),
        "duration_budget_ms" to (value.durationBudgetMs?.let(::JsonPrimitive) ?: JsonNull),
    ),
)

private fun <T> unique(values: List<T>): Boolean = values.distinct().size == values.size
private fun uniqueNonEmpty(values: List<String>): Boolean = values.all(String::isNotEmpty) && unique(values)
internal fun validDigest(value: String): Boolean = value.matches(Regex("[0-9a-f]{64}"))
internal fun sha256(value: String): String = MessageDigest.getInstance("SHA-256")
    .digest(value.encodeToByteArray()).joinToString("") { "%02x".format(it.toInt() and 0xff) }
internal fun <T> failure(code: PlanErrorCode): PlanResult<T> = PlanResult.Failure(PlanError(code))
