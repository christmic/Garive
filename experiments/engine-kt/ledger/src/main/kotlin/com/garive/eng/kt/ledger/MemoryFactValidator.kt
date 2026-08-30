package com.garive.eng.kt.ledger

import java.time.Instant
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

private val memoryKinds = setOf("preference", "constraint", "decision", "learned_fact", "summary")
private val sensitivities = setOf("ordinary", "restricted")
private val memoryTypes = setOf("semantic", "episodic", "lesson", "procedural")
private val memoryAuthorities = setOf("user_declared", "agent_learned", "organisation_published")
private val hypothesisStates = setOf("candidate", "active", "cold", "archived", "promoted")
private val controlScopes = setOf("session", "agent_instance", "user", "project", "platform")

internal fun validateMemoryFact(kind: String, value: JsonObject) {
    when (kind) {
        "memory.proposed" -> value.proposal()
        "memory.committed" -> value.committed()
        "memory.revision_classified" -> value.revisionClassified()
        "memory.rejected" -> value.rejected()
        "memory.superseded" -> value.superseded()
        "memory.tombstoned" -> value.tombstoned()
        "memory.retrieval_recorded" -> value.retrieval()
        "memory.recall_recorded" -> value.recall()
        "memory.obligation_opened" -> value.obligation()
        "memory.observation_recorded" -> value.observation()
        "memory.lifecycle_transitioned" -> value.lifecycle()
        "memory.candidate_recorded" -> value.candidate()
        "memory.maintenance_decided" -> value.maintenanceDecision()
        "memory.distillation_checkpointed" -> value.distillationCheckpoint()
        "memory.audit_recorded" -> value.audit()
        "memory.promotion_requested" -> value.promotionRequest()
        "memory.promotion_recorded" -> value.promotionReceipt()
        "memory.erasure_requested" -> value.erasureRequest()
        "memory.erasure_recorded" -> value.erasureReceipt()
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.revisionClassified() {
    exact(
        setOf("classification_id", "namespace_id", "record_id", "revision_id", "memory_type",
            "authority", "lifecycle", "scope", "scope_owner_id", "policy_revision", "source_commit"),
        setOf("aggregation_policy_digest", "authority_receipt_digest"),
    )
    listOf("classification_id", "namespace_id", "record_id", "revision_id", "scope_owner_id", "policy_revision").forEach(::nonEmpty)
    enum("memory_type", memoryTypes)
    val authority = enum("authority", memoryAuthorities)
    enum("lifecycle", setOf("candidate", "active"))
    val scope = enum("scope", controlScopes)
    getValue("source_commit").jsonObject.factReference()
    require((authority != "agent_learned") == ("authority_receipt_digest" in this))
    if (authority != "agent_learned") digest("authority_receipt_digest")
    require((scope == "platform") == ("aggregation_policy_digest" in this))
    if (scope == "platform") digest("aggregation_policy_digest")
}

private fun JsonObject.candidate() {
    exact(setOf("candidate_id", "namespace_id", "extractor_revision", "source", "intent_kind", "intent_digest"))
    listOf("candidate_id", "namespace_id", "extractor_revision").forEach(::nonEmpty)
    enum("source", setOf("explicit_user_command", "session_end", "exit_summary", "scheduled_distillation"))
    enum("intent_kind", setOf("learn", "forget"))
    digest("intent_digest")
}

private fun JsonObject.maintenanceDecision() {
    exact(setOf("decision_id", "candidate_id", "namespace_id", "decision_kind", "decision_digest"))
    listOf("decision_id", "candidate_id", "namespace_id").forEach(::nonEmpty)
    enum("decision_kind", setOf("add", "update", "delete", "noop"))
    digest("decision_digest")
}

private fun JsonObject.distillationCheckpoint() {
    exact(setOf("checkpoint_id", "namespace_id", "extractor_revision", "session_id", "through_position", "batch_digest"))
    listOf("checkpoint_id", "namespace_id", "extractor_revision", "session_id").forEach(::nonEmpty)
    ulong("through_position", true)
    digest("batch_digest")
}

private fun JsonObject.audit() {
    exact(setOf("audit_id", "namespace_id", "through_position", "policy_digest", "inventory_digest", "report_digest", "action_count", "truncated"))
    listOf("audit_id", "namespace_id").forEach(::nonEmpty)
    ulong("through_position", true)
    listOf("policy_digest", "inventory_digest", "report_digest").forEach(::digest)
    ulong("action_count")
    require(getValue("truncated").jsonPrimitive.booleanOrNull != null)
}

private fun JsonObject.promotionRequest() {
    exact(setOf("request_id", "namespace_id", "record_id", "revision_id", "memory_type", "policy_revision", "knowledge_proposal_id", "evidence_digest"))
    listOf("request_id", "namespace_id", "record_id", "revision_id", "policy_revision", "knowledge_proposal_id").forEach(::nonEmpty)
    enum("memory_type", memoryTypes)
    digest("evidence_digest")
}

private fun JsonObject.promotionReceipt() {
    exact(setOf("request_id", "namespace_id", "record_id", "revision_id", "knowledge_proposal_id", "knowledge_record_id", "knowledge_revision_id", "receipt_digest"))
    listOf("request_id", "namespace_id", "record_id", "revision_id", "knowledge_proposal_id", "knowledge_record_id", "knowledge_revision_id").forEach(::nonEmpty)
    digest("receipt_digest")
}

private fun JsonObject.erasureRequest() {
    exact(setOf("request_id", "namespace_id", "record_id", "revision_id", "tombstone_fact", "policy_revision", "targets"))
    listOf("request_id", "namespace_id", "record_id", "revision_id", "policy_revision").forEach(::nonEmpty)
    getValue("tombstone_fact").jsonObject.factReference()
    val targets = getValue("targets").jsonArray
    require(targets.isNotEmpty() && targets.size <= 64)
    var prior: Pair<Int, String>? = null
    targets.forEach { element ->
        val target = element.jsonObject
        target.exact(setOf("target_id", "kind"))
        target.nonEmpty("target_id")
        val kind = target.enum("kind", setOf("primary_store", "projection", "cache", "backup"))
        val order = when (kind) { "primary_store" -> 0; "projection" -> 1; "cache" -> 2; else -> 3 }
        val current = order to target.text("target_id")
        prior?.let { previous ->
            require(current.first > previous.first || current.first == previous.first && current.second > previous.second)
        }
        prior = current
    }
}

private fun JsonObject.erasureReceipt() {
    exact(setOf("request_id", "namespace_id", "record_id", "revision_id", "attempt_id", "attempted_at_position", "results", "disposition"))
    listOf("request_id", "namespace_id", "record_id", "revision_id", "attempt_id").forEach(::nonEmpty)
    ulong("attempted_at_position", true)
    val attempted = memoryUlong("attempted_at_position")
    val results = getValue("results").jsonArray
    require(results.isNotEmpty() && results.size <= 64)
    val targets = mutableSetOf<String>()
    var complete = true
    results.forEach { element ->
        val result = element.jsonObject
        result.exact(setOf("target_id", "status", "receipt_digest"), setOf("not_before_position"))
        result.nonEmpty("target_id")
        require(targets.add(result.text("target_id")))
        val status = result.enum("status", setOf("erased", "not_present", "pending_backup_retention", "pending_retry"))
        result.digest("receipt_digest")
        val pendingBackup = status == "pending_backup_retention"
        require(pendingBackup == ("not_before_position" in result))
        if (pendingBackup) {
            result.ulong("not_before_position", true)
            require(result.memoryUlong("not_before_position") > attempted)
        }
        complete = complete && status in setOf("erased", "not_present")
    }
    require((enum("disposition", setOf("complete", "partial")) == "complete") == complete)
}

private fun JsonObject.recall() {
    exact(
        setOf("selection_id", "request_digest", "namespace_id", "product", "selection_policy_revision",
            "through_position", "max_items", "max_total_bytes", "items", "truncated"),
        setOf("exploration"),
    )
    listOf("selection_id", "namespace_id", "selection_policy_revision").forEach(::nonEmpty)
    digest("request_digest")
    val product = enum("product", setOf("menu", "detail"))
    ulong("through_position")
    ulong("max_items", true)
    ulong("max_total_bytes", true)
    val maxItems = memoryUlong("max_items")
    val maxBytes = memoryUlong("max_total_bytes")
    val exploration = get("exploration")?.jsonObject
    exploration?.let {
        it.exact(setOf("algorithm_revision", "seed", "slots"))
        it.enum("algorithm_revision", setOf("hash-explore-v1"))
        it.ulong("seed")
        it.ulong("slots", true)
    }
    val items = getValue("items").jsonArray
    require(items.size.toULong() <= maxItems)
    var bytes = 0uL
    items.forEach { element ->
        val item = element.jsonObject
        item.exact(
            setOf("record_id", "revision_id", "memory_type", "role", "authority", "state", "safe_label",
                "content_digest", "content_byte_length", "evidence_count", "relevance_basis_points",
                "recency_basis_points", "importance_basis_points", "selection_kind"),
            setOf("draw_hex"),
        )
        listOf("record_id", "revision_id", "safe_label").forEach(item::nonEmpty)
        require(item.text("safe_label").encodeToByteArray().size <= 256)
        item.enum("memory_type", memoryTypes)
        item.enum("role", memoryKinds)
        item.enum("authority", memoryAuthorities)
        val state = item.enum("state", hypothesisStates)
        require(state != "promoted" && !(product == "menu" && state == "archived"))
        item.digest("content_digest")
        item.ulong("content_byte_length", true)
        val itemBytes = item.memoryUlong("content_byte_length")
        item.ulong("evidence_count", true)
        listOf("relevance_basis_points", "recency_basis_points", "importance_basis_points").forEach(item::basisPoints)
        val selection = item.enum("selection_kind", setOf("ranked", "explored"))
        require((selection == "explored") == ("draw_hex" in item))
        require(selection != "explored" || exploration != null)
        item["draw_hex"]?.jsonPrimitive?.content?.let { draw ->
            require(draw.matches(Regex("[0-9a-f]{16}")))
        }
        val next = bytes + itemBytes
        require(next >= bytes)
        bytes = next
    }
    require(bytes <= maxBytes)
    require(getValue("truncated").jsonPrimitive.booleanOrNull != null)
}

private fun JsonObject.obligation() {
    exact(setOf("obligation_id", "namespace_id", "record_id", "revision_id", "recall_fact", "selection_id", "application_fact",
        "expected_outcome_digest", "application_scope_digest", "attribution_policy_revision", "expires_at_position"))
    listOf("obligation_id", "namespace_id", "record_id", "revision_id", "selection_id", "attribution_policy_revision").forEach(::nonEmpty)
    getValue("recall_fact").jsonObject.factReference()
    getValue("application_fact").jsonObject.factReference()
    digest("expected_outcome_digest")
    digest("application_scope_digest")
    ulong("expires_at_position", true)
}

private fun JsonObject.observation() {
    exact(setOf("observation_id", "obligation_id", "namespace_id", "position", "verifier_revision", "evidence", "verdict"))
    listOf("observation_id", "obligation_id", "namespace_id", "verifier_revision").forEach(::nonEmpty)
    ulong("position", true)
    val evidence = getValue("evidence").jsonArray
    require(evidence.isNotEmpty())
    evidence.forEach { element ->
        val item = element.jsonObject
        item.exact(setOf("kind", "fact"))
        item.enum("kind", setOf("tool_result", "test_result", "effect_receipt", "user_correction", "deterministic_verifier"))
        item.getValue("fact").jsonObject.factReference()
    }
    val verdict = getValue("verdict").jsonObject
    when (verdict.text("kind")) {
        "verified" -> verdict.exact(setOf("kind"))
        "neutral" -> { verdict.exact(setOf("kind", "safe_reason")); verdict.nonEmpty("safe_reason") }
        "falsified" -> {
            verdict.exact(setOf("kind", "in_scope"), setOf("observed_scope_digest"))
            val inScope = verdict.getValue("in_scope").jsonPrimitive.booleanOrNull ?: throw IllegalArgumentException()
            require(inScope != ("observed_scope_digest" in verdict))
            if (!inScope) verdict.digest("observed_scope_digest")
        }
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.lifecycle() {
    exact(setOf("transition_id", "namespace_id", "record_id", "revision_id", "from_state", "to_state",
        "verified", "falsified", "neutral", "last_observed_position", "cause_kind", "cause_id"),
        setOf("promoted_knowledge_receipt_digest"))
    listOf("transition_id", "namespace_id", "record_id", "revision_id", "cause_id").forEach(::nonEmpty)
    enum("from_state", hypothesisStates)
    val state = enum("to_state", hypothesisStates)
    listOf("verified", "falsified", "neutral").forEach { ulong(it) }
    ulong("last_observed_position", true)
    enum("cause_kind", setOf("observation", "maintenance", "promotion", "toolchain_changed"))
    require((state == "promoted") == ("promoted_knowledge_receipt_digest" in this))
    if (state == "promoted") digest("promoted_knowledge_receipt_digest")
}

private fun JsonObject.factReference() {
    exact(setOf("session_id", "position", "fact_id", "payload_digest"))
    nonEmpty("session_id")
    ulong("position", true)
    nonEmpty("fact_id")
    digest("payload_digest")
}

private fun JsonObject.memoryUlong(key: String): ULong =
    getValue(key).jsonPrimitive.content.toULongOrNull() ?: throw IllegalArgumentException()

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
