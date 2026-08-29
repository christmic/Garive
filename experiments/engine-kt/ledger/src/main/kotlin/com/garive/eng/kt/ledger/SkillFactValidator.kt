package com.garive.eng.kt.ledger

import kotlinx.serialization.json.booleanOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonPrimitive

internal fun validateSkillFact(kind: String, value: kotlinx.serialization.json.JsonObject) {
    require(kind == "skill.activated")
    value.exact(setOf("activation_id", "request_digest", "mode", "through_position", "skills", "truncated"))
    value.nonEmpty("activation_id")
    value.digest("request_digest")
    val mode = value.enum("mode", setOf("explicit", "tagged"))
    value.ulong("through_position")
    val skills = value.getValue("skills").jsonArray
    skills.forEach { element ->
        val skill = element.asObject()
        skill.exact(setOf("skill_id", "skill_revision", "definition_digest", "instruction_digest", "reason"))
        skill.nonEmpty("skill_id")
        skill.nonEmpty("skill_revision")
        skill.digest("definition_digest")
        skill.digest("instruction_digest")
        val reason = skill.enum("reason", setOf("explicit", "tag_match"))
        require((mode == "explicit") == (reason == "explicit"))
    }
    val truncated = value.getValue("truncated").jsonPrimitive.booleanOrNull
        ?: throw IllegalArgumentException()
    require(mode != "explicit" || skills.size == 1 && !truncated)
}
