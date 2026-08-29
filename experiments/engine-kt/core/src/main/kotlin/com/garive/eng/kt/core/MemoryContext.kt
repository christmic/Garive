@file:OptIn(kotlinx.serialization.ExperimentalSerializationApi::class)

package com.garive.eng.kt.core

import com.garive.eng.kt.llm.ModelInputContent
import com.garive.eng.kt.llm.ModelInputItem
import com.garive.eng.kt.llm.ModelRole
import java.security.MessageDigest
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive

/** Committed recall product entering C2. */
public enum class MemoryRecallProduct { MENU, DETAIL }

/** Recallable lifecycle state; promoted state is intentionally absent. */
public enum class MemoryContextState { CANDIDATE, ACTIVE, COLD, ARCHIVED }

/** One exact item decoded from a committed recall fact. */
public data class MemoryContextItem(
    public val recordId: String,
    public val revisionId: String,
    public val memoryType: String,
    public val role: String,
    public val authority: String,
    public val state: MemoryContextState,
    public val safeLabel: String,
    public val contentDigest: String,
    public val contentByteLength: ULong,
    public val contentUtf8: String?,
)

/** Exact durable binding for one committed Memory recall. */
public data class MemoryRecallContextBatch(
    public val factRef: FactRef,
    public val factId: String,
    public val payloadDigest: String,
    public val selectionId: String,
    public val requestDigest: String,
    public val namespaceId: String,
    public val product: MemoryRecallProduct,
    public val selectionPolicyRevision: String,
    public val throughPosition: ULong,
    public val truncated: Boolean,
    public val items: List<MemoryContextItem>,
)

/** Stable committed-recall integration failure. */
public sealed class MemoryContextError(public val code: String) {
    public data object InvalidBinding : MemoryContextError("invalid-binding")
    public data object DuplicateRecall : MemoryContextError("duplicate-recall")
    public data class Context(public val contextCode: String) : MemoryContextError(contextCode)
}

/** Result of Memory-aware deterministic context derivation. */
public sealed interface MemoryContextResult {
    public data class Success(public val surface: ContextSurface) : MemoryContextResult
    public data class Failure(public val error: MemoryContextError) : MemoryContextResult
}

/** Adapts committed recall to ordinary optional C2 candidates. */
public fun deriveContextWithMemory(
    request: ContextRequest,
    candidates: List<ContextCandidate>,
    recalls: List<MemoryRecallContextBatch>,
): MemoryContextResult {
    if (recalls.size > 2 || recalls.map { it.product }.toSet().size != recalls.size) {
        return MemoryContextResult.Failure(MemoryContextError.DuplicateRecall)
    }
    val identities = mutableSetOf<Pair<String, String>>()
    val additions = mutableListOf<ContextCandidate>()
    for (recall in recalls) {
        if (!validBatch(request, recall, identities)) {
            return MemoryContextResult.Failure(MemoryContextError.InvalidBinding)
        }
        if (recall.items.isNotEmpty()) additions += candidate(recall)
    }
    return when (val result = deriveContext(request, (candidates + additions).sortedBy { it.factRef.position })) {
        is ContextDerivationResult.Success -> MemoryContextResult.Success(result.surface)
        is ContextDerivationResult.Failure -> MemoryContextResult.Failure(MemoryContextError.Context(result.error.code))
    }
}

private fun validBatch(
    request: ContextRequest,
    recall: MemoryRecallContextBatch,
    identities: MutableSet<Pair<String, String>>,
): Boolean {
    if (recall.factRef.sessionId != request.sessionId || recall.factRef.position <= recall.throughPosition ||
        recall.factRef.position > request.throughPosition || recall.factId.isEmpty() || !recall.payloadDigest.digest() ||
        recall.selectionId.isEmpty() || !recall.requestDigest.digest() || recall.namespaceId.isEmpty() ||
        recall.selectionPolicyRevision.isEmpty()
    ) return false
    return recall.items.all { item ->
        val identity = item.recordId to item.revisionId
        item.recordId.isNotEmpty() && item.revisionId.isNotEmpty() &&
            item.memoryType in setOf("semantic", "episodic", "lesson", "procedural") &&
            item.role in setOf("preference", "constraint", "decision", "learned_fact", "summary") &&
            item.authority in setOf("user_declared", "agent_learned", "organisation_published") &&
            item.safeLabel.isNotEmpty() && item.contentDigest.digest() && item.contentByteLength > 0uL &&
            identities.add(identity) && !(recall.product == MemoryRecallProduct.MENU && item.state == MemoryContextState.ARCHIVED) &&
            when (recall.product) {
                MemoryRecallProduct.MENU -> item.contentUtf8 == null
                MemoryRecallProduct.DETAIL -> item.contentUtf8?.let { content ->
                    content.isNotEmpty() && content.encodeToByteArray().size.toULong() == item.contentByteLength &&
                        sha256(content) == item.contentDigest
                } == true
            }
    }
}

private fun candidate(recall: MemoryRecallContextBatch): ContextCandidate = ContextCandidate(
    recall.factRef, CandidateKind.MEMORY, Retention.OPTIONAL,
    Visibility.Purposes(setOf(ContextPurpose.INFERENCE)), recall.items.map { render(recall, it) },
)

private fun render(recall: MemoryRecallContextBatch, item: MemoryContextItem): ModelInputItem {
    val fact = JsonObject(linkedMapOf(
        "session_id" to JsonPrimitive(recall.factRef.sessionId), "position" to JsonPrimitive(recall.factRef.position),
        "fact_id" to JsonPrimitive(recall.factId), "payload_digest" to JsonPrimitive(recall.payloadDigest),
    ))
    val value = JsonObject(linkedMapOf(
        "type" to JsonPrimitive("garive.memory.recall"), "selection_id" to JsonPrimitive(recall.selectionId),
        "request_digest" to JsonPrimitive(recall.requestDigest), "namespace_id" to JsonPrimitive(recall.namespaceId),
        "product" to JsonPrimitive(recall.product.name.lowercase()),
        "selection_policy_revision" to JsonPrimitive(recall.selectionPolicyRevision), "recall_fact" to fact,
        "record_id" to JsonPrimitive(item.recordId), "revision_id" to JsonPrimitive(item.revisionId),
        "memory_type" to JsonPrimitive(item.memoryType), "role" to JsonPrimitive(item.role),
        "authority" to JsonPrimitive(item.authority), "state" to JsonPrimitive(item.state.name.lowercase()),
        "safe_label" to JsonPrimitive(item.safeLabel), "content_digest" to JsonPrimitive(item.contentDigest),
        "content_byte_length" to JsonPrimitive(item.contentByteLength),
        "content" to (item.contentUtf8?.let(::JsonPrimitive) ?: JsonNull),
    ))
    return ModelInputItem.Message(ModelRole.USER, listOf(ModelInputContent.Text(value.toString())))
}

private fun String.digest(): Boolean = matches(Regex("[0-9a-f]{64}"))
private fun sha256(value: String): String = MessageDigest.getInstance("SHA-256")
    .digest(value.encodeToByteArray()).joinToString("") { "%02x".format(it) }
