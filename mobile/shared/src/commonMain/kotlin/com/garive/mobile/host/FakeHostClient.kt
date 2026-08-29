package com.garive.mobile.host

import com.garive.host.v1.FakeHostCommandV1
import com.garive.host.v1.FakeHostScenarioV1
import com.garive.host.v1.HostEventV1

/** Terminal classes admitted by the Host v1 mobile client. */
public enum class HostTerminalKind { COMPLETED, SUSPENDED, STOPPED, FAILED }
/** Stable validation failure while reducing a Host v1 event stream. */
public enum class HostClientError {
    COMMAND_MISMATCH, API_VERSION_MISMATCH, EMPTY_EVENTS, POSITION_GAP,
    IDENTITY_MISMATCH, DELTA_AFTER_TERMINAL, MISSING_TERMINAL, MULTIPLE_TERMINALS,
}

/** Verified identities, output, terminal, and watermark from one Host run. */
public data class HostRunResult(
    public val sessionId: String,
    public val turnId: String,
    public val executionId: String,
    public val text: String,
    public val terminal: HostTerminalKind,
    public val lastPosition: ULong,
)

/** Success/failure envelope for [FakeHostClient]. */
public sealed interface HostClientResult {
    public data class Success(public val value: HostRunResult) : HostClientResult
    public data class Failure(public val error: HostClientError) : HostClientResult
}

/** Strict reducer for one generated-Proto fake Host scenario. */
public class FakeHostClient(private val scenario: FakeHostScenarioV1) {
    /** Validates the command and reduces its ordered event stream. */
    public fun run(agentDefinitionId: String, text: String): HostClientResult {
        val command = scenario.command ?: return HostClientResult.Failure(HostClientError.COMMAND_MISMATCH)
        if (command.agent_definition_id != agentDefinitionId || command.text != text) {
            return HostClientResult.Failure(HostClientError.COMMAND_MISMATCH)
        }
        return reduce(scenario.api_version, scenario.events)
    }

    private fun reduce(apiVersion: String, events: List<HostEventV1>): HostClientResult {
        if (events.isEmpty()) return HostClientResult.Failure(HostClientError.EMPTY_EVENTS)
        val sessionId = events.first().session_id
        var turnId = ""
        var executionId = ""
        var output = ""
        var terminal: HostTerminalKind? = null
        events.forEachIndexed { index, event ->
            if (event.api_version != apiVersion) return HostClientResult.Failure(HostClientError.API_VERSION_MISMATCH)
            if (event.position != (index + 1).toLong()) return HostClientResult.Failure(HostClientError.POSITION_GAP)
            if (event.session_id != sessionId) return HostClientResult.Failure(HostClientError.IDENTITY_MISMATCH)
            if (event.turn_id.isNotEmpty()) {
                if (turnId.isNotEmpty() && event.turn_id != turnId) return HostClientResult.Failure(HostClientError.IDENTITY_MISMATCH)
                turnId = event.turn_id
            }
            if (event.execution_id.isNotEmpty()) {
                if (executionId.isNotEmpty() && event.execution_id != executionId) return HostClientResult.Failure(HostClientError.IDENTITY_MISMATCH)
                executionId = event.execution_id
            }
            if (event.event == "output.delta") {
                if (terminal != null) return HostClientResult.Failure(HostClientError.DELTA_AFTER_TERMINAL)
                output += event.text
            }
            terminalKind(event.event)?.let {
                if (terminal != null) return HostClientResult.Failure(HostClientError.MULTIPLE_TERMINALS)
                terminal = it
            }
        }
        val final = terminal ?: return HostClientResult.Failure(HostClientError.MISSING_TERMINAL)
        return HostClientResult.Success(HostRunResult(
            sessionId, turnId, executionId, output, final, events.last().position.toULong()
        ))
    }

    private fun terminalKind(event: String): HostTerminalKind? = when (event) {
        "turn.completed" -> HostTerminalKind.COMPLETED
        "turn.suspended" -> HostTerminalKind.SUSPENDED
        "turn.stopped" -> HostTerminalKind.STOPPED
        "turn.failed" -> HostTerminalKind.FAILED
        else -> null
    }
}

/** Deterministic fake Host shared by Android, iOS, and JVM tests. */
public object EmbeddedFakeHost {
    /** Runs the sole admitted default fixture command. */
    public fun runDefault(): HostClientResult = FakeHostClient(
        FakeHostScenarioV1(
            api_version = "garive.host.v1",
            command = FakeHostCommandV1("garive.default", "hello"),
            events = listOf(
                event(1, "session.created"),
                event(2, "turn.started", turn = true),
                event(3, "output.delta", "hello ", turn = true),
                event(4, "output.delta", "from Garive", turn = true),
                event(5, "turn.completed", turn = true),
            ),
        )
    ).run("garive.default", "hello")

    private fun event(position: Long, kind: String, text: String = "", turn: Boolean = false) =
        HostEventV1(
            api_version = "garive.host.v1",
            session_id = "session-fixture",
            position = position,
            event = kind,
            turn_id = if (turn) "turn-fixture" else "",
            execution_id = if (turn) "execution-fixture" else "",
            text = text,
        )
}

/** Swift-friendly bridge that renders the default fixture to plain text. */
public object EmbeddedFakeHostBridge {
    /** Returns output text or a stable failure string. */
    public fun runText(): String = when (val result = EmbeddedFakeHost.runDefault()) {
        is HostClientResult.Success -> result.value.text
        is HostClientResult.Failure -> "failed:${result.error.name.lowercase()}"
    }
}
