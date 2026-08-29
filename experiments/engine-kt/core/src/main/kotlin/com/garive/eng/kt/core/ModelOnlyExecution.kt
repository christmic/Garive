package com.garive.eng.kt.core

import com.garive.eng.kt.llm.InterruptionKind
import com.garive.eng.kt.llm.InvokeOutcome
import com.garive.eng.kt.llm.ModelItem
import com.garive.eng.kt.llm.ModelObserver
import com.garive.eng.kt.llm.ModelPortFailure
import com.garive.eng.kt.llm.ModelPortResult
import com.garive.eng.kt.llm.ModelRequest
import com.garive.eng.kt.llm.ModelRequestId
import com.garive.eng.kt.llm.ObserverDecision
import com.garive.eng.kt.llm.RejectionKind
import com.garive.eng.kt.llm.TokenCount

/**
 * Runs one bounded model-only kernel Execution against frozen ports.
 *
 * The driver validates immutable input, checks cancellation and limits at
 * defined boundaries, and returns exactly one terminal proposal. It neither
 * persists state nor executes tool intents.
 */
public suspend fun executeModelOnly(
    request: AgentTurnRequest,
    ports: AgentExecutionPorts,
): ExecutionReport {
    if (request.validate() != null) return invalidReport(request)
    val control = try {
        ExecutionControl.create(
            request.turnId,
            request.executionId,
            request.cursor.completedIterations,
            request.limits.execution,
        )
    } catch (_: ControlException) {
        return invalidReport(request)
    }
    val usage = UsageAccumulator()
    var rebuildAttempt = 0u
    var outputRetries = 0u
    var targetIndex = 0

    if (!emit(request, ports, AgentEventKind.ExecutionStarted)) {
        return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
    }
    while (true) {
        if (ports.cancellation.isCancelled()) {
            return finish(request, ports, control, usage, AgentOutcome.Stopped(StopReason.CANCELLED))
        }
        when (deadlineReached(request, ports)) {
            DeadlineResult.REACHED -> return finish(request, ports, control, usage, AgentOutcome.Stopped(StopReason.DEADLINE))
            DeadlineResult.FAILURE -> return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
            DeadlineResult.OPEN -> Unit
        }
        val iteration = when (val begin = control.beginIteration()) {
            BeginIteration.IterationLimitReached -> {
                return finish(request, ports, control, usage, AgentOutcome.Stopped(StopReason.ITERATION_LIMIT))
            }
            is BeginIteration.Started -> begin.iteration
        }
        if (!emit(request, ports, AgentEventKind.IterationStarted(iteration))) {
            return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
        }
        val surface = when (val context = ports.context.derive(request.contextRequest, rebuildAttempt)) {
            is ContextPortResult.Success -> context.surface
            ContextPortResult.RequiredFactsExceedBudget -> {
                return finish(request, ports, control, usage, AgentOutcome.Stopped(StopReason.TOKEN_LIMIT))
            }
            is ContextPortResult.Failure -> {
                return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
            }
        }
        if (!emit(request, ports, AgentEventKind.ContextDerived(surface.itemCount, surface.utf8Bytes))) {
            return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
        }
        if (ports.cancellation.isCancelled()) {
            return finish(request, ports, control, usage, AgentOutcome.Stopped(StopReason.CANCELLED))
        }
        val target = request.modelTargets[targetIndex]
        val requestId = "${request.executionId.value}:$iteration"
        val modelRequest = ModelRequest(
            ModelRequestId(requestId),
            target,
            request.requiredCapabilities,
            surface.items.mapNotNull { (it as? ContextItem.Input)?.item },
            emptyList(),
            request.modelOutput,
            listOf("turn_id" to request.turnId.value, "execution_id" to request.executionId.value),
        )
        if (modelRequest.validate() != null) {
            return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.INVALID_INPUT))
        }
        if (!emit(request, ports, AgentEventKind.ModelRequestPrepared(requestId, target.value))) {
            return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
        }
        var observerFailure = false
        val observer = ModelObserver { event ->
            when {
                ports.cancellation.isCancelled() -> ObserverDecision.CANCEL
                !emit(request, ports, AgentEventKind.ModelStream(event)) -> {
                    observerFailure = true
                    ObserverDecision.CANCEL
                }
                else -> ObserverDecision.CONTINUE
            }
        }
        val result = ports.model.invoke(modelRequest, observer, ports.cancellation)
        if (observerFailure) {
            return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
        }
        val outcome = when (result) {
            is ModelPortResult.Success -> result.outcome
            is ModelPortResult.Failure -> {
                val reason = when (result.failure) {
                    ModelPortFailure.INVALID_REQUEST -> AgentFailureReason.INVALID_INPUT
                    ModelPortFailure.UNSUPPORTED_CAPABILITY -> AgentFailureReason.REQUIRED_CAPABILITY_UNAVAILABLE
                    ModelPortFailure.ADAPTER_INVARIANT -> AgentFailureReason.INVALID_MODEL_OUTPUT
                    ModelPortFailure.REQUIRED_PORT_FAILURE -> AgentFailureReason.PORT_FAILURE
                }
                return finish(request, ports, control, usage, AgentOutcome.Failed(reason))
            }
        }

        when (outcome) {
            is InvokeOutcome.Completed -> {
                accountOrLimit(usage, outcome.usage, request)?.let {
                    return finish(request, ports, control, usage, it)
                }
                if (outcome.items.any { it is ModelItem.ToolIntent }) {
                    return finish(
                        request,
                        ports,
                        control,
                        usage,
                        AgentOutcome.Failed(AgentFailureReason.REQUIRED_CAPABILITY_UNAVAILABLE),
                    )
                }
                if (outcome.items.any { it is ModelItem.ToolObservation }) {
                    return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.INVALID_MODEL_OUTPUT))
                }
                return finish(request, ports, control, usage, AgentOutcome.Completed(outcome.items, usage.summary()))
            }
            is InvokeOutcome.Rejected -> {
                if (outcome.reason == RejectionKind.CONTEXT_OVERFLOW &&
                    rebuildAttempt < request.recoveryPolicy.maxContextRebuilds
                ) {
                    rebuildAttempt += 1u
                    continue
                }
                return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.INVALID_MODEL_OUTPUT))
            }
            is InvokeOutcome.Interrupted -> {
                accountOrLimit(usage, outcome.usage, request)?.let {
                    return finish(request, ports, control, usage, it)
                }
                when (outcome.reason) {
                    InterruptionKind.CANCELLED -> {
                        return finish(request, ports, control, usage, AgentOutcome.Stopped(StopReason.CANCELLED))
                    }
                    InterruptionKind.TRANSPORT -> {
                        return finishRecovery(request, ports, control, usage, request.recoveryPolicy.transport)
                    }
                    InterruptionKind.OUTPUT_LIMIT -> when (val action = request.recoveryPolicy.outputLimit) {
                        OutputLimitAction.CompletePartial -> {
                            if (outcome.partialItems.any { it is ModelItem.ToolIntent }) {
                                return finish(
                                    request,
                                    ports,
                                    control,
                                    usage,
                                    AgentOutcome.Failed(AgentFailureReason.REQUIRED_CAPABILITY_UNAVAILABLE),
                                )
                            }
                            return finish(
                                request,
                                ports,
                                control,
                                usage,
                                AgentOutcome.Completed(outcome.partialItems, usage.summary()),
                            )
                        }
                        is OutputLimitAction.Retry -> if (outputRetries < action.maxRetries) {
                            outputRetries += 1u
                            continue
                        } else {
                            return finish(
                                request,
                                ports,
                                control,
                                usage,
                                AgentOutcome.Suspended(
                                    SuspensionReason.PARTIAL_OUTPUT,
                                    outcome.partialItems,
                                    request.cursor.lastDurablePosition,
                                ),
                            )
                        }
                        OutputLimitAction.Suspend -> return finish(
                            request,
                            ports,
                            control,
                            usage,
                            AgentOutcome.Suspended(
                                SuspensionReason.PARTIAL_OUTPUT,
                                outcome.partialItems,
                                request.cursor.lastDurablePosition,
                            ),
                        )
                        OutputLimitAction.Stop -> return finish(
                            request, ports, control, usage, AgentOutcome.Stopped(StopReason.TOKEN_LIMIT),
                        )
                        OutputLimitAction.Fail -> return finish(
                            request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.INVALID_MODEL_OUTPUT),
                        )
                    }
                }
            }
            is InvokeOutcome.Unavailable -> {
                if (request.recoveryPolicy.unavailable == TerminalRecoveryAction.ALTERNATE_THEN_SUSPEND &&
                    targetIndex + 1 < request.modelTargets.size
                ) {
                    targetIndex += 1
                    continue
                }
                return finishRecovery(request, ports, control, usage, request.recoveryPolicy.unavailable)
            }
        }
    }
}

private class UsageAccumulator {
    var input: TokenCount = TokenCount.Known(0uL)
    var output: TokenCount = TokenCount.Known(0uL)
    var estimated = false

    fun add(value: com.garive.eng.kt.llm.ModelUsage, policy: MissingUsagePolicy): Boolean {
        val nextInput = accumulate(input, value.inputTokens, policy, true) ?: return false
        val nextOutput = accumulate(output, value.outputTokens, policy, false) ?: return false
        input = nextInput.count
        output = nextOutput.count
        estimated = estimated || nextInput.estimated || nextOutput.estimated
        return !nextInput.missing && !nextOutput.missing
    }

    fun total(): ULong? {
        val knownInput = (input as? TokenCount.Known)?.value ?: return null
        val knownOutput = (output as? TokenCount.Known)?.value ?: return null
        return if (ULong.MAX_VALUE - knownInput < knownOutput) null else knownInput + knownOutput
    }
    fun summary() = UsageSummary(input, output, estimated)
}

private data class AccumulatedCount(val count: TokenCount, val estimated: Boolean, val missing: Boolean)

private fun accumulate(
    current: TokenCount,
    next: TokenCount,
    policy: MissingUsagePolicy,
    input: Boolean,
): AccumulatedCount? = when (current) {
    TokenCount.Unknown -> AccumulatedCount(TokenCount.Unknown, false, next == TokenCount.Unknown)
    is TokenCount.Known -> when (next) {
        is TokenCount.Known -> {
            if (ULong.MAX_VALUE - current.value < next.value) null else {
                AccumulatedCount(TokenCount.Known(current.value + next.value), false, false)
            }
        }
        TokenCount.Unknown -> when (policy) {
            MissingUsagePolicy.Stop -> AccumulatedCount(TokenCount.Unknown, false, true)
            is MissingUsagePolicy.Estimate -> {
                val estimate = if (input) policy.inputTokens else policy.outputTokens
                if (ULong.MAX_VALUE - current.value < estimate) null else {
                    AccumulatedCount(TokenCount.Known(current.value + estimate), true, false)
                }
            }
        }
    }
}

private fun accountOrLimit(
    usage: UsageAccumulator,
    value: com.garive.eng.kt.llm.ModelUsage,
    request: AgentTurnRequest,
): AgentOutcome? {
    if (!usage.add(value, request.recoveryPolicy.missingUsage)) {
        return AgentOutcome.Stopped(StopReason.TOKEN_LIMIT)
    }
    val total = usage.total() ?: return AgentOutcome.Failed(AgentFailureReason.INVARIANT_VIOLATION)
    return if (request.limits.maxTotalTokens?.let { total > it } == true) {
        AgentOutcome.Stopped(StopReason.TOKEN_LIMIT)
    } else null
}

private fun invalidReport(request: AgentTurnRequest) = ExecutionReport(
    AgentOutcome.Failed(AgentFailureReason.INVALID_INPUT),
    request.cursor.completedIterations,
    UsageAccumulator().summary(),
)

private enum class DeadlineResult { OPEN, REACHED, FAILURE }

private fun deadlineReached(request: AgentTurnRequest, ports: AgentExecutionPorts): DeadlineResult {
    val deadline = request.limits.deadlineTick ?: return DeadlineResult.OPEN
    val now = ports.clock.nowTick().getOrElse { return DeadlineResult.FAILURE }
    return if (now >= deadline) DeadlineResult.REACHED else DeadlineResult.OPEN
}

private fun emit(request: AgentTurnRequest, ports: AgentExecutionPorts, kind: AgentEventKind): Boolean =
    ports.events.emit(AgentEvent(request.sessionId, request.turnId, request.executionId, kind)) == null

private fun finishRecovery(
    request: AgentTurnRequest,
    ports: AgentExecutionPorts,
    control: ExecutionControl,
    usage: UsageAccumulator,
    action: TerminalRecoveryAction,
): ExecutionReport = finish(
    request,
    ports,
    control,
    usage,
    when (action) {
        TerminalRecoveryAction.SUSPEND, TerminalRecoveryAction.ALTERNATE_THEN_SUSPEND -> AgentOutcome.Suspended(
            SuspensionReason.RESOURCE_UNAVAILABLE, emptyList(), request.cursor.lastDurablePosition,
        )
        TerminalRecoveryAction.STOP -> AgentOutcome.Stopped(StopReason.RESOURCE_UNAVAILABLE)
        TerminalRecoveryAction.FAIL -> AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE)
    },
)

private fun finish(
    request: AgentTurnRequest,
    ports: AgentExecutionPorts,
    control: ExecutionControl,
    usage: UsageAccumulator,
    proposed: AgentOutcome,
): ExecutionReport {
    var outcome = proposed
    if (!emit(request, ports, AgentEventKind.OutcomeProposed)) {
        outcome = AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE)
    }
    if (control.status is ExecutionStatus.Active) {
        val kind = when (outcome) {
            is AgentOutcome.Completed -> ExecutionOutcomeKind.COMPLETED
            is AgentOutcome.Suspended -> ExecutionOutcomeKind.SUSPENDED
            is AgentOutcome.Stopped -> ExecutionOutcomeKind.STOPPED
            is AgentOutcome.Failed -> ExecutionOutcomeKind.FAILED
        }
        try {
            control.close(kind)
        } catch (_: ControlException) {
            outcome = AgentOutcome.Failed(AgentFailureReason.INVARIANT_VIOLATION)
        }
    }
    return ExecutionReport(outcome, control.completedIterations, usage.summary())
}
