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

public class MemoryPromotionErasureFixtureTest {
    private val root: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/memory-promotion-erasure-v1.json").readText(),
    ).jsonObject

    @Test
    public fun sharedPromotionPolicyAdmitsOnlyEvidencedActiveOrColdMemory(): Unit {
        val policy = promotionPolicy(root.getValue("promotion_policy").jsonObject)
        root.objects("promotion_cases").forEach { case ->
            val result = requestMemoryPromotion(
                "promotion-request", "namespace", "record", "revision",
                memoryType(case.text("type")), lifecycle(case), case.ulong("helpful_uses"),
                policy, "knowledge-proposal", "e".repeat(64),
            )
            case.optional("failure")?.let { expected ->
                assertEquals(expected, result.failure().code.wireName, case.text("name"))
            } ?: run {
                assertEquals("promotion-request", result.success().requestId)
                assertEquals(case.text("expected"), "requested")
            }
        }
    }

    @Test
    public fun sharedPromotionReceiptsBindExactRequestAndTransition(): Unit {
        val policy = promotionPolicy(root.getValue("promotion_policy").jsonObject)
        root.objects("promotion_receipt_cases").forEach { case ->
            val lifecycle = MemoryLifecycle.create(
                HypothesisState.ACTIVE, EvidenceTally(3uL, 0uL, 0uL), 20uL, null,
            ).success()
            val request = requestMemoryPromotion(
                case.text("request_id"), "namespace", "record", "revision", MemoryType.LESSON,
                lifecycle, 2uL, policy, case.text("proposal_id"), "e".repeat(64),
            ).success()
            val receipt = MemoryPromotionReceipt.create(
                case.text("receipt_request_id"), case.text("receipt_proposal_id"),
                "knowledge-record", "knowledge-revision", "f".repeat(64),
            ).success()
            val result = completeMemoryPromotion(request, receipt, lifecycle, case.ulong("position"))
            case.optional("failure")?.let { expected ->
                assertEquals(expected, result.failure().code.wireName, case.text("name"))
            } ?: run {
                val promoted = result.success()
                assertEquals(HypothesisState.PROMOTED, promoted.state)
                assertEquals(receipt.receiptDigest, promoted.promotedKnowledgeReceiptDigest)
            }
        }
    }

    @Test
    public fun sharedErasureReceiptsCoverEveryTargetAndExposePendingWork(): Unit {
        val request = erasureRequest()
        root.objects("erasure_cases").forEach { case ->
            val results = case.objects("results").map { value ->
                MemoryErasureTargetResult.create(
                    value.text("target_id"), status(value.text("status")),
                    value.text("receipt_digest"), value.optional("not_before_position")?.toULong(),
                )
            }
            val firstFailure = results.filterIsInstance<MemoryContractResult.Failure>().firstOrNull()
            val result: MemoryContractResult<MemoryErasureReceipt> = firstFailure ?: recordMemoryErasure(
                request, "attempt-${case.text("name")}", case.ulong("attempted_at_position"),
                results.map { it.success() },
            )
            case.optional("failure")?.let { expected ->
                assertEquals(expected, result.failure().code.wireName, case.text("name"))
            } ?: run {
                val receipt = result.success()
                assertEquals(case.text("expected"), receipt.disposition.name.lowercase(), case.text("name"))
                assertEquals(request.targets.size, receipt.results.size)
            }
        }
    }

    private fun erasureRequest(): MemoryErasureRequest {
        val fact = root.getValue("tombstone_fact").jsonObject
        return MemoryErasureRequest.create(
            "erasure-request", "namespace", "record", "revision",
            DurableFactReference.create(
                fact.text("session_id"), fact.ulong("position"), fact.text("fact_id"),
                fact.text("payload_digest"),
            ).success(),
            "erasure-v1",
            root.objects("erasure_targets").map { value ->
                MemoryErasureTarget.create(
                    value.text("target_id"), targetKind(value.text("kind")),
                ).success()
            },
        ).success()
    }
}

private fun promotionPolicy(value: JsonObject): MemoryPromotionPolicy =
    MemoryPromotionPolicy.create(
        value.text("revision"), value.strings("allowed_types").map(::memoryType),
        value.ulong("min_verified"), value.ulong("max_falsified"), value.ulong("min_helpful_uses"),
    ).success()

private fun lifecycle(value: JsonObject): MemoryLifecycle = MemoryLifecycle.create(
    HypothesisState.entries.first { it.wireName == value.text("state") },
    EvidenceTally(value.ulong("verified"), value.ulong("falsified"), value.ulong("neutral")),
    value.ulong("last_position"), null,
).success()

private fun memoryType(value: String): MemoryType = MemoryType.entries.first { it.wireName == value }
private fun targetKind(value: String): ErasureTargetKind = when (value) {
    "primary_store" -> ErasureTargetKind.PRIMARY_STORE
    "projection" -> ErasureTargetKind.PROJECTION
    "cache" -> ErasureTargetKind.CACHE
    "backup" -> ErasureTargetKind.BACKUP
    else -> error("unknown target kind")
}
private fun status(value: String): ErasureTargetStatus = when (value) {
    "erased" -> ErasureTargetStatus.ERASED
    "not_present" -> ErasureTargetStatus.NOT_PRESENT
    "pending_backup_retention" -> ErasureTargetStatus.PENDING_BACKUP_RETENTION
    "pending_retry" -> ErasureTargetStatus.PENDING_RETRY
    else -> error("unknown target status")
}

private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
private fun JsonObject.optional(key: String): String? = get(key)?.jsonPrimitive?.contentOrNull
private fun JsonObject.objects(key: String): List<JsonObject> = getValue(key).jsonArray.map(JsonElement::jsonObject)
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
