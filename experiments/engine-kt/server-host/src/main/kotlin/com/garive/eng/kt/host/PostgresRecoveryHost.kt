@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.garive.eng.kt.host

import com.garive.eng.kt.ledger.CanonicalPayload
import com.garive.eng.kt.ledger.CanonicalPayloadResult
import com.garive.eng.kt.ledger.CommitResult
import com.garive.eng.kt.ledger.DurableFact
import com.garive.eng.kt.ledger.ExecutionId
import com.garive.eng.kt.ledger.FactDraft
import com.garive.eng.kt.ledger.FactId
import com.garive.eng.kt.ledger.FactKind
import com.garive.eng.kt.ledger.SessionId
import com.garive.eng.kt.ledger.TurnId
import com.garive.eng.kt.postgres.PostgresLedger
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

/** Explicit identities and policy for one PostgreSQL lost-Execution restart. */
public data class PostgresRecoveryRequest(
    public val sessionId: SessionId,
    public val turnId: TurnId,
    public val recoveryId: String,
    public val replacementExecutionId: ExecutionId,
    public val maxRecoveries: ULong,
    public val recordedAt: String,
)

/** Fail-closed error from the admitted PostgreSQL recovery-host subset. */
public class PostgresRecoveryRejected(public val action: RuntimeRecoveryAction) :
    IllegalStateException("PostgreSQL recovery rejected: ${action.name.lowercase()}")

/** Experimental C6 host composing PostgreSQL snapshots with portable recovery decisions. */
public class PostgresRecoveryHost(private val ledger: PostgresLedger) {
    /** Atomically abandons and replaces only a child-safe lost active Execution. */
    public fun restartLostExecution(request: PostgresRecoveryRequest): CommitResult {
        require(request.recoveryId.isNotEmpty() && request.maxRecoveries > 0uL)
        val snapshot = ledger.loadTurn(request.turnId)
        val facts = snapshot.facts
        require(facts.firstOrNull()?.sessionId == request.sessionId)
        val start = facts.lastOrNull { it.kind.value == "execution.started" }
            ?: throw PostgresRecoveryRejected(RuntimeRecoveryAction.FAIL_CORRUPT_LEDGER)
        val execution = requireNotNull(start.executionId)
        val terminal = facts.any { it.executionId == execution && it.kind.value in executionTerminals }
        val action = selectRuntimeRecovery(
            RuntimeRecoverySnapshot(
                if (terminal) ExecutionRecoveryPosition.TERMINAL else ExecutionRecoveryPosition.ACTIVE,
                modelPosition(facts, execution),
                effectPosition(facts, execution),
                start.payload.objectValue().ulong("recovery_ordinal"),
                request.maxRecoveries,
            ),
        )
        if (action != RuntimeRecoveryAction.ABANDON_AND_RESTART) {
            throw PostgresRecoveryRejected(action)
        }
        val old = start.payload.objectValue()
        val nextOrdinal = old.ulong("recovery_ordinal") + 1uL
        val abandoned = payload(
            JsonObject(
                mapOf(
                    "reason" to JsonPrimitive("runtime_lost"),
                    "last_safe_position" to JsonPrimitive(snapshot.throughPosition),
                    "recovery_ordinal" to JsonPrimitive(nextOrdinal),
                ),
            ),
        )
        val replacement = payload(
            JsonObject(
                mapOf(
                    "snapshot_digest" to requireNotNull(old["snapshot_digest"]),
                    "through_position" to JsonPrimitive(snapshot.throughPosition),
                    "completed_iterations" to JsonPrimitive(iterationCursor(facts, execution, old)),
                    "limits" to requireNotNull(old["limits"]),
                    "recovery_ordinal" to JsonPrimitive(nextOrdinal),
                ),
            ),
        )
        return ledger.commit(
            request.sessionId,
            snapshot.sessionVersion,
            listOf(
                draft("${request.recoveryId}:abandoned", request.turnId, execution, "execution.abandoned", abandoned, request.recordedAt),
                draft("${request.recoveryId}:started", request.turnId, request.replacementExecutionId, "execution.started", replacement, request.recordedAt),
            ),
        )
    }
}

private val executionTerminals = setOf(
    "execution.completed", "execution.suspended", "execution.stopped", "execution.failed", "execution.abandoned",
)

private fun modelPosition(facts: List<DurableFact>, execution: ExecutionId): ModelRecoveryPosition {
    val kinds = facts.filter { it.executionId == execution }.map { it.kind.value }.toSet()
    return when {
        kinds.any { it in setOf("model.completed", "model.rejected", "model.interrupted", "model.unavailable") } -> ModelRecoveryPosition.TERMINAL
        "model.uncertain" in kinds -> ModelRecoveryPosition.UNCERTAIN
        "model.started" in kinds -> ModelRecoveryPosition.STARTED
        "model.prepared" in kinds -> ModelRecoveryPosition.PREPARED
        else -> ModelRecoveryPosition.NONE
    }
}

private fun effectPosition(facts: List<DurableFact>, execution: ExecutionId): EffectRecoveryPosition {
    val kinds = facts.filter { it.executionId == execution }.map { it.kind.value }.toSet()
    return when {
        "interaction.requested" in kinds -> EffectRecoveryPosition.INTERACTION_REQUESTED
        "effect.uncertain" in kinds -> EffectRecoveryPosition.UNCERTAIN
        "effect.reconciled" in kinds -> EffectRecoveryPosition.RECONCILED
        kinds.any { it in setOf("effect.completed", "effect.failed") } -> EffectRecoveryPosition.TERMINAL
        "effect.receipt" in kinds -> EffectRecoveryPosition.RECEIPT
        "effect.started" in kinds -> EffectRecoveryPosition.STARTED
        "effect.prepared" in kinds -> EffectRecoveryPosition.PREPARED
        else -> EffectRecoveryPosition.NONE
    }
}

private fun iterationCursor(facts: List<DurableFact>, execution: ExecutionId, start: JsonObject): ULong =
    facts.filter { it.executionId == execution && it.kind.value == "execution.iteration_started" }
        .maxOfOrNull { it.payload.objectValue().ulong("iteration") }
        ?: start.ulong("completed_iterations")

private fun draft(
    id: String,
    turnId: TurnId,
    executionId: ExecutionId,
    kind: String,
    payload: CanonicalPayload,
    recordedAt: String,
) = FactDraft(FactId.of(id), turnId, executionId, null, null, FactKind.of(kind), 1u, payload, recordedAt)

private fun payload(value: JsonObject): CanonicalPayload =
    requireNotNull((CanonicalPayload.fromValue(value) as? CanonicalPayloadResult.Success)?.payload)

private fun CanonicalPayload.objectValue(): JsonObject = Json.parseToJsonElement(json).jsonObject

private fun JsonObject.ulong(key: String): ULong = getValue(key).jsonPrimitive.content.toULong()
