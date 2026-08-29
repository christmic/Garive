package com.garive.eng.kt.memory

import java.time.Instant

/** Untrusted candidate write carrying evidence but no authority. */
public class MemoryProposal private constructor(
    public val proposalId: String,
    public val namespaceId: String,
    public val scope: MemoryScope,
    public val kind: MemoryKind,
    public val content: ContentBinding,
    public val evidence: List<DurableFactReference>,
    public val sensitivity: MemorySensitivity,
    public val confidenceBasisPoints: Int,
    public val expectedActiveRevisionId: String?,
) {
    public companion object {
        /** Validates one proposal and its ordered non-empty evidence. */
        @Suppress("LongParameterList")
        public fun create(
            proposalId: String,
            namespaceId: String,
            scope: MemoryScope,
            kind: MemoryKind,
            content: ContentBinding,
            evidence: List<DurableFactReference>,
            sensitivity: MemorySensitivity,
            confidenceBasisPoints: Int,
            expectedActiveRevisionId: String?,
        ): MemoryContractResult<MemoryProposal> {
            if (!validId(proposalId) || !validId(namespaceId) || evidence.isEmpty() ||
                !orderedUnique(evidence) || confidenceBasisPoints !in 0..MAX_BASIS_POINTS ||
                expectedActiveRevisionId?.let { !validId(it) } == true
            ) return failure(MemoryErrorCode.INVALID_MEMORY)
            return MemoryContractResult.Success(
                MemoryProposal(
                    proposalId, namespaceId, scope, kind, content, evidence.toList(), sensitivity,
                    confidenceBasisPoints, expectedActiveRevisionId,
                ),
            )
        }
    }
}

/** Runtime-authorized immutable revision coordinates. */
public class MemoryCommit private constructor(
    public val recordId: String,
    public val revisionId: String,
    public val retentionPolicyDigest: String,
    public val validFromPosition: ULong,
    public val expiresAtUtc: String?,
    public val supersedesRevisionId: String?,
) {
    public companion object {
        /** Validates exact record, revision, retention, time and supersession bindings. */
        public fun create(
            recordId: String,
            revisionId: String,
            retentionPolicyDigest: String,
            validFromPosition: ULong,
            expiresAtUtc: String?,
            supersedesRevisionId: String?,
        ): MemoryContractResult<MemoryCommit> {
            if (!validId(recordId) || !validId(revisionId) || !validDigest(retentionPolicyDigest) ||
                validFromPosition == 0uL || expiresAtUtc?.let { !canonicalUtc(it) } == true ||
                supersedesRevisionId?.let { !validId(it) || it == revisionId } == true
            ) return failure(MemoryErrorCode.INVALID_MEMORY)
            return MemoryContractResult.Success(
                MemoryCommit(
                    recordId, revisionId, retentionPolicyDigest, validFromPosition,
                    expiresAtUtc, supersedesRevisionId,
                ),
            )
        }
    }
}

/** One immutable governed memory revision. */
public class MemoryRecord private constructor(
    public val recordId: String,
    public val revisionId: String,
    public val namespaceId: String,
    public val scope: MemoryScope,
    public val kind: MemoryKind,
    public val content: ContentBinding,
    public val evidence: List<DurableFactReference>,
    public val status: MemoryStatus,
    public val sensitivity: MemorySensitivity,
    public val confidenceBasisPoints: Int,
    public val validFromPosition: ULong,
    public val supersedesRevisionId: String?,
    public val expiresAtUtc: String?,
) {
    internal fun withStatus(status: MemoryStatus): MemoryRecord = MemoryRecord(
        recordId, revisionId, namespaceId, scope, kind, content, evidence, status, sensitivity,
        confidenceBasisPoints, validFromPosition, supersedesRevisionId, expiresAtUtc,
    )

    public companion object {
        /** Validates a complete immutable record revision. */
        @Suppress("LongParameterList")
        public fun create(
            recordId: String,
            revisionId: String,
            namespaceId: String,
            scope: MemoryScope,
            kind: MemoryKind,
            content: ContentBinding,
            evidence: List<DurableFactReference>,
            status: MemoryStatus,
            sensitivity: MemorySensitivity,
            confidenceBasisPoints: Int,
            validFromPosition: ULong,
            supersedesRevisionId: String?,
            expiresAtUtc: String?,
        ): MemoryContractResult<MemoryRecord> {
            if (!validId(recordId) || !validId(revisionId) || !validId(namespaceId) ||
                evidence.isEmpty() || !orderedUnique(evidence) ||
                confidenceBasisPoints !in 0..MAX_BASIS_POINTS || validFromPosition == 0uL ||
                supersedesRevisionId?.let { !validId(it) } == true ||
                expiresAtUtc?.let { !canonicalUtc(it) } == true
            ) return failure(MemoryErrorCode.INVALID_MEMORY)
            return MemoryContractResult.Success(
                MemoryRecord(
                    recordId, revisionId, namespaceId, scope, kind, content, evidence.toList(),
                    status, sensitivity, confidenceBasisPoints, validFromPosition,
                    supersedesRevisionId, expiresAtUtc,
                ),
            )
        }
    }

    public override fun equals(other: Any?): Boolean = other is MemoryRecord &&
        recordId == other.recordId && revisionId == other.revisionId && status == other.status &&
        namespaceId == other.namespaceId && scope == other.scope && kind == other.kind &&
        content == other.content && evidence == other.evidence && sensitivity == other.sensitivity &&
        confidenceBasisPoints == other.confidenceBasisPoints && validFromPosition == other.validFromPosition &&
        supersedesRevisionId == other.supersedesRevisionId && expiresAtUtc == other.expiresAtUtc

    public override fun hashCode(): Int = listOf(
        recordId, revisionId, namespaceId, scope, kind, content, evidence, status, sensitivity,
        confidenceBasisPoints, validFromPosition, supersedesRevisionId, expiresAtUtc,
    ).hashCode()
}

/** Exact old/new binding emitted with an accepted supersession. */
public data class MemorySupersession(
    public val recordId: String,
    public val oldRevisionId: String,
    public val newRevisionId: String,
    public val proposalId: String,
)

/** Successful pure write reduction. */
public data class MemoryWriteOutcome(
    public val record: MemoryRecord,
    public val supersession: MemorySupersession?,
)

/** Exact tombstone command target. */
public data class MemoryTombstone(public val recordId: String, public val revisionId: String)

/** Deterministic append-only record state used by Runtime projections. */
public class MemoryState private constructor(revisions: List<MemoryRecord>) {
    private val mutableRevisions: MutableList<MemoryRecord> = revisions.toMutableList()

    /** All immutable revisions in projection order. */
    public val revisions: List<MemoryRecord> get() = mutableRevisions.toList()

    /** Applies an authorized commit atomically or leaves state unchanged. */
    public fun commit(
        proposal: MemoryProposal,
        commit: MemoryCommit,
    ): MemoryContractResult<MemoryWriteOutcome> {
        val activeIndex = mutableRevisions.indexOfFirst {
            it.recordId == commit.recordId && it.status == MemoryStatus.ACTIVE
        }
        val activeRevision = mutableRevisions.getOrNull(activeIndex)?.revisionId
        if (activeRevision != proposal.expectedActiveRevisionId ||
            activeRevision != commit.supersedesRevisionId ||
            mutableRevisions.any { it.recordId == commit.recordId && it.revisionId == commit.revisionId }
        ) return failure(MemoryErrorCode.REVISION_CONFLICT)
        val created = MemoryRecord.create(
            commit.recordId, commit.revisionId, proposal.namespaceId, proposal.scope, proposal.kind,
            proposal.content, proposal.evidence, MemoryStatus.ACTIVE, proposal.sensitivity,
            proposal.confidenceBasisPoints, commit.validFromPosition, commit.supersedesRevisionId,
            commit.expiresAtUtc,
        )
        val record = (created as? MemoryContractResult.Success)?.value
            ?: return created as MemoryContractResult.Failure
        val supersession = if (activeIndex < 0) null else MemorySupersession(
            commit.recordId, activeRevision!!, commit.revisionId, proposal.proposalId,
        )
        if (activeIndex >= 0) mutableRevisions[activeIndex] = mutableRevisions[activeIndex].withStatus(MemoryStatus.SUPERSEDED)
        mutableRevisions += record
        return MemoryContractResult.Success(MemoryWriteOutcome(record, supersession))
    }

    /** Tombstones only the exact active revision. */
    public fun tombstone(target: MemoryTombstone): MemoryContractResult<Unit> {
        val index = mutableRevisions.indexOfFirst {
            it.recordId == target.recordId && it.revisionId == target.revisionId
        }
        if (index < 0 || mutableRevisions[index].status != MemoryStatus.ACTIVE) {
            return failure(MemoryErrorCode.REVISION_CONFLICT)
        }
        mutableRevisions[index] = mutableRevisions[index].withStatus(MemoryStatus.TOMBSTONED)
        return MemoryContractResult.Success(Unit)
    }

    public companion object {
        /** Validates unique revision identities and at most one active revision per record. */
        public fun create(revisions: List<MemoryRecord>): MemoryContractResult<MemoryState> {
            val identities = revisions.map { it.recordId to it.revisionId }
            val active = revisions.filter { it.status == MemoryStatus.ACTIVE }.map { it.recordId }
            if (identities.distinct().size != identities.size || active.distinct().size != active.size) {
                return failure(MemoryErrorCode.CORRUPT_MEMORY_STATE)
            }
            return MemoryContractResult.Success(MemoryState(revisions))
        }
    }
}

internal fun canonicalUtc(value: String): Boolean = runCatching {
    Instant.parse(value).toString() == value
}.getOrDefault(false)
