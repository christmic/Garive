package com.garive.eng.kt.ledger

data class TurnSnapshot(
    val facts: List<DurableFact>,
    val sessionVersion: ULong,
    val throughPosition: ULong,
)

private data class SessionLedger(
    var version: ULong = 0u,
    val facts: MutableList<DurableFact> = mutableListOf(),
    val drafts: MutableMap<ULong, FactDraft> = mutableMapOf(),
    val projection: LedgerProjection = LedgerProjection(),
) {
    fun copyLedger() = SessionLedger(
        version,
        facts.toMutableList(),
        drafts.toMutableMap(),
        projection.copy(),
    )
}

private data class FactIndexEntry(
    val sessionId: SessionId,
    val position: ULong,
    val draft: FactDraft,
)

class LedgerState {
    private val sessions = mutableMapOf<SessionId, SessionLedger>()
    private val factIndex = mutableMapOf<FactId, FactIndexEntry>()

    fun commit(
        sessionId: SessionId,
        expectedSessionVersion: ULong,
        drafts: List<FactDraft>,
    ): LedgerResult<CommitResult> {
        if (drafts.isEmpty()) return LedgerResult.Failure(LedgerError.EmptyBatch)
        val identities = mutableSetOf<FactId>()
        val replayPositions = mutableListOf<ULong>()
        var replayed = 0
        for (draft in drafts) {
            draft.validate()?.let { return LedgerResult.Failure(it) }
            if (identityOwnedByOtherSession(sessionId, draft)) {
                return LedgerResult.Failure(LedgerError.InvalidTransition)
            }
            if (!identities.add(draft.factId)) return LedgerResult.Failure(LedgerError.InvalidFact)
            factIndex[draft.factId]?.let { existing ->
                if (existing.sessionId != sessionId || !existing.draft.sameSemantics(draft)) {
                    return LedgerResult.Failure(LedgerError.IdempotencyCollision)
                }
                replayed += 1
                replayPositions += existing.position
            }
        }
        if (replayed == drafts.size) {
            val version = sessions[sessionId]?.version
                ?: return LedgerResult.Failure(LedgerError.MissingReference)
            return LedgerResult.Success(
                CommitResult(CommitDisposition.REPLAYED, version, replayPositions),
            )
        }
        if (replayed != 0) return LedgerResult.Failure(LedgerError.IncompleteReplay)

        val next = sessions[sessionId]?.copyLedger() ?: SessionLedger()
        if (next.version != expectedSessionVersion) {
            return LedgerResult.Failure(LedgerError.ConcurrentModification)
        }
        var position = next.facts.lastOrNull()?.position?.incrementOrNull()
            ?: if (next.facts.isEmpty()) 1u else return LedgerResult.Failure(LedgerError.PositionOverflow)
        val positions = mutableListOf<ULong>()
        for ((index, draft) in drafts.withIndex()) {
            next.projection.apply(draft)?.let { return LedgerResult.Failure(it) }
            val durable = draft.toDurable(sessionId, position)
            durable.verify()?.let { return LedgerResult.Failure(it) }
            next.drafts[position] = draft
            next.facts += durable
            positions += position
            if (index != drafts.lastIndex) {
                position = position.incrementOrNull()
                    ?: return LedgerResult.Failure(LedgerError.PositionOverflow)
            }
        }
        next.version = next.version.incrementOrNull()
            ?: return LedgerResult.Failure(LedgerError.PositionOverflow)

        drafts.zip(positions).forEach { (draft, committedPosition) ->
            factIndex[draft.factId] = FactIndexEntry(sessionId, committedPosition, draft)
        }
        sessions[sessionId] = next
        return LedgerResult.Success(
            CommitResult(CommitDisposition.COMMITTED, next.version, positions),
        )
    }

    fun readFacts(
        sessionId: SessionId,
        afterPosition: ULong,
        throughPosition: ULong,
        kinds: Set<FactKind>? = null,
    ): LedgerResult<List<DurableFact>> {
        if (throughPosition == 0uL || afterPosition >= throughPosition) {
            return LedgerResult.Failure(LedgerError.InvalidReadRange)
        }
        val session = sessions[sessionId] ?: return LedgerResult.Failure(LedgerError.MissingReference)
        var previous = afterPosition
        val output = mutableListOf<DurableFact>()
        for (fact in session.facts.filter { it.position > afterPosition && it.position <= throughPosition }) {
            fact.verify()?.let { return LedgerResult.Failure(it) }
            if (fact.position <= previous) return LedgerResult.Failure(LedgerError.InvalidTransition)
            previous = fact.position
            if (kinds == null || fact.kind in kinds) output += fact
        }
        return LedgerResult.Success(output)
    }

    fun loadTurn(turnId: TurnId): LedgerResult<TurnSnapshot> {
        for (session in sessions.values) {
            val facts = session.facts.filter { it.turnId == turnId }
            if (facts.isNotEmpty()) {
                return LedgerResult.Success(
                    TurnSnapshot(facts, session.version, session.facts.lastOrNull()?.position ?: 0u),
                )
            }
        }
        return LedgerResult.Failure(LedgerError.MissingReference)
    }

    fun findModelRequest(requestId: ModelRequestId) = findInvocation { it.modelRequestId == requestId }

    fun findToolInvocation(invocationId: ToolInvocationId) =
        findInvocation { it.toolInvocationId == invocationId }

    fun listUncertainModelRequests(sessionId: SessionId): LedgerResult<List<ModelRequestId>> =
        sessions[sessionId]?.let { LedgerResult.Success(it.projection.uncertainModelRequests()) }
            ?: LedgerResult.Failure(LedgerError.MissingReference)

    fun listUncertainToolInvocations(sessionId: SessionId): LedgerResult<List<ToolInvocationId>> =
        sessions[sessionId]?.let { LedgerResult.Success(it.projection.uncertainToolInvocations()) }
            ?: LedgerResult.Failure(LedgerError.MissingReference)

    fun sessionVersion(sessionId: SessionId) = sessions[sessionId]?.version

    fun factCount(sessionId: SessionId) = sessions[sessionId]?.facts?.size ?: 0

    fun factAt(sessionId: SessionId, position: ULong) =
        sessions[sessionId]?.facts?.find { it.position == position }

    private fun findInvocation(predicate: (DurableFact) -> Boolean) =
        sessions.values.flatMap { it.facts }.filter(predicate)

    private fun identityOwnedByOtherSession(sessionId: SessionId, draft: FactDraft) =
        factIndex.values.any { existing ->
            existing.sessionId != sessionId &&
                (samePresent(existing.draft.turnId, draft.turnId) ||
                    samePresent(existing.draft.executionId, draft.executionId) ||
                    samePresent(existing.draft.modelRequestId, draft.modelRequestId) ||
                    samePresent(existing.draft.toolInvocationId, draft.toolInvocationId))
        }
}

private fun <T> samePresent(left: T?, right: T?) = left != null && right != null && left == right

private fun FactDraft.toDurable(sessionId: SessionId, position: ULong) = DurableFact(
    factId,
    sessionId,
    position,
    turnId,
    executionId,
    modelRequestId,
    toolInvocationId,
    kind,
    schemaVersion,
    payload,
    recordedAt,
)

private fun ULong.incrementOrNull() = if (this == ULong.MAX_VALUE) null else this + 1u
