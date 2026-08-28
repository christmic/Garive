package com.garive.runtime.server.agent

import com.garive.runtime.server.llm.MediaKind
import com.garive.runtime.server.llm.ModelInputContent
import com.garive.runtime.server.llm.ModelInputItem

enum class ContextPurpose { INFERENCE, GOVERNANCE, TOOL_PREPARATION, SUMMARIZATION }

enum class CandidateKind {
    INSTRUCTION,
    USER_INPUT,
    MODEL_OUTPUT,
    TOOL_OBSERVATION,
    APPROVAL,
    SUMMARY,
    SYSTEM_NOTICE,
}

enum class Retention { REQUIRED, OPTIONAL }

sealed interface Visibility {
    data object Visible : Visibility
    data object Redacted : Visibility
    data class Purposes(val purposes: Set<ContextPurpose>) : Visibility
}

data class FactRef(val sessionId: String, val position: ULong)

data class ContextCandidate(
    val factRef: FactRef,
    val kind: CandidateKind,
    val retention: Retention,
    val visibility: Visibility,
    val items: List<ModelInputItem>,
)

data class ContextRequest(
    val sessionId: String,
    val turnId: String,
    val purpose: ContextPurpose,
    val afterPosition: ULong?,
    val throughPosition: ULong,
    val maxItems: Int,
    val maxUtf8Bytes: Int,
)

sealed interface ContextItem {
    data class Input(val factRef: FactRef, val item: ModelInputItem) : ContextItem
    data class RedactedItem(val factRef: FactRef) : ContextItem
}

data class ContextSurface(
    val purpose: ContextPurpose,
    val fromPosition: ULong,
    val throughPosition: ULong,
    val items: List<ContextItem>,
    val retainedRefs: List<FactRef>,
    val droppedRefs: List<FactRef>,
    val filteredRefs: List<FactRef>,
    val itemCount: Int,
    val utf8Bytes: Int,
)

sealed interface ContextDerivationResult {
    data class Success(val surface: ContextSurface) : ContextDerivationResult
    data class Failure(val error: ContextDerivationError) : ContextDerivationResult
}

sealed class ContextDerivationError(val code: String) {
    data object InvalidRequest : ContextDerivationError("invalid-request")
    data object SessionMismatch : ContextDerivationError("session-mismatch")
    data object PositionBeyondSurface : ContextDerivationError("position-beyond-surface")
    data object NonIncreasingPosition : ContextDerivationError("non-increasing-position")
    data object DuplicateReference : ContextDerivationError("duplicate-reference")
    data object EmptyRequiredContent : ContextDerivationError("empty-required-content")
    data object InvalidVisibility : ContextDerivationError("invalid-visibility")
    data object BudgetOverflow : ContextDerivationError("budget-overflow")
    data class RequiredFactsExceedBudget(val itemCount: Int, val utf8Bytes: Int) :
        ContextDerivationError("required-facts-exceed-budget")
}

private data class Eligible(
    val candidate: ContextCandidate,
    val itemCount: Int,
    val utf8Bytes: Int,
    val redacted: Boolean,
)

fun deriveContext(
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
            items += value.candidate.items.map { ContextItem.Input(value.candidate.factRef, it) }
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
        is ModelInputItem.ReasoningReference -> listOf(item.reference)
    }
    var total = 0
    for (value in strings) total = checkedAdd(total, value.encodeToByteArray().size) ?: return null
    return total
}

private fun checkedAdd(left: Int, right: Int): Int? =
    if (right < 0 || Int.MAX_VALUE - left < right) null else left + right
