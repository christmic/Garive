package com.garive.mobile.host

import com.garive.host.v1.FakeHostScenarioV1
import com.garive.host.v1.HostEventV1

enum class HostTerminalKind { COMPLETED, SUSPENDED, STOPPED, FAILED }
enum class HostClientError {
    COMMAND_MISMATCH, API_VERSION_MISMATCH, EMPTY_EVENTS, POSITION_GAP,
    IDENTITY_MISMATCH, DELTA_AFTER_TERMINAL, MISSING_TERMINAL, MULTIPLE_TERMINALS,
}

data class HostRunResult(
    val sessionId: String,
    val turnId: String,
    val executionId: String,
    val text: String,
    val terminal: HostTerminalKind,
    val lastPosition: ULong,
)

sealed interface HostClientResult {
    data class Success(val value: HostRunResult) : HostClientResult
    data class Failure(val error: HostClientError) : HostClientResult
}

class FakeHostClient(private val scenario: FakeHostScenarioV1) {
    fun run(agentDefinitionId: String, text: String): HostClientResult {
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
