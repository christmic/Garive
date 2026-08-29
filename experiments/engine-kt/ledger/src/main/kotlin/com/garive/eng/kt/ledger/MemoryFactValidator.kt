package com.garive.eng.kt.ledger

import java.time.Instant
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

private val memoryKinds = setOf("preference", "constraint", "decision", "learned_fact", "summary")
private val sensitivities = setOf("ordinary", "restricted")

internal fun validateMemoryFact(kind: String, value: JsonObject) {
    when (kind) {
        "memory.proposed" -> value.proposal()
        "memory.committed" -> value.committed()
        "memory.rejected" -> value.rejected()
        "memory.superseded" -> value.superseded()
        "memory.tombstoned" -> value.tombstoned()
        "memory.retrieval_recorded" -> value.retrieval()
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.proposal() {
    exact(
        setOf("proposal_id", "namespace_id", "scope", "kind", "content", "evidence", "sensitivity", "confidence_basis_points"),
        setOf("expected_active_revision_id"),
    )
    listOf("proposal_id", "namespace_id").forEach(::nonEmpty)
    memoryScope("scope")
    enum("kind", memoryKinds)
    content("content")
    memoryEvidence("evidence")
    enum("sensitivity", sensitivities)
    basisPoints("confidence_basis_points")
    optionalNonEmpty("expected_active_revision_id")
}

private fun JsonObject.committed() {
    exact(
        setOf(
            "proposal_id", "record_id", "revision_id", "namespace_id", "scope", "kind", "content", "evidence",
            "sensitivity", "confidence_basis_points", "valid_from_position", "retention_policy_digest",
        ),
        setOf("expires_at_utc", "supersedes_revision_id"),
    )
    listOf("proposal_id", "record_id", "revision_id", "namespace_id").forEach(::nonEmpty)
    memoryScope("scope")
    enum("kind", memoryKinds)
    content("content")
    memoryEvidence("evidence")
    enum("sensitivity", sensitivities)
    basisPoints("confidence_basis_points")
    ulong("valid_from_position", true)
    digest("retention_policy_digest")
    optionalNonEmpty("supersedes_revision_id")
    if ("expires_at_utc" in this) timestamp("expires_at_utc")
}

private fun JsonObject.rejected() {
    exact(setOf("proposal_id", "reason"))
    nonEmpty("proposal_id")
    enum(
        "reason",
        setOf(
            "namespace_denied", "evidence_not_found", "evidence_mismatch", "revision_conflict",
            "retention_rejected", "sensitivity_denied", "limit_exceeded", "unsupported",
        ),
    )
}

private fun JsonObject.superseded() {
    exact(setOf("record_id", "old_revision_id", "new_revision_id", "proposal_id"))
    listOf("record_id", "old_revision_id", "new_revision_id", "proposal_id").forEach(::nonEmpty)
    require(text("old_revision_id") != text("new_revision_id"))
}

private fun JsonObject.tombstoned() {
    exact(setOf("command_id", "record_id", "revision_id", "reason"))
    listOf("command_id", "record_id", "revision_id").forEach(::nonEmpty)
    enum("reason", setOf("expired", "superseded", "user_request", "policy", "corrupt_source"))
}

private fun JsonObject.retrieval() {
    exact(
        setOf(
            "query_id", "query_digest", "namespace_id", "retriever_revision", "through_position", "as_of_utc",
            "max_results", "max_total_bytes", "include_restricted", "matches", "truncated",
        ),
        setOf("restricted_grant_digest"),
    )
    listOf("query_id", "namespace_id", "retriever_revision").forEach(::nonEmpty)
    digest("query_digest")
    ulong("through_position")
    timestamp("as_of_utc")
    ulong("max_results", true)
    ulong("max_total_bytes", true)
    val includeRestricted = getValue("include_restricted").jsonPrimitive.booleanOrNull
        ?: throw IllegalArgumentException()
    require(includeRestricted == ("restricted_grant_digest" in this))
    if (includeRestricted) digest("restricted_grant_digest")
    getValue("matches").jsonArray.forEach { element ->
        val match = element.jsonObject
        match.exact(
            setOf(
                "record_id", "revision_id", "content", "content_byte_length", "evidence",
                "relevance_basis_points", "sensitivity",
            ),
        )
        listOf("record_id", "revision_id").forEach(match::nonEmpty)
        match.content("content")
        match.ulong("content_byte_length", true)
        match["content"]!!.jsonObject["inline_utf8"]?.jsonPrimitive?.content?.let { inline ->
            require(match.getValue("content_byte_length").jsonPrimitive.content.toULong() == inline.encodeToByteArray().size.toULong())
        }
        match.memoryEvidence("evidence")
        match.basisPoints("relevance_basis_points")
        require(match.enum("sensitivity", sensitivities) != "restricted" || includeRestricted)
    }
    require(getValue("truncated").jsonPrimitive.booleanOrNull != null)
}

private fun JsonObject.memoryScope(key: String) {
    val scope = getValue(key).jsonObject
    when (scope.enum("kind", setOf("namespace", "session", "agent_instance"))) {
        "namespace" -> scope.exact(setOf("kind"))
        else -> {
            scope.exact(setOf("kind", "owner_id"))
            scope.nonEmpty("owner_id")
        }
    }
}

private fun JsonObject.memoryEvidence(key: String) {
    val evidence = getValue(key).jsonArray
    require(evidence.isNotEmpty())
    evidence.forEach { element ->
        val item = element.jsonObject
        item.exact(setOf("session_id", "position", "fact_id", "payload_digest"))
        item.nonEmpty("session_id")
        item.ulong("position", true)
        item.nonEmpty("fact_id")
        item.digest("payload_digest")
    }
}

private fun JsonObject.basisPoints(key: String) {
    val points = getValue(key).jsonPrimitive.content.toULongOrNull() ?: throw IllegalArgumentException()
    require(points <= 10_000uL)
}

private fun JsonObject.timestamp(key: String) {
    val raw = text(key)
    require(Instant.parse(raw).toString() == raw)
}
