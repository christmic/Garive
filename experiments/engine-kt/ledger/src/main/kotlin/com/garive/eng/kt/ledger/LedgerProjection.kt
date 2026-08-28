package com.garive.eng.kt.ledger

internal enum class TurnState { OPEN, SUSPENDED, COMPLETED, STOPPED, FAILED }
internal enum class ExecutionState { ACTIVE, COMPLETED, SUSPENDED, STOPPED, FAILED }
internal enum class InvocationState {
    PREPARED, AUTHORIZED, STARTED, RECEIPT, COMPLETED, REJECTED, INTERRUPTED, UNAVAILABLE,
    FAILED, DENIED, UNCERTAIN,
}

internal class LedgerProjection(
    private var opened: Boolean = false,
    private var closed: Boolean = false,
    private val turns: MutableMap<TurnId, TurnState> = mutableMapOf(),
    private val executions: MutableMap<ExecutionId, Pair<TurnId, ExecutionState>> = mutableMapOf(),
    private val models: MutableMap<ModelRequestId, InvocationState> = mutableMapOf(),
    private val tools: MutableMap<ToolInvocationId, InvocationState> = mutableMapOf(),
) {
    fun copy() = LedgerProjection(
        opened,
        closed,
        turns.toMutableMap(),
        executions.toMutableMap(),
        models.toMutableMap(),
        tools.toMutableMap(),
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
            "turn.started" -> startTurn(fact.turnId ?: return LedgerError.MissingReference)
            "turn.suspended" -> transitionTurn(fact.turnId, TurnState.SUSPENDED)
            "turn.completed" -> transitionTurn(fact.turnId, TurnState.COMPLETED)
            "turn.stopped" -> transitionTurn(fact.turnId, TurnState.STOPPED)
            "turn.failed" -> transitionTurn(fact.turnId, TurnState.FAILED)
            "turn.input" -> requireOpenTurn(fact.turnId)
            "execution.started" -> startExecution(fact)
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
            else -> null
        }
    }

    fun uncertainModelRequests() = models.entries
        .filter { it.value == InvocationState.STARTED }
        .map { it.key }
        .sortedBy { it.value }

    private fun closeSession(): LedgerError? {
        if (turns.values.any { it == TurnState.OPEN || it == TurnState.SUSPENDED }) {
            return LedgerError.InvalidTransition
        }
        closed = true
        return null
    }

    private fun startTurn(turnId: TurnId): LedgerError? = when (turns[turnId]) {
        null, TurnState.SUSPENDED -> {
            turns[turnId] = TurnState.OPEN
            null
        }
        else -> LedgerError.InvalidTransition
    }

    private fun requireOpenTurn(turnId: TurnId?): LedgerError? = when {
        turnId == null -> LedgerError.MissingReference
        turns[turnId] != TurnState.OPEN -> LedgerError.InvalidTransition
        else -> null
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
        turns[turnId] = next
        return null
    }

    private fun startExecution(fact: FactDraft): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        requireOpenTurn(turn)?.let { return it }
        val execution = fact.executionId ?: return LedgerError.MissingReference
        if (execution in executions) return LedgerError.InvalidTransition
        executions[execution] = turn to ExecutionState.ACTIVE
        return null
    }

    private fun transitionExecution(fact: FactDraft, next: ExecutionState): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val current = executions[execution] ?: return LedgerError.MissingReference
        if (current.first != turn || current.second != ExecutionState.ACTIVE) return LedgerError.InvalidTransition
        executions[execution] = turn to next
        return null
    }

    private fun prepareModel(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val request = fact.modelRequestId ?: return LedgerError.MissingReference
        if (models.put(request, InvocationState.PREPARED) != null) return LedgerError.InvalidTransition
        return null
    }

    private fun transitionModel(fact: FactDraft, next: InvocationState): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val request = fact.modelRequestId ?: return LedgerError.MissingReference
        val current = models[request] ?: return LedgerError.MissingReference
        val valid = (current == InvocationState.PREPARED && next == InvocationState.STARTED) ||
            (current == InvocationState.STARTED && next != InvocationState.PREPARED && next != InvocationState.STARTED)
        if (!valid) return LedgerError.InvalidTransition
        models[request] = next
        return null
    }

    private fun prepareTool(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val tool = fact.toolInvocationId ?: return LedgerError.MissingReference
        if (tools.put(tool, InvocationState.PREPARED) != null) return LedgerError.InvalidTransition
        return null
    }

    private fun transitionTool(fact: FactDraft, next: InvocationState): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val tool = fact.toolInvocationId ?: return LedgerError.MissingReference
        val current = tools[tool] ?: return LedgerError.MissingReference
        val valid = when (current to next) {
            InvocationState.PREPARED to InvocationState.AUTHORIZED,
            InvocationState.PREPARED to InvocationState.STARTED,
            InvocationState.PREPARED to InvocationState.DENIED,
            InvocationState.AUTHORIZED to InvocationState.STARTED,
            InvocationState.AUTHORIZED to InvocationState.DENIED,
            InvocationState.STARTED to InvocationState.RECEIPT,
            InvocationState.STARTED to InvocationState.COMPLETED,
            InvocationState.STARTED to InvocationState.FAILED,
            InvocationState.STARTED to InvocationState.UNCERTAIN,
            InvocationState.RECEIPT to InvocationState.COMPLETED,
            InvocationState.RECEIPT to InvocationState.FAILED,
            -> true
            else -> false
        }
        if (!valid) return LedgerError.InvalidTransition
        tools[tool] = next
        return null
    }

    private fun requireActiveExecution(fact: FactDraft): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val current = executions[execution] ?: return LedgerError.MissingReference
        return if (current.first == turn && current.second == ExecutionState.ACTIVE) null else LedgerError.InvalidTransition
    }
}
