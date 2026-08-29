package com.garive.eng.kt.host

import com.garive.host.v1.Host

/** Deterministic generated-Proto Host fixture used by JVM shell conformance. */
public object EmbeddedServerHost {
    /** Runs the sole admitted fake command and returns its ordered Host v1 events. */
    public fun runFixture(input: String): Host.FakeHostScenarioV1 {
        require(input == "hello") { "fixture host accepts only hello" }
        val scenario = Host.FakeHostScenarioV1.newBuilder()
            .setApiVersion("garive.host.v1")
            .setCommand(Host.FakeHostCommandV1.newBuilder()
                .setAgentDefinitionId("garive.default").setText(input))
        listOf(
            event(1, "session.created"),
            event(2, "turn.started", turn = true),
            event(3, "output.delta", "hello ", turn = true),
            event(4, "output.delta", "from Garive", turn = true),
            event(5, "turn.completed", turn = true),
        ).forEach(scenario::addEvents)
        return scenario.build()
    }

    private fun event(position: Long, kind: String, text: String = "", turn: Boolean = false) =
        Host.HostEventV1.newBuilder()
            .setApiVersion("garive.host.v1")
            .setSessionId("session-fixture")
            .setPosition(position)
            .setEvent(kind)
            .setTurnId(if (turn) "turn-fixture" else "")
            .setExecutionId(if (turn) "execution-fixture" else "")
            .setText(text)
            .build()
}

/** Prints the deterministic fake Host response for local smoke execution. */
public fun main(): Unit {
    EmbeddedServerHost.runFixture("hello").eventsList
        .filter { it.event == "output.delta" }
        .forEach { print(it.text) }
    println()
}
