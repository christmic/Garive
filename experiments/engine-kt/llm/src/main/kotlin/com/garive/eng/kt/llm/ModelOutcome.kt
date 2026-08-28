package com.garive.eng.kt.llm

import kotlin.time.Duration

sealed interface TokenCount {
    data class Known(val value: ULong) : TokenCount
    data object Unknown : TokenCount
}

sealed interface UsageTotal {
    data class Known(val value: ULong) : UsageTotal
    data object Unknown : UsageTotal
    data object Overflow : UsageTotal
}

enum class UsageSource { PROVIDER_REPORTED, ESTIMATED }

data class ModelUsage(
    val inputTokens: TokenCount,
    val outputTokens: TokenCount,
    val cacheReadTokens: TokenCount? = null,
    val cacheWriteTokens: TokenCount? = null,
    val source: UsageSource,
) {
    fun totalTokens(): UsageTotal {
        val input = (inputTokens as? TokenCount.Known)?.value ?: return UsageTotal.Unknown
        val output = (outputTokens as? TokenCount.Known)?.value ?: return UsageTotal.Unknown
        if (ULong.MAX_VALUE - input < output) return UsageTotal.Overflow
        return UsageTotal.Known(input + output)
    }
}

sealed interface ReasoningContent {
    data class ModelVisible(val text: String) : ReasoningContent
    data class OpaqueReference(val reference: String) : ReasoningContent
}

sealed interface MediaKind {
    data object Image : MediaKind
    data object Audio : MediaKind
    data object Video : MediaKind
    data object File : MediaKind
    data class Other(val name: String) : MediaKind
}

sealed interface ModelItem {
    data class Text(val text: String) : ModelItem
    data class Reasoning(val content: ReasoningContent) : ModelItem
    data class ToolIntent(
        val modelCallId: String,
        val toolName: String,
        val argumentsJson: String,
    ) : ModelItem
    data class ToolObservation(val modelCallId: String, val resultJson: String) : ModelItem
    data class MediaReference(val mediaKind: MediaKind, val reference: String) : ModelItem
}

sealed interface ModelStopReason {
    data object EndTurn : ModelStopReason
    data object ToolUse : ModelStopReason
    data class Other(val name: String) : ModelStopReason
}

enum class RejectionKind { CONTEXT_OVERFLOW, AUTHENTICATION, CONTENT_POLICY }
enum class InterruptionKind { CANCELLED, OUTPUT_LIMIT, TRANSPORT }
enum class UnavailableKind { RATE_LIMITED, MODEL_UNAVAILABLE, CIRCUIT_OPEN }
enum class InvokeOutcomeKind { COMPLETED, REJECTED, INTERRUPTED, UNAVAILABLE }

sealed interface InvokeOutcome {
    val kind: InvokeOutcomeKind
    val isSuccess: Boolean get() = this is Completed
    val isPartial: Boolean get() = this is Interrupted

    data class Completed(
        val items: List<ModelItem>,
        val usage: ModelUsage,
        val stopReason: ModelStopReason,
    ) : InvokeOutcome {
        override val kind = InvokeOutcomeKind.COMPLETED
    }

    data class Rejected(val reason: RejectionKind, val sanitizedEvidence: String) : InvokeOutcome {
        override val kind = InvokeOutcomeKind.REJECTED
    }

    data class Interrupted(
        val reason: InterruptionKind,
        val partialItems: List<ModelItem>,
        val usage: ModelUsage,
    ) : InvokeOutcome {
        override val kind = InvokeOutcomeKind.INTERRUPTED
    }

    data class Unavailable(val reason: UnavailableKind, val retryAfter: Duration?) : InvokeOutcome {
        override val kind = InvokeOutcomeKind.UNAVAILABLE
    }
}
