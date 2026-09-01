package com.garive.eng.kt.goal

import java.security.MessageDigest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

/** Stable portable Goal failure classification. */
public enum class GoalErrorCode(public val wireName: String) {
    /** Definition, identity, bound, evidence, or digest is malformed. */
    GOAL_INVALID("goal_invalid"),
    /** Expected revision does not match. */
    GOAL_REVISION_CONFLICT("goal_revision_conflict"),
    /** Lifecycle edge is not admitted. */
    GOAL_TRANSITION_INVALID("goal_transition_invalid"),
    /** Success evidence is incomplete or mismatched. */
    GOAL_EVIDENCE_INSUFFICIENT("goal_evidence_insufficient"),
    /** Child scope, capability, parent identity, or bound exceeds its parent. */
    GOAL_SCOPE_EXCEEDED("goal_scope_exceeded"),
}

/** Typed portable Goal failure. */
public data class GoalError(public val code: GoalErrorCode)

/** Typed success or stable Goal failure. */
public sealed interface GoalResult<out T> {
    /** Successful immutable value. */
    public data class Success<T>(public val value: T) : GoalResult<T>

    /** Stable contract failure. */
    public data class Failure(public val error: GoalError) : GoalResult<Nothing>
}

/** Non-empty opaque Goal identity. */
@ConsistentCopyVisibility
public data class GoalId private constructor(public val value: String) : Comparable<GoalId> {
    public override fun compareTo(other: GoalId): Int = value.compareTo(other.value)

    public companion object {
        /** Validates and constructs a Goal identity. */
        public fun create(value: String): GoalResult<GoalId> =
            if (value.isEmpty()) failure(GoalErrorCode.GOAL_INVALID) else GoalResult.Success(GoalId(value))
    }
}

/** Non-empty opaque Goal criterion identity. */
@ConsistentCopyVisibility
public data class GoalCriterionId private constructor(public val value: String) : Comparable<GoalCriterionId> {
    public override fun compareTo(other: GoalCriterionId): Int = value.compareTo(other.value)

    public companion object {
        /** Validates and constructs a criterion identity. */
        public fun create(value: String): GoalResult<GoalCriterionId> =
            if (value.isEmpty()) failure(GoalErrorCode.GOAL_INVALID)
            else GoalResult.Success(GoalCriterionId(value))
    }
}

/** Non-empty opaque Goal evidence identity. */
@ConsistentCopyVisibility
public data class GoalEvidenceId private constructor(public val value: String) : Comparable<GoalEvidenceId> {
    public override fun compareTo(other: GoalEvidenceId): Int = value.compareTo(other.value)

    public companion object {
        /** Validates and constructs an evidence identity. */
        public fun create(value: String): GoalResult<GoalEvidenceId> =
            if (value.isEmpty()) failure(GoalErrorCode.GOAL_INVALID) else GoalResult.Success(GoalEvidenceId(value))
    }
}

/** Exact capability revision available to one Goal. */
@ConsistentCopyVisibility
public data class GoalCapabilityReference private constructor(
    public val name: String,
    public val exactRevision: String,
) : Comparable<GoalCapabilityReference> {
    public override fun compareTo(other: GoalCapabilityReference): Int =
        compareValuesBy(this, other, GoalCapabilityReference::name, GoalCapabilityReference::exactRevision)

    public companion object {
        /** Validates one exact non-empty capability reference. */
        public fun create(name: String, exactRevision: String): GoalResult<GoalCapabilityReference> =
            if (name.isEmpty() || exactRevision.isEmpty()) failure(GoalErrorCode.GOAL_INVALID)
            else GoalResult.Success(GoalCapabilityReference(name, exactRevision))
    }
}

/** Bounded scope references; workspace values are opaque Runtime capabilities. */
public class GoalScopeV1 private constructor(
    public val sessionId: String?,
    workspaceCapabilityIds: List<String>,
) {
    /** Canonical unique workspace capability references. */
    public val workspaceCapabilityIds: List<String> = workspaceCapabilityIds.toList()

    public companion object {
        /** Requires a Session or at least one unique non-empty workspace capability. */
        public fun create(sessionId: String?, workspaceCapabilityIds: List<String>): GoalResult<GoalScopeV1> {
            val sorted = workspaceCapabilityIds.sorted()
            return if (sessionId == "" || sorted.any(String::isEmpty) || sorted.distinct().size != sorted.size ||
                sessionId == null && sorted.isEmpty()
            ) {
                failure(GoalErrorCode.GOAL_INVALID)
            } else {
                GoalResult.Success(GoalScopeV1(sessionId, sorted))
            }
        }
    }
}

/** Explicit non-zero hard bounds for one Goal definition. */
@ConsistentCopyVisibility
public data class GoalBoundsV1 private constructor(
    public val maxAttempts: Int,
    public val maxPlanRevisions: Int,
    public val maxChildGoals: Int,
    public val tokenBudget: Long?,
    public val durationBudgetMs: Long?,
) {
    public companion object {
        /** Validates all mandatory and optional bounds as non-zero. */
        public fun create(
            maxAttempts: Int,
            maxPlanRevisions: Int,
            maxChildGoals: Int,
            tokenBudget: Long?,
            durationBudgetMs: Long?,
        ): GoalResult<GoalBoundsV1> =
            if (maxAttempts <= 0 || maxPlanRevisions <= 0 || maxChildGoals <= 0 ||
                tokenBudget != null && tokenBudget <= 0 || durationBudgetMs != null && durationBudgetMs <= 0
            ) {
                failure(GoalErrorCode.GOAL_INVALID)
            } else {
                GoalResult.Success(
                    GoalBoundsV1(maxAttempts, maxPlanRevisions, maxChildGoals, tokenBudget, durationBudgetMs),
                )
            }
    }
}

/** Closed success criterion set; all declared criteria must be satisfied. */
public sealed interface GoalCriterion {
    /** Stable criterion identity. */
    public val criterionId: GoalCriterionId

    /** Explicit schema-bound user acceptance. */
    public data class UserAcceptance(
        override val criterionId: GoalCriterionId,
        public val responseSchemaDigest: String,
    ) : GoalCriterion

    /** Durable Artifact evidence. */
    public data class Artifact(
        override val criterionId: GoalCriterionId,
        public val artifactKind: String,
        public val requiredDigest: String?,
    ) : GoalCriterion

    /** One exact durable fact/subject binding. */
    public data class DurableFact(
        override val criterionId: GoalCriterionId,
        public val factKind: String,
        public val subjectDigest: String,
    ) : GoalCriterion

    /** Completion of a non-empty exact child Goal set. */
    public data class ChildGoals(
        override val criterionId: GoalCriterionId,
        public val childGoalIds: List<GoalId>,
    ) : GoalCriterion
}

/** Immutable canonical Goal definition revision content. */
public class GoalDefinitionV1 private constructor(
    public val goalId: GoalId,
    public val objective: String,
    criteria: List<GoalCriterion>,
    public val scope: GoalScopeV1,
    public val bounds: GoalBoundsV1,
    public val parentGoalId: GoalId?,
    capabilityReferences: List<GoalCapabilityReference>,
) {
    /** Criteria in semantic declaration order. */
    public val criteria: List<GoalCriterion> = criteria.toList()
    /** Canonical unique exact capability references. */
    public val capabilityReferences: List<GoalCapabilityReference> = capabilityReferences.toList()

    /** Proves that this child definition only narrows one exact parent grant. */
    public fun validateChildOf(parent: GoalDefinitionV1): GoalResult<Unit> {
        val sessionWithin = scope.sessionId == null || scope.sessionId == parent.scope.sessionId
        val workspaceWithin = parent.scope.workspaceCapabilityIds.containsAll(scope.workspaceCapabilityIds)
        val boundsWithin = bounds.maxAttempts <= parent.bounds.maxAttempts &&
            bounds.maxPlanRevisions <= parent.bounds.maxPlanRevisions &&
            bounds.maxChildGoals <= parent.bounds.maxChildGoals &&
            optionalBoundWithin(bounds.tokenBudget, parent.bounds.tokenBudget) &&
            optionalBoundWithin(bounds.durationBudgetMs, parent.bounds.durationBudgetMs)
        val capabilitiesWithin = parent.capabilityReferences.containsAll(capabilityReferences)
        return if (parentGoalId == parent.goalId && sessionWithin && workspaceWithin &&
            boundsWithin && capabilitiesWithin
        ) {
            GoalResult.Success(Unit)
        } else {
            failure(GoalErrorCode.GOAL_SCOPE_EXCEEDED)
        }
    }

    /** Returns lowercase SHA-256 over the RFC 8785 canonical definition. */
    public fun digest(): GoalResult<String> = runCatching {
        MessageDigest.getInstance("SHA-256").digest(JsonCanonicalizer(canonicalJson().toString()).encodedUTF8)
            .joinToString("") { byte -> "%02x".format(byte.toInt() and 0xff) }
    }.fold(
        onSuccess = { GoalResult.Success(it) },
        onFailure = { failure(GoalErrorCode.GOAL_INVALID) },
    )

    internal fun canonicalJson(): JsonObject = JsonObject(
        mapOf(
            "contract" to JsonPrimitive("garive.goal-definition"),
            "version" to JsonPrimitive(1),
            "goal_id" to JsonPrimitive(goalId.value),
            "objective" to JsonPrimitive(objective),
            "criteria" to JsonArray(criteria.map(::criterionJson)),
            "scope" to JsonObject(
                mapOf(
                    "session_id" to (scope.sessionId?.let(::JsonPrimitive) ?: JsonNull),
                    "workspace_capability_ids" to JsonArray(scope.workspaceCapabilityIds.map(::JsonPrimitive)),
                ),
            ),
            "bounds" to JsonObject(
                mapOf(
                    "max_attempts" to JsonPrimitive(bounds.maxAttempts),
                    "max_plan_revisions" to JsonPrimitive(bounds.maxPlanRevisions),
                    "max_child_goals" to JsonPrimitive(bounds.maxChildGoals),
                    "token_budget" to (bounds.tokenBudget?.let(::JsonPrimitive) ?: JsonNull),
                    "duration_budget_ms" to (bounds.durationBudgetMs?.let(::JsonPrimitive) ?: JsonNull),
                ),
            ),
            "parent_goal_id" to (parentGoalId?.value?.let(::JsonPrimitive) ?: JsonNull),
            "capability_references" to JsonArray(
                capabilityReferences.map {
                    JsonObject(
                        mapOf("name" to JsonPrimitive(it.name), "exact_revision" to JsonPrimitive(it.exactRevision)),
                    )
                },
            ),
        ),
    )

    public companion object {
        /** Validates required text, unique criteria/capabilities and self-parenting. */
        @Suppress("LongParameterList")
        public fun create(
            goalId: GoalId,
            objective: String,
            criteria: List<GoalCriterion>,
            scope: GoalScopeV1,
            bounds: GoalBoundsV1,
            parentGoalId: GoalId?,
            capabilityReferences: List<GoalCapabilityReference>,
        ): GoalResult<GoalDefinitionV1> {
            val capabilities = capabilityReferences.sorted()
            val valid = objective.isNotEmpty() && criteria.isNotEmpty() &&
                criteria.map(GoalCriterion::criterionId).distinct().size == criteria.size &&
                criteria.all(::validCriterion) && capabilities.distinct().size == capabilities.size &&
                parentGoalId != goalId
            return if (!valid) failure(GoalErrorCode.GOAL_INVALID)
            else GoalResult.Success(
                GoalDefinitionV1(goalId, objective, criteria, scope, bounds, parentGoalId, capabilities),
            )
        }
    }
}

private fun validCriterion(value: GoalCriterion): Boolean = when (value) {
    is GoalCriterion.UserAcceptance -> validDigest(value.responseSchemaDigest)
    is GoalCriterion.Artifact -> value.artifactKind.isNotEmpty() &&
        (value.requiredDigest == null || validDigest(value.requiredDigest))
    is GoalCriterion.DurableFact -> value.factKind.isNotEmpty() && validDigest(value.subjectDigest)
    is GoalCriterion.ChildGoals -> value.childGoalIds.isNotEmpty() &&
        value.childGoalIds.sorted().distinct().size == value.childGoalIds.size
}

private fun criterionJson(value: GoalCriterion): JsonObject = when (value) {
    is GoalCriterion.UserAcceptance -> JsonObject(
        mapOf(
            "kind" to JsonPrimitive("user_acceptance"),
            "criterion_id" to JsonPrimitive(value.criterionId.value),
            "response_schema_digest" to JsonPrimitive(value.responseSchemaDigest),
        ),
    )
    is GoalCriterion.Artifact -> JsonObject(
        mapOf(
            "kind" to JsonPrimitive("artifact"),
            "criterion_id" to JsonPrimitive(value.criterionId.value),
            "artifact_kind" to JsonPrimitive(value.artifactKind),
            "required_digest" to (value.requiredDigest?.let(::JsonPrimitive) ?: JsonNull),
        ),
    )
    is GoalCriterion.DurableFact -> JsonObject(
        mapOf(
            "kind" to JsonPrimitive("durable_fact"),
            "criterion_id" to JsonPrimitive(value.criterionId.value),
            "fact_kind" to JsonPrimitive(value.factKind),
            "subject_digest" to JsonPrimitive(value.subjectDigest),
        ),
    )
    is GoalCriterion.ChildGoals -> JsonObject(
        mapOf(
            "kind" to JsonPrimitive("child_goals"),
            "criterion_id" to JsonPrimitive(value.criterionId.value),
            "child_goal_ids" to JsonArray(value.childGoalIds.sorted().map { JsonPrimitive(it.value) }),
        ),
    )
}

internal fun validDigest(value: String): Boolean =
    value.length == 64 && value.all { it in '0'..'9' || it in 'a'..'f' }

private fun optionalBoundWithin(child: Long?, parent: Long?): Boolean = when {
    parent == null -> true
    child == null -> false
    else -> child <= parent
}

internal fun failure(code: GoalErrorCode): GoalResult.Failure = GoalResult.Failure(GoalError(code))
