package com.garive.eng.kt.knowledge

import java.security.MessageDigest
import java.time.Instant

/** Stable K0 validation, connector, citation or durability failure. */
public enum class KnowledgeErrorCode(public val wireName: String) {
    INVALID_QUERY("invalid_query"), SOURCE_NOT_FOUND("source_not_found"),
    SOURCE_REVISION_MISMATCH("source_revision_mismatch"), SOURCE_DENIED("source_denied"),
    FILTER_UNSUPPORTED("filter_unsupported"), FRESHNESS_UNAVAILABLE("freshness_unavailable"),
    CONNECTOR_UNAVAILABLE("connector_unavailable"), CONNECTOR_REJECTED("connector_rejected"),
    RETRIEVAL_UNCERTAIN("retrieval_uncertain"), CITATION_INVALID("citation_invalid"),
    CONTENT_DIGEST_MISMATCH("content_digest_mismatch"), LIMIT_EXCEEDED("limit_exceeded"),
    DURABILITY_FAILURE("durability_failure"), CORRUPT_KNOWLEDGE_STATE("corrupt_knowledge_state"),
}

/** Typed result for portable Knowledge construction and reduction. */
public sealed interface KnowledgeContractResult<out T> {
    public data class Success<T>(public val value: T) : KnowledgeContractResult<T>
    public data class Failure(public val code: KnowledgeErrorCode) : KnowledgeContractResult<Nothing>
}

/** Exact inline or Runtime-resolvable content with a SHA-256 binding. */
@ConsistentCopyVisibility
public data class ContentBinding private constructor(
    public val digest: String,
    public val inlineUtf8: String?,
    public val reference: String?,
) {
    public companion object {
        /** Constructs trusted inline UTF-8 and computes its digest. */
        public fun fromInline(value: String): ContentBinding = ContentBinding(sha256(value.encodeToByteArray()), value, null)
        /** Validates exact inline UTF-8. */
        public fun inline(digest: String, value: String): KnowledgeContractResult<ContentBinding> =
            if (validDigest(digest) && sha256(value.encodeToByteArray()) == digest) success(ContentBinding(digest, value, null))
            else failure(KnowledgeErrorCode.CONTENT_DIGEST_MISMATCH)
        /** Validates an opaque Runtime-resolvable reference. */
        public fun referenced(digest: String, reference: String): KnowledgeContractResult<ContentBinding> =
            if (validDigest(digest) && validText(reference)) success(ContentBinding(digest, null, reference))
            else failure(KnowledgeErrorCode.INVALID_QUERY)
    }
}

/** Portable source category. */
public enum class KnowledgeSourceKind { REPOSITORY, DOCUMENTATION, DATASET, SEARCH_INDEX, SERVICE }
/** Portable query mode. */
public enum class KnowledgeQueryMode(public val wireName: String) { KEYWORD("keyword"), SEMANTIC("semantic"), STRUCTURED("structured") }
/** Declared source trust classification. */
public enum class KnowledgeTrustClass(public val wireName: String) { CURATED("curated"), FIRST_PARTY("first_party"), THIRD_PARTY("third_party"), UNTRUSTED("untrusted") }
/** Citation locator scheme. */
public enum class CitationScheme(public val wireName: String) { URI_FRAGMENT("uri_fragment"), DOCUMENT_OFFSET("document_offset"), RECORD_KEY("record_key"), OPAQUE_LOCATOR("opaque_locator") }
/** Evidence freshness classification. */
public enum class KnowledgeFreshness(public val wireName: String) { FRESH("fresh"), CACHED("cached"), STALE("stale") }

/** Exact source descriptor frozen into an effective Agent snapshot. */
public class KnowledgeSourceDescriptor private constructor(
    public val sourceId: String, public val sourceRevision: String,
    public val kind: KnowledgeSourceKind, public val contentDomain: String,
    public val trustClass: KnowledgeTrustClass, public val supportedQueryModes: List<KnowledgeQueryMode>,
    public val freshnessPolicyDigest: String, public val citationScheme: CitationScheme,
    public val capabilityMetadataDigest: String,
) {
    public companion object {
        /** Validates an exact portable source descriptor. */
        @Suppress("LongParameterList")
        public fun create(
            sourceId: String, sourceRevision: String, kind: KnowledgeSourceKind, contentDomain: String,
            trustClass: KnowledgeTrustClass, supportedQueryModes: List<KnowledgeQueryMode>,
            freshnessPolicyDigest: String, citationScheme: CitationScheme, capabilityMetadataDigest: String,
        ): KnowledgeContractResult<KnowledgeSourceDescriptor> =
            if (!validId(sourceId) || !validId(sourceRevision) || !validText(contentDomain) || supportedQueryModes.isEmpty() ||
                supportedQueryModes.zipWithNext().any { (left, right) -> left >= right } ||
                !validDigest(freshnessPolicyDigest) || !validDigest(capabilityMetadataDigest)
            ) failure(KnowledgeErrorCode.INVALID_QUERY)
            else success(KnowledgeSourceDescriptor(sourceId, sourceRevision, kind, contentDomain, trustClass,
                supportedQueryModes.toList(), freshnessPolicyDigest, citationScheme, capabilityMetadataDigest))
    }
}

/** Sanitized exact citation binding. */
public class Citation private constructor(
    public val locatorKind: CitationScheme, public val locator: String, public val title: String?,
    public val canonicalUri: String?, public val contentDigest: String,
) {
    public companion object {
        /** Validates one bounded sanitized citation. */
        public fun create(locatorKind: CitationScheme, locator: String, title: String?, canonicalUri: String?, contentDigest: String): KnowledgeContractResult<Citation> =
            if (!validText(locator) || title?.let { !validText(it) } == true || canonicalUri?.let { !validText(it) } == true || !validDigest(contentDigest))
                failure(KnowledgeErrorCode.CITATION_INVALID)
            else success(Citation(locatorKind, locator, title, canonicalUri, contentDigest))
    }
}

/** One exact attributed evidence chunk returned by Runtime. */
public data class KnowledgeEvidence(
    public val evidenceId: String, public val sourceId: String, public val sourceRevision: String,
    public val sourceSnapshotDigest: String?, public val content: ContentBinding,
    public val contentByteLength: ULong, public val citation: Citation, public val retrievedAtUtc: String,
    public val freshness: KnowledgeFreshness, public val trustClass: KnowledgeTrustClass,
    public val rankBasisPoints: Int,
) {
    /** Validates every content, citation, size, time, trust and rank binding. */
    public fun validate(): KnowledgeErrorCode? = when {
        !validId(evidenceId) || !validId(sourceId) || !validId(sourceRevision) -> KnowledgeErrorCode.INVALID_QUERY
        sourceSnapshotDigest?.let { !validDigest(it) } == true -> KnowledgeErrorCode.INVALID_QUERY
        contentByteLength == 0uL || content.inlineUtf8?.encodeToByteArray()?.size?.toULong()?.let { it != contentByteLength } == true -> KnowledgeErrorCode.CONTENT_DIGEST_MISMATCH
        citation.contentDigest != content.digest -> KnowledgeErrorCode.CONTENT_DIGEST_MISMATCH
        !canonicalUtc(retrievedAtUtc) || rankBasisPoints !in 0..10_000 -> KnowledgeErrorCode.INVALID_QUERY
        else -> null
    }
}

internal fun validId(value: String): Boolean = validText(value, 128)
internal fun validText(value: String, maxBytes: Int = 512): Boolean = value.isNotEmpty() && value.encodeToByteArray().size <= maxBytes && value.trim() == value
internal fun validDigest(value: String): Boolean = value.matches(Regex("[0-9a-f]{64}"))
internal fun canonicalUtc(value: String): Boolean = runCatching { Instant.parse(value).toString() == value }.getOrDefault(false)
internal fun sha256(value: ByteArray): String = MessageDigest.getInstance("SHA-256").digest(value).joinToString("") { "%02x".format(it) }
internal fun <T> success(value: T): KnowledgeContractResult<T> = KnowledgeContractResult.Success(value)
internal fun failure(code: KnowledgeErrorCode): KnowledgeContractResult.Failure = KnowledgeContractResult.Failure(code)
