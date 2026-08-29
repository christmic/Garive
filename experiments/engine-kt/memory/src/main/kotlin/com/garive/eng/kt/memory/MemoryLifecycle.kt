package com.garive.eng.kt.memory

/** Recall/evidence lifecycle independent of M0 revision status. */
public enum class HypothesisState(public val wireName: String) {
    CANDIDATE("candidate"), ACTIVE("active"), COLD("cold"), ARCHIVED("archived"), PROMOTED("promoted"),
}

/** Exact portable evidence counts. */
public data class EvidenceTally(
    public val verified: ULong,
    public val falsified: ULong,
    public val neutral: ULong,
)

/** One immutable lifecycle transition input. */
public sealed interface LifecycleEvent {
    /** Durable position for strict ordered reduction. */
    public val position: ULong

    /** Reality-backed positive observation. */
    public data class Verified(override val position: ULong) : LifecycleEvent
    /** Reality-backed negative observation with explicit attribution. */
    public data class Falsified(override val position: ULong, public val inScope: Boolean) : LifecycleEvent
    /** Inconclusive admitted observation. */
    public data class Neutral(override val position: ULong) : LifecycleEvent
    /** Explicit retention/use-policy down-rank. */
    public data class Cool(override val position: ULong) : LifecycleEvent
    /** Explicit maintenance archive decision. */
    public data class Archive(override val position: ULong) : LifecycleEvent
    /** Committed Knowledge publication receipt. */
    public data class Promote(override val position: ULong, public val receiptDigest: String?) : LifecycleEvent
}

/** Pure M1 hypothesis lifecycle projection. */
@ConsistentCopyVisibility
public data class MemoryLifecycle private constructor(
    public val state: HypothesisState,
    public val tally: EvidenceTally,
    public val lastObservedPosition: ULong,
    public val promotedKnowledgeReceiptDigest: String?,
) {
    /** Applies one strictly later transition without mutating this projection. */
    public fun apply(event: LifecycleEvent): MemoryContractResult<MemoryLifecycle> {
        if (event.position <= lastObservedPosition) return failure(MemoryErrorCode.DUPLICATE_OBSERVATION)
        if (state == HypothesisState.PROMOTED) return failure(MemoryErrorCode.INVALID_TRANSITION)
        var nextState = state
        var nextTally = tally
        var receipt = promotedKnowledgeReceiptDigest
        when (event) {
            is LifecycleEvent.Verified -> {
                val count = increment(tally.verified) ?: return failure(MemoryErrorCode.INVALID_TRANSITION)
                nextTally = tally.copy(verified = count)
                nextState = when (state) {
                    HypothesisState.CANDIDATE, HypothesisState.COLD -> HypothesisState.ACTIVE
                    HypothesisState.ACTIVE -> HypothesisState.ACTIVE
                    HypothesisState.ARCHIVED, HypothesisState.PROMOTED ->
                        return failure(MemoryErrorCode.INVALID_TRANSITION)
                }
            }
            is LifecycleEvent.Falsified -> {
                if (event.inScope) {
                    val count = increment(tally.falsified) ?: return failure(MemoryErrorCode.INVALID_TRANSITION)
                    nextTally = tally.copy(falsified = count)
                } else {
                    val count = increment(tally.neutral) ?: return failure(MemoryErrorCode.INVALID_TRANSITION)
                    nextTally = tally.copy(neutral = count)
                }
            }
            is LifecycleEvent.Neutral -> {
                val count = increment(tally.neutral) ?: return failure(MemoryErrorCode.INVALID_TRANSITION)
                nextTally = tally.copy(neutral = count)
            }
            is LifecycleEvent.Cool -> if (state == HypothesisState.ACTIVE) nextState = HypothesisState.COLD
                else return failure(MemoryErrorCode.INVALID_TRANSITION)
            is LifecycleEvent.Archive -> if (state == HypothesisState.COLD) nextState = HypothesisState.ARCHIVED
                else return failure(MemoryErrorCode.INVALID_TRANSITION)
            is LifecycleEvent.Promote -> {
                val digest = event.receiptDigest ?: return failure(MemoryErrorCode.PROMOTION_RECEIPT_REQUIRED)
                if (!validDigest(digest)) return failure(MemoryErrorCode.INVALID_TRANSITION)
                nextState = HypothesisState.PROMOTED
                receipt = digest
            }
        }
        return MemoryContractResult.Success(MemoryLifecycle(nextState, nextTally, event.position, receipt))
    }

    public companion object {
        /** Constructs a projection recovered from a verified durable prefix. */
        public fun create(
            state: HypothesisState,
            tally: EvidenceTally,
            lastObservedPosition: ULong,
            promotedKnowledgeReceiptDigest: String?,
        ): MemoryContractResult<MemoryLifecycle> =
            if (lastObservedPosition == 0uL ||
                (state == HypothesisState.PROMOTED) != (promotedKnowledgeReceiptDigest != null) ||
                promotedKnowledgeReceiptDigest?.let(::validDigest) == false
            ) failure(MemoryErrorCode.INVALID_TRANSITION)
            else MemoryContractResult.Success(
                MemoryLifecycle(state, tally, lastObservedPosition, promotedKnowledgeReceiptDigest),
            )
    }
}

private fun increment(value: ULong): ULong? = if (value == ULong.MAX_VALUE) null else value + 1uL
