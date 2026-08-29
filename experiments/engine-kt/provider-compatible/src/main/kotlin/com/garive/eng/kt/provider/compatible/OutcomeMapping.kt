package com.garive.eng.kt.provider.compatible

import com.garive.eng.kt.anthropic.MessageResponse
import com.garive.eng.kt.anthropic.OutputBlock as MessagesOutput
import com.garive.eng.kt.anthropic.StopReason
import com.garive.eng.kt.anthropic.Usage as MessagesUsage
import com.garive.eng.kt.llm.InterruptionKind
import com.garive.eng.kt.llm.InvokeOutcome
import com.garive.eng.kt.llm.ModelItem
import com.garive.eng.kt.llm.ModelStopReason
import com.garive.eng.kt.llm.ModelUsage
import com.garive.eng.kt.llm.ReasoningContent
import com.garive.eng.kt.llm.RejectionKind
import com.garive.eng.kt.llm.TokenCount
import com.garive.eng.kt.llm.UsageSource
import com.garive.eng.kt.openai.OutputContent as ResponsesContent
import com.garive.eng.kt.openai.ResponseEnvelope
import com.garive.eng.kt.openai.ResponseOutputItem
import com.garive.eng.kt.openai.ResponseStatus
import com.garive.eng.kt.openai.ResponseUsage
import kotlin.time.Duration
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject

/** Classifies an exact typed protocol error without inspecting its message. */
public fun classifyProtocolError(
    policy: ProtocolErrorPolicy,
    signature: ErrorSignature,
    retryAfter: Duration?,
): InvokeOutcome = when (val disposition = policy.classify(signature)
    ?: fail(CompatibleProviderError.UNCLASSIFIED_PROTOCOL_ERROR)) {
    is ErrorDisposition.Rejected -> InvokeOutcome.Rejected(
        disposition.kind,
        "status=${signature.status};type=${signature.protocolType};code=${signature.code.orEmpty()}",
    )
    is ErrorDisposition.Unavailable -> InvokeOutcome.Unavailable(disposition.kind, retryAfter)
    is ErrorDisposition.Interrupted -> InvokeOutcome.Interrupted(disposition.kind, emptyList(), unknownUsage())
}

/** Normalizes one adapter-validated buffered Responses terminal. */
public fun normalizeResponses(response: ResponseEnvelope, reasoningVisibility: Boolean): InvokeOutcome {
    val items = responsesItems(response.output, reasoningVisibility)
    val usage = response.usage?.let(::responsesUsage) ?: unknownUsage()
    return when {
        response.status == ResponseStatus.COMPLETED && response.error == null -> InvokeOutcome.Completed(
            items,
            usage,
            when {
                items.any { it is ModelItem.ToolIntent } -> ModelStopReason.ToolUse
                items.any { it is ModelItem.Refusal } -> ModelStopReason.Refusal
                else -> ModelStopReason.EndTurn
            },
        )
        response.status == ResponseStatus.INCOMPLETE && response.error == null &&
            response.incompleteDetails?.reason == "max_output_tokens" -> InvokeOutcome.Interrupted(
            InterruptionKind.OUTPUT_LIMIT,
            items,
            usage,
        )
        response.status == ResponseStatus.CANCELLED && response.error == null -> InvokeOutcome.Interrupted(
            InterruptionKind.CANCELLED,
            items,
            usage,
        )
        else -> fail(CompatibleProviderError.PROTOCOL_INVARIANT)
    }
}

/** Normalizes one adapter-validated buffered Messages terminal. */
public fun normalizeMessages(response: MessageResponse, reasoningVisibility: Boolean): InvokeOutcome {
    val refusal = response.stopReason == StopReason.REFUSAL
    val items = response.content.map { messagesItem(it, reasoningVisibility, refusal) }
    return when (val reason = response.stopReason ?: fail(CompatibleProviderError.PROTOCOL_INVARIANT)) {
        StopReason.MAX_TOKENS -> InvokeOutcome.Interrupted(InterruptionKind.OUTPUT_LIMIT, items, messagesUsage(response.usage))
        StopReason.MODEL_CONTEXT_WINDOW_EXCEEDED -> InvokeOutcome.Rejected(
            RejectionKind.CONTEXT_OVERFLOW,
            "model_context_window_exceeded",
        )
        else -> InvokeOutcome.Completed(items, messagesUsage(response.usage), messagesStop(reason))
    }
}

internal fun responsesItems(
    output: List<ResponseOutputItem>,
    reasoningVisibility: Boolean,
): List<ModelItem> = buildList {
    output.forEach { item ->
        when (item) {
            is ResponseOutputItem.Message -> item.content.forEach { content ->
                add(when (content) {
                    is ResponsesContent.Text -> ModelItem.Text(content.text)
                    is ResponsesContent.Refusal -> ModelItem.Refusal(content.refusal)
                    is ResponsesContent.Extension -> fail(CompatibleProviderError.UNSUPPORTED_EXTENSION)
                })
            }
            is ResponseOutputItem.FunctionCall -> add(ModelItem.ToolIntent(
                item.callId,
                item.name,
                canonicalObject(item.arguments),
            ))
            is ResponseOutputItem.Reasoning -> {
                if (reasoningVisibility) {
                    val parts = item.content ?: item.summary
                    if (parts.isNotEmpty()) add(ModelItem.Reasoning(ReasoningContent.ModelVisible(parts.joinToString("") { it.text })))
                } else {
                    item.encryptedContent?.let { add(ModelItem.Reasoning(ReasoningContent.OpaqueReference(it))) }
                }
            }
            is ResponseOutputItem.Extension -> fail(CompatibleProviderError.UNSUPPORTED_EXTENSION)
        }
    }
}

internal fun messagesItem(block: MessagesOutput, reasoningVisibility: Boolean, refusal: Boolean): ModelItem =
    when (block) {
        is MessagesOutput.Text -> if (refusal) ModelItem.Refusal(block.text) else ModelItem.Text(block.text)
        is MessagesOutput.Thinking -> ModelItem.Reasoning(
            if (reasoningVisibility) ReasoningContent.ModelVisible(block.thinking)
            else ReasoningContent.OpaqueReference(block.signature),
        )
        is MessagesOutput.RedactedThinking -> ModelItem.Reasoning(ReasoningContent.OpaqueReference(block.data))
        is MessagesOutput.ToolUse -> ModelItem.ToolIntent(block.id, block.name, block.input.toString())
        is MessagesOutput.Extension -> fail(CompatibleProviderError.UNSUPPORTED_EXTENSION)
    }

internal fun messagesStop(reason: StopReason): ModelStopReason = when (reason) {
    StopReason.END_TURN -> ModelStopReason.EndTurn
    StopReason.STOP_SEQUENCE -> ModelStopReason.StopSequence
    StopReason.TOOL_USE -> ModelStopReason.ToolUse
    StopReason.PAUSE_TURN -> ModelStopReason.PauseTurn
    StopReason.REFUSAL -> ModelStopReason.Refusal
    StopReason.MAX_TOKENS, StopReason.MODEL_CONTEXT_WINDOW_EXCEEDED -> fail(CompatibleProviderError.PROTOCOL_INVARIANT)
}

private fun canonicalObject(encoded: String): String = try {
    (Json.parseToJsonElement(encoded) as? JsonObject)?.toString()
        ?: fail(CompatibleProviderError.PROTOCOL_INVARIANT)
} catch (error: CompatibleProviderException) {
    throw error
} catch (_: IllegalArgumentException) {
    fail(CompatibleProviderError.PROTOCOL_INVARIANT)
}

private fun responsesUsage(value: ResponseUsage): ModelUsage = ModelUsage(
    TokenCount.Known(value.inputTokens),
    TokenCount.Known(value.outputTokens),
    TokenCount.Known(value.inputTokensDetails.cachedTokens),
    TokenCount.Known(value.inputTokensDetails.cacheWriteTokens),
    UsageSource.PROVIDER_REPORTED,
)

internal fun messagesUsage(value: MessagesUsage): ModelUsage = ModelUsage(
    TokenCount.Known(value.inputTokens),
    TokenCount.Known(value.outputTokens),
    value.cacheReadInputTokens?.let(TokenCount::Known),
    value.cacheCreationInputTokens?.let(TokenCount::Known),
    UsageSource.PROVIDER_REPORTED,
)

internal fun unknownUsage(): ModelUsage = ModelUsage(
    TokenCount.Unknown,
    TokenCount.Unknown,
    source = UsageSource.PROVIDER_REPORTED,
)

internal fun fail(error: CompatibleProviderError): Nothing = throw CompatibleProviderException(error)
