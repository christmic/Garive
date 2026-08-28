package com.garive.mobile.host

import com.garive.host.v1.FakeHostCommandV1
import com.garive.host.v1.FakeHostScenarioV1
import com.garive.host.v1.HostEventV1
import java.nio.file.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.*

class FakeHostClientTest {
    @Test fun fixtureUsesGeneratedWireTypesAndReducesOneTerminal() {
        val root = Path.of(System.getProperty("garive.repo.root"))
        val json = Json.parseToJsonElement(root.resolve("spec/fixtures/host/fake-session.json").readText()).jsonObject
        val command = json.getValue("command").jsonObject
        val scenario = FakeHostScenarioV1(
            api_version = json.text("api_version"),
            command = FakeHostCommandV1(command.text("agent_definition_id"), command.text("text")),
            events = json.getValue("events").jsonArray.map { it.jsonObject.toWire() },
        )
        val decoded = FakeHostScenarioV1.ADAPTER.decode(scenario.encode())
        val result = assertIs<HostClientResult.Success>(
            FakeHostClient(decoded).run("garive.default", "hello")
        ).value
        assertEquals("hello from Garive", result.text)
        assertEquals(HostTerminalKind.COMPLETED, result.terminal)
        assertEquals(5uL, result.lastPosition)
        assertEquals(result, assertIs<HostClientResult.Success>(EmbeddedFakeHost.runDefault()).value)
    }

    @Test fun positionGapFailsClosed() {
        val scenario = FakeHostScenarioV1(
            api_version = "garive.host.v1",
            command = FakeHostCommandV1("garive.default", "hello"),
            events = listOf(HostEventV1("garive.host.v1", "session", 2L, "turn.completed")),
        )
        assertEquals(HostClientResult.Failure(HostClientError.POSITION_GAP),
            FakeHostClient(scenario).run("garive.default", "hello"))
    }

    private fun JsonObject.toWire() = HostEventV1(
        api_version = text("api_version"), session_id = text("session_id"),
        position = text("position").toLong(), event = text("event"),
        turn_id = optional("turn_id"), execution_id = optional("execution_id"), text = optional("text"),
    )
    private fun JsonObject.text(key: String) = getValue(key).jsonPrimitive.content
    private fun JsonObject.optional(key: String) = get(key)?.jsonPrimitive?.content.orEmpty()
}
