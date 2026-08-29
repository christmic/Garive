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
    FAILED, DENIED, UNCERTAIN, OBSERVED,
}

internal data class InteractionRecord(
    val execution: ExecutionId,
    val tool: ToolInvocationId,
    val suspensionId: String,
    val preparedDigest: String,
    var terminal: Boolean,
)

internal class LedgerProjection(
    private var opened: Boolean = false,
    private var closed: Boolean = false,
    private val turns: MutableMap<TurnId, TurnState> = mutableMapOf(),
    private val executions: MutableMap<ExecutionId, Pair<TurnId, ExecutionState>> = mutableMapOf(),
    private val models: MutableMap<ModelRequestId, Pair<ExecutionId, InvocationState>> = mutableMapOf(),
    private val tools: MutableMap<ToolInvocationId, Pair<ExecutionId, InvocationState>> = mutableMapOf(),
    private val toolDigests: MutableMap<ToolInvocationId, String> = mutableMapOf(),
    private val suspensions: MutableMap<TurnId, String> = mutableMapOf(),
    private val interactions: MutableMap<String, InteractionRecord> = mutableMapOf(),
) {
    fun copy() = LedgerProjection(
        opened,
        closed,
        turns.toMutableMap(),
        executions.toMutableMap(),
        models.toMutableMap(),
        tools.toMutableMap(),
        toolDigests.toMutableMap(),
        suspensions.toMutableMap(),
        interactions.mapValues { (_, value) -> value.copy() }.toMutableMap(),
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
            "turn.completed" -> transitionTurn(fact.turnId, TurnState.COMPLETED)
            "turn.stopped" -> transitionTurn(fact.turnId, TurnState.STOPPED)
            "turn.failed" -> transitionTurn(fact.turnId, TurnState.FAILED)
            "turn.input" -> requireOpenTurn(fact.turnId)
            "turn.cancel_requested" -> requireNonTerminalTurn(fact.turnId)
            "execution.started" -> startExecution(fact)
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
            "effect.observation" -> observeTool(fact)
            "tool.preparation_rejected" -> rejectToolPreparation(fact)
            "interaction.requested" -> requestInteraction(fact)
            "interaction.resolved", "interaction.cancelled" -> finishInteraction(fact)
            else -> null
        }
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
                !hasPendingInteractionForTurn(turn)
            else -> false
        }
        if (!valid) return LedgerError.InvalidTransition
        turns[turn] = TurnState.OPEN
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
        return null
    }

    private fun requireOpenTurn(turnId: TurnId?): LedgerError? = when {
        turnId == null -> LedgerError.MissingReference
        turns[turnId] != TurnState.OPEN -> LedgerError.InvalidTransition
        else -> null
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
        return null
    }

    private fun transitionExecution(fact: FactDraft, next: ExecutionState): LedgerError? {
        val turn = fact.turnId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val current = executions[execution] ?: return LedgerError.MissingReference
        if (current.first != turn || current.second != ExecutionState.ACTIVE) return LedgerError.InvalidTransition
        if (hasRecoveryPendingInvocation(execution) ||
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
        return null
    }

    private fun transitionModel(fact: FactDraft, next: InvocationState): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val request = fact.modelRequestId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val current = models[request] ?: return LedgerError.MissingReference
        if (current.first != execution) return LedgerError.InvalidTransition
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
        val valid = when (current.second to next) {
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
        tools[tool] = execution to next
        return null
    }

    private fun observeTool(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        val tool = fact.toolInvocationId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
        val current = tools[tool] ?: return LedgerError.MissingReference
        if (current.first != execution || current.second !in setOf(
                InvocationState.COMPLETED,
                InvocationState.FAILED,
                InvocationState.DENIED,
                InvocationState.UNCERTAIN,
            )
        ) {
            return LedgerError.InvalidTransition
        }
        tools[tool] = execution to InvocationState.OBSERVED
        return null
    }

    private fun rejectToolPreparation(fact: FactDraft): LedgerError? {
        requireActiveExecution(fact)?.let { return it }
        if (fact.toolInvocationId != null) return LedgerError.InvalidTransition
        val request = fact.modelRequestId ?: return LedgerError.MissingReference
        val execution = fact.executionId ?: return LedgerError.MissingReference
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

    private fun hasRecoveryPendingInvocation(executionId: ExecutionId? = null): Boolean {
        fun pending(value: Pair<ExecutionId, InvocationState>) =
            (executionId == null || value.first == executionId) &&
                (value.second == InvocationState.STARTED || value.second == InvocationState.RECEIPT)
        return models.values.any(::pending) || tools.values.any(::pending)
    }

    private fun hasPendingInteraction(executionId: ExecutionId): Boolean =
        interactions.values.any { it.execution == executionId && !it.terminal }

    private fun hasPendingInteractionForTurn(turnId: TurnId): Boolean = interactions.values.any {
        !it.terminal && executions[it.execution]?.first == turnId
    }
}

private fun FactDraft.payloadObject(): JsonObject = Json.parseToJsonElement(payload.json).jsonObject
