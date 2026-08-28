package com.garive.eng.kt.llm

data class ModelRequestId(val value: String)
data class ModelTargetId(val value: String)

enum class ModelCapability { TEXT, VISION, REASONING, TOOLS, JSON_OUTPUT, STREAMING }
enum class ModelRole { SYSTEM, DEVELOPER, USER, ASSISTANT }

sealed interface ModelInputContent {
    data class Text(val text: String) : ModelInputContent
    data class MediaReference(
        val mediaKind: MediaKind,
        val reference: String,
        val mediaType: String,
    ) : ModelInputContent
}

sealed interface ModelInputItem {
    data class Message(val role: ModelRole, val content: List<ModelInputContent>) : ModelInputItem
    data class ToolObservation(val modelCallId: String, val resultJson: String) : ModelInputItem
    data class ReasoningReference(val reference: String) : ModelInputItem
}

data class ToolDescriptor(
    val name: String,
    val description: String,
    val definitionRevision: String,
    val inputSchemaJson: String,
    val strict: Boolean,
)

sealed interface TextMode {
    data object Plain : TextMode
    data object JsonObject : TextMode
    data class JsonSchema(val schemaJson: String) : TextMode
}

data class ModelOutputSettings(
    val maxOutputTokens: ULong?,
    val textMode: TextMode,
    val reasoningVisibility: Boolean,
)

data class ModelRequest(
    val requestId: ModelRequestId,
    val targetId: ModelTargetId,
    val requiredCapabilities: List<ModelCapability>,
    val inputItems: List<ModelInputItem>,
    val tools: List<ToolDescriptor>,
    val output: ModelOutputSettings,
    val traceMetadata: List<Pair<String, String>>,
) {
    fun validate(): RequestValidationError? {
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

enum class RequestValidationError(val code: String) {
    EMPTY_IDENTITY("empty-identity"),
    DUPLICATE_CAPABILITY("duplicate-capability"),
    INVALID_TOOL("invalid-tool"),
    DUPLICATE_TOOL("duplicate-tool"),
    ZERO_OUTPUT_LIMIT("zero-output-limit"),
    INVALID_METADATA("invalid-metadata"),
    DUPLICATE_METADATA("duplicate-metadata"),
}

sealed interface ModelOutputKind {
    data object Text : ModelOutputKind
    data object Refusal : ModelOutputKind
    data object Reasoning : ModelOutputKind
    data class ToolIntent(val modelCallId: String) : ModelOutputKind
    data object ToolObservation : ModelOutputKind
    data object MediaReference : ModelOutputKind
}

sealed interface ModelStreamEvent {
    data class OutputItemStarted(val outputIndex: UInt, val kind: ModelOutputKind) : ModelStreamEvent
    data class TextDelta(val outputIndex: UInt, val delta: String) : ModelStreamEvent
    data class RefusalDelta(val outputIndex: UInt, val delta: String) : ModelStreamEvent
    data class ReasoningDelta(val outputIndex: UInt, val delta: String) : ModelStreamEvent
    data class ToolArgumentsDelta(
        val outputIndex: UInt,
        val modelCallId: String,
        val delta: String,
    ) : ModelStreamEvent
    data class OutputItemCompleted(val outputIndex: UInt, val item: ModelItem) : ModelStreamEvent
    data class UsageUpdated(val usage: ModelUsage) : ModelStreamEvent
}

class StreamValidator {
    private val started = mutableMapOf<UInt, ModelOutputKind>()
    private val completed = mutableSetOf<UInt>()
    private var lastStarted: UInt? = null

    fun accept(event: ModelStreamEvent): StreamInvariantError? = when (event) {
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

enum class StreamInvariantError(val code: String) {
    NON_MONOTONIC_START("non-monotonic-start"),
    ITEM_NOT_STARTED("item-not-started"),
    ITEM_ALREADY_COMPLETED("item-already-completed"),
    ITEM_KIND_MISMATCH("item-kind-mismatch"),
}

enum class ObserverDecision { CONTINUE, CANCEL }
fun interface ModelObserver { fun observe(event: ModelStreamEvent): ObserverDecision }
fun interface ModelCancellation { fun isCancelled(): Boolean }

enum class ModelPortFailure {
    INVALID_REQUEST,
    UNSUPPORTED_CAPABILITY,
    ADAPTER_INVARIANT,
    REQUIRED_PORT_FAILURE,
}

sealed interface ModelPortResult {
    data class Success(val outcome: InvokeOutcome) : ModelPortResult
    data class Failure(val failure: ModelPortFailure) : ModelPortResult
}

interface ModelPort {
    suspend fun invoke(
        request: ModelRequest,
        observer: ModelObserver,
        cancellation: ModelCancellation,
    ): ModelPortResult
}
