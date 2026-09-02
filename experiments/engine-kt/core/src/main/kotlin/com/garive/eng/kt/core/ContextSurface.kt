package com.garive.eng.kt.core

import com.garive.eng.kt.llm.MediaKind
import com.garive.eng.kt.llm.ModelInputContent
import com.garive.eng.kt.llm.ModelInputItem
import com.garive.eng.kt.llm.ModelRole

/** Consumer-specific reason for deriving a bounded context surface. */
public enum class ContextPurpose { INFERENCE, GOVERNANCE, TOOL_PREPARATION, SUMMARIZATION }

/** Semantic class of a durable fact considered for context. */
public enum class CandidateKind {
    INSTRUCTION,
    SKILL,
    USER_INPUT,
    MODEL_OUTPUT,
    TOOL_OBSERVATION,
    APPROVAL,
    SUMMARY,
    SYSTEM_NOTICE,
    MEMORY,
    KNOWLEDGE,
}

/** Merges two independently ordered streams without repairing either. */
public fun mergeContextCandidates(
    base: List<ContextCandidate>,
    capability: List<ContextCandidate>,
): ContextMergeResult {
    streamOrderError(base)?.let { return ContextMergeResult.Failure(it) }
    streamOrderError(capability)?.let { return ContextMergeResult.Failure(it) }
    val merged = ArrayList<ContextCandidate>(base.size + capability.size)
    var left = 0
    var right = 0
    while (left < base.size && right < capability.size) {
        when {
            base[left].factRef.position < capability[right].factRef.position -> merged += base[left++]
            base[left].factRef.position > capability[right].factRef.position -> merged += capability[right++]
            else -> return ContextMergeResult.Failure(ContextDerivationError.DuplicateReference)
        }
    }
    merged += base.subList(left, base.size)
    merged += capability.subList(right, capability.size)
    return ContextMergeResult.Success(merged)
}

public sealed interface ContextMergeResult {
    public data class Success(public val candidates: List<ContextCandidate>) : ContextMergeResult
    public data class Failure(public val error: ContextDerivationError) : ContextMergeResult
}

private fun streamOrderError(values: List<ContextCandidate>): ContextDerivationError? =
    values.zipWithNext().firstNotNullOfOrNull { (left, right) ->
        when {
            left.factRef.position == right.factRef.position -> ContextDerivationError.DuplicateReference
            left.factRef.position > right.factRef.position -> ContextDerivationError.NonIncreasingPosition
            else -> null
        }
    }

/** Whether a candidate may be dropped under budget pressure. */
public enum class Retention { REQUIRED, OPTIONAL }

/** Purpose-based disclosure rule applied before budgeting. */
public sealed interface Visibility {
    public data object Visible : Visibility
    public data object Redacted : Visibility
    public data class Purposes(public val purposes: Set<ContextPurpose>) : Visibility
}

/** Stable reference to one ordered Session ledger fact. */
public data class FactRef(public val sessionId: String, public val position: ULong)

/** Input candidate presented to the pure derivation algorithm. */
public data class ContextCandidate(
    public val factRef: FactRef,
    public val kind: CandidateKind,
    public val retention: Retention,
    public val visibility: Visibility,
    public val items: List<ModelInputItem>,
)

/** Frozen ledger window and budgets for deterministic context derivation. */
public data class ContextRequest(
    public val sessionId: String,
    public val turnId: String,
    public val purpose: ContextPurpose,
    public val afterPosition: ULong?,
    public val throughPosition: ULong,
    public val maxItems: Int,
    public val maxUtf8Bytes: Int,
)

/** Auditable visible input or redaction emitted in a surface. */
public sealed interface ContextItem {
    public data class Input(
        public val factRef: FactRef,
        public val kind: CandidateKind,
        public val item: ModelInputItem,
    ) : ContextItem
    public data class RedactedItem(public val factRef: FactRef) : ContextItem
}

/** Deterministic bounded projection supplied to one context consumer. */
public data class ContextSurface(
    public val purpose: ContextPurpose,
    public val fromPosition: ULong,
    public val throughPosition: ULong,
    public val items: List<ContextItem>,
    public val retainedRefs: List<FactRef>,
    public val droppedRefs: List<FactRef>,
    public val filteredRefs: List<FactRef>,
    public val itemCount: Int,
    public val utf8Bytes: Int,
)

/** Success/failure envelope for pure context derivation. */
public sealed interface ContextDerivationResult {
    public data class Success(public val surface: ContextSurface) : ContextDerivationResult
    public data class Failure(public val error: ContextDerivationError) : ContextDerivationResult
}

/** Stable contract violation or bounded-derivation failure. */
public sealed class ContextDerivationError protected constructor(public val code: String) {
    public data object InvalidRequest : ContextDerivationError("invalid-request")
    public data object SessionMismatch : ContextDerivationError("session-mismatch")
    public data object PositionBeyondSurface : ContextDerivationError("position-beyond-surface")
    public data object NonIncreasingPosition : ContextDerivationError("non-increasing-position")
    public data object DuplicateReference : ContextDerivationError("duplicate-reference")
    public data object EmptyRequiredContent : ContextDerivationError("empty-required-content")
    public data object InvalidVisibility : ContextDerivationError("invalid-visibility")
    public data object BudgetOverflow : ContextDerivationError("budget-overflow")
    public data class RequiredFactsExceedBudget(
        public val itemCount: Int,
        public val utf8Bytes: Int,
    ) :
        ContextDerivationError("required-facts-exceed-budget")
}

private data class Eligible(
    val candidate: ContextCandidate,
    val itemCount: Int,
    val utf8Bytes: Int,
    val redacted: Boolean,
)

/**
 * Derives a deterministic surface from strictly ordered candidates.
 *
 * Visibility filtering precedes budgeting; required candidates must fit and
 * newest optional candidates are retained until either budget is exhausted.
 */
public fun deriveContext(
    request: ContextRequest,
    candidates: List<ContextCandidate>,
): ContextDerivationResult {
    if (request.sessionId.isEmpty() || request.turnId.isEmpty() ||
        request.throughPosition == 0uL || request.maxItems <= 0 || request.maxUtf8Bytes <= 0 ||
        request.afterPosition?.let { it >= request.throughPosition || it == ULong.MAX_VALUE } == true
    ) {
        return ContextDerivationResult.Failure(ContextDerivationError.InvalidRequest)
    }
    val fromPosition = (request.afterPosition ?: 0uL) + 1uL
    val filtered = mutableListOf<FactRef>()
    val eligible = mutableListOf<Eligible>()
    var lastPosition: ULong? = null

    for (candidate in candidates) {
        val validation = validateCandidate(request, candidate, lastPosition)
        if (validation != null) return ContextDerivationResult.Failure(validation)
        lastPosition = candidate.factRef.position
        val visible = when (val visibility = candidate.visibility) {
            Visibility.Visible, Visibility.Redacted -> true
            is Visibility.Purposes -> {
                if (visibility.purposes.isEmpty()) {
                    return ContextDerivationResult.Failure(ContextDerivationError.InvalidVisibility)
                }
                request.purpose in visibility.purposes
            }
        }
        if (candidate.factRef.position <= (request.afterPosition ?: 0uL) || !visible) {
            filtered += candidate.factRef
            continue
        }
        val redacted = candidate.visibility == Visibility.Redacted
        val cost = if (redacted) 1 to 0 else candidateCost(candidate.items)
            ?: return ContextDerivationResult.Failure(ContextDerivationError.BudgetOverflow)
        if (candidate.retention == Retention.REQUIRED && !redacted && (cost.first == 0 || cost.second == 0)) {
            return ContextDerivationResult.Failure(ContextDerivationError.EmptyRequiredContent)
        }
        eligible += Eligible(candidate, cost.first, cost.second, redacted)
    }

    var itemCount = 0
    var utf8Bytes = 0
    val retainedPositions = mutableSetOf<ULong>()
    for (value in eligible.filter { it.candidate.retention == Retention.REQUIRED }) {
        itemCount = checkedAdd(itemCount, value.itemCount)
            ?: return ContextDerivationResult.Failure(ContextDerivationError.BudgetOverflow)
        utf8Bytes = checkedAdd(utf8Bytes, value.utf8Bytes)
            ?: return ContextDerivationResult.Failure(ContextDerivationError.BudgetOverflow)
        retainedPositions += value.candidate.factRef.position
    }
    if (itemCount > request.maxItems || utf8Bytes > request.maxUtf8Bytes) {
        return ContextDerivationResult.Failure(
            ContextDerivationError.RequiredFactsExceedBudget(itemCount, utf8Bytes),
        )
    }
    for (value in eligible.asReversed()) {
        if (value.candidate.retention == Retention.REQUIRED) continue
        val nextItems = checkedAdd(itemCount, value.itemCount)
            ?: return ContextDerivationResult.Failure(ContextDerivationError.BudgetOverflow)
        val nextBytes = checkedAdd(utf8Bytes, value.utf8Bytes)
            ?: return ContextDerivationResult.Failure(ContextDerivationError.BudgetOverflow)
        if (nextItems <= request.maxItems && nextBytes <= request.maxUtf8Bytes) {
            itemCount = nextItems
            utf8Bytes = nextBytes
            retainedPositions += value.candidate.factRef.position
        }
    }

    val items = mutableListOf<ContextItem>()
    val retained = mutableListOf<FactRef>()
    val dropped = mutableListOf<FactRef>()
    for (value in eligible) {
        if (value.candidate.factRef.position !in retainedPositions) {
            dropped += value.candidate.factRef
            continue
        }
        retained += value.candidate.factRef
        if (value.redacted) {
            items += ContextItem.RedactedItem(value.candidate.factRef)
        } else {
            items += value.candidate.items.map {
                ContextItem.Input(value.candidate.factRef, value.candidate.kind, it)
            }
        }
    }
    return ContextDerivationResult.Success(
        ContextSurface(
            request.purpose,
            fromPosition,
            request.throughPosition,
            items,
            retained,
            dropped,
            filtered,
            itemCount,
            utf8Bytes,
        ),
    )
}

private fun validateCandidate(
    request: ContextRequest,
    candidate: ContextCandidate,
    lastPosition: ULong?,
): ContextDerivationError? {
    if (candidate.factRef.sessionId != request.sessionId) return ContextDerivationError.SessionMismatch
    if (candidate.factRef.position == 0uL || candidate.factRef.position > request.throughPosition) {
        return ContextDerivationError.PositionBeyondSurface
    }
    if (candidate.factRef.position == lastPosition) return ContextDerivationError.DuplicateReference
    if (lastPosition != null && candidate.factRef.position < lastPosition) {
        return ContextDerivationError.NonIncreasingPosition
    }
    if (candidate.retention == Retention.REQUIRED && candidate.items.isEmpty()) {
        return ContextDerivationError.EmptyRequiredContent
    }
    return null
}

private fun candidateCost(items: List<ModelInputItem>): Pair<Int, Int>? {
    var bytes = 0
    for (item in items) bytes = checkedAdd(bytes, itemUtf8Bytes(item) ?: return null) ?: return null
    return items.size to bytes
}

private fun itemUtf8Bytes(item: ModelInputItem): Int? {
    val strings = when (item) {
        is ModelInputItem.Message -> item.content.flatMap { content ->
            when (content) {
                is ModelInputContent.Text -> listOf(content.text)
                is ModelInputContent.MediaReference -> buildList {
                    add(content.reference)
                    add(content.mediaType)
                    val mediaKind = content.mediaKind
                    if (mediaKind is MediaKind.Other) add(mediaKind.name)
                }
            }
        }
        is ModelInputItem.ToolObservation -> listOf(item.modelCallId, item.resultJson)
        is ModelInputItem.ToolIntent -> listOf(item.modelCallId, item.toolName, item.argumentsJson)
        is ModelInputItem.ReasoningReference -> listOf(item.reference)
    }
    var total = 0
    for (value in strings) total = checkedAdd(total, value.encodeToByteArray().size) ?: return null
    return total
}

private fun checkedAdd(left: Int, right: Int): Int? =
    if (right < 0 || Int.MAX_VALUE - left < right) null else left + right

/** Assembles model inputs into instruction, Skill, evidence, then history groups. */
public fun assembleModelInputs(surface: ContextSurface): List<ModelInputItem> {
    val visible = surface.items.mapNotNull { it as? ContextItem.Input }
    val skills = visible.filter { it.kind == CandidateKind.SKILL }.map { it.item }
    val memory = visible.filter { it.kind == CandidateKind.MEMORY }.map { it.item }
    val knowledge = visible.filter { it.kind == CandidateKind.KNOWLEDGE }.map { it.item }
    val ordinary = visible.filter {
        it.kind !in setOf(CandidateKind.SKILL, CandidateKind.MEMORY, CandidateKind.KNOWLEDGE)
    }.map { it.item }
    val instructions = ordinary.filter { item ->
        item is ModelInputItem.Message && item.role in setOf(ModelRole.SYSTEM, ModelRole.DEVELOPER)
    }
    val history = ordinary.filterNot { item ->
        item is ModelInputItem.Message && item.role in setOf(ModelRole.SYSTEM, ModelRole.DEVELOPER)
    }
    return instructions + skills + memory + knowledge + history
}
