package com.garive.eng.kt.llm

import kotlin.time.Duration

/** Provider-neutral count that keeps missing usage distinct from zero. */
public sealed interface TokenCount {
    /** A reported or deliberately estimated token [value]. */
    public data class Known(public val value: ULong) : TokenCount
    /** No trustworthy token count was available. */
    public data object Unknown : TokenCount
}

/** Checked sum of input and output token evidence. */
public sealed interface UsageTotal {
    /** Both components were known and added without overflow. */
    public data class Known(public val value: ULong) : UsageTotal
    /** At least one component was unknown. */
    public data object Unknown : UsageTotal
    /** Known components exceeded [ULong.MAX_VALUE]. */
    public data object Overflow : UsageTotal
}

/** Provenance of normalized usage evidence. */
public enum class UsageSource {
    /** Counts came from the provider. */
    PROVIDER_REPORTED,
    /** Counts came from an explicit conservative policy. */
    ESTIMATED,
}

/** Normalized usage evidence; cache counts are not re-added by [totalTokens]. */
public data class ModelUsage(
    public val inputTokens: TokenCount,
    public val outputTokens: TokenCount,
    public val cacheReadTokens: TokenCount? = null,
    public val cacheWriteTokens: TokenCount? = null,
    public val source: UsageSource,
) {
    /** Adds input and output counts using checked unsigned arithmetic. */
    public fun totalTokens(): UsageTotal {
        val input = (inputTokens as? TokenCount.Known)?.value ?: return UsageTotal.Unknown
        val output = (outputTokens as? TokenCount.Known)?.value ?: return UsageTotal.Unknown
        if (ULong.MAX_VALUE - input < output) return UsageTotal.Overflow
        return UsageTotal.Known(input + output)
    }
}

/** Reasoning representation admitted across the provider-neutral boundary. */
public sealed interface ReasoningContent {
    /** Reasoning [text] that provider policy allows consumers to see. */
    public data class ModelVisible(public val text: String) : ReasoningContent
    /** Opaque provider [reference] retained without exposing hidden reasoning. */
    public data class OpaqueReference(public val reference: String) : ReasoningContent
}

/** Provider-neutral media classification. */
public sealed interface MediaKind {
    /** Still image content. */
    public data object Image : MediaKind
    /** Audio content. */
    public data object Audio : MediaKind
    /** Video content. */
    public data object Video : MediaKind
    /** Generic file content. */
    public data object File : MediaKind
    /** Forward-compatible media class with an adapter-supplied [name]. */
    public data class Other(public val name: String) : MediaKind
}

/** Ordered provider-neutral item used by model requests and outcomes. */
public sealed interface ModelItem {
    /** Ordinary generated [text]. */
    public data class Text(public val text: String) : ModelItem
    /** Valid provider-declared refusal [text]. */
    public data class Refusal(public val text: String) : ModelItem
    /** Model-visible or opaque reasoning [content]. */
    public data class Reasoning(public val content: ReasoningContent) : ModelItem
    /** Untrusted proposal to invoke [toolName] with [argumentsJson]. */
    public data class ToolIntent(
        public val modelCallId: String,
        public val toolName: String,
        public val argumentsJson: String,
    ) : ModelItem
    /** Neutral [resultJson] correlated to [modelCallId]. */
    public data class ToolObservation(
        public val modelCallId: String,
        public val resultJson: String,
    ) : ModelItem
    /** External media [reference] with its neutral [mediaKind]. */
    public data class MediaReference(
        public val mediaKind: MediaKind,
        public val reference: String,
    ) : ModelItem
}

/** Normalized reason a completed response stopped generating. */
public sealed interface ModelStopReason {
    /** Provider declared the turn complete. */
    public data object EndTurn : ModelStopReason
    /** Provider stopped to request a tool call. */
    public data object ToolUse : ModelStopReason
    /** A configured stop sequence matched. */
    public data object StopSequence : ModelStopReason
    /** Provider requested a resumable pause. */
    public data object PauseTurn : ModelStopReason
    /** Provider completed with a refusal. */
    public data object Refusal : ModelStopReason
    /** Forward-compatible normalized stop [name]. */
    public data class Other(public val name: String) : ModelStopReason
}

/** Request rejection before a valid model response. */
public enum class RejectionKind {
    CONTEXT_OVERFLOW,
    AUTHENTICATION,
    CONTENT_POLICY,
}

/** Invocation began but did not return a complete response. */
public enum class InterruptionKind {
    CANCELLED,
    OUTPUT_LIMIT,
    TRANSPORT,
}

/** Resource condition preventing model dispatch. */
public enum class UnavailableKind {
    RATE_LIMITED,
    MODEL_UNAVAILABLE,
    CIRCUIT_OPEN,
}

/** Field-free classification of [InvokeOutcome]. */
public enum class InvokeOutcomeKind {
    COMPLETED,
    REJECTED,
    INTERRUPTED,
    UNAVAILABLE,
}

/** Exactly one normalized fact envelope returned by [ModelPort]. */
public sealed interface InvokeOutcome {
    /** Stable top-level outcome class. */
    public val kind: InvokeOutcomeKind
    /** True only for [Completed]. */
    public val isSuccess: Boolean get() = this is Completed
    /** True only when [Interrupted.partialItems] may be present. */
    public val isPartial: Boolean get() = this is Interrupted

    /** Complete ordered output with normalized usage evidence. */
    public data class Completed(
        public val items: List<ModelItem>,
        public val usage: ModelUsage,
        public val stopReason: ModelStopReason,
    ) : InvokeOutcome {
        public override val kind: InvokeOutcomeKind = InvokeOutcomeKind.COMPLETED
    }

    /** Request rejection with bounded secret-free evidence. */
    public data class Rejected(
        public val reason: RejectionKind,
        public val sanitizedEvidence: String,
    ) : InvokeOutcome {
        public override val kind: InvokeOutcomeKind = InvokeOutcomeKind.REJECTED
    }

    /** Interrupted processing with valid output and usage observed so far. */
    public data class Interrupted(
        public val reason: InterruptionKind,
        public val partialItems: List<ModelItem>,
        public val usage: ModelUsage,
    ) : InvokeOutcome {
        public override val kind: InvokeOutcomeKind = InvokeOutcomeKind.INTERRUPTED
    }

    /** Resource unavailability with an optional provider retry delay. */
    public data class Unavailable(
        public val reason: UnavailableKind,
        public val retryAfter: Duration?,
    ) : InvokeOutcome {
        public override val kind: InvokeOutcomeKind = InvokeOutcomeKind.UNAVAILABLE
    }
}
