package com.garive.eng.kt.memory

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

public class MemoryRecallQualityFixtureTest {
    private val root: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/memory-recall-quality-v1.json").readText(),
    ).jsonObject

    @Test
    public fun pinnedRecallQualityIsExactAndReplayable(): Unit {
        assertEquals("synthetic-semantic-v1", root.text("dataset_revision"))
        val cases = root.getValue("cases").jsonArray.map { case(it.jsonObject) }
        val summary = (evaluateRecallQuality(cases) as MemoryContractResult.Success).value
        val expected = root.getValue("expected_summary").jsonObject
        assertEquals(expected.ulong("cases"), summary.cases)
        assertEquals(ratio(expected, "recall"), summary.recall)
        assertEquals(ratio(expected, "precision"), summary.precision)
        assertEquals(expected.ulong("forbidden_admissions"), summary.forbiddenAdmissions)
        assertEquals(expected.ulong("replay_mismatches"), summary.replayMismatches)

        val invalid = cases.first().copy(selected = cases.first().selected + cases.first().selected.first())
        assertEquals(
            MemoryErrorCode.INVALID_MEMORY,
            (evaluateRecallQuality(listOf(invalid)) as MemoryContractResult.Failure).error.code,
        )
    }

    private fun case(value: JsonObject): RecallQualityCase = RecallQualityCase(
        value.text("case_id"), value.identities("expected"), value.identities("forbidden"),
        value.identities("selected"), value.identities("replay"),
    )
}

private fun JsonObject.identities(name: String): List<RecallQualityIdentity> =
    getValue(name).jsonArray.map { value ->
        val (record, revision) = value.jsonPrimitive.content.split(':', limit = 2)
        RecallQualityIdentity(record, revision)
    }
private fun JsonObject.text(name: String): String = getValue(name).jsonPrimitive.content
private fun JsonObject.ulong(name: String): ULong = getValue(name).jsonPrimitive.content.toULong()
private fun ratio(value: JsonObject, prefix: String): RecallQualityRatio =
    RecallQualityRatio(value.ulong("${prefix}_numerator"), value.ulong("${prefix}_denominator"))
