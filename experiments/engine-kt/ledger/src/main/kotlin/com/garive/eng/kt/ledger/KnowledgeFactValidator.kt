package com.garive.eng.kt.ledger

import java.time.Instant
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

internal fun validateKnowledgeFact(kind: String, value: JsonObject) {
    when (kind) {
        "knowledge.requested" -> value.requested()
        "knowledge.dispatched" -> value.dispatched()
        "knowledge.completed" -> value.completed()
        "knowledge.failed" -> value.failed()
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.requested() {
    exact(
        setOf(
            "request_id", "source_id", "source_revision", "request_digest", "mode", "query", "filters",
            "through_position", "max_chunks", "max_total_bytes", "deadline_budget_ms", "freshness_kind",
        ),
        setOf("exact_snapshot_digest"),
    )
    listOf("request_id", "source_id", "source_revision").forEach(::nonEmpty)
    digest("request_digest")
    enum("mode", setOf("keyword", "semantic", "structured"))
    content("query")
    content("filters")
    ulong("through_position")
    listOf("max_chunks", "max_total_bytes", "deadline_budget_ms").forEach { ulong(it, true) }
    val freshness = enum("freshness_kind", setOf("cached_allowed", "revalidate", "exact_snapshot"))
    require((freshness == "exact_snapshot") == ("exact_snapshot_digest" in this))
    if (freshness == "exact_snapshot") digest("exact_snapshot_digest")
}

private fun JsonObject.dispatched() {
    exact(setOf("request_id", "request_digest", "dispatch_attempt_id"))
    nonEmpty("request_id")
    digest("request_digest")
    nonEmpty("dispatch_attempt_id")
}

private fun JsonObject.completed() {
    exact(setOf("request_id", "request_digest", "evidence", "truncated"))
    nonEmpty("request_id")
    digest("request_digest")
    getValue("evidence").jsonArray.forEach { it.jsonObject.evidence() }
    require(getValue("truncated").jsonPrimitive.booleanOrNull != null)
}

private fun JsonObject.evidence() {
    exact(
        setOf(
            "evidence_id", "content", "content_byte_length", "citation_kind", "citation_locator",
            "citation_content_digest", "retrieved_at_utc", "freshness", "trust_class", "rank_basis_points",
        ),
        setOf("source_snapshot_digest", "citation_title", "canonical_uri"),
    )
    nonEmpty("evidence_id")
    if ("source_snapshot_digest" in this) digest("source_snapshot_digest")
    content("content")
    ulong("content_byte_length", true)
    getValue("content").jsonObject["inline_utf8"]?.jsonPrimitive?.content?.let { inline ->
        require(getValue("content_byte_length").jsonPrimitive.content.toULong() == inline.encodeToByteArray().size.toULong())
    }
    enum("citation_kind", setOf("uri_fragment", "document_offset", "record_key", "opaque_locator"))
    nonEmpty("citation_locator")
    optionalNonEmpty("citation_title")
    optionalNonEmpty("canonical_uri")
    digest("citation_content_digest")
    require(text("citation_content_digest") == getValue("content").jsonObject.text("digest"))
    require(Instant.parse(text("retrieved_at_utc")).toString() == text("retrieved_at_utc"))
    enum("freshness", setOf("fresh", "cached", "stale"))
    enum("trust_class", setOf("curated", "first_party", "third_party", "untrusted"))
    val rank = getValue("rank_basis_points").jsonPrimitive.content.toULongOrNull()
        ?: throw IllegalArgumentException()
    require(rank <= 10_000uL)
}

private fun JsonObject.failed() {
    exact(
        setOf("request_id", "request_digest", "phase", "reason", "ambiguous"),
        setOf("retry_after_ms"),
    )
    nonEmpty("request_id")
    digest("request_digest")
    val phase = enum("phase", setOf("pre_dispatch", "dispatched", "response_validation"))
    val reason = enum(
        "reason",
        setOf(
            "invalid_query", "source_not_found", "source_revision_mismatch", "source_denied",
            "filter_unsupported", "freshness_unavailable", "connector_unavailable", "connector_rejected",
            "retrieval_uncertain", "citation_invalid", "content_digest_mismatch", "limit_exceeded",
            "durability_failure", "corrupt_knowledge_state",
        ),
    )
    val ambiguous = getValue("ambiguous").jsonPrimitive.booleanOrNull ?: throw IllegalArgumentException()
    require(ambiguous == (phase == "dispatched" && reason == "retrieval_uncertain"))
    if ("retry_after_ms" in this) ulong("retry_after_ms", true)
}
