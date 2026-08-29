package com.garive.eng.kt.memory

/** Admitted class of committed reality evidence. */
public enum class ObservationEvidenceKind {
    TOOL_RESULT, TEST_RESULT, EFFECT_RECEIPT, USER_CORRECTION, DETERMINISTIC_VERIFIER,
}

/** Typed durable evidence used by one observation. */
public data class ObservationEvidence(
    public val kind: ObservationEvidenceKind,
    public val fact: DurableFactReference,
) : Comparable<ObservationEvidence> {
    public override fun compareTo(other: ObservationEvidence): Int =
        compareValuesBy(this, other, { it.kind.ordinal }, ObservationEvidence::fact)
}

/** Bounded application claim awaiting real-world reconciliation. */
@ConsistentCopyVisibility
public data class MemoryObligation private constructor(
    public val obligationId: String,
    public val recordId: String,
    public val revisionId: String,
    public val applicationFact: DurableFactReference,
    public val expectedOutcomeDigest: String,
    public val applicationScopeDigest: String,
    public val attributionPolicyRevision: String,
    public val expiresAtPosition: ULong,
) {
    public companion object {
        /** Constructs an obligation from an application fact, never citation text alone. */
        @Suppress("LongParameterList")
        public fun create(
            obligationId: String, recordId: String, revisionId: String,
            applicationFact: DurableFactReference, expectedOutcomeDigest: String,
            applicationScopeDigest: String, attributionPolicyRevision: String,
            expiresAtPosition: ULong,
        ): MemoryContractResult<MemoryObligation> =
            if (!validId(obligationId) || !validId(recordId) || !validId(revisionId) ||
                !validDigest(expectedOutcomeDigest) || !validDigest(applicationScopeDigest) ||
                !validText(attributionPolicyRevision, MAX_REFERENCE_BYTES) ||
                expiresAtPosition <= applicationFact.position
            ) failure(MemoryErrorCode.INVALID_MEMORY)
            else MemoryContractResult.Success(
                MemoryObligation(obligationId, recordId, revisionId, applicationFact,
                    expectedOutcomeDigest, applicationScopeDigest, attributionPolicyRevision, expiresAtPosition),
            )
    }
}

/** Reality verdict bound to an observation. */
public sealed interface ObservationVerdict {
    /** Outcome verified the applied hypothesis. */
    public data object Verified : ObservationVerdict
    /** Outcome falsified it, with explicit scope attribution. */
    public data class Falsified(public val inScope: Boolean, public val observedScopeDigest: String?) : ObservationVerdict
    /** Evidence was inconclusive. */
    public data class Neutral(public val safeReason: String) : ObservationVerdict
}

/** One typed observation of an open obligation. */
@ConsistentCopyVisibility
public data class MemoryObservation private constructor(
    public val observationId: String,
    public val obligationId: String,
    public val position: ULong,
    public val verifierRevision: String,
    public val evidence: List<ObservationEvidence>,
    public val verdict: ObservationVerdict,
) {
    public companion object {
        /** Validates identity, ordering, bounds, attribution and safe reason. */
        public fun create(
            observationId: String, obligationId: String, position: ULong,
            verifierRevision: String, evidence: List<ObservationEvidence>, verdict: ObservationVerdict,
        ): MemoryContractResult<MemoryObservation> {
            if (!validId(observationId) || !validId(obligationId) || position == 0uL ||
                !validText(verifierRevision, MAX_REFERENCE_BYTES) || evidence.isEmpty() ||
                evidence.size > MAX_OBSERVATION_EVIDENCE || !orderedUnique(evidence)
            ) return failure(MemoryErrorCode.INVALID_MEMORY)
            when (verdict) {
                is ObservationVerdict.Falsified -> if (
                    verdict.inScope == (verdict.observedScopeDigest != null) ||
                    verdict.observedScopeDigest?.let(::validDigest) == false
                ) return failure(MemoryErrorCode.ATTRIBUTION_UNSUPPORTED)
                is ObservationVerdict.Neutral -> if (!validText(verdict.safeReason, MAX_REASON_BYTES)) {
                    return failure(MemoryErrorCode.INVALID_MEMORY)
                }
                ObservationVerdict.Verified -> Unit
            }
            return MemoryContractResult.Success(
                MemoryObservation(observationId, obligationId, position, verifierRevision, evidence, verdict),
            )
        }
    }
}

/** Candidate for explicit supersession with a narrower observed scope. */
public data class ScopeNarrowingCandidate(
    public val recordId: String,
    public val revisionId: String,
    public val applicationScopeDigest: String,
    public val observedScopeDigest: String,
    public val evidence: List<ObservationEvidence>,
)

/** Pure observation reduction output. */
public data class ObservationReduction(
    public val lifecycle: MemoryLifecycle,
    public val narrowing: ScopeNarrowingCandidate?,
)

/** Reconciles one observation without treating an application citation as proof. */
public fun reduceObservation(
    obligation: MemoryObligation,
    observation: MemoryObservation,
    lifecycle: MemoryLifecycle,
): MemoryContractResult<ObservationReduction> {
    if (observation.obligationId != obligation.obligationId) {
        return failure(MemoryErrorCode.ATTRIBUTION_UNSUPPORTED)
    }
    if (observation.position > obligation.expiresAtPosition) return failure(MemoryErrorCode.INVALID_TRANSITION)
    val event: LifecycleEvent
    var narrowing: ScopeNarrowingCandidate? = null
    when (val verdict = observation.verdict) {
        ObservationVerdict.Verified -> event = LifecycleEvent.Verified(observation.position)
        is ObservationVerdict.Falsified -> {
            event = LifecycleEvent.Falsified(observation.position, verdict.inScope)
            if (!verdict.inScope) {
                val observed = verdict.observedScopeDigest ?: return failure(MemoryErrorCode.ATTRIBUTION_UNSUPPORTED)
                narrowing = ScopeNarrowingCandidate(
                    obligation.recordId, obligation.revisionId, obligation.applicationScopeDigest,
                    observed, observation.evidence,
                )
            }
        }
        is ObservationVerdict.Neutral -> event = LifecycleEvent.Neutral(observation.position)
    }
    return when (val reduced = lifecycle.apply(event)) {
        is MemoryContractResult.Success -> MemoryContractResult.Success(ObservationReduction(reduced.value, narrowing))
        is MemoryContractResult.Failure -> reduced
    }
}

private const val MAX_OBSERVATION_EVIDENCE: Int = 64
private const val MAX_REASON_BYTES: Int = 256
