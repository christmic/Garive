package com.garive.eng.kt.goal

/** Closed evidence family matching the four G1 criterion variants. */
public enum class GoalEvidenceKind(public val wireName: String) {
    /** Schema-validated user response evidence. */
    USER_ACCEPTANCE("user_acceptance"),
    /** Durable Artifact evidence. */
    ARTIFACT("artifact"),
    /** Durable Ledger fact evidence. */
    DURABLE_FACT("durable_fact"),
    /** Verified child-Goal terminal evidence. */
    CHILD_GOALS("child_goals"),
}

/** Exact evidence reference evaluated at a frozen commit version. */
public class GoalEvidenceV1 private constructor(
    public val evidenceId: GoalEvidenceId,
    public val criterionId: GoalCriterionId,
    public val kind: GoalEvidenceKind,
    public val durableReference: String,
    public val evidenceDigest: String,
    public val observedAtCommitVersion: Long,
) {
    public companion object {
        /** Validates a non-empty reference, canonical digest, and positive position. */
        public fun create(
            evidenceId: GoalEvidenceId,
            criterionId: GoalCriterionId,
            kind: GoalEvidenceKind,
            durableReference: String,
            evidenceDigest: String,
            observedAtCommitVersion: Long,
        ): GoalResult<GoalEvidenceV1> =
            if (durableReference.isEmpty() || !validDigest(evidenceDigest) || observedAtCommitVersion <= 0) {
                failure(GoalErrorCode.GOAL_INVALID)
            } else {
                GoalResult.Success(
                    GoalEvidenceV1(
                        evidenceId,
                        criterionId,
                        kind,
                        durableReference,
                        evidenceDigest,
                        observedAtCommitVersion,
                    ),
                )
            }
    }
}

/** Closed durable Goal lifecycle state. */
public enum class GoalState(public val wireName: String) {
    /** Definition may still be revised before work starts. */
    DRAFT("draft"),
    /** Work is currently admitted. */
    ACTIVE("active"),
    /** Typed external input or reconciliation is required. */
    SUSPENDED("suspended"),
    /** Every success criterion has verified evidence. */
    SUCCEEDED("succeeded"),
    /** Work ended unsuccessfully. */
    FAILED("failed"),
    /** Authenticated actor cancelled the Goal. */
    CANCELLED("cancelled"),
}

/** One requested pure lifecycle transition after Runtime command validation. */
public sealed interface GoalTransition {
    /** Start or resume work. */
    public data object Activate : GoalTransition
    /** Pause for one stable reason code. */
    public data class Suspend(public val reason: String) : GoalTransition
    /** Close successfully with complete exact evidence. */
    public data class Succeed(public val evidence: List<GoalEvidenceV1>) : GoalTransition
    /** Close unsuccessfully with one stable reason code. */
    public data class Fail(public val reason: String) : GoalTransition
    /** Cancel with one stable reason code. */
    public data class Cancel(public val reason: String) : GoalTransition
    /** Replace definition content and return to Draft. */
    public data class Revise(public val definition: GoalDefinitionV1) : GoalTransition
}

/** Immutable Goal projection after one contiguous durable prefix. */
public class GoalSnapshot private constructor(
    public val definition: GoalDefinitionV1,
    public val revision: Long,
    public val state: GoalState,
    terminalEvidence: List<GoalEvidenceV1>,
) {
    /** Exact success evidence, empty before Succeeded. */
    public val terminalEvidence: List<GoalEvidenceV1> = terminalEvidence.toList()

    /** Applies one transition only at the caller's exact expected revision. */
    public fun apply(expectedRevision: Long, transition: GoalTransition): GoalResult<GoalSnapshot> {
        if (expectedRevision != revision) return failure(GoalErrorCode.GOAL_REVISION_CONFLICT)
        if (state in TERMINAL_STATES) return failure(GoalErrorCode.GOAL_TRANSITION_INVALID)
        val nextRevision = revision + 1
        if (nextRevision <= revision) return failure(GoalErrorCode.GOAL_INVALID)
        return when (transition) {
            GoalTransition.Activate -> if (state == GoalState.DRAFT || state == GoalState.SUSPENDED) {
                GoalResult.Success(GoalSnapshot(definition, nextRevision, GoalState.ACTIVE, emptyList()))
            } else {
                failure(GoalErrorCode.GOAL_TRANSITION_INVALID)
            }
            is GoalTransition.Suspend -> if (state == GoalState.ACTIVE && transition.reason.isNotEmpty()) {
                GoalResult.Success(GoalSnapshot(definition, nextRevision, GoalState.SUSPENDED, emptyList()))
            } else {
                failure(GoalErrorCode.GOAL_TRANSITION_INVALID)
            }
            is GoalTransition.Succeed -> if (state != GoalState.ACTIVE) {
                failure(GoalErrorCode.GOAL_TRANSITION_INVALID)
            } else if (!validEvidence(definition.criteria, transition.evidence)) {
                failure(GoalErrorCode.GOAL_EVIDENCE_INSUFFICIENT)
            } else {
                GoalResult.Success(
                    GoalSnapshot(definition, nextRevision, GoalState.SUCCEEDED, transition.evidence),
                )
            }
            is GoalTransition.Fail -> if (
                state in setOf(GoalState.ACTIVE, GoalState.SUSPENDED) && transition.reason.isNotEmpty()
            ) {
                GoalResult.Success(GoalSnapshot(definition, nextRevision, GoalState.FAILED, emptyList()))
            } else {
                failure(GoalErrorCode.GOAL_TRANSITION_INVALID)
            }
            is GoalTransition.Cancel -> if (transition.reason.isNotEmpty()) {
                GoalResult.Success(GoalSnapshot(definition, nextRevision, GoalState.CANCELLED, emptyList()))
            } else {
                failure(GoalErrorCode.GOAL_TRANSITION_INVALID)
            }
            is GoalTransition.Revise -> if (transition.definition.goalId == definition.goalId) {
                GoalResult.Success(GoalSnapshot(transition.definition, nextRevision, GoalState.DRAFT, emptyList()))
            } else {
                failure(GoalErrorCode.GOAL_TRANSITION_INVALID)
            }
        }
    }

    public companion object {
        private val TERMINAL_STATES: Set<GoalState> =
            setOf(GoalState.SUCCEEDED, GoalState.FAILED, GoalState.CANCELLED)

        /** Creates revision 1 in Draft from a validated definition. */
        public fun create(definition: GoalDefinitionV1): GoalSnapshot =
            GoalSnapshot(definition, 1, GoalState.DRAFT, emptyList())
    }
}

private fun validEvidence(criteria: List<GoalCriterion>, evidence: List<GoalEvidenceV1>): Boolean {
    val byCriterion = evidence.associateBy(GoalEvidenceV1::criterionId)
    return byCriterion.size == evidence.size && evidence.map(GoalEvidenceV1::evidenceId).distinct().size == evidence.size &&
        criteria.size == evidence.size && criteria.all { criterion ->
            byCriterion[criterion.criterionId]?.kind == criterionKind(criterion)
        }
}

private fun criterionKind(value: GoalCriterion): GoalEvidenceKind = when (value) {
    is GoalCriterion.UserAcceptance -> GoalEvidenceKind.USER_ACCEPTANCE
    is GoalCriterion.Artifact -> GoalEvidenceKind.ARTIFACT
    is GoalCriterion.DurableFact -> GoalEvidenceKind.DURABLE_FACT
    is GoalCriterion.ChildGoals -> GoalEvidenceKind.CHILD_GOALS
}
