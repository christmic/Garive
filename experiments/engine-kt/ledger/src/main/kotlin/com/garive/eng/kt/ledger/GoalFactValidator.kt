package com.garive.eng.kt.ledger

import kotlinx.serialization.json.JsonObject

internal fun validateGoalFact(kind: String, value: JsonObject) {
    when (kind) {
        "goal.created" -> value.created()
        "goal.revised" -> value.revised()
        "goal.activated" -> value.activated()
        "goal.suspended" -> value.suspended()
        "goal.succeeded" -> value.succeeded()
        "goal.failed" -> value.failed()
        "goal.cancelled" -> value.cancelled()
        else -> throw IllegalArgumentException()
    }
}

private fun JsonObject.created() {
    exact(setOf("command_id", "goal_id", "revision", "definition_digest", "definition", "actor_reference"))
    common()
    require(ulong("revision") == 1uL)
    digest("definition_digest")
    content("definition")
    nonEmpty("actor_reference")
}

private fun JsonObject.revised() {
    exact(setOf("command_id", "goal_id", "previous_revision", "revision", "previous_definition_digest", "definition_digest", "definition", "actor_reference"))
    common()
    val previous = ulong("previous_revision", true)
    require(previous != ULong.MAX_VALUE && previous + 1uL == ulong("revision"))
    digest("previous_definition_digest")
    digest("definition_digest")
    content("definition")
    nonEmpty("actor_reference")
}

private fun JsonObject.activated() {
    exact(setOf("command_id", "goal_id", "revision", "attempt_number"), setOf("plan_reference"))
    common()
    ulong("attempt_number", true)
    optionalNonEmpty("plan_reference")
}

private fun JsonObject.suspended() {
    exact(setOf("command_id", "goal_id", "revision", "reason"), setOf("suspension_reference"))
    common()
    nonEmpty("reason")
    optionalNonEmpty("suspension_reference")
}

private fun JsonObject.succeeded() {
    exact(setOf("command_id", "goal_id", "revision", "evidence"))
    common()
    content("evidence")
}

private fun JsonObject.failed() {
    exact(setOf("command_id", "goal_id", "revision", "code"), setOf("evidence"))
    common()
    nonEmpty("code")
    optionalContent("evidence")
}

private fun JsonObject.cancelled() {
    exact(setOf("command_id", "goal_id", "revision", "reason", "actor_reference"))
    common()
    nonEmpty("reason")
    nonEmpty("actor_reference")
}

private fun JsonObject.common() {
    nonEmpty("command_id")
    nonEmpty("goal_id")
    ulong("revision", true)
}
