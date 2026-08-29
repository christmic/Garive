package com.garive.eng.kt.memory

private const val MAX_ERASURE_TARGETS: Int = 64

/** Runtime-owned storage class participating in physical erasure. */
public enum class ErasureTargetKind { PRIMARY_STORE, PROJECTION, CACHE, BACKUP }

/** One canonical configured erasure target. */
@ConsistentCopyVisibility
public data class MemoryErasureTarget private constructor(
    public val targetId: String,
    public val kind: ErasureTargetKind,
) {
    public companion object {
        /** Validates an opaque configured target identity. */
        public fun create(
            targetId: String,
            kind: ErasureTargetKind,
        ): MemoryContractResult<MemoryErasureTarget> =
            if (!validId(targetId)) failure(MemoryErrorCode.INVALID_MEMORY)
            else MemoryContractResult.Success(MemoryErasureTarget(targetId, kind))
    }
}

/** Physical erasure request admitted only after an exact logical tombstone. */
@ConsistentCopyVisibility
public data class MemoryErasureRequest private constructor(
    public val requestId: String,
    public val namespaceId: String,
    public val recordId: String,
    public val revisionId: String,
    public val tombstoneFact: DurableFactReference,
    public val policyRevision: String,
    public val targets: List<MemoryErasureTarget>,
) {
    public companion object {
        /** Validates exact target identity, ordering, and tombstone binding shape. */
        public fun create(
            requestId: String,
            namespaceId: String,
            recordId: String,
            revisionId: String,
            tombstoneFact: DurableFactReference,
            policyRevision: String,
            targets: List<MemoryErasureTarget>,
        ): MemoryContractResult<MemoryErasureRequest> {
            val ids = listOf(requestId, namespaceId, recordId, revisionId)
            val ordered = targets.zipWithNext().all { (left, right) ->
                left.kind.ordinal < right.kind.ordinal ||
                    left.kind == right.kind && left.targetId < right.targetId
            }
            return if (ids.any { !validId(it) } || !validText(policyRevision, MAX_REFERENCE_BYTES) ||
                targets.isEmpty() || targets.size > MAX_ERASURE_TARGETS || !ordered
            ) failure(MemoryErrorCode.INVALID_MEMORY)
            else MemoryContractResult.Success(
                MemoryErasureRequest(
                    requestId, namespaceId, recordId, revisionId, tombstoneFact,
                    policyRevision, targets,
                ),
            )
        }
    }
}

/** Per-target physical erasure outcome. */
public enum class ErasureTargetStatus { ERASED, NOT_PRESENT, PENDING_BACKUP_RETENTION, PENDING_RETRY }

/** Receipt-shaped result for one configured target. */
@ConsistentCopyVisibility
public data class MemoryErasureTargetResult private constructor(
    public val targetId: String,
    public val status: ErasureTargetStatus,
    public val receiptDigest: String,
    public val notBeforePosition: ULong?,
) {
    public companion object {
        /** Validates the target identity, receipt digest, and optional position shape. */
        public fun create(
            targetId: String,
            status: ErasureTargetStatus,
            receiptDigest: String,
            notBeforePosition: ULong?,
        ): MemoryContractResult<MemoryErasureTargetResult> =
            if (!validId(targetId) || !validDigest(receiptDigest) ||
                (status == ErasureTargetStatus.PENDING_BACKUP_RETENTION) != (notBeforePosition != null)
            ) failure(MemoryErrorCode.INVALID_MEMORY)
            else MemoryContractResult.Success(
                MemoryErasureTargetResult(targetId, status, receiptDigest, notBeforePosition),
            )
    }
}

/** Aggregate erasure state derived from every configured target. */
public enum class ErasureDisposition { COMPLETE, PARTIAL }

/** Immutable result of one complete target-coverage attempt. */
@ConsistentCopyVisibility
public data class MemoryErasureReceipt internal constructor(
    public val requestId: String,
    public val attemptId: String,
    public val attemptedAtPosition: ULong,
    public val results: List<MemoryErasureTargetResult>,
    public val disposition: ErasureDisposition,
)

/** Validates exact target coverage and derives Complete versus Partial. */
public fun recordMemoryErasure(
    request: MemoryErasureRequest,
    attemptId: String,
    attemptedAtPosition: ULong,
    results: List<MemoryErasureTargetResult>,
): MemoryContractResult<MemoryErasureReceipt> {
    if (!validId(attemptId) || attemptedAtPosition <= request.tombstoneFact.position ||
        results.size != request.targets.size
    ) return failure(MemoryErrorCode.INVALID_MEMORY)
    request.targets.zip(results).forEach { (target, result) ->
        if (target.targetId != result.targetId ||
            result.status == ErasureTargetStatus.PENDING_BACKUP_RETENTION &&
            (target.kind != ErasureTargetKind.BACKUP ||
                result.notBeforePosition == null || result.notBeforePosition <= attemptedAtPosition)
        ) return failure(MemoryErrorCode.INVALID_MEMORY)
    }
    val complete = results.all {
        it.status == ErasureTargetStatus.ERASED || it.status == ErasureTargetStatus.NOT_PRESENT
    }
    return MemoryContractResult.Success(
        MemoryErasureReceipt(
            request.requestId, attemptId, attemptedAtPosition, results,
            if (complete) ErasureDisposition.COMPLETE else ErasureDisposition.PARTIAL,
        ),
    )
}
