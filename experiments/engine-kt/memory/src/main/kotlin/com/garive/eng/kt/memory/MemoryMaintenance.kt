package com.garive.eng.kt.memory

private const val MAX_CANDIDATE_EVIDENCE: Int = 64
private const val MAX_AUDIT_ENTRIES: Int = 4_096
private const val MAX_AUDIT_CONTRADICTIONS: Int = 4_096

/** Explicit origin of one untrusted candidate. */
public enum class MemoryCandidateSource {
    EXPLICIT_USER_COMMAND, SESSION_END, EXIT_SUMMARY, SCHEDULED_DISTILLATION,
}

/** Candidate operation before authority and durable M0 mutation. */
public sealed interface MemoryCandidateIntent {
    /** Learned content with explicit classification and evidence. */
    public data class Learn(
        public val memoryType: MemoryType,
        public val role: MemoryKind,
        public val authority: MemoryAuthorityBinding,
        public val scope: MemoryScopeBinding,
        public val content: ContentBinding,
        public val contentBytes: ULong,
        public val evidence: List<DurableFactReference>,
    ) : MemoryCandidateIntent

    /** Explicit request to forget one exact active revision. */
    public data class Forget(
        public val recordId: String,
        public val revisionId: String,
        public val authority: MemoryAuthorityBinding,
    ) : MemoryCandidateIntent
}

/** Bounded untrusted input to the four-decision reducer. */
@ConsistentCopyVisibility
public data class MemoryCandidate private constructor(
    public val candidateId: String,
    public val namespaceId: String,
    public val extractorRevision: String,
    public val source: MemoryCandidateSource,
    public val intent: MemoryCandidateIntent,
) {
    public companion object {
        /** Validates source authority, content bytes, and evidence ordering. */
        public fun create(
            candidateId: String,
            namespaceId: String,
            extractorRevision: String,
            source: MemoryCandidateSource,
            intent: MemoryCandidateIntent,
        ): MemoryContractResult<MemoryCandidate> {
            if (!validId(candidateId) || !validId(namespaceId) ||
                !validText(extractorRevision, MAX_REFERENCE_BYTES)
            ) return failure(MemoryErrorCode.INVALID_MEMORY)
            val validIntent = when (intent) {
                is MemoryCandidateIntent.Learn -> {
                    val expected = if (source == MemoryCandidateSource.EXPLICIT_USER_COMMAND) {
                        MemoryAuthority.USER_DECLARED
                    } else {
                        MemoryAuthority.AGENT_LEARNED
                    }
                    intent.authority.authority == expected && intent.contentBytes > 0uL &&
                        intent.evidence.isNotEmpty() && intent.evidence.size <= MAX_CANDIDATE_EVIDENCE &&
                        orderedUnique(intent.evidence) &&
                        intent.content.inlineUtf8?.encodeToByteArray()?.size?.toULong()
                            ?.let { it == intent.contentBytes } != false
                }
                is MemoryCandidateIntent.Forget ->
                    source == MemoryCandidateSource.EXPLICIT_USER_COMMAND &&
                        intent.authority.authority == MemoryAuthority.USER_DECLARED &&
                        validId(intent.recordId) && validId(intent.revisionId)
            }
            return if (!validIntent) failure(MemoryErrorCode.INVALID_MEMORY)
            else MemoryContractResult.Success(
                MemoryCandidate(candidateId, namespaceId, extractorRevision, source, intent),
            )
        }
    }
}

/** Stability conclusion supplied by a versioned admission policy. */
public enum class CandidateStability { CONFIRMED, UNCERTAIN }

/** Exact inputs to deterministic candidate admission. */
@ConsistentCopyVisibility
public data class AdmissionAssessment private constructor(
    public val generalizable: Boolean,
    public val stability: CandidateStability,
    public val exactDuplicateRevisionId: String?,
    public val conflictingActiveRevisionId: String?,
) {
    public companion object {
        /** Rejects ambiguous duplicate/conflict or malformed revision bindings. */
        public fun create(
            generalizable: Boolean,
            stability: CandidateStability,
            exactDuplicateRevisionId: String?,
            conflictingActiveRevisionId: String?,
        ): MemoryContractResult<AdmissionAssessment> =
            if (exactDuplicateRevisionId != null && conflictingActiveRevisionId != null ||
                exactDuplicateRevisionId?.let(::validId) == false ||
                conflictingActiveRevisionId?.let(::validId) == false
            ) failure(MemoryErrorCode.INVALID_MEMORY)
            else MemoryContractResult.Success(
                AdmissionAssessment(
                    generalizable, stability, exactDuplicateRevisionId, conflictingActiveRevisionId,
                ),
            )
    }
}

/** Safe reason why a candidate produced no write proposal. */
public enum class MaintenanceNoopCode { NOT_GENERALIZABLE, UNSTABLE_DEFERRED, DUPLICATE }

/** Explicit ADD/UPDATE/DELETE/NOOP output with no write authority. */
public sealed interface MemoryMaintenanceDecision {
    public data class Add(public val proposalId: String) : MemoryMaintenanceDecision
    public data class Update(
        public val proposalId: String,
        public val expectedActiveRevisionId: String,
    ) : MemoryMaintenanceDecision
    public data class Delete(
        public val commandId: String,
        public val recordId: String,
        public val revisionId: String,
    ) : MemoryMaintenanceDecision
    public data class Noop(public val code: MaintenanceNoopCode) : MemoryMaintenanceDecision
}

/** Reduces one candidate under the normative admission order. */
public fun decideCandidate(
    candidate: MemoryCandidate,
    assessment: AdmissionAssessment?,
    decisionId: String,
): MemoryContractResult<MemoryMaintenanceDecision> {
    if (!validId(decisionId)) return failure(MemoryErrorCode.INVALID_MEMORY)
    return when (val intent = candidate.intent) {
        is MemoryCandidateIntent.Forget -> if (assessment != null) {
            failure(MemoryErrorCode.INVALID_MEMORY)
        } else MemoryContractResult.Success(
            MemoryMaintenanceDecision.Delete(decisionId, intent.recordId, intent.revisionId),
        )
        is MemoryCandidateIntent.Learn -> when {
            assessment == null -> failure(MemoryErrorCode.INVALID_MEMORY)
            !assessment.generalizable -> successNoop(MaintenanceNoopCode.NOT_GENERALIZABLE)
            assessment.stability == CandidateStability.UNCERTAIN ->
                successNoop(MaintenanceNoopCode.UNSTABLE_DEFERRED)
            assessment.exactDuplicateRevisionId != null -> successNoop(MaintenanceNoopCode.DUPLICATE)
            assessment.conflictingActiveRevisionId != null -> MemoryContractResult.Success(
                MemoryMaintenanceDecision.Update(decisionId, assessment.conflictingActiveRevisionId),
            )
            else -> MemoryContractResult.Success(MemoryMaintenanceDecision.Add(decisionId))
        }
    }
}

/** Exact scheduled distillation progress for one extractor and Session. */
@ConsistentCopyVisibility
public data class DistillationWatermark private constructor(
    public val extractorRevision: String,
    public val sessionId: String,
    public val throughPosition: ULong,
    public val batchDigest: String,
) {
    public companion object {
        /** Validates one non-zero checkpoint. */
        public fun create(
            extractorRevision: String,
            sessionId: String,
            throughPosition: ULong,
            batchDigest: String,
        ): MemoryContractResult<DistillationWatermark> =
            if (!validText(extractorRevision, MAX_REFERENCE_BYTES) || !validId(sessionId) ||
                throughPosition == 0uL || !validDigest(batchDigest)
            ) failure(MemoryErrorCode.INVALID_MEMORY)
            else MemoryContractResult.Success(
                DistillationWatermark(extractorRevision, sessionId, throughPosition, batchDigest),
            )
    }
}

/** Idempotent watermark reduction disposition. */
public enum class WatermarkDisposition { ADVANCED, REPLAYED }

/** Validates monotonic progress under one frozen extractor and Session binding. */
public fun advanceDistillation(
    prior: DistillationWatermark?,
    next: DistillationWatermark,
): MemoryContractResult<WatermarkDisposition> = when {
    prior == null -> MemoryContractResult.Success(WatermarkDisposition.ADVANCED)
    prior.extractorRevision != next.extractorRevision || prior.sessionId != next.sessionId ||
        next.throughPosition < prior.throughPosition ||
        next.throughPosition == prior.throughPosition && next.batchDigest != prior.batchDigest ->
        failure(MemoryErrorCode.INVALID_TRANSITION)
    next.throughPosition == prior.throughPosition ->
        MemoryContractResult.Success(WatermarkDisposition.REPLAYED)
    else -> MemoryContractResult.Success(WatermarkDisposition.ADVANCED)
}

/** Frozen policy controlling one read-only health audit. */
public data class MemoryAuditPolicy(
    public val maxActiveRecords: UInt,
    public val maxActiveBytes: ULong,
    public val staleAfterPositions: ULong,
    public val lowUseThreshold: ULong,
    public val maxReportItems: UInt,
)

/** One immutable inventory row supplied to audit. */
public data class MemoryAuditEntry(
    public val recordId: String,
    public val revisionId: String,
    public val memoryType: MemoryType,
    public val state: HypothesisState,
    public val contentDigest: String,
    public val contentBytes: ULong,
    public val useCount: ULong,
    public val lastVerifiedPosition: ULong,
    public val retentionScoreBasisPoints: UInt,
) {
    internal val identity: MemoryIdentity get() = MemoryIdentity(recordId, revisionId)
}

/** Canonically ordered record/revision identity. */
public data class MemoryIdentity(public val recordId: String, public val revisionId: String) : Comparable<MemoryIdentity> {
    public override fun compareTo(other: MemoryIdentity): Int =
        compareValuesBy(this, other, MemoryIdentity::recordId, MemoryIdentity::revisionId)
}

/** Explicit contradiction candidate from a versioned detector. */
public data class MemoryContradiction(public val left: MemoryIdentity, public val right: MemoryIdentity)

/** Read-only maintenance proposal; applying it requires a later durable transition. */
public sealed interface MemoryAuditAction {
    public data class Cool(public val identity: MemoryIdentity) : MemoryAuditAction
    public data class Archive(public val identity: MemoryIdentity) : MemoryAuditAction
}

/** Bounded deterministic audit report with no mutation authority. */
public data class MemoryAuditReport(
    public val duplicateGroups: List<List<MemoryIdentity>>,
    public val contradictions: List<MemoryContradiction>,
    public val stale: List<MemoryIdentity>,
    public val lowUse: List<MemoryIdentity>,
    public val actions: List<MemoryAuditAction>,
    public val truncated: Boolean,
)

/** Audits canonical inventory under frozen position and policy inputs. */
public fun auditMemory(
    entries: List<MemoryAuditEntry>,
    contradictions: List<MemoryContradiction>,
    currentPosition: ULong,
    policy: MemoryAuditPolicy,
): MemoryContractResult<MemoryAuditReport> {
    if (currentPosition == 0uL || policy.maxActiveRecords == 0u || policy.maxActiveBytes == 0uL ||
        policy.staleAfterPositions == 0uL || policy.maxReportItems == 0u ||
        policy.maxReportItems > Int.MAX_VALUE.toUInt() || entries.size > MAX_AUDIT_ENTRIES ||
        contradictions.size > MAX_AUDIT_CONTRADICTIONS ||
        !orderedUnique(entries.map { it.identity }) || entries.any { !validAuditEntry(it, currentPosition) }
    ) return failure(MemoryErrorCode.INVALID_MEMORY)
    val identities = entries.map { it.identity }.toSet()
    if (!contradictions.zipWithNext().all { (left, right) ->
            left.left < right.left || left.left == right.left && left.right < right.right
        } || contradictions.any {
            it.left >= it.right || it.left !in identities || it.right !in identities
        }
    ) return failure(MemoryErrorCode.INVALID_MEMORY)
    val duplicates = entries.groupBy { it.contentDigest }.toSortedMap().values
        .map { group -> group.map { it.identity } }.filter { it.size > 1 }
    val stale = entries.filter {
        it.state != HypothesisState.PROMOTED && currentPosition - it.lastVerifiedPosition >= policy.staleAfterPositions
    }.map { it.identity }
    val lowUse = entries.filter {
        it.state != HypothesisState.PROMOTED && it.useCount < policy.lowUseThreshold
    }.map { it.identity }
    val activeEntries = entries.filter { it.state == HypothesisState.ACTIVE }
    var activeCount = activeEntries.size.toUInt()
    var activeBytes = 0uL
    activeEntries.forEach { entry ->
        if (ULong.MAX_VALUE - activeBytes < entry.contentBytes) {
            return failure(MemoryErrorCode.INVALID_MEMORY)
        }
        activeBytes += entry.contentBytes
    }
    val actions = mutableListOf<MemoryAuditAction>()
    activeEntries.sortedWith(compareBy(
            MemoryAuditEntry::retentionScoreBasisPoints, MemoryAuditEntry::useCount,
            MemoryAuditEntry::lastVerifiedPosition, MemoryAuditEntry::recordId,
            MemoryAuditEntry::revisionId,
        )).forEach { entry ->
            if (activeCount > policy.maxActiveRecords || activeBytes > policy.maxActiveBytes) {
                activeCount--
                activeBytes -= entry.contentBytes
                actions += MemoryAuditAction.Cool(entry.identity)
            }
        }
    entries.filter {
        it.state == HypothesisState.COLD && currentPosition - it.lastVerifiedPosition >= policy.staleAfterPositions
    }.forEach { actions += MemoryAuditAction.Archive(it.identity) }
    if (actions.size > policy.maxReportItems.toInt()) return failure(MemoryErrorCode.LIMIT_EXCEEDED)
    var remaining = policy.maxReportItems.toInt() - actions.size
    var truncated = false
    fun <T> bounded(values: List<T>): List<T> {
        val selected = values.take(remaining)
        if (selected.size != values.size) truncated = true
        remaining -= selected.size
        return selected
    }
    return MemoryContractResult.Success(
        MemoryAuditReport(
            bounded(duplicates), bounded(contradictions), bounded(stale), bounded(lowUse),
            actions, truncated,
        ),
    )
}

private fun successNoop(code: MaintenanceNoopCode): MemoryContractResult<MemoryMaintenanceDecision> =
    MemoryContractResult.Success(MemoryMaintenanceDecision.Noop(code))

private fun validAuditEntry(entry: MemoryAuditEntry, currentPosition: ULong): Boolean =
    validId(entry.recordId) && validId(entry.revisionId) && validDigest(entry.contentDigest) &&
        entry.contentBytes > 0uL && entry.lastVerifiedPosition > 0uL &&
        entry.lastVerifiedPosition <= currentPosition && entry.retentionScoreBasisPoints <= MAX_BASIS_POINTS.toUInt()
