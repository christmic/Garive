package com.garive.eng.kt.ledger

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

internal enum class TurnState { OPEN, SUSPENDED, COMPLETED, STOPPED, FAILED }
internal enum class ExecutionState { ACTIVE, ABANDONED, COMPLETED, SUSPENDED, STOPPED, FAILED }
internal enum class InvocationState {
    PREPARED, AUTHORIZED, STARTED, RECEIPT, COMPLETED, REJECTED, INTERRUPTED, UNAVAILABLE,
    FAILED, DENIED, UNCERTAIN, RECONCILED, OBSERVED,
}

internal data class InteractionRecord(
    val execution: ExecutionId,
    val tool: ToolInvocationId,
    val suspensionId: String,
    val preparedDigest: String,
    var terminal: Boolean,
)

internal data class KnowledgeRecord(
    val execution: ExecutionId,
    val requestDigest: String,
    val dispatchAttempts: MutableSet<String>,
    var terminal: Boolean,
)

internal enum class ScheduleState { ACTIVE, SUPERSEDING, CANCELLED, FAILED, EXHAUSTED }

internal data class ScheduleClaim(
    val occurrenceId: String,
    val ordinal: ULong,
    val dueAtUtc: String,
    val leaseEpoch: ULong,
)

internal data class ScheduleRecord(
    val revisionId: String,
    val intentDigest: String,
    var lastHandledOrdinal: ULong,
    var pendingClaim: ScheduleClaim?,
    var state: ScheduleState,
)

internal enum class DelegationState { REQUESTED, AUTHORIZED, DENIED, STARTED, TERMINAL, OBSERVED }
internal data class DelegationRecord(
    val parentTurn: TurnId, val parentExecution: ExecutionId, val intentDigest: String,
    var grantId: String? = null, var suspensionId: String? = null, var childTurn: TurnId? = null,
    var resultId: String? = null, var resultDigest: String? = null,
    var inputAdmitted: Boolean = false, var state: DelegationState = DelegationState.REQUESTED,
)

internal class LedgerProjection(
    private var opened: Boolean = false,
    private var closed: Boolean = false,
    private val turns: MutableMap<TurnId, TurnState> = mutableMapOf(),
    private val executions: MutableMap<ExecutionId, Pair<TurnId, ExecutionState>> = mutableMapOf(),
    private val executionIterations: MutableMap<ExecutionId, ULong> = mutableMapOf(),
    private val models: MutableMap<ModelRequestId, Pair<ExecutionId, InvocationState>> = mutableMapOf(),
    private val modelDigests: MutableMap<ModelRequestId, String> = mutableMapOf(),
    private val tools: MutableMap<ToolInvocationId, Pair<ExecutionId, InvocationState>> = mutableMapOf(),
    private val toolDigests: MutableMap<ToolInvocationId, String> = mutableMapOf(),
    private val toolGrants: MutableMap<ToolInvocationId, String> = mutableMapOf(),
    private val toolReceipts: MutableMap<ToolInvocationId, String> = mutableMapOf(),
    private val toolExecutors: MutableMap<ToolInvocationId, Pair<String, String>> = mutableMapOf(),
    private val toolReconciledObservations: MutableMap<ToolInvocationId, kotlinx.serialization.json.JsonElement> = mutableMapOf(),
    private val suspensions: MutableMap<TurnId, String> = mutableMapOf(),
    private val interactions: MutableMap<String, InteractionRecord> = mutableMapOf(),
    private val knowledge: MutableMap<String, KnowledgeRecord> = mutableMapOf(),
    private val schedules: MutableMap<String, ScheduleRecord> = mutableMapOf(),
    private val turnAgents: MutableMap<TurnId, Pair<String, String>> = mutableMapOf(),
    private val delegations: MutableMap<String, DelegationRecord> = mutableMapOf(),
    private val turnsStartedInCommit: MutableSet<TurnId> = mutableSetOf(),
    private val turnsSuspendedInCommit: MutableSet<TurnId> = mutableSetOf(),
    private val turnsTerminalInCommit: MutableSet<TurnId> = mutableSetOf(),
) {
    fun copy() = LedgerProjection(
        opened,
        closed,
        turns.toMutableMap(),
        executions.toMutableMap(),
        executionIterations.toMutableMap(),
        models.toMutableMap(),
        modelDigests.toMutableMap(),
        tools.toMutableMap(),
        toolDigests.toMutableMap(),
        toolGrants.toMutableMap(),
        toolReceipts.toMutableMap(),
        toolExecutors.toMutableMap(),
        toolReconciledObservations.toMutableMap(),
        suspensions.toMutableMap(),
        interactions.mapValues { (_, value) -> value.copy() }.toMutableMap(),
        knowledge.mapValues { (_, value) -> value.copy(dispatchAttempts = value.dispatchAttempts.toMutableSet()) }.toMutableMap(),
        schedules.mapValues { (_, value) -> value.copy() }.toMutableMap(),
        turnAgents.toMutableMap(),
        delegations.mapValues { (_, value) -> value.copy() }.toMutableMap(),
        turnsStartedInCommit.toMutableSet(),
        turnsSuspendedInCommit.toMutableSet(),
        turnsTerminalInCommit.toMutableSet(),
    )

    fun apply(fact: FactDraft): LedgerError? {
        fact.validate()?.let { return it }
        val kind = fact.kind.value
        if (kind == "session.opened") {
            if (opened || closed) return LedgerError.InvalidTransition
            opened = true
            return null
        }
        if (!opened || closed) return LedgerError.InvalidTransition
        return when (kind) {
            "session.closed" -> closeSession()
            "turn.started" -> startTurn(fact)
            "turn.suspended" -> suspendTurn(fact)
            "turn.completed" -> terminalTurn(fact, TurnState.COMPLETED)
            "turn.stopped" -> terminalTurn(fact, TurnState.STOPPED)
            "turn.failed" -> terminalTurn(fact, TurnState.FAILED)
            "turn.input" -> admitTurnInput(fact)
            "turn.cancel_requested" -> requireNonTerminalTurn(fact.turnId)
            "execution.started" -> startExecution(fact)
            "execution.iteration_started" -> startIteration(fact)
            "execution.abandoned" -> transitionExecution(fact, ExecutionState.ABANDONED)
            "execution.completed" -> transitionExecution(fact, ExecutionState.COMPLETED)
            "execution.suspended" -> transitionExecution(fact, ExecutionState.SUSPENDED)
            "execution.stopped" -> transitionExecution(fact, ExecutionState.STOPPED)
            "execution.failed" -> transitionExecution(fact, ExecutionState.FAILED)
            "model.prepared" -> prepareModel(fact)
            "model.started" -> transitionModel(fact, InvocationState.STARTED)
            "model.completed" -> transitionModel(fact, InvocationState.COMPLETED)
            "model.rejected" -> transitionModel(fact, InvocationState.REJECTED)
            "model.interrupted" -> transitionModel(fact, InvocationState.INTERRUPTED)
            "model.unavailable" -> transitionModel(fact, InvocationState.UNAVAILABLE)
            "model.uncertain" -> transitionModel(fact, InvocationState.UNCERTAIN)
            "effect.prepared" -> prepareTool(fact)
            "effect.authorized" -> transitionTool(fact, InvocationState.AUTHORIZED)
            "effect.started" -> transitionTool(fact, InvocationState.STARTED)
            "effect.receipt" -> transitionTool(fact, InvocationState.RECEIPT)
            "effect.completed" -> transitionTool(fact, InvocationState.COMPLETED)
            "effect.failed" -> transitionTool(fact, InvocationState.FAILED)
            "effect.denied" -> transitionTool(fact, InvocationState.DENIED)
            "effect.uncertain" -> transitionTool(fact, InvocationState.UNCERTAIN)
            "effect.reconciled" -> reconcileTool(fact)
            "effect.observation" -> observeTool(fact)
            "tool.preparation_rejected" -> rejectToolPreparation(fact)
            "interaction.requested" -> requestInteraction(fact)
            "interaction.resolved", "interaction.cancelled" -> finishInteraction(fact)
            "knowledge.requested" -> requestKnowledge(fact)
            "knowledge.dispatched" -> dispatchKnowledge(fact)
            "knowledge.completed", "knowledge.failed" -> terminalKnowledge(fact)
            "schedule.created" -> createSchedule(fact)
            "schedule.claimed" -> claimSchedule(fact)
            "schedule.fired" -> fireSchedule(fact)
            "schedule.skipped" -> skipSchedule(fact)
            "schedule.cancelled" -> cancelSchedule(fact)
            "schedule.failed" -> failSchedule(fact)
            "schedule.exhausted" -> exhaustSchedule(fact)
            "delegation.requested" -> requestDelegation(fact)
            "delegation.authorized" -> authorizeDelegation(fact)
            "delegation.denied" -> denyDelegation(fact)
            "delegation.child_started" -> startDelegationChild(fact)
            "delegation.child_terminal" -> terminalDelegationChild(fact)
            "delegation.observed" -> observeDelegation(fact)
            else -> null
        }
    }

    fun beginCommit() {
        turnsStartedInCommit.clear(); turnsSuspendedInCommit.clear(); turnsTerminalInCommit.clear()
    }

    fun validateCommitBoundary(): LedgerError? =
        if (schedules.values.any { it.state == ScheduleState.SUPERSEDING } || delegations.values.any { record ->
                turnsSuspendedInCommit.contains(record.parentTurn) && record.state == DelegationState.AUTHORIZED ||
                    record.childTurn?.let(turnsTerminalInCommit::contains) == true && record.state == DelegationState.STARTED
            }
        ) {
            LedgerError.InvalidTransition
        } else {
            null
        }

    fun uncertainModelRequests() = models.entries
        .filter { it.value.second == InvocationState.STARTED }
        .map { it.key }
        .sortedBy { it.value }

    fun uncertainToolInvocations() = tools.entries
        .filter { it.value.second == InvocationState.STARTED }
        .map { it.key }
        .sortedBy { it.value }

    private fun closeSession(): LedgerError? {
        if (turns.values.any { it == TurnState.OPEN || it == TurnState.SUSPENDED } ||
            executions.values.any { it.second == ExecutionState.ACTIVE } ||
            hasRecoveryPendingInvocation()
            || schedules.values.any { it.state == ScheduleState.ACTIVE }
            || delegations.values.any { it.state !in setOf(DelegationState.DENIED, DelegationState.OBSERVED) }
        ) {
            return LedgerError.InvalidTransition
        }
        closed = true
        return null
    }

    private fun startTurn(fact: FactDraft): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        val payload = fact.payloadObject()
        val kind = payload.text("kind")
        val prior = payload["prior_suspension_id"]?.jsonPrimitive?.contentOrNull
        val valid = when (turns[turn]) {
            null -> kind == "start" && prior == null
            TurnState.SUSPENDED -> kind == "continue" && suspensions[turn] == prior &&
                !hasPendingInteractionForTurn(turn) && delegationContinuationReady(turn, prior)
            else -> false
        }
        if (!valid) return LedgerError.InvalidTransition
        turns[turn] = TurnState.OPEN
        if (kind == "start") {
            turnAgents[turn] = payload.text("agent_instance_id") to payload.text("snapshot_digest")
            turnsStartedInCommit += turn
        }
        suspensions.remove(turn)
        return null
    }

    private fun suspendTurn(fact: FactDraft): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        val payload = fact.payloadObject()
        val execution = ExecutionId.of(payload.text("execution_id"))
        if (executions[execution] != (turn to ExecutionState.SUSPENDED)) {
            return LedgerError.InvalidTransition
        }
        transitionTurn(turn, TurnState.SUSPENDED)?.let { return it }
        suspensions[turn] = payload.text("suspension_id")
        if (payload.text("reason") == "delegation_pending") turnsSuspendedInCommit += turn
        return null
    }

    private fun terminalTurn(fact: FactDraft, next: TurnState): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        val execution = ExecutionId.of(fact.payloadObject().text("execution_id"))
        val expected = when (next) {
            TurnState.COMPLETED -> ExecutionState.COMPLETED
            TurnState.STOPPED -> ExecutionState.STOPPED
            TurnState.FAILED -> ExecutionState.FAILED
            else -> return LedgerError.InvalidTransition
        }
        val actual = executions[execution]
        val suspendedClose = next in setOf(TurnState.STOPPED, TurnState.FAILED) &&
            turns[turn] == TurnState.SUSPENDED && actual == (turn to ExecutionState.SUSPENDED)
        if (actual != (turn to expected) && !suspendedClose) return LedgerError.InvalidTransition
        transitionTurn(turn, next)?.let { return it }
        turnsTerminalInCommit += turn
        return null
    }

    private fun requireOpenTurn(turnId: TurnId?): LedgerError? = when {
        turnId == null -> LedgerError.MissingReference
        turns[turnId] != TurnState.OPEN -> LedgerError.InvalidTransition
        else -> null
    }

    private fun admitTurnInput(fact: FactDraft): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        val payload = fact.payloadObject()
        if (payload.text("input_kind") == "delegation_result") {
            val suspension = payload.text("suspension_id")
            val digest = payload.getValue("content").jsonObject.text("digest")
            val record = delegations.values.singleOrNull {
                it.parentTurn == turn && it.state == DelegationState.OBSERVED &&
                    it.suspensionId == suspension && it.resultDigest == digest
            } ?: return LedgerError.InvalidTransition
            record.inputAdmitted = true
            return null
        }
        return if (payload.text("input_kind") !in setOf("trusted_user", "trusted_system")) {
            if (turns[turn] == TurnState.SUSPENDED &&
                suspensions[turn] == payload["suspension_id"]?.jsonPrimitive?.contentOrNull
            ) null else LedgerError.InvalidTransition
        } else {
            requireOpenTurn(turn)
        }
    }

    private fun requireNonTerminalTurn(turnId: TurnId?): LedgerError? = when {
        turnId == null -> LedgerError.MissingReference
        turns[turnId] == TurnState.OPEN || turns[turnId] == TurnState.SUSPENDED -> null
        turnId in turns -> LedgerError.InvalidTransition
        else -> LedgerError.MissingReference
    }

    private fun transitionTurn(turnId: TurnId?, next: TurnState): LedgerError? {
        if (turnId == null) return LedgerError.MissingReference
        val current = turns[turnId] ?: return LedgerError.MissingReference
        val valid = when (next) {
            TurnState.SUSPENDED, TurnState.COMPLETED -> current == TurnState.OPEN
            TurnState.STOPPED, TurnState.FAILED -> current == TurnState.OPEN || current == TurnState.SUSPENDED
            TurnState.OPEN -> false
        }
        if (!valid) return LedgerError.InvalidTransition
        if (executions.values.any { it.first == turnId && it.second == ExecutionState.ACTIVE }) {
            return LedgerError.InvalidTransition
        }
        if (next != TurnState.SUSPENDED && hasPendingInteractionForTurn(turnId)) {
            return LedgerError.InvalidTransition
        }
        turns[turnId] = next
        return null
    }

    private fun startExecution(fact: FactDraft): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        requireOpenTurn(turn)?.let { return it }
        val execution = fact.executionId ?: return LedgerError.MissingReference
        if (execution in executions) return LedgerError.InvalidTransition
        executions[execution] = turn to ExecutionState.ACTIVE
        executionIterations[execution] = fact.payloadObject().ulong("completed_iterations")
        return null
    }

    private fun startIteration(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val current = executionIterations[execution] ?: return LedgerError.MissingReference
        val iteration = fact.payloadObject().ulong("iteration")
        if (current == ULong.MAX_VALUE || current + 1uL != iteration) return LedgerError.InvalidTransition
        executionIterations[execution] = iteration
        return null
    }

    private fun transitionExecution(fact: FactDraft, next: ExecutionState): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val current = executions[execution] ?: return LedgerError.MissingReference
        if (current.first != turn || current.second != ExecutionState.ACTIVE) return LedgerError.InvalidTransition
        if (hasRecoveryPendingInvocation(execution) || hasPendingKnowledge(execution) ||
            (next != ExecutionState.SUSPENDED && hasPendingInteraction(execution))
        ) return LedgerError.InvalidTransition
        executions[execution] = turn to next
        return null
    }

    private fun prepareModel(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val request = fact.modelRequestId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        if (models.put(request, execution to InvocationState.PREPARED) != null) {
            return LedgerError.InvalidTransition
        }
        modelDigests[request] = fact.payloadObject().text("request_digest")
        return null
    }

    private fun transitionModel(fact: FactDraft, next: InvocationState): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val request = fact.modelRequestId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val current = models[request] ?: return LedgerError.MissingReference
        if (current.first != execution) return LedgerError.InvalidTransition
        if (modelDigests[request] != fact.payloadObject().text("request_digest")) {
            return LedgerError.InvalidTransition
        }
        val valid = (current.second == InvocationState.PREPARED && next == InvocationState.STARTED) ||
            (current.second == InvocationState.STARTED && next != InvocationState.PREPARED && next != InvocationState.STARTED)
        if (!valid) return LedgerError.InvalidTransition
        models[request] = execution to next
        return null
    }

    private fun prepareTool(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val tool = fact.toolInvocationId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        if (tools.put(tool, execution to InvocationState.PREPARED) != null) {
            return LedgerError.InvalidTransition
        }
        toolDigests[tool] = fact.payloadObject().text("prepared_digest")
        return null
    }

    private fun requestInteraction(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val tool = fact.toolInvocationId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val toolState = tools[tool] ?: return LedgerError.MissingReference
        if (toolState.first != execution || toolState.second !in setOf(
                InvocationState.PREPARED,
                InvocationState.AUTHORIZED,
            )
        ) return LedgerError.InvalidTransition
        val payload = fact.payloadObject()
        val interactionId = payload.text("interaction_id")
        val preparedDigest = payload.text("prepared_digest")
        if (toolDigests[tool] != preparedDigest || interactionId in interactions) {
            return LedgerError.InvalidTransition
        }
        interactions[interactionId] = InteractionRecord(
            execution,
            tool,
            payload.text("suspension_id"),
            preparedDigest,
            false,
        )
        return null
    }

    private fun finishInteraction(fact: FactDraft): LedgerError? {
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val tool = fact.toolInvocationId ?: return LedgerError.MissingReference
        val payload = fact.payloadObject()
        val interaction = interactions[payload.text("interaction_id")]
            ?: return LedgerError.MissingReference
        if (interaction.terminal || interaction.execution != execution || interaction.tool != tool ||
            interaction.suspensionId != payload.text("suspension_id") ||
            interaction.preparedDigest != payload.text("prepared_digest")
        ) return LedgerError.InvalidTransition
        interaction.terminal = true
        return null
    }

    private fun transitionTool(fact: FactDraft, next: InvocationState): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val tool = fact.toolInvocationId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val current = tools[tool] ?: return LedgerError.MissingReference
        if (current.first != execution) return LedgerError.InvalidTransition
        val payload = fact.payloadObject()
        if (toolDigests[tool] != payload.text("prepared_digest")) return LedgerError.InvalidTransition
        val valid = when (current.second to next) {
            InvocationState.PREPARED to InvocationState.AUTHORIZED,
            InvocationState.PREPARED to InvocationState.STARTED,
            InvocationState.PREPARED to InvocationState.DENIED,
            InvocationState.AUTHORIZED to InvocationState.STARTED,
            InvocationState.AUTHORIZED to InvocationState.DENIED,
            InvocationState.AUTHORIZED to InvocationState.FAILED,
            InvocationState.STARTED to InvocationState.RECEIPT,
            InvocationState.STARTED to InvocationState.FAILED,
            InvocationState.STARTED to InvocationState.UNCERTAIN,
            InvocationState.RECEIPT to InvocationState.COMPLETED,
            InvocationState.RECEIPT to InvocationState.FAILED,
            -> true
            else -> false
        }
        if (!valid) return LedgerError.InvalidTransition
        validateEffectBinding(tool, current.second, next, payload)?.let { return it }
        tools[tool] = execution to next
        return null
    }

    private fun validateEffectBinding(
        tool: ToolInvocationId,
        current: InvocationState,
        next: InvocationState,
        payload: JsonObject,
    ): LedgerError? {
        when (next) {
            InvocationState.AUTHORIZED -> toolGrants[tool] = payload.text("grant_id")
            InvocationState.STARTED -> {
                val grant = payload.text("grant_id")
                if (toolGrants[tool]?.let { it != grant } == true) return LedgerError.InvalidTransition
                toolGrants[tool] = grant
                toolExecutors[tool] = payload.text("executor_id") to payload.text("executor_revision")
            }
            InvocationState.RECEIPT -> {
                if (toolGrants[tool] != payload.text("grant_id") ||
                    toolExecutors[tool] != (payload.text("executor_id") to payload.text("executor_revision"))
                ) return LedgerError.InvalidTransition
                toolReceipts[tool] = payload.text("receipt_id")
            }
            InvocationState.COMPLETED -> {
                if (toolReceipts[tool] != payload.text("receipt_id")) return LedgerError.InvalidTransition
            }
            InvocationState.FAILED -> if (
                current == InvocationState.RECEIPT &&
                toolReceipts[tool] != payload["receipt_id"]?.jsonPrimitive?.contentOrNull
            ) return LedgerError.InvalidTransition
            else -> Unit
        }
        return null
    }

    private fun observeTool(fact: FactDraft): LedgerError? {
        val tool = fact.toolInvocationId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val current = tools[tool] ?: return LedgerError.MissingReference
        if (current.first != execution || current.second !in setOf(
                InvocationState.COMPLETED,
                InvocationState.FAILED,
                InvocationState.DENIED,
                InvocationState.RECONCILED,
            )
        ) {
            return LedgerError.InvalidTransition
        }
        val payload = fact.payloadObject()
        if (current.second == InvocationState.RECONCILED) {
            requireSuspendedExecution(fact)?.let { return it }
            if (toolReconciledObservations[tool] != payload["observation"]) {
                return LedgerError.InvalidTransition
            }
        } else {
            requireActiveExecution(fact)?.let { return it }
        }
        if (toolDigests[tool] != payload.text("prepared_digest")) {
            return LedgerError.InvalidTransition
        }
        tools[tool] = execution to InvocationState.OBSERVED
        return null
    }

    private fun reconcileTool(fact: FactDraft): LedgerError? {
        requireSuspendedExecution(fact)?.let { return it }
        val tool = fact.toolInvocationId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val payload = fact.payloadObject()
        if (toolDigests[tool] != payload.text("prepared_digest") ||
            tools[tool] != (execution to InvocationState.UNCERTAIN)
        ) return LedgerError.InvalidTransition
        tools[tool] = execution to InvocationState.RECONCILED
        toolReconciledObservations[tool] = payload["observation"] ?: return LedgerError.InvalidFact
        return null
    }

    private fun rejectToolPreparation(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        if (fact.toolInvocationId != null) return LedgerError.InvalidTransition
        val request = fact.modelRequestId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        if (fact.payloadObject().text("source_model_request_id") != request.value) {
            return LedgerError.InvalidTransition
        }
        val current = models[request] ?: return LedgerError.MissingReference
        return if (current.first == execution && current.second == InvocationState.COMPLETED) {
            null
        } else {
            LedgerError.InvalidTransition
        }
    }

    private fun requireActiveExecution(fact: FactDraft): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val current = executions[execution] ?: return LedgerError.MissingReference
        return if (current.first == turn && current.second == ExecutionState.ACTIVE) null else LedgerError.InvalidTransition
    }

    private fun requireSuspendedExecution(fact: FactDraft): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        return if (turns[turn] == TurnState.SUSPENDED &&
            executions[execution] == (turn to ExecutionState.SUSPENDED)
        ) null else LedgerError.InvalidTransition
    }

    private fun hasRecoveryPendingInvocation(executionId: ExecutionId? = null): Boolean {
        fun pending(value: Pair<ExecutionId, InvocationState>) =
            (executionId == null || value.first == executionId) &&
                (value.second == InvocationState.STARTED || value.second == InvocationState.RECEIPT)
        return models.values.any(::pending) || tools.values.any(::pending)
    }

    private fun requestKnowledge(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val payload = fact.payloadObject()
        val requestId = payload.text("request_id")
        val record = KnowledgeRecord(
            execution,
            payload.text("request_digest"),
            mutableSetOf(),
            false,
        )
        return if (knowledge.put(requestId, record) == null) null else LedgerError.InvalidTransition
    }

    private fun dispatchKnowledge(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val payload = fact.payloadObject()
        val record = knowledge[payload.text("request_id")] ?: return LedgerError.InvalidTransition
        val valid = record.execution == execution && !record.terminal &&
            record.requestDigest == payload.text("request_digest") &&
            record.dispatchAttempts.add(payload.text("dispatch_attempt_id"))
        return if (valid) null else LedgerError.InvalidTransition
    }

    private fun terminalKnowledge(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val payload = fact.payloadObject()
        val record = knowledge[payload.text("request_id")] ?: return LedgerError.InvalidTransition
        val dispatched = record.dispatchAttempts.isNotEmpty()
        val validPhase = if (fact.kind.value == "knowledge.completed") {
            dispatched
        } else {
            when (payload.text("phase")) {
                "pre_dispatch" -> !dispatched
                "dispatched", "response_validation" -> dispatched
                else -> false
            }
        }
        if (record.execution != execution || record.terminal ||
            record.requestDigest != payload.text("request_digest") || !validPhase
        ) return LedgerError.InvalidTransition
        record.terminal = true
        return null
    }

    private fun hasPendingKnowledge(executionId: ExecutionId): Boolean =
        knowledge.values.any { it.execution == executionId && !it.terminal }

    private fun requestDelegation(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val turn = fact.turnId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val payload = fact.payloadObject(); val id = payload.text("delegation_id")
        if (id in delegations || delegations.values.any { it.parentTurn == turn && it.state !in setOf(DelegationState.DENIED, DelegationState.OBSERVED) } ||
            turnAgents[turn]?.first != payload.text("parent_agent_instance_id")
        ) return LedgerError.InvalidTransition
        delegations[id] = DelegationRecord(turn, execution, payload.text("intent_digest"))
        return null
    }

    private fun authorizeDelegation(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val payload = fact.payloadObject()
        val record = delegations[payload.text("delegation_id")] ?: return LedgerError.MissingReference
        if (!record.matchesOwner(fact) || record.state != DelegationState.REQUESTED || record.intentDigest != payload.text("intent_digest")) return LedgerError.InvalidTransition
        record.grantId = payload.text("grant_id"); record.state = DelegationState.AUTHORIZED
        return null
    }

    private fun denyDelegation(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val payload = fact.payloadObject()
        val record = delegations[payload.text("delegation_id")] ?: return LedgerError.MissingReference
        if (!record.matchesOwner(fact) || record.state != DelegationState.REQUESTED || record.intentDigest != payload.text("intent_digest")) return LedgerError.InvalidTransition
        record.state = DelegationState.DENIED
        return null
    }

    private fun startDelegationChild(fact: FactDraft): LedgerError? {
        val payload = fact.payloadObject(); val childTurn = TurnId.of(payload.text("child_turn_id"))
        val record = delegations[payload.text("delegation_id")] ?: return LedgerError.MissingReference
        val child = turnAgents[childTurn]
        if (!record.matchesOwner(fact) || record.state != DelegationState.AUTHORIZED || record.grantId != payload.text("grant_id") ||
            record.parentTurn !in turnsSuspendedInCommit || childTurn !in turnsStartedInCommit ||
            child?.first != payload.text("child_agent_instance_id") || child.second != payload.text("child_snapshot_digest") ||
            suspensions[record.parentTurn] != payload.text("suspension_id")
        ) return LedgerError.InvalidTransition
        record.suspensionId = payload.text("suspension_id"); record.childTurn = childTurn; record.state = DelegationState.STARTED
        return null
    }

    private fun terminalDelegationChild(fact: FactDraft): LedgerError? {
        val payload = fact.payloadObject(); val childTurn = TurnId.of(payload.text("child_turn_id"))
        val record = delegations[payload.text("delegation_id")] ?: return LedgerError.MissingReference
        if (!record.matchesOwner(fact) || record.state != DelegationState.STARTED || record.grantId != payload.text("grant_id") ||
            record.suspensionId != payload.text("suspension_id") || record.childTurn != childTurn || childTurn !in turnsTerminalInCommit
        ) return LedgerError.InvalidTransition
        record.resultId = payload.text("result_id"); record.resultDigest = payload.text("result_digest"); record.state = DelegationState.TERMINAL
        return null
    }

    private fun observeDelegation(fact: FactDraft): LedgerError? {
        val payload = fact.payloadObject()
        val record = delegations[payload.text("delegation_id")] ?: return LedgerError.MissingReference
        if (!record.matchesOwner(fact) || record.state != DelegationState.TERMINAL || record.grantId != payload.text("grant_id") ||
            record.suspensionId != payload.text("suspension_id") || record.resultId != payload.text("result_id") || record.resultDigest != payload.text("result_digest")
        ) return LedgerError.InvalidTransition
        record.state = DelegationState.OBSERVED
        return null
    }

    private fun delegationContinuationReady(turn: TurnId, suspension: String?): Boolean {
        val records = delegations.values.filter { it.parentTurn == turn && it.suspensionId == suspension }
        return records.isEmpty() || records.all { it.state == DelegationState.OBSERVED && it.inputAdmitted }
    }

    private fun DelegationRecord.matchesOwner(fact: FactDraft): Boolean =
        parentTurn == fact.turnId && parentExecution == fact.executionId

    private fun createSchedule(fact: FactDraft): LedgerError? {
        val payload = fact.payloadObject()
        val scheduleId = payload.text("schedule_id")
        val revisionId = payload.text("revision_id")
        val current = schedules[scheduleId]
        if (current != null &&
            (current.state != ScheduleState.SUPERSEDING || current.revisionId == revisionId)
        ) return LedgerError.InvalidTransition
        schedules[scheduleId] = ScheduleRecord(
            revisionId,
            payload.text("intent_digest"),
            0u,
            null,
            ScheduleState.ACTIVE,
        )
        return null
    }

    private fun claimSchedule(fact: FactDraft): LedgerError? {
        val payload = fact.payloadObject()
        val record = schedule(payload) ?: return LedgerError.InvalidTransition
        val ordinal = payload.ulong("ordinal")
        val occurrenceId = payload.text("occurrence_id")
        val dueAtUtc = payload.text("due_at_utc")
        val leaseEpoch = payload.ulong("lease_epoch")
        val next = record.lastHandledOrdinal.nextOrdinalOrNull() ?: return LedgerError.InvalidTransition
        val valid = record.pendingClaim?.let {
            it.ordinal == ordinal && it.occurrenceId == occurrenceId &&
                it.dueAtUtc == dueAtUtc && leaseEpoch > it.leaseEpoch
        } ?: (ordinal == next)
        if (!valid) return LedgerError.InvalidTransition
        record.pendingClaim = ScheduleClaim(occurrenceId, ordinal, dueAtUtc, leaseEpoch)
        return null
    }

    private fun fireSchedule(fact: FactDraft): LedgerError? {
        val payload = fact.payloadObject()
        val record = schedule(payload) ?: return LedgerError.InvalidTransition
        val ordinal = payload.ulong("ordinal")
        val occurrenceId = payload.text("occurrence_id")
        val pending = record.pendingClaim
        if (pending?.ordinal != ordinal || pending.occurrenceId != occurrenceId) {
            return LedgerError.InvalidTransition
        }
        record.lastHandledOrdinal = ordinal
        record.pendingClaim = null
        return null
    }

    private fun skipSchedule(fact: FactDraft): LedgerError? {
        val payload = fact.payloadObject()
        val record = schedule(payload) ?: return LedgerError.InvalidTransition
        val first = payload.ulong("first_ordinal")
        if (record.pendingClaim != null || record.lastHandledOrdinal.nextOrdinalOrNull() != first) {
            return LedgerError.InvalidTransition
        }
        record.lastHandledOrdinal = payload.ulong("last_ordinal")
        return null
    }

    private fun cancelSchedule(fact: FactDraft): LedgerError? {
        val payload = fact.payloadObject()
        val record = schedules[payload.text("schedule_id")] ?: return LedgerError.InvalidTransition
        if (record.state != ScheduleState.ACTIVE || record.pendingClaim != null ||
            record.revisionId != payload.text("expected_revision_id")
        ) return LedgerError.InvalidTransition
        record.state = if (payload.text("reason") == "superseded") {
            ScheduleState.SUPERSEDING
        } else {
            ScheduleState.CANCELLED
        }
        return null
    }

    private fun failSchedule(fact: FactDraft): LedgerError? {
        val payload = fact.payloadObject()
        val record = schedule(payload) ?: return LedgerError.InvalidTransition
        val occurrenceId = payload["occurrence_id"]?.jsonPrimitive?.contentOrNull
        val ordinal = payload["ordinal"]?.jsonPrimitive?.contentOrNull?.toULongOrNull()
        val pending = record.pendingClaim
        val matchesClaim = if (pending == null) {
            (occurrenceId == null && ordinal == null) ||
                (occurrenceId != null && record.lastHandledOrdinal.nextOrdinalOrNull() == ordinal)
        } else {
            pending.occurrenceId == occurrenceId && pending.ordinal == ordinal
        }
        if (!matchesClaim) return LedgerError.InvalidTransition
        record.pendingClaim = null
        record.state = ScheduleState.FAILED
        return null
    }

    private fun exhaustSchedule(fact: FactDraft): LedgerError? {
        val payload = fact.payloadObject()
        val record = schedule(payload) ?: return LedgerError.InvalidTransition
        if (record.pendingClaim != null ||
            record.lastHandledOrdinal != payload.ulong("last_handled_ordinal")
        ) return LedgerError.InvalidTransition
        record.state = ScheduleState.EXHAUSTED
        return null
    }

    private fun schedule(payload: JsonObject): ScheduleRecord? =
        schedules[payload.text("schedule_id")]?.takeIf {
            it.state == ScheduleState.ACTIVE && it.revisionId == payload.text("revision_id")
        }

    private fun hasPendingInteraction(executionId: ExecutionId): Boolean =
        interactions.values.any { it.execution == executionId && !it.terminal }

    private fun hasPendingInteractionForTurn(turnId: TurnId): Boolean = interactions.values.any {
        !it.terminal && executions[it.execution]?.first == turnId
    }
}

private fun FactDraft.payloadObject(): JsonObject = Json.parseToJsonElement(payload.json).jsonObject

private fun JsonObject.ulong(key: String): ULong =
    getValue(key).jsonPrimitive.content.toULongOrNull() ?: throw IllegalArgumentException(key)

private fun ULong.nextOrdinalOrNull(): ULong? = if (this == ULong.MAX_VALUE) null else this + 1u
