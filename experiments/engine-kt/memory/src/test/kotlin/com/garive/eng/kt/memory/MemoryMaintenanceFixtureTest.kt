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

public class MemoryMaintenanceFixtureTest {
    private val root: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/memory-maintenance-v1.json").readText(),
    ).jsonObject

    @Test
    public fun sharedCandidatesReduceToExactFourWayDecisions(): Unit {
        root.objects("candidate_cases").forEach { case ->
            val result = when (val candidate = candidate(case)) {
                is MemoryContractResult.Failure -> candidate
                is MemoryContractResult.Success -> when (val assessment = assessment(case)) {
                    is MemoryContractResult.Failure -> assessment
                    is MemoryContractResult.Success ->
                        decideCandidate(candidate.value, assessment.value, "maintenance-decision")
                }
            }
            case.optional("failure")?.let { expected ->
                assertEquals(expected, result.failure().code.wireName, case.text("name"))
            } ?: assertEquals(
                case.text("expected"), decisionName(result.success()), case.text("name"),
            )
        }
    }

    @Test
    public fun sharedWatermarksAreMonotonicAndReplayable(): Unit {
        root.objects("watermark_cases").forEach { case ->
            val prior = case["prior"]?.jsonObject?.let(::watermark)
            val next = watermark(case.getValue("next").jsonObject)
            val result = advanceDistillation(prior, next)
            case.optional("failure")?.let { expected ->
                assertEquals(expected, result.failure().code.wireName, case.text("name"))
            } ?: assertEquals(
                case.text("expected"), result.success().name.lowercase(), case.text("name"),
            )
        }
    }

    @Test
    public fun sharedAuditIsBoundedDeterministicAndReadOnly(): Unit {
        val audit = root.getValue("audit").jsonObject
        val entries = audit.objects("entries").map(::auditEntry)
        val contradictions = audit.objects("contradictions").map(::contradiction)
        val policyJson = audit.getValue("policy").jsonObject
        val policy = MemoryAuditPolicy(
            policyJson.ulong("max_active_records").toUInt(),
            policyJson.ulong("max_active_bytes"),
            policyJson.ulong("stale_after_positions"),
            policyJson.ulong("low_use_threshold"),
            policyJson.ulong("max_report_items").toUInt(),
        )
        val report = auditMemory(
            entries, contradictions, audit.ulong("current_position"), policy,
        ).success()
        val expected = audit.getValue("expected").jsonObject
        assertEquals(
            expected.getValue("duplicate_groups").jsonArray.map { group ->
                group.jsonArray.map { it.jsonPrimitive.content }
            },
            report.duplicateGroups.map { group -> group.map(::identityName) },
        )
        assertEquals(expected.strings("stale"), report.stale.map(::identityName))
        assertEquals(expected.strings("low_use"), report.lowUse.map(::identityName))
        assertEquals(expected.strings("actions"), report.actions.map(::actionName))
        assertEquals(expected.getValue("truncated").jsonPrimitive.content.toBooleanStrict(), report.truncated)
        assertEquals(contradictions, report.contradictions)
        assertEquals(
            report,
            auditMemory(entries, contradictions, audit.ulong("current_position"), policy).success(),
        )
        assertEquals(
            MemoryErrorCode.LIMIT_EXCEEDED,
            auditMemory(entries, contradictions, audit.ulong("current_position"), policy.copy(maxReportItems = 3u))
                .failure().code,
        )
    }

    private fun candidate(case: JsonObject): MemoryContractResult<MemoryCandidate> {
        val authority = when (val value = MemoryAuthorityBinding.create(
            authority(case.text("authority")), case.optional("receipt_digest"),
        )) {
            is MemoryContractResult.Failure -> return value
            is MemoryContractResult.Success -> value.value
        }
        val intent = when (case.text("intent")) {
            "learn" -> MemoryCandidateIntent.Learn(
                MemoryType.LESSON, MemoryKind.LEARNED_FACT, authority,
                MemoryScopeBinding.create(MemoryScopeClass.USER, null).success(),
                ContentBinding.fromInline("memory"), 6uL,
                listOf(factReference(root.getValue("evidence").jsonObject)),
            )
            "forget" -> MemoryCandidateIntent.Forget(
                case.text("target_record_id"), case.text("target_revision_id"), authority,
            )
            else -> error("unknown intent")
        }
        return MemoryCandidate.create(
            case.text("name"), "namespace-maintenance", "extractor-v1",
            source(case.text("source")), intent,
        )
    }

    private fun assessment(case: JsonObject): MemoryContractResult<AdmissionAssessment?> {
        if (case.text("intent") == "forget") return MemoryContractResult.Success(null)
        return when (val result = AdmissionAssessment.create(
            case.boolean("generalizable"),
            CandidateStability.entries.first { it.name.lowercase() == case.text("stability") },
            case.optional("duplicate_revision_id"), case.optional("conflicting_revision_id"),
        )) {
            is MemoryContractResult.Failure -> result
            is MemoryContractResult.Success -> MemoryContractResult.Success(result.value)
        }
    }
}

private fun decisionName(value: MemoryMaintenanceDecision): String = when (value) {
    is MemoryMaintenanceDecision.Add -> "add"
    is MemoryMaintenanceDecision.Update -> "update:${value.expectedActiveRevisionId}"
    is MemoryMaintenanceDecision.Delete -> "delete:${value.recordId}:${value.revisionId}"
    is MemoryMaintenanceDecision.Noop -> when (value.code) {
        MaintenanceNoopCode.NOT_GENERALIZABLE -> "noop_not_generalizable"
        MaintenanceNoopCode.UNSTABLE_DEFERRED -> "noop_unstable_deferred"
        MaintenanceNoopCode.DUPLICATE -> "noop_duplicate"
    }
}

private fun watermark(value: JsonObject): DistillationWatermark = DistillationWatermark.create(
    value.text("extractor_revision"), value.text("session_id"), value.ulong("through_position"),
    value.text("batch_digest"),
).success()

private fun auditEntry(value: JsonObject): MemoryAuditEntry = MemoryAuditEntry(
    value.text("record_id"), value.text("revision_id"),
    MemoryType.entries.first { it.wireName == value.text("type") },
    HypothesisState.entries.first { it.wireName == value.text("state") },
    value.text("content_digest"), value.ulong("content_bytes"), value.ulong("use_count"),
    value.ulong("last_verified_position"), value.ulong("retention_score").toUInt(),
)

private fun contradiction(value: JsonObject): MemoryContradiction = MemoryContradiction(
    MemoryIdentity(value.text("left_record_id"), value.text("left_revision_id")),
    MemoryIdentity(value.text("right_record_id"), value.text("right_revision_id")),
)

private fun factReference(value: JsonObject): DurableFactReference = DurableFactReference.create(
    value.text("session_id"), value.ulong("position"), value.text("fact_id"),
    value.text("payload_digest"),
).success()

private fun actionName(value: MemoryAuditAction): String = when (value) {
    is MemoryAuditAction.Cool -> "cool:${identityName(value.identity)}"
    is MemoryAuditAction.Archive -> "archive:${identityName(value.identity)}"
}

private fun identityName(value: MemoryIdentity): String = "${value.recordId}:${value.revisionId}"

private fun source(value: String): MemoryCandidateSource = when (value) {
    "explicit_user_command" -> MemoryCandidateSource.EXPLICIT_USER_COMMAND
    "session_end" -> MemoryCandidateSource.SESSION_END
    "exit_summary" -> MemoryCandidateSource.EXIT_SUMMARY
    "scheduled_distillation" -> MemoryCandidateSource.SCHEDULED_DISTILLATION
    else -> error("unknown source")
}

private fun authority(value: String): MemoryAuthority =
    MemoryAuthority.entries.first { it.wireName == value }

private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
private fun JsonObject.optional(key: String): String? = get(key)?.jsonPrimitive?.contentOrNull
private fun JsonObject.objects(key: String): List<JsonObject> = getValue(key).jsonArray.map(JsonElement::jsonObject)
private fun JsonObject.strings(key: String): List<String> = getValue(key).jsonArray.map { it.jsonPrimitive.content }
private fun JsonObject.ulong(key: String): ULong = text(key).toULong()
private fun JsonObject.boolean(key: String): Boolean = getValue(key).jsonPrimitive.content.toBooleanStrict()
private fun <T> MemoryContractResult<T>.success(): T = when (this) {
    is MemoryContractResult.Success -> value
    is MemoryContractResult.Failure -> error("unexpected failure: $error")
}
private fun MemoryContractResult<*>.failure(): MemoryError = when (this) {
    is MemoryContractResult.Success -> error("unexpected success: $value")
    is MemoryContractResult.Failure -> error
}
