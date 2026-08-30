package com.garive.eng.kt.memory

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals

public class MemoryFeedbackQualityFixtureTest {
    private val root: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/memory-recall-feedback-v1.json").readText(),
    ).jsonObject

    @Test
    public fun sharedFeedbackQualityIsExactAndFailClosed(): Unit {
        val request = request()
        val actual = evaluateRecallFeedbackQuality(request).success()
        val expected = root.getValue("expected").jsonObject
        assertEquals(expected.number("exposures"), actual.exposures)
        assertEquals(expected.number("applications"), actual.applications)
        assertEquals(expected.number("censored"), actual.censored)
        assertEquals(expected.number("pending"), actual.pending)
        assertEquals(expected.number("verified"), actual.verified)
        assertEquals(expected.number("falsified"), actual.falsified)
        assertEquals(expected.number("neutral"), actual.neutral)
        assertEquals(expected.ratio("application_ratio"), actual.applicationRatio)
        assertEquals(expected.ratio("verified_outcome_ratio"), actual.verifiedOutcomeRatio)

        val invalid = request.copy(rows = request.rows.toMutableList().also {
            it[0] = it[0].copy(outcome = RecallFeedbackOutcome.VERIFIED)
        })
        assertEquals(MemoryErrorCode.INVALID_MEMORY, evaluateRecallFeedbackQuality(invalid).failure().code)
        assertEquals(null, evaluateRecallFeedbackQuality(request.copy(rows = emptyList())).success().applicationRatio)
    }

    private fun request(): RecallFeedbackQualityRequest = RecallFeedbackQualityRequest(
        root.text("policy_revision"), root.text("candidate_port_revision"),
        root.text("attribution_policy_revision"), root.text("verifier_revision"), root.text("corpus_digest"),
        root.getValue("rows").jsonArray.map { element ->
            val row = element.jsonObject
            RecallFeedbackRow(
                row.text("exposure_id"), row.text("selection_id"), row.text("record_id"), row.text("revision_id"),
                row.getValue("applied").jsonPrimitive.boolean,
                row["outcome"]?.jsonPrimitive?.content?.let { RecallFeedbackOutcome.valueOf(it.uppercase()) },
            )
        },
    )
}

private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
private fun JsonObject.number(key: String): ULong = getValue(key).jsonPrimitive.content.toULong()
private fun JsonObject.ratio(key: String): RecallQualityRatio {
    val values = getValue(key).jsonArray
    return RecallQualityRatio(values[0].jsonPrimitive.content.toULong(), values[1].jsonPrimitive.content.toULong())
}
private fun <T> MemoryContractResult<T>.success(): T = (this as MemoryContractResult.Success<T>).value
private fun MemoryContractResult<*>.failure(): MemoryError = (this as MemoryContractResult.Failure).error
