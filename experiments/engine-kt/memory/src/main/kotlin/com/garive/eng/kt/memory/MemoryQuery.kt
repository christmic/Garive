package com.garive.eng.kt.memory

import java.time.Instant
import kotlinx.serialization.ExperimentalSerializationApi
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

/** Consumer purpose for one bounded memory query. */
public enum class MemoryPurpose(public val wireName: String) {
    CONTEXT("context"),
    PLANNING("planning"),
    CONFLICT_CHECK("conflict_check"),
}

/** Exact deterministic memory query excluding its outer idempotency identity. */
public class MemoryQuery private constructor(
    public val queryId: String,
    public val namespaceId: String,
    public val allowedScopes: List<MemoryScope>,
    public val purpose: MemoryPurpose,
    public val retrieverRevision: String,
    public val query: ContentBinding,
    public val throughPosition: ULong,
    public val asOfUtc: String,
    public val maxResults: UInt,
    public val maxTotalBytes: ULong,
    public val includeRestricted: Boolean,
    public val restrictedGrantDigest: String?,
) {
    /** Computes RFC 8785 SHA-256 over all semantics except query ID. */
    @OptIn(ExperimentalSerializationApi::class)
    public fun queryDigest(): MemoryContractResult<String> {
        val fields = linkedMapOf<String, kotlinx.serialization.json.JsonElement>(
            "namespace_id" to JsonPrimitive(namespaceId),
            "allowed_scopes" to JsonArray(allowedScopes.map(::scopeJson)),
            "purpose" to JsonPrimitive(purpose.wireName),
            "retriever_revision" to JsonPrimitive(retrieverRevision),
            "query" to contentJson(query),
            "through_position" to JsonPrimitive(throughPosition),
            "as_of_utc" to JsonPrimitive(asOfUtc),
            "max_results" to JsonPrimitive(maxResults),
            "max_total_bytes" to JsonPrimitive(maxTotalBytes),
            "include_restricted" to JsonPrimitive(includeRestricted),
        )
        restrictedGrantDigest?.let { fields["restricted_grant_digest"] = JsonPrimitive(it) }
        val preimage = JsonObject(
            mapOf(
                "contract" to JsonPrimitive(QUERY_CONTRACT),
                "version" to JsonPrimitive(CONTRACT_VERSION),
                "query" to JsonObject(fields),
            ),
        )
        return runCatching { JsonCanonicalizer(preimage.toString()).encodedUTF8 }
            .fold(
                onSuccess = { MemoryContractResult.Success(sha256(it)) },
                onFailure = { failure(MemoryErrorCode.INVALID_MEMORY) },
            )
    }

    public companion object {
        /** Validates scope order, fixed time, bounds and restricted-grant shape. */
        @Suppress("LongParameterList")
        public fun create(
            queryId: String,
            namespaceId: String,
            allowedScopes: List<MemoryScope>,
            purpose: MemoryPurpose,
            retrieverRevision: String,
            query: ContentBinding,
            throughPosition: ULong,
            asOfUtc: String,
            maxResults: UInt,
            maxTotalBytes: ULong,
            includeRestricted: Boolean,
            restrictedGrantDigest: String?,
        ): MemoryContractResult<MemoryQuery> {
            if (!validId(queryId) || !validId(namespaceId) || allowedScopes.isEmpty() ||
                !orderedScopes(allowedScopes) || !validText(retrieverRevision, MAX_REFERENCE_BYTES) ||
                !canonicalUtc(asOfUtc) || maxResults == 0u || maxTotalBytes == 0uL ||
                includeRestricted != (restrictedGrantDigest != null) ||
                restrictedGrantDigest?.let { !validDigest(it) } == true
            ) return failure(MemoryErrorCode.INVALID_MEMORY)
            return MemoryContractResult.Success(
                MemoryQuery(
                    queryId, namespaceId, allowedScopes.toList(), purpose, retrieverRevision,
                    query, throughPosition, asOfUtc, maxResults, maxTotalBytes,
                    includeRestricted, restrictedGrantDigest,
                ),
            )
        }
    }
}

/** Retriever-owned score and verified content size for one exact revision. */
@ConsistentCopyVisibility
public data class MemoryScore private constructor(
    public val recordId: String,
    public val revisionId: String,
    public val relevanceBasisPoints: Int,
    public val contentByteLength: ULong,
) {
    public companion object {
        /** Validates an exact scored revision reference. */
        public fun create(
            recordId: String,
            revisionId: String,
            relevanceBasisPoints: Int,
            contentByteLength: ULong,
        ): MemoryContractResult<MemoryScore> =
            if (!validId(recordId) || !validId(revisionId) ||
                relevanceBasisPoints !in 0..MAX_BASIS_POINTS || contentByteLength == 0uL
            ) failure(MemoryErrorCode.INVALID_MEMORY)
            else MemoryContractResult.Success(
                MemoryScore(recordId, revisionId, relevanceBasisPoints, contentByteLength),
            )
    }
}

/** Exact authorized active revision returned by bounded retrieval. */
public data class MemoryMatch(
    public val recordId: String,
    public val revisionId: String,
    public val kind: MemoryKind,
    public val content: ContentBinding,
    public val contentByteLength: ULong,
    public val evidence: List<DurableFactReference>,
    public val relevanceBasisPoints: Int,
    public val sensitivity: MemorySensitivity,
)

/** Completed deterministic retrieval and prefix truncation evidence. */
public data class MemoryRetrieval(
    public val matches: List<MemoryMatch>,
    public val truncated: Boolean,
)

/** Filters and orders scored exact revisions under one frozen query. */
public fun retrieveMemory(
    records: List<MemoryRecord>,
    scores: List<MemoryScore>,
    query: MemoryQuery,
): MemoryContractResult<MemoryRetrieval> {
    val byIdentity = records.associateBy { it.recordId to it.revisionId }
    val seen = mutableSetOf<Pair<String, String>>()
    val eligible = mutableListOf<Pair<MemoryRecord, MemoryScore>>()
    scores.forEach { score ->
        val key = score.recordId to score.revisionId
        if (!seen.add(key)) return failure(MemoryErrorCode.INVALID_MEMORY)
        val record = byIdentity[key] ?: return failure(MemoryErrorCode.CORRUPT_MEMORY_STATE)
        if (record.namespaceId != query.namespaceId || record.scope !in query.allowedScopes ||
            record.status != MemoryStatus.ACTIVE ||
            record.expiresAtUtc?.let { Instant.parse(it) <= Instant.parse(query.asOfUtc) } == true ||
            record.sensitivity == MemorySensitivity.RESTRICTED && !query.includeRestricted
        ) return@forEach
        if (record.content.inlineUtf8?.encodeToByteArray()?.size?.toULong()?.let { it != score.contentByteLength } == true) {
            return failure(MemoryErrorCode.INVALID_MEMORY)
        }
        eligible += record to score
    }
    eligible.sortWith(
        compareByDescending<Pair<MemoryRecord, MemoryScore>> { it.second.relevanceBasisPoints }
            .thenBy { it.first.recordId }
            .thenBy { it.first.revisionId },
    )
    val matches = mutableListOf<MemoryMatch>()
    var bytes = 0uL
    var truncated = false
    eligible.forEach { (record, score) ->
        val overflow = ULong.MAX_VALUE - bytes < score.contentByteLength
        val next = if (overflow) ULong.MAX_VALUE else bytes + score.contentByteLength
        if (matches.size.toUInt() == query.maxResults || overflow || next > query.maxTotalBytes) {
            truncated = true
            return@forEach
        }
        if (truncated) return@forEach
        bytes = next
        matches += MemoryMatch(
            record.recordId, record.revisionId, record.kind, record.content,
            score.contentByteLength, record.evidence, score.relevanceBasisPoints, record.sensitivity,
        )
    }
    return MemoryContractResult.Success(MemoryRetrieval(matches.toList(), truncated))
}

private const val QUERY_CONTRACT: String = "garive.memory-query"
private const val CONTRACT_VERSION: Int = 1

private fun orderedScopes(values: List<MemoryScope>): Boolean =
    values.map(::scopeKey).zipWithNext().all { (left, right) -> left < right }

private fun scopeKey(value: MemoryScope): String = when (value) {
    is MemoryScope.Session -> "0:${value.ownerId}"
    is MemoryScope.AgentInstance -> "1:${value.ownerId}"
    MemoryScope.Namespace -> "2:"
}

private fun scopeJson(value: MemoryScope): JsonObject = when (value) {
    is MemoryScope.Session -> JsonObject(
        mapOf("kind" to JsonPrimitive("session"), "owner_id" to JsonPrimitive(value.ownerId)),
    )
    is MemoryScope.AgentInstance -> JsonObject(
        mapOf("kind" to JsonPrimitive("agent_instance"), "owner_id" to JsonPrimitive(value.ownerId)),
    )
    MemoryScope.Namespace -> JsonObject(mapOf("kind" to JsonPrimitive("namespace")))
}

private fun contentJson(value: ContentBinding): JsonObject = JsonObject(
    mapOf(
        "digest" to JsonPrimitive(value.digest),
        "inline_utf8" to (value.inlineUtf8?.let(::JsonPrimitive) ?: JsonNull),
        "reference" to (value.reference?.let(::JsonPrimitive) ?: JsonNull),
    ).filterValues { it != JsonNull },
)
