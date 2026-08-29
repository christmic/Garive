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
import com.garive.eng.kt.llm.ToolDescriptor
import com.garive.eng.kt.tools.GovernedFailureCode
import com.garive.eng.kt.tools.GovernedToolResult
import com.garive.eng.kt.tools.InteractionKind
import com.garive.eng.kt.tools.SuspensionRequirement
import com.garive.eng.kt.tools.ToolCatalog
import com.garive.eng.kt.tools.ToolContractResult
import com.garive.eng.kt.tools.ToolDefinition
import com.garive.eng.kt.tools.ToolIntent

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
): ExecutionReport = executeKernel(request, ports, emptyList(), null)

internal suspend fun executeKernel(
    request: AgentTurnRequest,
    ports: AgentExecutionPorts,
    definitions: List<ToolDefinition>,
    effects: GovernedEffectPort?,
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
    var requestOrdinal = 0u
    var throughPosition = request.contextRequest.throughPosition
    val catalog = when (val value = ToolCatalog.create(definitions)) {
        is ToolContractResult.Success -> value.value
        is ToolContractResult.Failure -> return invalidReport(request)
    }
    val descriptors = definitions.map { definition ->
        ToolDescriptor(
            definition.name,
            definition.description,
            definition.revision,
            definition.inputSchema.toString(),
            true,
        )
    }

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
        val contextRequest = request.contextRequest.copy(throughPosition = throughPosition)
        val base = when (val context = ports.context.readCandidates(contextRequest, rebuildAttempt)) {
            is ContextPortResult.Success -> context.candidates
            is ContextPortResult.Failure -> {
                return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
            }
        }
        val merged = when (val value = mergeContextCandidates(base, request.capabilityContextCandidates)) {
            is ContextMergeResult.Success -> value.candidates
            is ContextMergeResult.Failure -> {
                return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
            }
        }
        val surface = when (val value = deriveContext(contextRequest, merged)) {
            is ContextDerivationResult.Success -> value.surface
            is ContextDerivationResult.Failure -> when (value.error) {
                is ContextDerivationError.RequiredFactsExceedBudget ->
                    return finish(request, ports, control, usage, AgentOutcome.Stopped(StopReason.TOKEN_LIMIT))
                else -> return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
            }
        }
        if (!emit(request, ports, AgentEventKind.ContextDerived(surface.itemCount, surface.utf8Bytes))) {
            return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
        }
        if (ports.cancellation.isCancelled()) {
            return finish(request, ports, control, usage, AgentOutcome.Stopped(StopReason.CANCELLED))
        }
        val target = request.modelTargets[targetIndex]
        if (requestOrdinal == UInt.MAX_VALUE) {
            return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.INVARIANT_VIOLATION))
        }
        requestOrdinal += 1u
        val requestId = "${request.executionId.value}:$iteration:$requestOrdinal"
        val inputItems = assembleModelInputs(surface)
        val modelRequest = ModelRequest(
            ModelRequestId(requestId),
            target,
            request.requiredCapabilities,
            inputItems,
            descriptors,
            request.modelOutput,
            listOf("turn_id" to request.turnId.value, "execution_id" to request.executionId.value),
        )
        if (modelRequest.validate() != null) {
            return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.INVALID_INPUT))
        }
        ports.model.preflight(modelRequest)?.let { failure ->
            return finish(request, ports, control, usage, AgentOutcome.Failed(modelFailure(failure)))
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
                return finish(request, ports, control, usage, AgentOutcome.Failed(modelFailure(result.failure)))
            }
        }

        when (outcome) {
            is InvokeOutcome.Completed -> {
                accountOrLimit(usage, outcome.usage, request)?.let {
                    return finish(request, ports, control, usage, it)
                }
                if (outcome.items.any { it is ModelItem.ToolObservation }) {
                    return finish(request, ports, control, usage, AgentOutcome.Failed(AgentFailureReason.INVALID_MODEL_OUTPUT))
                }
                if (outcome.items.any { it is ModelItem.ToolIntent }) {
                    if (effects == null) {
                        return finish(
                            request,
                            ports,
                            control,
                            usage,
                            AgentOutcome.Failed(AgentFailureReason.REQUIRED_CAPABILITY_UNAVAILABLE),
                        )
                    }
                    when (val step = governToolIntents(
                        outcome.items, catalog, effects, ports, requestId, throughPosition,
                    )) {
                        is ToolStep.Continue -> {
                            throughPosition = step.position
                            continue
                        }
                        is ToolStep.Terminal -> return finish(request, ports, control, usage, step.outcome)
                    }
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
                        return finishRecovery(
                            request, ports, control, usage, request.recoveryPolicy.transport, throughPosition,
                        )
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
                                    throughPosition,
                                    null,
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
                                throughPosition,
                                null,
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
                return finishRecovery(
                    request, ports, control, usage, request.recoveryPolicy.unavailable, throughPosition,
                )
            }
        }
    }
}

private fun modelFailure(failure: ModelPortFailure): AgentFailureReason = when (failure) {
    ModelPortFailure.INVALID_REQUEST -> AgentFailureReason.INVALID_INPUT
    ModelPortFailure.UNSUPPORTED_CAPABILITY -> AgentFailureReason.REQUIRED_CAPABILITY_UNAVAILABLE
    ModelPortFailure.ADAPTER_INVARIANT -> AgentFailureReason.INVALID_MODEL_OUTPUT
    ModelPortFailure.REQUIRED_PORT_FAILURE -> AgentFailureReason.PORT_FAILURE
}

private sealed interface ToolStep {
    data class Continue(val position: ULong) : ToolStep
    data class Terminal(val outcome: AgentOutcome) : ToolStep
}

private suspend fun governToolIntents(
    items: List<ModelItem>,
    catalog: ToolCatalog,
    effects: GovernedEffectPort,
    ports: AgentExecutionPorts,
    sourceModelRequestId: String,
    initialPosition: ULong,
): ToolStep {
    var position = initialPosition
    for (item in items) {
        if (item !is ModelItem.ToolIntent) continue
        if (ports.cancellation.isCancelled()) return ToolStep.Terminal(AgentOutcome.Stopped(StopReason.CANCELLED))
        val intent = ToolIntent(item.modelCallId, item.toolName, item.argumentsJson)
        val committed = when (val prepared = catalog.prepare(intent)) {
            is ToolContractResult.Success -> effects.invoke(sourceModelRequestId, prepared.value)
            is ToolContractResult.Failure -> effects.reject(sourceModelRequestId, intent, prepared.error)
        }.getOrElse {
            return ToolStep.Terminal(AgentOutcome.Failed(AgentFailureReason.PORT_FAILURE))
        }
        if (committed.throughPosition < position) {
            return ToolStep.Terminal(AgentOutcome.Failed(AgentFailureReason.INVARIANT_VIOLATION))
        }
        position = committed.throughPosition
        when (val result = committed.result) {
            is GovernedToolResult.Observation -> Unit
            is GovernedToolResult.Suspend -> {
                val binding = committed.suspensionBinding
                    ?: return ToolStep.Terminal(AgentOutcome.Failed(AgentFailureReason.INVARIANT_VIOLATION))
                if (!suspensionBinds(result.requirement, binding)) {
                    return ToolStep.Terminal(AgentOutcome.Failed(AgentFailureReason.INVARIANT_VIOLATION))
                }
                val reason = when (val requirement = result.requirement) {
                    is SuspensionRequirement.Interaction -> when (requirement.request.kind) {
                        InteractionKind.APPROVAL -> SuspensionReason.APPROVAL_REQUIRED
                        InteractionKind.EXTERNAL_INPUT -> SuspensionReason.EXTERNAL_INPUT_REQUIRED
                    }
                    is SuspensionRequirement.OperatorReconciliation -> SuspensionReason.OPERATOR_RECONCILIATION
                }
                return ToolStep.Terminal(AgentOutcome.Suspended(reason, items, position, binding))
            }
            is GovernedToolResult.Fail -> {
                val reason = if (result.code == GovernedFailureCode.INVALID_MODEL_OUTPUT) {
                    AgentFailureReason.INVALID_MODEL_OUTPUT
                } else {
                    AgentFailureReason.INVARIANT_VIOLATION
                }
                return ToolStep.Terminal(AgentOutcome.Failed(reason))
            }
        }
    }
    return ToolStep.Continue(position)
}

private fun suspensionBinds(
    requirement: SuspensionRequirement,
    binding: GovernedSuspensionBinding,
): Boolean = when {
    requirement is SuspensionRequirement.Interaction && binding is GovernedSuspensionBinding.Interaction ->
        binding.suspensionId.isNotEmpty() &&
            binding.interactionId == requirement.request.interactionId.value &&
            binding.invocationId == requirement.request.invocationId.value &&
            binding.preparedDigest == requirement.request.preparedDigest
    requirement is SuspensionRequirement.OperatorReconciliation &&
        binding is GovernedSuspensionBinding.OperatorReconciliation ->
        binding.suspensionId.isNotEmpty() && binding.invocationId.isNotEmpty() && binding.preparedDigest.isNotEmpty()
    else -> false
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
    throughPosition: ULong,
): ExecutionReport = finish(
    request,
    ports,
    control,
    usage,
    when (action) {
        TerminalRecoveryAction.SUSPEND, TerminalRecoveryAction.ALTERNATE_THEN_SUSPEND -> AgentOutcome.Suspended(
            SuspensionReason.RESOURCE_UNAVAILABLE, emptyList(), throughPosition, null,
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
