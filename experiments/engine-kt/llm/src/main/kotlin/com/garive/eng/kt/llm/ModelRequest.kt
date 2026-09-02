package com.garive.eng.kt.llm

/** Opaque identity for one logical provider-neutral request. */
public data class ModelRequestId(public val value: String)

/** Opaque identity of a frozen model capability target. */
public data class ModelTargetId(public val value: String)

/** Provider-neutral capability required from the selected target. */
public enum class ModelCapability { TEXT, VISION, REASONING, TOOLS, JSON_OUTPUT, STREAMING }

/** Semantic role of one ordered input message. */
public enum class ModelRole { SYSTEM, DEVELOPER, USER, ASSISTANT }

/** One content part inside a provider-neutral input message. */
public sealed interface ModelInputContent {
    /** UTF-8 message [text]. */
    public data class Text(public val text: String) : ModelInputContent

    /** External media [reference] with an asserted [mediaType]. */
    public data class MediaReference(
        public val mediaKind: MediaKind,
        public val reference: String,
        public val mediaType: String,
    ) : ModelInputContent
}

/** Ordered input item admitted to a model request. */
public sealed interface ModelInputItem {
    /** Role-bearing message with ordered [content]. */
    public data class Message(
        public val role: ModelRole,
        public val content: List<ModelInputContent>,
    ) : ModelInputItem

    /** Prior model tool invocation retained for protocol correlation. */
    public data class ToolIntent(
        public val modelCallId: String,
        public val toolName: String,
        public val argumentsJson: String,
    ) : ModelInputItem

    /** Neutral [resultJson] answering [modelCallId]. */
    public data class ToolObservation(
        public val modelCallId: String,
        public val resultJson: String,
    ) : ModelInputItem

    /** Opaque reasoning state returned to a compatible provider. */
    public data class ReasoningReference(public val reference: String) : ModelInputItem
}

/** Exact tool definition exposed to the model for one request. */
public data class ToolDescriptor(
    public val name: String,
    public val description: String,
    public val definitionRevision: String,
    public val inputSchemaJson: String,
    public val strict: Boolean,
)

/** Requested plain or structured response mode. */
public sealed interface TextMode {
    /** Unconstrained plain text. */
    public data object Plain : TextMode
    /** A syntactically valid JSON object. */
    public data object JsonObject : TextMode
    /** JSON output constrained by exact [schemaJson]. */
    public data class JsonSchema(public val schemaJson: String) : TextMode
}

/** Provider-neutral output constraints for one request. */
public data class ModelOutputSettings(
    public val maxOutputTokens: ULong?,
    public val textMode: TextMode,
    public val reasoningVisibility: Boolean,
)

/** Immutable provider-neutral input to [ModelPort]. */
public data class ModelRequest(
    public val requestId: ModelRequestId,
    public val targetId: ModelTargetId,
    public val requiredCapabilities: List<ModelCapability>,
    public val inputItems: List<ModelInputItem>,
    public val tools: List<ToolDescriptor>,
    public val output: ModelOutputSettings,
    public val traceMetadata: List<Pair<String, String>>,
) {
    /** Validates identities, duplicates, tool definitions, limits, and metadata. */
    public fun validate(): RequestValidationError? {
        if (requestId.value.isEmpty() || targetId.value.isEmpty()) return RequestValidationError.EMPTY_IDENTITY
        if (requiredCapabilities.distinct().size != requiredCapabilities.size) {
            return RequestValidationError.DUPLICATE_CAPABILITY
        }
        val names = mutableSetOf<String>()
        tools.forEach { tool ->
            if (tool.name.isEmpty() || tool.definitionRevision.isEmpty() || tool.inputSchemaJson.isEmpty()) {
                return RequestValidationError.INVALID_TOOL
            }
            if (!names.add(tool.name)) return RequestValidationError.DUPLICATE_TOOL
        }
        if (output.maxOutputTokens == 0uL) return RequestValidationError.ZERO_OUTPUT_LIMIT
        val keys = mutableSetOf<String>()
        traceMetadata.forEach { (key, value) ->
            if (key.isEmpty() || key.toByteArray().size > 64 || value.toByteArray().size > 512) {
                return RequestValidationError.INVALID_METADATA
            }
            if (!keys.add(key)) return RequestValidationError.DUPLICATE_METADATA
        }
        return null
    }
}

/** Stable validation failure for [ModelRequest]. */
public enum class RequestValidationError(public val code: String) {
    EMPTY_IDENTITY("empty-identity"),
    DUPLICATE_CAPABILITY("duplicate-capability"),
    INVALID_TOOL("invalid-tool"),
    DUPLICATE_TOOL("duplicate-tool"),
    ZERO_OUTPUT_LIMIT("zero-output-limit"),
    INVALID_METADATA("invalid-metadata"),
    DUPLICATE_METADATA("duplicate-metadata"),
}

/** Expected semantic class for one indexed streaming item. */
public sealed interface ModelOutputKind {
    public data object Text : ModelOutputKind
    public data object Refusal : ModelOutputKind
    public data object Reasoning : ModelOutputKind
    public data class ToolIntent(public val modelCallId: String) : ModelOutputKind
    public data object ToolObservation : ModelOutputKind
    public data object MediaReference : ModelOutputKind
}

/** Ordered provider-neutral progress event for one invocation. */
public sealed interface ModelStreamEvent {
    public data class OutputItemStarted(
        public val outputIndex: UInt,
        public val kind: ModelOutputKind,
    ) : ModelStreamEvent
    public data class TextDelta(public val outputIndex: UInt, public val delta: String) : ModelStreamEvent
    public data class RefusalDelta(public val outputIndex: UInt, public val delta: String) : ModelStreamEvent
    public data class ReasoningDelta(public val outputIndex: UInt, public val delta: String) : ModelStreamEvent
    public data class ToolArgumentsDelta(
        public val outputIndex: UInt,
        public val modelCallId: String,
        public val delta: String,
    ) : ModelStreamEvent
    public data class OutputItemCompleted(
        public val outputIndex: UInt,
        public val item: ModelItem,
    ) : ModelStreamEvent
    public data class UsageUpdated(public val usage: ModelUsage) : ModelStreamEvent
}

/** Stateful validator for indexed stream ordering and item kinds. */
public class StreamValidator {
    private val started = mutableMapOf<UInt, ModelOutputKind>()
    private val completed = mutableSetOf<UInt>()
    private var lastStarted: UInt? = null

    /** Applies one event without silently repairing an invalid sequence. */
    public fun accept(event: ModelStreamEvent): StreamInvariantError? = when (event) {
        is ModelStreamEvent.OutputItemStarted -> {
            if (lastStarted?.let { event.outputIndex <= it } == true) {
                StreamInvariantError.NON_MONOTONIC_START
            } else {
                started[event.outputIndex] = event.kind
                lastStarted = event.outputIndex
                null
            }
        }
        is ModelStreamEvent.TextDelta -> requireKind(event.outputIndex, ModelOutputKind.Text)
        is ModelStreamEvent.RefusalDelta -> requireKind(event.outputIndex, ModelOutputKind.Refusal)
        is ModelStreamEvent.ReasoningDelta -> requireKind(event.outputIndex, ModelOutputKind.Reasoning)
        is ModelStreamEvent.ToolArgumentsDelta -> {
            requireKind(event.outputIndex, ModelOutputKind.ToolIntent(event.modelCallId))
        }
        is ModelStreamEvent.OutputItemCompleted -> {
            if (event.outputIndex in completed) {
                StreamInvariantError.ITEM_ALREADY_COMPLETED
            } else {
                requireKind(event.outputIndex, kindOf(event.item))?.also { return it }
                completed += event.outputIndex
                null
            }
        }
        is ModelStreamEvent.UsageUpdated -> null
    }

    private fun requireKind(index: UInt, expected: ModelOutputKind): StreamInvariantError? {
        val actual = started[index] ?: return StreamInvariantError.ITEM_NOT_STARTED
        if (index in completed) return StreamInvariantError.ITEM_ALREADY_COMPLETED
        return if (actual == expected) null else StreamInvariantError.ITEM_KIND_MISMATCH
    }

    private fun kindOf(item: ModelItem): ModelOutputKind = when (item) {
        is ModelItem.Text -> ModelOutputKind.Text
        is ModelItem.Refusal -> ModelOutputKind.Refusal
        is ModelItem.Reasoning -> ModelOutputKind.Reasoning
        is ModelItem.ToolIntent -> ModelOutputKind.ToolIntent(item.modelCallId)
        is ModelItem.ToolObservation -> ModelOutputKind.ToolObservation
        is ModelItem.MediaReference -> ModelOutputKind.MediaReference
    }
}

/** Stable normalized stream contract violation. */
public enum class StreamInvariantError(public val code: String) {
    NON_MONOTONIC_START("non-monotonic-start"),
    ITEM_NOT_STARTED("item-not-started"),
    ITEM_ALREADY_COMPLETED("item-already-completed"),
    ITEM_KIND_MISMATCH("item-kind-mismatch"),
}

/** Backpressure/cancellation decision returned by [ModelObserver]. */
public enum class ObserverDecision { CONTINUE, CANCEL }

/** Consumer of ordered normalized live events. */
public fun interface ModelObserver {
    /** Observes one event and decides whether dispatch should continue. */
    public fun observe(event: ModelStreamEvent): ObserverDecision
}

/** Cooperative cancellation signal sampled by adapters. */
public fun interface ModelCancellation {
    /** Returns whether the enclosing execution requested cancellation. */
    public fun isCancelled(): Boolean
}

/** Model port contract failure rather than a valid provider outcome. */
public enum class ModelPortFailure {
    INVALID_REQUEST,
    UNSUPPORTED_CAPABILITY,
    ADAPTER_INVARIANT,
    REQUIRED_PORT_FAILURE,
}

/** Success/failure envelope returned by [ModelPort]. */
public sealed interface ModelPortResult {
    public data class Success(public val outcome: InvokeOutcome) : ModelPortResult
    public data class Failure(public val failure: ModelPortFailure) : ModelPortResult
}

/** Provider-neutral suspending model invocation boundary. */
public interface ModelPort {
    /** Performs deterministic admission and protocol mapping without I/O. */
    public fun preflight(request: ModelRequest): ModelPortFailure? =
        request.validate()?.let { ModelPortFailure.INVALID_REQUEST }

    /** Maps one request, emits normalized events, and returns one terminal envelope. */
    public suspend fun invoke(
        request: ModelRequest,
        observer: ModelObserver,
        cancellation: ModelCancellation,
    ): ModelPortResult
}
