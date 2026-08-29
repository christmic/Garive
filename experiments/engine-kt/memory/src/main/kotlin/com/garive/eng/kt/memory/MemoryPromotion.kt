package com.garive.eng.kt.memory

/** Frozen policy that admits a Memory-to-Knowledge promotion request. */
@ConsistentCopyVisibility
public data class MemoryPromotionPolicy private constructor(
    public val revision: String,
    public val allowedTypes: List<MemoryType>,
    public val minVerified: ULong,
    public val maxFalsified: ULong,
    public val minHelpfulUses: ULong,
) {
    public companion object {
        /** Validates a non-empty canonical type set and explicit thresholds. */
        public fun create(
            revision: String,
            allowedTypes: List<MemoryType>,
            minVerified: ULong,
            maxFalsified: ULong,
            minHelpfulUses: ULong,
        ): MemoryContractResult<MemoryPromotionPolicy> =
            if (!validText(revision, MAX_REFERENCE_BYTES) || allowedTypes.isEmpty() ||
                !allowedTypes.zipWithNext().all { (left, right) -> left.ordinal < right.ordinal }
            ) failure(MemoryErrorCode.INVALID_MEMORY)
            else MemoryContractResult.Success(
                MemoryPromotionPolicy(
                    revision, allowedTypes, minVerified, maxFalsified, minHelpfulUses,
                ),
            )
    }
}

/** Opaque request binding one eligible Memory revision to a Knowledge proposal. */
@ConsistentCopyVisibility
public data class MemoryPromotionRequest private constructor(
    public val requestId: String,
    public val namespaceId: String,
    public val recordId: String,
    public val revisionId: String,
    public val memoryType: MemoryType,
    public val policyRevision: String,
    public val knowledgeProposalId: String,
    public val evidenceDigest: String,
) {
    internal companion object {
        internal fun create(
            requestId: String,
            namespaceId: String,
            recordId: String,
            revisionId: String,
            memoryType: MemoryType,
            policyRevision: String,
            knowledgeProposalId: String,
            evidenceDigest: String,
        ): MemoryContractResult<MemoryPromotionRequest> {
            val ids = listOf(requestId, namespaceId, recordId, revisionId, knowledgeProposalId)
            return if (ids.any { !validId(it) } || !validDigest(evidenceDigest)) {
                failure(MemoryErrorCode.INVALID_MEMORY)
            } else MemoryContractResult.Success(
                MemoryPromotionRequest(
                    requestId, namespaceId, recordId, revisionId, memoryType, policyRevision,
                    knowledgeProposalId, evidenceDigest,
                ),
            )
        }
    }
}

/** Checks policy eligibility and produces a request without publishing Knowledge. */
@Suppress("LongParameterList")
public fun requestMemoryPromotion(
    requestId: String,
    namespaceId: String,
    recordId: String,
    revisionId: String,
    memoryType: MemoryType,
    lifecycle: MemoryLifecycle,
    helpfulUses: ULong,
    policy: MemoryPromotionPolicy,
    knowledgeProposalId: String,
    evidenceDigest: String,
): MemoryContractResult<MemoryPromotionRequest> {
    if (lifecycle.state !in setOf(HypothesisState.ACTIVE, HypothesisState.COLD) ||
        memoryType !in policy.allowedTypes || lifecycle.tally.verified < policy.minVerified ||
        lifecycle.tally.falsified > policy.maxFalsified || helpfulUses < policy.minHelpfulUses
    ) return failure(MemoryErrorCode.PROMOTION_NOT_ELIGIBLE)
    return MemoryPromotionRequest.create(
        requestId, namespaceId, recordId, revisionId, memoryType, policy.revision,
        knowledgeProposalId, evidenceDigest,
    )
}

/** Receipt-shaped proof that Knowledge published the exact proposal. */
@ConsistentCopyVisibility
public data class MemoryPromotionReceipt private constructor(
    public val requestId: String,
    public val knowledgeProposalId: String,
    public val knowledgeRecordId: String,
    public val knowledgeRevisionId: String,
    public val receiptDigest: String,
) {
    public companion object {
        /** Validates receipt identity and digest shape; Runtime verifies authenticity. */
        public fun create(
            requestId: String,
            knowledgeProposalId: String,
            knowledgeRecordId: String,
            knowledgeRevisionId: String,
            receiptDigest: String,
        ): MemoryContractResult<MemoryPromotionReceipt> {
            val ids = listOf(requestId, knowledgeProposalId, knowledgeRecordId, knowledgeRevisionId)
            return if (ids.any { !validId(it) } || !validDigest(receiptDigest)) {
                failure(MemoryErrorCode.INVALID_MEMORY)
            } else MemoryContractResult.Success(
                MemoryPromotionReceipt(
                    requestId, knowledgeProposalId, knowledgeRecordId, knowledgeRevisionId,
                    receiptDigest,
                ),
            )
        }
    }
}

/** Verifies receipt bindings and produces the Promoted lifecycle projection. */
public fun completeMemoryPromotion(
    request: MemoryPromotionRequest,
    receipt: MemoryPromotionReceipt,
    lifecycle: MemoryLifecycle,
    position: ULong,
): MemoryContractResult<MemoryLifecycle> {
    if (receipt.requestId != request.requestId ||
        receipt.knowledgeProposalId != request.knowledgeProposalId
    ) return failure(MemoryErrorCode.INVALID_MEMORY)
    if (position <= lifecycle.lastObservedPosition) return failure(MemoryErrorCode.INVALID_TRANSITION)
    return lifecycle.apply(LifecycleEvent.Promote(position, receipt.receiptDigest))
}
