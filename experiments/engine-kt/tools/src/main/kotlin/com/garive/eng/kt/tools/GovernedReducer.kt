package com.garive.eng.kt.tools

/** Durable authorization verdict for one exact invocation. */
public sealed interface AuthorizationVerdict {
    /** Exact authority grant. */
    public data class Approve(public val grant: InvocationGrant) : AuthorizationVerdict
    /** Stable safe denial. */
    public data class Deny(public val code: String, public val details: String?) : AuthorizationVerdict
    /** Original invocation terminates without authority. */
    public data object ReplacementRequired : AuthorizationVerdict
    /** Interaction must suspend the current Execution. */
    public data class InteractionRequired(public val request: InteractionRequest) : AuthorizationVerdict
}

/** Suspension terminal for the current Execution. */
public sealed interface SuspensionRequirement {
    /** Committed interaction awaits continuation. */
    public data class Interaction(public val request: InteractionRequest) : SuspensionRequirement
    /** Uncertain effect requires operator reconciliation. */
    public data class OperatorReconciliation(public val evidence: String) : SuspensionRequirement
}

/** Next action after one committed portable input. */
public sealed interface GovernedAction {
    /** Ask Runtime authorization again. */
    public data object Authorize : GovernedAction
    /** Ask Runtime execution for the exact grant. */
    public data class Dispatch(public val grant: InvocationGrant) : GovernedAction
    /** Return safe model-visible feedback. */
    public data class Observation(public val observation: GovernedObservation) : GovernedAction
    /** Suspend the current Execution. */
    public data class Suspend(public val requirement: SuspensionRequirement) : GovernedAction
    /** Fail closed. */
    public data class Fail(public val code: GovernedFailureCode) : GovernedAction
    /** Idempotent duplicate produced no action. */
    public data object None : GovernedAction
}

private sealed interface ReducerState {
    data object Prepared : ReducerState
    data class Awaiting(val request: InteractionRequest) : ReducerState
    data class Authorized(val grant: InvocationGrant) : ReducerState
    data class Started(val grant: InvocationGrant) : ReducerState
    data object Denied : ReducerState
    data object Replaced : ReducerState
    data object Completed : ReducerState
    data object Failed : ReducerState
    data object Uncertain : ReducerState
}

/** One sequential governed invocation reducer carrying no Runtime I/O. */
public class GovernedEffect private constructor(
    private val invocationId: ToolInvocationId,
    private val prepared: PreparedToolCall,
) {
    private var reducerState: ReducerState = ReducerState.Prepared
    private var lastInteractionResolution: InteractionResolution? = null

    /** Current portable lifecycle state. */
    public val state: EffectState
        get() = when (reducerState) {
            ReducerState.Prepared -> EffectState.PREPARED
            is ReducerState.Awaiting -> EffectState.AWAITING_INTERACTION
            is ReducerState.Authorized -> EffectState.AUTHORIZED
            is ReducerState.Started -> EffectState.STARTED
            ReducerState.Denied -> EffectState.DENIED
            ReducerState.Replaced -> EffectState.REPLACED
            ReducerState.Completed -> EffectState.COMPLETED
            ReducerState.Failed -> EffectState.FAILED
            ReducerState.Uncertain -> EffectState.UNCERTAIN
        }

    /** Reduces one durably committed authorization verdict. */
    public fun applyAuthorization(verdict: AuthorizationVerdict): GovernedAction {
        val current = reducerState
        if (current is ReducerState.Authorized && verdict is AuthorizationVerdict.Approve) {
            return if (current.grant == verdict.grant) GovernedAction.None else fail(GovernedFailureCode.INVOCATION_CONFLICT)
        }
        if (current != ReducerState.Prepared) return fail(GovernedFailureCode.INVOCATION_CONFLICT)
        return when (verdict) {
            is AuthorizationVerdict.Approve -> {
                if (!grantBinds(verdict.grant)) return fail(GovernedFailureCode.GRANT_MISMATCH)
                reducerState = ReducerState.Authorized(verdict.grant)
                GovernedAction.Dispatch(verdict.grant)
            }
            is AuthorizationVerdict.Deny -> {
                if (verdict.code.isEmpty()) return fail(GovernedFailureCode.INVOCATION_CONFLICT)
                reducerState = ReducerState.Denied
                GovernedAction.Observation(observation(ObservationOutcome.Rejected(verdict.code, verdict.details)))
            }
            AuthorizationVerdict.ReplacementRequired -> {
                reducerState = ReducerState.Replaced
                GovernedAction.Observation(observation(ObservationOutcome.Rejected("replacement_required", null)))
            }
            is AuthorizationVerdict.InteractionRequired -> {
                val request = verdict.request
                if (request.preparedDigest.isEmpty() || request.expiryPolicy.isEmpty() ||
                    PortableSchema.validateValueDefinition(request.responseSchema) != null ||
                    request.invocationId != invocationId || request.preparedDigest != prepared.inputDigest
                ) return fail(GovernedFailureCode.INTERACTION_CONFLICT)
                reducerState = ReducerState.Awaiting(request)
                lastInteractionResolution = null
                GovernedAction.Suspend(SuspensionRequirement.Interaction(request))
            }
        }
    }

    /** Reduces committed interaction continuation without inventing authority. */
    public fun applyInteraction(resolution: InteractionResolution): GovernedAction {
        lastInteractionResolution?.let { existing ->
            return if (existing == resolution) GovernedAction.None else fail(GovernedFailureCode.INTERACTION_CONFLICT)
        }
        val request = (reducerState as? ReducerState.Awaiting)?.request
            ?: return fail(GovernedFailureCode.INTERACTION_CONFLICT)
        val matches = when (resolution) {
            is InteractionResolution.Resolved -> resolution.interactionId == request.interactionId && resolution.invocationId == request.invocationId && resolution.preparedDigest == request.preparedDigest && PortableSchema.validateArguments(request.responseSchema, resolution.response).isEmpty()
            is InteractionResolution.Cancelled -> resolution.interactionId == request.interactionId && resolution.invocationId == request.invocationId && resolution.preparedDigest == request.preparedDigest
        }
        if (!matches) return fail(GovernedFailureCode.INTERACTION_CONFLICT)
        return when (resolution) {
            is InteractionResolution.Resolved -> { lastInteractionResolution = resolution; reducerState = ReducerState.Prepared; GovernedAction.Authorize }
            is InteractionResolution.Cancelled -> { lastInteractionResolution = resolution; reducerState = ReducerState.Denied; GovernedAction.Observation(observation(ObservationOutcome.Rejected("interaction_cancelled", null))) }
        }
    }

    /** Reduces one Runtime execution fact after its durable commit. */
    public fun applyExecution(fact: ExecutionFact): GovernedAction {
        val current = reducerState
        return when {
            current is ReducerState.Authorized && fact is ExecutionFact.Started -> {
                reducerState = ReducerState.Started(current.grant); GovernedAction.None
            }
            current is ReducerState.Authorized && fact is ExecutionFact.Unsupported -> {
                if (fact.requirement.isEmpty()) fail(GovernedFailureCode.CORRUPT_RECOVERY_STATE) else fail(GovernedFailureCode.REQUIREMENT_UNSUPPORTED)
            }
            current is ReducerState.Started && fact is ExecutionFact.Completed -> {
                val receipt = fact.receipt ?: return fail(GovernedFailureCode.CORRUPT_RECOVERY_STATE)
                if (!receiptBinds(receipt, current.grant, TerminalClassification.COMPLETED) || fact.content.toString().encodeToByteArray().size.toLong() > current.grant.grantedRequirements.maxOutputBytes) return fail(GovernedFailureCode.CORRUPT_RECOVERY_STATE)
                reducerState = ReducerState.Completed
                GovernedAction.Observation(observation(ObservationOutcome.Succeeded(fact.content, fact.truncated)))
            }
            current is ReducerState.Started && fact is ExecutionFact.Failed -> {
                val receipt = fact.receipt ?: return fail(GovernedFailureCode.CORRUPT_RECOVERY_STATE)
                if (fact.code.isEmpty() || !receiptBinds(receipt, current.grant, TerminalClassification.FAILED) || fact.partial?.toString()?.encodeToByteArray()?.size?.toLong()?.let { it > current.grant.grantedRequirements.maxOutputBytes } == true) return fail(GovernedFailureCode.CORRUPT_RECOVERY_STATE)
                reducerState = ReducerState.Failed
                GovernedAction.Observation(observation(ObservationOutcome.Failed(fact.code, fact.details, fact.partial)))
            }
            current is ReducerState.Started && fact is ExecutionFact.Uncertain -> {
                if (fact.evidence.isEmpty()) return fail(GovernedFailureCode.CORRUPT_RECOVERY_STATE)
                reducerState = ReducerState.Uncertain
                GovernedAction.Suspend(SuspensionRequirement.OperatorReconciliation(fact.evidence))
            }
            else -> fail(GovernedFailureCode.INVOCATION_CONFLICT)
        }
    }

    private fun grantBinds(grant: InvocationGrant): Boolean =
        grant.constraintsDigest.isNotEmpty() && grant.authorityRevision.isNotEmpty() && grant.invocationId == invocationId && grant.preparedDigest == prepared.inputDigest && grant.toolName == prepared.toolName && grant.toolRevision == prepared.toolRevision && grant.grantedRequirements.maxDurationMs <= prepared.requirements.maxDurationMs && grant.grantedRequirements.maxOutputBytes <= prepared.requirements.maxOutputBytes && prepared.requirements.capabilities.containsAll(grant.grantedRequirements.capabilities)

    private fun receiptBinds(receipt: EffectReceipt, grant: InvocationGrant, terminal: TerminalClassification): Boolean =
        receipt.preparedDigest.isNotEmpty() && receipt.executorId.isNotEmpty() && receipt.executorRevision.isNotEmpty() && receipt.resultDigest.isNotEmpty() && receipt.invocationId == invocationId && receipt.preparedDigest == prepared.inputDigest && receipt.grantId == grant.grantId && receipt.terminalClassification == terminal

    private fun observation(outcome: ObservationOutcome): GovernedObservation =
        GovernedObservation(invocationId, prepared.inputDigest, prepared.modelCallId, prepared.toolName, outcome)

    private fun fail(code: GovernedFailureCode): GovernedAction {
        reducerState = ReducerState.Failed
        return GovernedAction.Fail(code)
    }

    public companion object {
        /** Starts at Prepared and asks Runtime for authorization. */
        public fun start(invocationId: ToolInvocationId, prepared: PreparedToolCall): Pair<GovernedEffect, GovernedAction> =
            GovernedEffect(invocationId, prepared) to GovernedAction.Authorize
    }
}

/** Selects recovery without treating a replay declaration as executor proof. */
public fun recoverEffect(position: RecoveryPosition, replayClass: ReplayClass, executorProvesReplay: Boolean): RecoveryDecision =
    when (position) {
        RecoveryPosition.AUTHORIZED -> RecoveryDecision.REVALIDATE_GRANT
        RecoveryPosition.RECEIPT_NO_RESULT -> RecoveryDecision.RECONSTRUCT_FROM_RECEIPT
        RecoveryPosition.TERMINAL -> RecoveryDecision.RETURN_TERMINAL
        RecoveryPosition.STARTED_NO_RECEIPT -> when {
            replayClass == ReplayClass.RECEIPT_RECOVERABLE && executorProvesReplay -> RecoveryDecision.RECOVER_EXECUTOR_RECEIPT
            replayClass in setOf(ReplayClass.READ_ONLY, ReplayClass.IDEMPOTENT) && executorProvesReplay -> RecoveryDecision.RETRY_SAME_INVOCATION
            else -> RecoveryDecision.RECONCILE_OPERATOR
        }
    }
