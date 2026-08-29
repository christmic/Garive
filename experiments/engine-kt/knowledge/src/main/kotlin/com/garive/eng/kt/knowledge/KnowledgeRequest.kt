package com.garive.eng.kt.knowledge

import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

/** Portable strict filter operator. */
public enum class KnowledgeFilterOperator(public val wireName: String) {
    EQUAL("equal"), LESS_THAN("less_than"), LESS_THAN_OR_EQUAL("less_than_or_equal"),
    GREATER_THAN("greater_than"), GREATER_THAN_OR_EQUAL("greater_than_or_equal"),
}

/** Strict I-JSON filter value subset. */
public sealed interface KnowledgeFilterValue {
    public data object Null : KnowledgeFilterValue
    public data class BooleanValue(public val value: Boolean) : KnowledgeFilterValue
    public data class IntegerValue(public val value: Long) : KnowledgeFilterValue
    public data class StringValue(public val value: String) : KnowledgeFilterValue
}

/** One ordered portable source filter. */
public data class KnowledgeFilter(
    public val field: String,
    public val operator: KnowledgeFilterOperator,
    public val value: KnowledgeFilterValue,
) {
    /** Validates bounded field and string values. */
    public fun validate(): KnowledgeErrorCode? =
        if (!validText(field) || (value as? KnowledgeFilterValue.StringValue)?.value?.let { !validText(it) } == true)
            KnowledgeErrorCode.INVALID_QUERY else null
}

/** Exact request freshness requirement. */
public sealed interface FreshnessRequirement {
    public data object CachedAllowed : FreshnessRequirement
    public data object Revalidate : FreshnessRequirement
    public data class ExactSnapshot(public val snapshotDigest: String) : FreshnessRequirement
}

/** Exact bounded retrieval request excluding connector configuration. */
public class KnowledgeRequest private constructor(
    public val requestId: String, public val sourceId: String, public val sourceRevision: String,
    public val mode: KnowledgeQueryMode, public val query: ContentBinding,
    public val filters: List<KnowledgeFilter>, public val throughPosition: ULong,
    public val maxChunks: UInt, public val maxTotalBytes: ULong, public val deadlineBudgetMs: ULong,
    public val freshnessRequirement: FreshnessRequirement,
) {
    /** Computes RFC 8785 SHA-256 over all semantics except request ID. */
    @OptIn(ExperimentalSerializationApi::class)
    public fun requestDigest(): KnowledgeContractResult<String> = runCatching {
        val preimage = JsonObject(
            mapOf(
                "contract" to JsonPrimitive("garive.knowledge-request"),
                "version" to JsonPrimitive(1),
                "request" to requestJson(this),
            ),
        )
        sha256(JsonCanonicalizer(preimage.toString()).encodedUTF8)
    }.fold(::success) { failure(KnowledgeErrorCode.INVALID_QUERY) }

    /** Returns canonical filter-array content for durable requested facts. */
    public fun filtersBinding(): ContentBinding = ContentBinding.fromInline(JsonArray(filters.map(::filterJson)).toString())

    /** Validates exact descriptor identity and supported mode. */
    public fun validateSource(source: KnowledgeSourceDescriptor): KnowledgeErrorCode? = when {
        sourceId != source.sourceId -> KnowledgeErrorCode.SOURCE_NOT_FOUND
        sourceRevision != source.sourceRevision -> KnowledgeErrorCode.SOURCE_REVISION_MISMATCH
        mode !in source.supportedQueryModes -> KnowledgeErrorCode.INVALID_QUERY
        else -> null
    }

    public companion object {
        /** Validates exact source, unique filters and all non-zero bounds. */
        @Suppress("LongParameterList")
        public fun create(
            requestId: String, sourceId: String, sourceRevision: String, mode: KnowledgeQueryMode,
            query: ContentBinding, filters: List<KnowledgeFilter>, throughPosition: ULong,
            maxChunks: UInt, maxTotalBytes: ULong, deadlineBudgetMs: ULong,
            freshnessRequirement: FreshnessRequirement,
        ): KnowledgeContractResult<KnowledgeRequest> =
            if (!validId(requestId) || !validId(sourceId) || !validId(sourceRevision) ||
                filters.any { it.validate() != null } || filters.map { it.field }.toSet().size != filters.size ||
                maxChunks == 0u || maxTotalBytes == 0uL || deadlineBudgetMs == 0uL ||
                (freshnessRequirement as? FreshnessRequirement.ExactSnapshot)?.snapshotDigest?.let { !validDigest(it) } == true
            ) failure(KnowledgeErrorCode.INVALID_QUERY)
            else success(KnowledgeRequest(requestId, sourceId, sourceRevision, mode, query, filters.toList(),
                throughPosition, maxChunks, maxTotalBytes, deadlineBudgetMs, freshnessRequirement))
    }
}

/** Bounded normalized completed Knowledge result. */
public data class KnowledgeCompleted(public val evidence: List<KnowledgeEvidence>, public val truncated: Boolean)

/** Validates source/freshness, normalizes order and applies prefix bounds. */
public fun completeKnowledge(
    request: KnowledgeRequest, source: KnowledgeSourceDescriptor,
    input: List<KnowledgeEvidence>, connectorOrderStable: Boolean,
): KnowledgeContractResult<KnowledgeCompleted> {
    request.validateSource(source)?.let { return failure(it) }
    if (input.map { it.evidenceId }.toSet().size != input.size || input.any { value ->
            value.validate() != null || value.sourceId != request.sourceId || value.sourceRevision != request.sourceRevision ||
                value.trustClass != source.trustClass || value.citation.locatorKind != source.citationScheme ||
                !freshnessAllowed(request.freshnessRequirement, value)
        }
    ) return failure(KnowledgeErrorCode.INVALID_QUERY)
    val ordered = if (connectorOrderStable) input else input.sortedWith(
        compareByDescending<KnowledgeEvidence> { it.rankBasisPoints }
            .thenBy { it.citation.locator }.thenBy { it.evidenceId },
    )
    val admitted = mutableListOf<KnowledgeEvidence>()
    var bytes = 0uL
    var truncated = false
    ordered.forEach { value ->
        if (!truncated) {
            val overflow = ULong.MAX_VALUE - bytes < value.contentByteLength
            val next = if (overflow) ULong.MAX_VALUE else bytes + value.contentByteLength
            if (admitted.size.toUInt() == request.maxChunks || overflow || next > request.maxTotalBytes) truncated = true
            else { bytes = next; admitted += value }
        }
    }
    return success(KnowledgeCompleted(admitted.toList(), truncated))
}

private fun freshnessAllowed(requirement: FreshnessRequirement, value: KnowledgeEvidence): Boolean = when (requirement) {
    FreshnessRequirement.CachedAllowed -> true
    FreshnessRequirement.Revalidate -> value.freshness == KnowledgeFreshness.FRESH
    is FreshnessRequirement.ExactSnapshot -> value.freshness != KnowledgeFreshness.STALE && value.sourceSnapshotDigest == requirement.snapshotDigest
}
private fun filterJson(value: KnowledgeFilter): JsonObject = JsonObject(mapOf(
    "field" to JsonPrimitive(value.field), "operator" to JsonPrimitive(value.operator.wireName), "value" to when (val item = value.value) {
        KnowledgeFilterValue.Null -> JsonNull
        is KnowledgeFilterValue.BooleanValue -> JsonPrimitive(item.value)
        is KnowledgeFilterValue.IntegerValue -> JsonPrimitive(item.value)
        is KnowledgeFilterValue.StringValue -> JsonPrimitive(item.value)
    },
))
private fun contentJson(value: ContentBinding): JsonObject = JsonObject(mapOf(
    "digest" to JsonPrimitive(value.digest),
    "inline_utf8" to (value.inlineUtf8?.let(::JsonPrimitive) ?: JsonNull),
    "reference" to (value.reference?.let(::JsonPrimitive) ?: JsonNull),
).filterValues { it != JsonNull })
private fun freshnessJson(value: FreshnessRequirement): JsonObject = when (value) {
    FreshnessRequirement.CachedAllowed -> JsonObject(mapOf("kind" to JsonPrimitive("cached_allowed")))
    FreshnessRequirement.Revalidate -> JsonObject(mapOf("kind" to JsonPrimitive("revalidate")))
    is FreshnessRequirement.ExactSnapshot -> JsonObject(mapOf("kind" to JsonPrimitive("exact_snapshot"), "snapshot_digest" to JsonPrimitive(value.snapshotDigest)))
}
@OptIn(ExperimentalSerializationApi::class)
private fun requestJson(value: KnowledgeRequest): JsonObject = JsonObject(mapOf(
    "source_id" to JsonPrimitive(value.sourceId), "source_revision" to JsonPrimitive(value.sourceRevision),
    "mode" to JsonPrimitive(value.mode.wireName), "query" to contentJson(value.query),
    "filters" to JsonArray(value.filters.map(::filterJson)), "through_position" to JsonPrimitive(value.throughPosition),
    "max_chunks" to JsonPrimitive(value.maxChunks), "max_total_bytes" to JsonPrimitive(value.maxTotalBytes),
    "deadline_budget_ms" to JsonPrimitive(value.deadlineBudgetMs), "freshness_requirement" to freshnessJson(value.freshnessRequirement),
))
