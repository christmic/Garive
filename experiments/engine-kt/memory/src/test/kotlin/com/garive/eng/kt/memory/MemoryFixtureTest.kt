package com.garive.eng.kt.memory

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals

public class MemoryFixtureTest {
    private val root: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/memory-capability-v1.json").readText(),
    ).jsonObject

    @Test
    public fun sharedQueriesEnforceOrderAndRuntimePrevalidatedVisibility(): Unit {
        val records = root.array("records").map(::record)
        val scores = root.array("scores").map { element ->
            val value = element.jsonObject
            MemoryScore.create(
                value.string("record_id"), value.string("revision_id"),
                value.int("relevance_basis_points"), value.ulong("content_byte_length"),
            ).success()
        }
        root.array("queries").forEach { element ->
            val value = element.jsonObject
            val query = query(value)
            value["expected_query_digest"]?.jsonPrimitive?.contentOrNull?.let { expected ->
                assertEquals(expected, query.queryDigest().success(), value.string("name"))
            }
            val result = retrieveMemory(records, scores, query).success()
            assertEquals(
                value.objectValue("expected").array("record_ids").map { it.jsonPrimitive.content },
                result.matches.map(MemoryMatch::recordId),
                value.string("name"),
            )
            assertEquals(
                value.objectValue("expected").boolean("truncated"), result.truncated, value.string("name"),
            )
        }
    }

    @Test
    public fun sharedWritesApplyAtomically(): Unit {
        val records = root.array("records").map(::record)
        root.array("write_cases").forEach { element ->
            val value = element.jsonObject
            val state = MemoryState.create(records).success()
            val before = state.revisions
            val result = state.commit(
                proposal(value.objectValue("proposal")), commit(value.objectValue("commit")),
            )
            when (value.string("expected")) {
                "committed" -> {
                    assertEquals(MemoryStatus.ACTIVE, result.success().record.status)
                    assertEquals(before.size + 1, state.revisions.size)
                }
                "revision_conflict" -> {
                    assertEquals(MemoryErrorCode.REVISION_CONFLICT, result.failure().code)
                    assertEquals(before, state.revisions)
                }
                else -> error("unknown expected")
            }
        }
    }

    @Test
    public fun invalidAuthoritySizeEvidenceAndTombstoneShapesFailClosed(): Unit {
        assertEquals(
            MemoryErrorCode.INVALID_MEMORY,
            MemoryQuery.create(
                "q", "ns", listOf(MemoryScope.Namespace), MemoryPurpose.CONTEXT, "r",
                ContentBinding.fromInline("q"), 0uL, "2026-08-29T00:00:00Z", 1u, 1uL, true, null,
            ).failure().code,
        )
        val active = record(root.array("records")[1])
        assertEquals(
            MemoryErrorCode.INVALID_MEMORY,
            retrieveMemory(
                listOf(active),
                listOf(MemoryScore.create(active.recordId, active.revisionId, 1, 99uL).success()),
                query(root.array("queries")[0].jsonObject),
            ).failure().code,
        )
        val proof = evidence()
        assertEquals(
            MemoryErrorCode.INVALID_MEMORY,
            MemoryProposal.create(
                "p", "ns", MemoryScope.Namespace, MemoryKind.SUMMARY, ContentBinding.fromInline("x"),
                listOf(proof, proof), MemorySensitivity.ORDINARY, 1, null,
            ).failure().code,
        )
        val old = record(root.array("records")[3])
        val state = MemoryState.create(listOf(old)).success()
        assertEquals(
            MemoryErrorCode.REVISION_CONFLICT,
            state.tombstone(MemoryTombstone(old.recordId, old.revisionId)).failure().code,
        )
    }

    private fun evidence(): DurableFactReference {
        val value = root.objectValue("evidence")
        return DurableFactReference.create(
            value.string("session_id"), value.ulong("position"), value.string("fact_id"),
            value.string("payload_digest"),
        ).success()
    }

    private fun content(value: JsonObject): ContentBinding =
        value["inline_utf8"]?.jsonPrimitive?.contentOrNull?.let { inline ->
            ContentBinding.inline(value.string("digest"), inline).success()
        } ?: ContentBinding.referenced(value.string("digest"), value.string("reference")).success()

    private fun scope(value: JsonObject): MemoryScope = when (value.string("kind")) {
        "session" -> MemoryScope.session(value.string("owner_id")).success()
        "agent_instance" -> MemoryScope.agentInstance(value.string("owner_id")).success()
        "namespace" -> MemoryScope.Namespace
        else -> error("unknown scope")
    }

    private fun record(element: JsonElement): MemoryRecord {
        val value = element.jsonObject
        return MemoryRecord.create(
            value.string("record_id"), value.string("revision_id"), value.string("namespace_id"),
            scope(value.objectValue("scope")), kind(value.string("kind")), content(value.objectValue("content")),
            listOf(evidence()), MemoryStatus.entries.first { it.wireName == value.string("status") },
            sensitivity(value.string("sensitivity")), value.int("confidence_basis_points"),
            value.ulong("valid_from_position"), value.optional("supersedes_revision_id"),
            value.optional("expires_at_utc"),
        ).success()
    }

    private fun query(value: JsonObject): MemoryQuery = MemoryQuery.create(
        value.string("query_id"), value.string("namespace_id"),
        value.array("allowed_scopes").map { scope(it.jsonObject) },
        MemoryPurpose.entries.first { it.wireName == value.string("purpose") },
        value.string("retriever_revision"), content(value.objectValue("query")),
        value.ulong("through_position"), value.string("as_of_utc"),
        value.uint("max_results"), value.ulong("max_total_bytes"), value.boolean("include_restricted"),
        value.optional("restricted_grant_digest"),
    ).success()

    private fun proposal(value: JsonObject): MemoryProposal = MemoryProposal.create(
        value.string("proposal_id"), value.string("namespace_id"), scope(value.objectValue("scope")),
        kind(value.string("kind")), content(value.objectValue("content")), listOf(evidence()),
        sensitivity(value.string("sensitivity")), value.int("confidence_basis_points"),
        value.optional("expected_active_revision_id"),
    ).success()

    private fun commit(value: JsonObject): MemoryCommit = MemoryCommit.create(
        value.string("record_id"), value.string("revision_id"), value.string("retention_policy_digest"),
        value.ulong("valid_from_position"), value.optional("expires_at_utc"),
        value.optional("supersedes_revision_id"),
    ).success()

    private fun kind(value: String): MemoryKind = MemoryKind.entries.first { it.wireName == value }
    private fun sensitivity(value: String): MemorySensitivity =
        MemorySensitivity.entries.first { it.wireName == value }
}

private fun JsonObject.string(key: String): String = getValue(key).jsonPrimitive.content
private fun JsonObject.optional(key: String): String? = get(key)?.jsonPrimitive?.contentOrNull
private fun JsonObject.array(key: String) = getValue(key).jsonArray
private fun JsonObject.objectValue(key: String) = getValue(key).jsonObject
private fun JsonObject.int(key: String): Int = string(key).toInt()
private fun JsonObject.uint(key: String): UInt = string(key).toUInt()
private fun JsonObject.ulong(key: String): ULong = string(key).toULong()
private fun JsonObject.boolean(key: String): Boolean = string(key).toBooleanStrict()

private fun <T> MemoryContractResult<T>.success(): T = when (this) {
    is MemoryContractResult.Success -> value
    is MemoryContractResult.Failure -> error("unexpected failure: $error")
}

private fun MemoryContractResult<*>.failure(): MemoryError = when (this) {
    is MemoryContractResult.Success -> error("unexpected success: $value")
    is MemoryContractResult.Failure -> error
}
