package com.garive.eng.kt.memory

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse

public class MemoryHypothesisFixtureTest {
    private val root: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/memory-hypothesis-lifecycle-v1.json").readText(),
    ).jsonObject

    @Test
    public fun sharedRegistryAndImportsAreExact(): Unit {
        val registry = registry()
        root.values("imports").forEach { value ->
            val binding = MemoryAuthorityBinding.create(
                authority(value.text("authority")), value.optional("receipt_digest"),
            ).success()
            val imported = importM0Classification(role(value.text("m0_kind")), binding)
            assertEquals(type(value.text("expected_type")), imported.memoryType)
            assertEquals(role(value.text("expected_role")), imported.role)
            assertEquals(true, registry.admits(imported.memoryType, imported.role, binding.authority))
        }
    }

    @Test
    public fun sharedInvalidCasesFailClosed(): Unit {
        val registry = registry()
        root.values("invalid").forEach { value ->
            val actual = when (value.text("name")) {
                "user_without_receipt", "agent_with_receipt" -> MemoryAuthorityBinding.create(
                    authority(value.text("authority")), value.optional("receipt_digest"),
                ).failure().code.wireName
                "platform_without_policy" -> MemoryScopeBinding.create(
                    MemoryScopeClass.PLATFORM, null,
                ).failure().code.wireName
                "unsupported_pair" -> {
                    assertFalse(registry.admits(type(value.text("type")), role(value.text("role")), authority(value.text("authority"))))
                    MemoryErrorCode.UNKNOWN_MEMORY_TYPE.wireName
                }
                else -> error("unknown case")
            }
            assertEquals(value.text("expected"), actual, value.text("name"))
        }
    }

    @Test
    public fun registryOrderAndPlatformPolicyAreStrict(): Unit {
        val rows = descriptors().toMutableList().also { it[0] = it[1].also { row -> it[1] = it[0] } }
        assertEquals(MemoryErrorCode.UNKNOWN_MEMORY_TYPE, MemoryTypeRegistry.create("r", rows).failure().code)
        assertEquals(MemoryScopeClass.PLATFORM, MemoryScopeBinding.create(MemoryScopeClass.PLATFORM, "b".repeat(64)).success().scope)
        assertEquals(MemoryErrorCode.INVALID_MEMORY, MemoryScopeBinding.create(MemoryScopeClass.PROJECT, "b".repeat(64)).failure().code)
    }

    @Test
    public fun sharedLifecycleReducesExactTalliesAndFailures(): Unit {
        root.values("lifecycle_cases").forEach { value ->
            val initial = value.getValue("initial").jsonObject
            val lifecycle = MemoryLifecycle.create(
                state(initial.text("state")),
                EvidenceTally(initial.ulong("verified"), initial.ulong("falsified"), initial.ulong("neutral")),
                initial.ulong("last_position"), null,
            ).success()
            val result = lifecycle.apply(event(value.getValue("event").jsonObject))
            value.optional("failure")?.let { expected ->
                assertEquals(expected, result.failure().code.wireName, value.text("name"))
            } ?: run {
                val expected = value.getValue("expected").jsonObject
                val actual = result.success()
                assertEquals(state(expected.text("state")), actual.state, value.text("name"))
                assertEquals(
                    EvidenceTally(expected.ulong("verified"), expected.ulong("falsified"), expected.ulong("neutral")),
                    actual.tally,
                )
                if (actual.state == HypothesisState.PROMOTED) {
                    assertEquals(true, actual.promotedKnowledgeReceiptDigest != null)
                }
            }
        }
    }

    @Test
    public fun sharedRecallIsBoundedRankedAndReplayable(): Unit {
        val candidates = root.getValue("recall_candidates").jsonArray.map(::recallCandidate)
        root.values("recall_cases").forEach { value ->
            val request = recallRequest(value)
            value.optional("failure")?.let { expected ->
                assertEquals(expected, request.failure().code.wireName, value.text("name"))
            } ?: run {
                val admitted = request.success()
                val first = selectRecall(candidates, admitted).success()
                val second = selectRecall(candidates, admitted).success()
                assertEquals(first, second, value.text("name"))
                assertEquals(value.strings("expected_ids"), first.items.map { it.candidate.recordId })
                assertEquals(
                    value.strings("expected_kinds"),
                    first.items.map { it.kind.name.lowercase() },
                )
                value["expected_draws"]?.jsonArray?.let { draws ->
                    assertEquals(draws.map { it.jsonPrimitive.contentOrNull }, first.items.map { it.drawHex })
                }
                assertEquals(value.getValue("truncated").jsonPrimitive.content.toBooleanStrict(), first.truncated)
            }
        }
        val request = recallRequest(root.values("recall_cases").first()).success()
        assertEquals(
            MemoryErrorCode.INVALID_MEMORY,
            selectRecall(listOf(candidates.first(), candidates.first()), request).failure().code,
        )
    }

    private fun registry(): MemoryTypeRegistry = MemoryTypeRegistry.create(
        root.getValue("registry").jsonObject.text("revision"), descriptors(),
    ).success()

    private fun descriptors(): List<MemoryTypeDescriptor> =
        root.getValue("registry").jsonObject.values("descriptors").map { value ->
            MemoryTypeDescriptor.create(
                type(value.text("type")), value.strings("roles").map(::role),
                value.strings("authorities").map(::authority), value.text("lifecycle"),
                value.text("recall"), value.text("retention"), value.text("surface_kind"),
            ).success()
        }
}

private fun type(value: String): MemoryType = MemoryType.entries.first { it.wireName == value }
private fun role(value: String): MemoryKind = MemoryKind.entries.first { it.wireName == value }
private fun authority(value: String): MemoryAuthority = MemoryAuthority.entries.first { it.wireName == value }
private fun state(value: String): HypothesisState = HypothesisState.entries.first { it.wireName == value }
private fun event(value: JsonObject): LifecycleEvent = when (value.text("kind")) {
    "verified" -> LifecycleEvent.Verified(value.ulong("position"))
    "falsified_in_scope" -> LifecycleEvent.Falsified(value.ulong("position"), true)
    "falsified_out_of_scope" -> LifecycleEvent.Falsified(value.ulong("position"), false)
    "neutral" -> LifecycleEvent.Neutral(value.ulong("position"))
    "cool" -> LifecycleEvent.Cool(value.ulong("position"))
    "archive" -> LifecycleEvent.Archive(value.ulong("position"))
    "promote" -> LifecycleEvent.Promote(value.ulong("position"), value.optional("receipt_digest"))
    else -> error("unknown event")
}
private fun recallCandidate(element: JsonElement): MemoryRecallCandidate {
    val value = element.jsonObject
    return MemoryRecallCandidate.create(
        value.text("record_id"), value.text("revision_id"), type(value.text("type")),
        role(value.text("role")), authority(value.text("authority")), state(value.text("state")),
        value.text("safe_label"), value.text("content_digest"), value.ulong("content_bytes"),
        value.text("evidence_count").toUInt(), RecallScore(
            value.text("relevance").toInt(), value.text("recency").toInt(), value.text("importance").toInt(),
        ),
    ).success()
}
private fun recallRequest(value: JsonObject): MemoryContractResult<RecallSelectionRequest> {
    val exploration = value["exploration"]?.jsonObject?.let { item ->
        when (val result = RecallExploration.create(
            item.text("algorithm"), item.ulong("seed"), item.text("slots").toUInt(),
        )) {
            is MemoryContractResult.Success -> result.value
            is MemoryContractResult.Failure -> return result
        }
    }
    return RecallSelectionRequest.create(
        if (value.text("product") == "menu") RecallProduct.MENU else RecallProduct.DETAIL,
        value.strings("types").map(::type), value.strings("roles").map(::role),
        value.strings("states").map(::state), "score-sum-v1", value.text("max_items").toUInt(),
        value.ulong("max_bytes"), exploration,
    )
}
private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
private fun JsonObject.optional(key: String): String? = get(key)?.jsonPrimitive?.contentOrNull
private fun JsonObject.values(key: String): List<JsonObject> = getValue(key).jsonArray.map { it.jsonObject }
private fun JsonObject.strings(key: String): List<String> = getValue(key).jsonArray.map { it.jsonPrimitive.content }
private fun JsonObject.ulong(key: String): ULong = text(key).toULong()
private fun <T> MemoryContractResult<T>.success(): T = when (this) {
    is MemoryContractResult.Success -> value
    is MemoryContractResult.Failure -> error("unexpected failure: $error")
}
private fun MemoryContractResult<*>.failure(): MemoryError = when (this) {
    is MemoryContractResult.Success -> error("unexpected success: $value")
    is MemoryContractResult.Failure -> error
}
