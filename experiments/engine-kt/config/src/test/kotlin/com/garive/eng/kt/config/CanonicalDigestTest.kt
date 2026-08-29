package com.garive.eng.kt.config

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.io.path.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals

class CanonicalDigestTest {
    @Test
    fun sharedDigestRelationsHold() {
        val root = Path(System.getProperty("garive.repo.root"))
        val fixture = Json.parseToJsonElement(
            root.resolve("spec/fixtures/agent/agent-definition-snapshot.json").readText(),
        ).jsonObject
        fixture.getValue("digest_relations").jsonArray.forEach { element ->
            val case = element.jsonObject
            val left = Json.parseToJsonElement(case.getValue("left_json").jsonPrimitive.content)
            val right = Json.parseToJsonElement(case.getValue("right_json").jsonPrimitive.content)
            val equal = (digestCanonicalValue(left) as DefinitionResult.Success).value ==
                (digestCanonicalValue(right) as DefinitionResult.Success).value
            assertEquals(case.getValue("relation").jsonPrimitive.content == "equal", equal)
        }
    }
}
