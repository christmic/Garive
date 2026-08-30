package com.garive.eng.kt.plan

/** Closed lifecycle for one immutable Plan revision. */
public enum class PlanState(public val wireName: String) {
    /** Valid proposal awaiting Runtime authority. */ PROPOSED("proposed"),
    /** Authoritative revision with no started step yet. */ ADOPTED("adopted"),
    /** At least one step started or remains available. */ RUNNING("running"),
    /** Plan-level continuation is required. */ SUSPENDED("suspended"),
    /** Every step and criterion reduction was verified. */ COMPLETED("completed"),
    /** Explicit terminal failure. */ FAILED("failed"),
    /** A newer revision atomically replaced this revision. */ SUPERSEDED("superseded"),
    /** Runtime authority rejected this proposal. */ REJECTED("rejected"),
}

/** Closed progress state for one declared step. */
public enum class StepState(public val wireName: String) {
    /** Dependencies are not complete. */ PENDING("pending"),
    /** Dependencies and bounds admit a claim. */ READY("ready"),
    /** Fenced claim exists, no attempt started. */ CLAIMED("claimed"),
    /** One attempt owns a Kernel Execution. */ RUNNING("running"),
    /** Typed continuation is required. */ SUSPENDED("suspended"),
    /** Verified terminal evidence exists. */ COMPLETED("completed"),
    /** Last attempt failed; policy may admit retry. */ FAILED("failed"),
}

/** Immutable public progress for one step. */
public data class StepProgress(public val state: StepState, public val attempts: Int)

/** One requested pure Plan/step transition after Runtime validation. */
public sealed interface PlanTransition {
    /** Adopt a valid proposal. */ public data object Adopt : PlanTransition
    /** Adopt with Runtime-verified completed steps from a prior revision. */
    public data class AdoptWithCarryForward(public val stepIds: Set<PlanStepId>) : PlanTransition
    /** Reject a proposal. */ public data object Reject : PlanTransition
    /** Suspend Plan-level dispatch. */ public data object Suspend : PlanTransition
    /** Resume Plan-level dispatch. */ public data object Resume : PlanTransition
    /** Supersede with another validated revision. */ public data object Supersede : PlanTransition
    /** Explicitly terminalize as failed. */ public data object Fail : PlanTransition
    /** Complete only with full step and criterion evidence. */
    public data class Complete(public val criteriaComplete: Boolean) : PlanTransition
    /** Claim one Ready step. */ public data class Claim(public val stepId: PlanStepId) : PlanTransition
    /** Expire a never-started claim. */ public data class ExpireClaim(public val stepId: PlanStepId) : PlanTransition
    /** Start one claimed attempt. */ public data class Start(public val stepId: PlanStepId) : PlanTransition
    /** Complete one running attempt. */ public data class CompleteStep(public val stepId: PlanStepId) : PlanTransition
    /** Suspend one running attempt. */ public data class SuspendStep(public val stepId: PlanStepId) : PlanTransition
    /** Resume after continuation resolution. */ public data class ResumeStep(public val stepId: PlanStepId) : PlanTransition
    /** Fail one running attempt. */ public data class FailStep(public val stepId: PlanStepId) : PlanTransition
    /** Admit a bounded retry. */ public data class RetryStep(public val stepId: PlanStepId) : PlanTransition
}

/** Immutable Plan projection after one contiguous transition prefix. */
public class PlanSnapshot private constructor(
    public val definition: PlanDefinitionV1,
    public val state: PlanState,
    progress: Map<PlanStepId, StepProgress>,
    public val totalAttempts: Int,
) {
    private val progress: Map<PlanStepId, StepProgress> = progress.toMap()

    /** Returns progress for one declared step. */
    public fun step(stepId: PlanStepId): StepProgress? = progress[stepId]

    /** Returns Ready steps in declaration order. */
    public fun readySteps(): List<PlanStepId> = definition.steps
        .filter { progress.getValue(it.stepId).state == StepState.READY }
        .map(PlanStepV1::stepId)

    /** Applies one legal transition without allocating Runtime identities. */
    public fun apply(transition: PlanTransition): PlanResult<PlanSnapshot> {
        if (state in TERMINAL) return failure(PlanErrorCode.PLAN_TRANSITION_INVALID)
        return when (transition) {
            PlanTransition.Adopt -> if (state == PlanState.PROPOSED) updated(PlanState.ADOPTED, progress).refresh()
            else invalid()
            is PlanTransition.AdoptWithCarryForward -> carryForward(transition.stepIds)
            PlanTransition.Reject -> if (state == PlanState.PROPOSED) success(updated(PlanState.REJECTED, progress)) else invalid()
            PlanTransition.Suspend -> if (state == PlanState.RUNNING) success(updated(PlanState.SUSPENDED, progress)) else invalid()
            PlanTransition.Resume -> if (state == PlanState.SUSPENDED) updated(PlanState.RUNNING, progress).refresh()
            else invalid()
            PlanTransition.Supersede -> if (state in ACTIVE) success(updated(PlanState.SUPERSEDED, progress)) else invalid()
            PlanTransition.Fail -> if (state == PlanState.RUNNING || state == PlanState.SUSPENDED) {
                success(updated(PlanState.FAILED, progress))
            } else invalid()
            is PlanTransition.Complete -> if (state == PlanState.RUNNING && transition.criteriaComplete &&
                progress.values.all { it.state == StepState.COMPLETED }
            ) success(updated(PlanState.COMPLETED, progress)) else invalid()
            is PlanTransition.Claim -> claim(transition.stepId)
            is PlanTransition.ExpireClaim -> replace(transition.stepId, StepState.CLAIMED, StepState.READY)
            is PlanTransition.Start -> start(transition.stepId)
            is PlanTransition.CompleteStep -> replace(
                transition.stepId, StepState.RUNNING, StepState.COMPLETED, refresh = true,
            )
            is PlanTransition.SuspendStep -> replace(transition.stepId, StepState.RUNNING, StepState.SUSPENDED)
            is PlanTransition.ResumeStep -> replace(
                transition.stepId, StepState.SUSPENDED, StepState.PENDING, refresh = true,
            )
            is PlanTransition.FailStep -> replace(transition.stepId, StepState.RUNNING, StepState.FAILED)
            is PlanTransition.RetryStep -> retry(transition.stepId)
        }
    }

    private fun claim(stepId: PlanStepId): PlanResult<PlanSnapshot> {
        val active = progress.values.count { it.state == StepState.CLAIMED || it.state == StepState.RUNNING }
        if (state !in setOf(PlanState.ADOPTED, PlanState.RUNNING) ||
            active >= definition.bounds.maxParallelReady || progress[stepId]?.state != StepState.READY
        ) return failure(PlanErrorCode.STEP_NOT_READY)
        return replace(stepId, StepState.READY, StepState.CLAIMED)
    }

    private fun carryForward(stepIds: Set<PlanStepId>): PlanResult<PlanSnapshot> {
        if (state != PlanState.PROPOSED || stepIds.any { id ->
                definition.step(id)?.dependsOn?.all(stepIds::contains) != true
            }
        ) return invalid()
        val next = progress.mapValues { (id, value) ->
            if (id in stepIds) value.copy(state = StepState.COMPLETED) else value
        }
        val nextState = if (stepIds.isEmpty()) PlanState.ADOPTED else PlanState.RUNNING
        return updated(nextState, next).refresh()
    }

    private fun start(stepId: PlanStepId): PlanResult<PlanSnapshot> {
        val current = progress[stepId] ?: return invalid()
        val limit = definition.step(stepId)?.maxAttempts ?: return invalid()
        if (current.state != StepState.CLAIMED) return invalid()
        if (current.attempts >= limit || totalAttempts >= definition.bounds.maxTotalAttempts) {
            return failure(PlanErrorCode.PLAN_BOUND_EXCEEDED)
        }
        val next = progress + (stepId to StepProgress(StepState.RUNNING, current.attempts + 1))
        return PlanResult.Success(PlanSnapshot(definition, PlanState.RUNNING, next, totalAttempts + 1))
    }

    private fun retry(stepId: PlanStepId): PlanResult<PlanSnapshot> {
        val current = progress[stepId] ?: return invalid()
        val limit = definition.step(stepId)?.maxAttempts ?: return invalid()
        if (current.state != StepState.FAILED) return invalid()
        if (current.attempts >= limit || totalAttempts >= definition.bounds.maxTotalAttempts) {
            return failure(PlanErrorCode.PLAN_BOUND_EXCEEDED)
        }
        return replace(stepId, StepState.FAILED, StepState.PENDING, refresh = true)
    }

    private fun replace(
        stepId: PlanStepId,
        expected: StepState,
        next: StepState,
        refresh: Boolean = false,
    ): PlanResult<PlanSnapshot> {
        val current = progress[stepId] ?: return invalid()
        if (current.state != expected) return invalid()
        val snapshot = PlanSnapshot(
            definition, state, progress + (stepId to current.copy(state = next)), totalAttempts,
        )
        return if (refresh) snapshot.refresh() else PlanResult.Success(snapshot)
    }

    private fun refresh(): PlanResult<PlanSnapshot> {
        if (state !in setOf(PlanState.ADOPTED, PlanState.RUNNING) ||
            totalAttempts >= definition.bounds.maxTotalAttempts
        ) return PlanResult.Success(this)
        val completed = progress.filterValues { it.state == StepState.COMPLETED }.keys
        val next = progress.toMutableMap()
        definition.steps.forEach { step ->
            val current = next.getValue(step.stepId)
            if (current.state == StepState.PENDING && current.attempts < step.maxAttempts &&
                step.dependsOn.all(completed::contains)
            ) next[step.stepId] = current.copy(state = StepState.READY)
        }
        return PlanResult.Success(PlanSnapshot(definition, state, next, totalAttempts))
    }

    private fun updated(next: PlanState, values: Map<PlanStepId, StepProgress>): PlanSnapshot =
        PlanSnapshot(definition, next, values, totalAttempts)

    private fun success(value: PlanSnapshot): PlanResult<PlanSnapshot> = PlanResult.Success(value)

    private fun invalid(): PlanResult<PlanSnapshot> = failure(PlanErrorCode.PLAN_TRANSITION_INVALID)

    public companion object {
        private val TERMINAL = setOf(
            PlanState.COMPLETED, PlanState.FAILED, PlanState.SUPERSEDED, PlanState.REJECTED,
        )
        private val ACTIVE = setOf(PlanState.ADOPTED, PlanState.RUNNING, PlanState.SUSPENDED)

        /** Creates a Proposed projection with every step Pending. */
        public fun create(definition: PlanDefinitionV1): PlanSnapshot = PlanSnapshot(
            definition,
            PlanState.PROPOSED,
            definition.steps.associate { it.stepId to StepProgress(StepState.PENDING, 0) },
            0,
        )
    }
}
