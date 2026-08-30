package com.garive.eng.kt.host

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class HostProjectionFixtureTest {
    private fun fixture(name: String): JsonObject = Json.parseToJsonElement(
        File(
            System.getProperty("garive.repo.root"),
            "spec/fixtures/host/$name",
        ).readText(),
    ).jsonObject

    private fun assertFields(value: JsonObject, vararg expected: String) {
        assertEquals(expected.toSet(), value.keys)
    }

    private fun validateCases(
        root: JsonObject,
        section: String,
        names: MutableSet<String>,
        vararg fields: String,
    ) {
        root.getValue(section).jsonArray.forEach { element ->
            val case = element.jsonObject
            assertFields(case, *fields)
            val name = case.getValue("name").jsonPrimitive.content
            assertTrue(name.isNotEmpty())
            assertTrue(names.add(name), "duplicate fixture case: $name")
        }
    }

    @Test
    fun `H2 fixture schema and names remain exact`() {
        val root = fixture("host-read-model-v1.json")
        assertFields(
            root,
            "schema_version",
            "contract",
            "definition_cases",
            "session_page_cases",
            "session_view_cases",
            "timeline_cases",
            "cursor_cases",
            "failure_cases",
        )
        assertEquals("1", root.getValue("schema_version").jsonPrimitive.content)
        assertEquals("host-read-model-v1", root.getValue("contract").jsonPrimitive.content)
        val names = mutableSetOf<String>()
        validateCases(root, "definition_cases", names, "name", "limit", "expected_ids", "error")
        validateCases(
            root,
            "session_page_cases",
            names,
            "name",
            "limit",
            "before",
            "opened",
            "expected_ids",
            "has_next",
            "error",
        )
        validateCases(
            root,
            "session_view_cases",
            names,
            "name",
            "prefix",
            "expected_state",
            "expected_turn_count",
            "error",
        )
        validateCases(
            root,
            "timeline_cases",
            names,
            "name",
            "after_position",
            "limit",
            "prefix",
            "expected_states",
            "truncated",
            "error",
        )
        validateCases(root, "cursor_cases", names, "name", "scenario", "error")
        validateCases(root, "failure_cases", names, "name", "status", "code")
        assertEquals(
            setOf(
                "invalid_request",
                "not_found",
                "read_bound_exceeded",
                "durability_unavailable",
                "corrupt_state",
            ),
            root.getValue("failure_cases").jsonArray
                .map { it.jsonObject.getValue("code").jsonPrimitive.content }
                .toSet(),
        )
    }

    @Test
    fun `H3 fixture schema mappings and names remain exact`() {
        val root = fixture("host-agent-activity-v1.json")
        assertFields(
            root,
            "schema_version",
            "contract",
            "projection_cases",
            "timeline_cases",
            "reducer_cases",
            "bound_cases",
            "redaction_cases",
        )
        assertEquals("1", root.getValue("schema_version").jsonPrimitive.content)
        assertEquals("host-agent-activity-v1", root.getValue("contract").jsonPrimitive.content)
        val names = mutableSetOf<String>()
        validateCases(
            root,
            "projection_cases",
            names,
            "name",
            "fact",
            "event",
            "state",
            "terminal",
            "safe_code",
        )
        validateCases(root, "timeline_cases", names, "name", "facts", "expected_states", "error")
        validateCases(root, "reducer_cases", names, "name", "from", "fact", "to", "valid")
        validateCases(root, "bound_cases", names, "name", "bound", "error")
        validateCases(root, "redaction_cases", names, "name", "canary", "must_be_absent")
        assertEquals(
            setOf(
                "tool.preparation_rejected",
                "effect.prepared",
                "interaction.requested",
                "interaction.resolved",
                "interaction.cancelled",
                "effect.authorized",
                "effect.denied",
                "effect.started",
                "effect.receipt",
                "effect.completed",
                "effect.failed",
                "effect.uncertain",
                "effect.reconciled",
                "effect.observation",
            ),
            root.getValue("projection_cases").jsonArray
                .map { it.jsonObject.getValue("fact").jsonPrimitive.content }
                .toSet(),
        )
    }
}
