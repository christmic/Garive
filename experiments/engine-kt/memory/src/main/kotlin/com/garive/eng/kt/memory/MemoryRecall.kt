package com.garive.eng.kt.memory

/** Context product receiving a bounded memory selection. */
public enum class RecallProduct { MENU, DETAIL }

/** Exact integer scoring inputs supplied by a frozen retriever revision. */
public data class RecallScore(
    public val relevance: Int,
    public val recency: Int,
    public val importance: Int,
) {
    internal fun valid(): Boolean = listOf(relevance, recency, importance).all { it in 0..MAX_RECALL_SCORE }
    internal val total: Int get() = relevance + recency + importance
}

/** Authorized, scored metadata eligible for menu or detail selection. */
@ConsistentCopyVisibility
public data class MemoryRecallCandidate private constructor(
    public val recordId: String,
    public val revisionId: String,
    public val memoryType: MemoryType,
    public val role: MemoryKind,
    public val authority: MemoryAuthority,
    public val state: HypothesisState,
    public val safeLabel: String,
    public val contentDigest: String,
    public val contentBytes: ULong,
    public val evidenceCount: UInt,
    public val score: RecallScore,
) {
    public companion object {
        /** Validates one already-authorized candidate and its exact byte charge. */
        @Suppress("LongParameterList")
        public fun create(
            recordId: String, revisionId: String, memoryType: MemoryType, role: MemoryKind,
            authority: MemoryAuthority, state: HypothesisState, safeLabel: String,
            contentDigest: String, contentBytes: ULong, evidenceCount: UInt, score: RecallScore,
        ): MemoryContractResult<MemoryRecallCandidate> =
            if (!validId(recordId) || !validId(revisionId) || !validText(safeLabel, MAX_MENU_LABEL_BYTES) ||
                !validDigest(contentDigest) || contentBytes == 0uL || evidenceCount == 0u || !score.valid()
            ) failure(MemoryErrorCode.INVALID_MEMORY)
            else MemoryContractResult.Success(
                MemoryRecallCandidate(recordId, revisionId, memoryType, role, authority, state, safeLabel,
                    contentDigest, contentBytes, evidenceCount, score),
            )
    }
}

/** Explicit deterministic exploration inputs. */
@ConsistentCopyVisibility
public data class RecallExploration private constructor(
    public val algorithmRevision: String,
    public val seed: ULong,
    public val slots: UInt,
) {
    public companion object {
        /** Admits only the implemented hash exploration revision and non-zero slots. */
        public fun create(algorithmRevision: String, seed: ULong, slots: UInt): MemoryContractResult<RecallExploration> =
            if (algorithmRevision != EXPLORATION_ALGORITHM || slots == 0u) {
                failure(MemoryErrorCode.SELECTION_UNREPLAYABLE)
            } else MemoryContractResult.Success(RecallExploration(algorithmRevision, seed, slots))
    }
}

/** Frozen filters and budgets for one menu or detail selection. */
@ConsistentCopyVisibility
public data class RecallSelectionRequest private constructor(
    public val product: RecallProduct,
    public val allowedTypes: List<MemoryType>,
    public val allowedRoles: List<MemoryKind>,
    public val allowedStates: List<HypothesisState>,
    public val selectionPolicyRevision: String,
    public val maxItems: UInt,
    public val maxTotalBytes: ULong,
    public val exploration: RecallExploration?,
) {
    public companion object {
        /** Validates canonical filters, product rules, budgets and exploration shape. */
        @Suppress("LongParameterList")
        public fun create(
            product: RecallProduct, allowedTypes: List<MemoryType>, allowedRoles: List<MemoryKind>,
            allowedStates: List<HypothesisState>, selectionPolicyRevision: String,
            maxItems: UInt, maxTotalBytes: ULong, exploration: RecallExploration?,
        ): MemoryContractResult<RecallSelectionRequest> {
            if (allowedTypes.isEmpty() || !orderedRecallEnums(allowedTypes) || allowedRoles.isEmpty() ||
                !orderedRecallEnums(allowedRoles) || allowedStates.isEmpty() || !orderedRecallEnums(allowedStates) ||
                !validText(selectionPolicyRevision, MAX_REFERENCE_BYTES) || maxItems !in 1u..MAX_RECALL_ITEMS ||
                maxTotalBytes !in 1uL..MAX_RECALL_BYTES
            ) return failure(MemoryErrorCode.INVALID_MEMORY)
            if (HypothesisState.PROMOTED in allowedStates ||
                product == RecallProduct.MENU && HypothesisState.ARCHIVED in allowedStates ||
                HypothesisState.CANDIDATE in allowedStates && exploration == null ||
                exploration?.slots?.let { it > maxItems } == true
            ) return failure(MemoryErrorCode.SELECTION_UNREPLAYABLE)
            return MemoryContractResult.Success(
                RecallSelectionRequest(product, allowedTypes, allowedRoles, allowedStates,
                    selectionPolicyRevision, maxItems, maxTotalBytes, exploration),
            )
        }
    }
}

/** Why an item entered one exact result. */
public enum class RecallSelectionKind { RANKED, EXPLORED }

/** One selected item and optional committed exploration draw. */
public data class RecallSelectionItem(
    public val candidate: MemoryRecallCandidate,
    public val kind: RecallSelectionKind,
    public val drawHex: String?,
)

/** Ordered bounded selection suitable for commit-before-context. */
public data class RecallSelection(public val items: List<RecallSelectionItem>, public val truncated: Boolean)

/** Selects authorized candidates under exact deterministic and exploration rules. */
public fun selectRecall(
    candidates: List<MemoryRecallCandidate>,
    request: RecallSelectionRequest,
): MemoryContractResult<RecallSelection> {
    if (candidates.map { it.recordId to it.revisionId }.toSet().size != candidates.size) {
        return failure(MemoryErrorCode.INVALID_MEMORY)
    }
    val eligible = candidates.filter {
        it.memoryType in request.allowedTypes && it.role in request.allowedRoles && it.state in request.allowedStates
    }
    var exploredBytes = 0uL
    val explored = mutableListOf<RecallSelectionItem>()
    request.exploration?.let { config ->
        val draws = eligible.filter { it.state == HypothesisState.CANDIDATE }
            .map { it to explorationDraw(config, it) }
            .sortedWith(compareBy<Pair<MemoryRecallCandidate, String>> { it.second }
                .thenBy { it.first.recordId }.thenBy { it.first.revisionId })
        for ((candidate, draw) in draws.take(config.slots.toInt())) {
            val next = exploredBytes + candidate.contentBytes
            if (next < exploredBytes || next > request.maxTotalBytes) break
            exploredBytes = next
            explored += RecallSelectionItem(candidate, RecallSelectionKind.EXPLORED, draw)
        }
    }
    val exploredIds = explored.map { it.candidate.recordId to it.candidate.revisionId }.toSet()
    val ranked = eligible.filter {
        it.state != HypothesisState.CANDIDATE && (it.recordId to it.revisionId) !in exploredIds
    }.sortedWith(compareByDescending<MemoryRecallCandidate> { it.score.total }
        .thenByDescending { it.score.relevance }.thenByDescending { it.score.recency }
        .thenByDescending { it.score.importance }.thenBy { it.recordId }.thenBy { it.revisionId })
    var bytes = exploredBytes
    val rankedItems = mutableListOf<RecallSelectionItem>()
    for (candidate in ranked.take(request.maxItems.toInt() - explored.size)) {
        val next = bytes + candidate.contentBytes
        if (next < bytes || next > request.maxTotalBytes) break
        bytes = next
        rankedItems += RecallSelectionItem(candidate, RecallSelectionKind.RANKED, null)
    }
    val items = rankedItems + explored
    return MemoryContractResult.Success(RecallSelection(items, items.size < eligible.size))
}

private fun explorationDraw(config: RecallExploration, candidate: MemoryRecallCandidate): String =
    sha256(listOf(EXPLORATION_DOMAIN, config.algorithmRevision, config.seed.toString(),
        candidate.recordId, candidate.revisionId).joinToString("\u0000").encodeToByteArray()).take(16)

private fun <T : Enum<T>> orderedRecallEnums(values: List<T>): Boolean =
    values.zipWithNext().all { (left, right) -> left.ordinal < right.ordinal }

private const val MAX_MENU_LABEL_BYTES: Int = 256
private const val MAX_RECALL_SCORE: Int = 10_000
private const val MAX_RECALL_ITEMS: UInt = 256u
private const val MAX_RECALL_BYTES: ULong = 1_048_576uL
private const val EXPLORATION_ALGORITHM: String = "hash-explore-v1"
private const val EXPLORATION_DOMAIN: String = "garive.memory-explore.v1"
