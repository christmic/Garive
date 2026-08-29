package com.garive.eng.kt.skill

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs

public class SkillActivationFixtureTest {
    private val root: JsonObject = Json.parseToJsonElement(
        File(
            System.getProperty("garive.repo.root"),
            "spec/fixtures/agent/skill-activation-v1.json",
        ).readText(),
    ).jsonObject

    @Test
    public fun sharedVectorsCoverDigestsOrderBoundsAndFailures(): Unit {
        val definitions = root.array("definitions").map(::definition)
        root.array("definitions").zip(definitions).forEach { (source, parsed) ->
            assertEquals(source.jsonObject.string("expected_definition_digest"), parsed.definitionDigest().success())
        }
        val capabilities = root.array("available_capabilities").map(::capability).toSet()
        val tools = root.array("available_tools").map(::tool).toSet()

        root.array("cases").forEach { element ->
            val case = element.jsonObject
            val request = request(case.objectValue("request"))
            case["expected_request_digest"]?.jsonPrimitive?.contentOrNull?.let { expected ->
                assertEquals(expected, request.requestDigest().success(), case.string("name"))
            }
            val expected = case.objectValue("expected")
            when (expected.string("status")) {
                "none" -> assertEquals(
                    SkillActivationResult.None,
                    activateSkills(definitions, capabilities, tools, request).success(),
                    case.string("name"),
                )
                "activated" -> {
                    val actual = assertIs<SkillActivationResult.Activated>(
                        activateSkills(definitions, capabilities, tools, request).success(),
                    )
                    assertEquals(
                        expected.array("skill_ids").map { it.jsonPrimitive.content },
                        actual.orderedSkills.map(ActivatedSkill::skillId),
                        case.string("name"),
                    )
                    assertEquals(expected["truncated"]!!.jsonPrimitive.boolean, actual.truncated, case.string("name"))
                }
                "error" -> assertEquals(
                    expected.string("code"),
                    activateSkills(definitions, capabilities, tools, request).failure().code.wireName,
                    case.string("name"),
                )
                else -> error("unknown expected status")
            }
        }
    }

    @Test
    public fun rejectsUnsupportedOrSnapshotWideningInputs(): Unit {
        assertEquals(
            SkillErrorCode.ACTIVATION_MODE_UNSUPPORTED,
            ActivationMode.fromWire("semantic").failure().code,
        )
        assertEquals(
            SkillErrorCode.INSTRUCTION_DIGEST_MISMATCH,
            ContentBinding.create("0".repeat(64), "wrong").failure().code,
        )
        val code = definition(root.array("definitions")[1])
        val tagged = request(root.array("cases")[1].jsonObject.objectValue("request"))
        assertEquals(
            SkillErrorCode.REQUIRED_CAPABILITY_UNAVAILABLE,
            activateSkills(listOf(code), emptySet(), emptySet(), tagged).failure().code,
        )
        val capabilities = setOf(capability(root.array("available_capabilities")[0]))
        assertEquals(
            SkillErrorCode.SKILL_NOT_ENABLED,
            activateSkills(listOf(code), capabilities, emptySet(), tagged).failure().code,
        )
        val conflict = SkillDefinition.create(
            "code", "2", "Other", "Conflicting definition.", ContentBinding.fromInline("Other."),
            ActivationPolicy.Tagged.create(listOf("code", "rust")).success(), emptyList(), emptyList(), 64, "1",
        ).success()
        assertEquals(
            SkillErrorCode.ACTIVATION_CONFLICT,
            activateSkills(listOf(code, conflict), capabilities, emptySet(), tagged).failure().code,
        )
    }

    @Test
    public fun activationIdentityIsNotRequestSemantics(): Unit {
        val source = root.array("cases")[0].jsonObject.objectValue("request")
        val first = request(source)
        val changed = JsonObject(source + ("activation_id" to JsonPrimitive("activation-other")))
        assertEquals(first.requestDigest().success(), request(changed).requestDigest().success())
    }

    private fun definition(element: JsonElement): SkillDefinition {
        val value = element.jsonObject
        val activation = when (value.objectValue("activation").string("kind")) {
            "explicit_only" -> ActivationPolicy.ExplicitOnly
            "tagged" -> ActivationPolicy.Tagged.create(
                value.objectValue("activation").array("tags").map { it.jsonPrimitive.content },
            ).success()
            else -> error("unknown activation policy")
        }
        return SkillDefinition.create(
            value.string("skill_id"), value.string("skill_revision"), value.string("name"),
            value.string("description"), ContentBinding.create(
                value.objectValue("instructions").string("digest"),
                value.objectValue("instructions").string("inline_utf8"),
            ).success(),
            activation,
            value.array("required_capabilities").map(::capability),
            value.array("allowed_tool_references").map(::tool),
            value["max_instruction_bytes"]!!.jsonPrimitive.long,
            value.string("contract_version"),
        ).success()
    }

    private fun request(value: JsonObject): SkillActivationRequest = SkillActivationRequest.create(
        value.string("activation_id"), value.string("turn_id"), value.string("execution_id"),
        value["iteration"]!!.jsonPrimitive.long,
        ActivationMode.fromWire(value.string("mode")).success(),
        value["requested_skill_id"]?.jsonPrimitive?.contentOrNull,
        value.array("trusted_tags").map { it.jsonPrimitive.content },
        value["through_position"]!!.jsonPrimitive.long,
        value["max_active_skills"]!!.jsonPrimitive.long,
        value["max_total_instruction_bytes"]!!.jsonPrimitive.long,
    ).success()

    private fun capability(element: JsonElement): CapabilityReference {
        val value = element.jsonObject
        return CapabilityReference.create(
            value.string("kind"), value.string("name"), value.string("exact_revision"),
            value.string("contract_version"),
        ).success()
    }

    private fun tool(element: JsonElement): ExactToolReference {
        val value = element.jsonObject
        return ExactToolReference.create(value.string("name"), value.string("exact_revision")).success()
    }
}

private fun JsonObject.string(name: String): String = getValue(name).jsonPrimitive.content
private fun JsonObject.array(name: String): JsonArray = getValue(name).jsonArray
private fun JsonObject.objectValue(name: String): JsonObject = getValue(name).jsonObject

private fun <T> SkillContractResult<T>.success(): T = when (this) {
    is SkillContractResult.Success -> value
    is SkillContractResult.Failure -> error("unexpected failure: $error")
}

private fun SkillContractResult<*>.failure(): SkillError = when (this) {
    is SkillContractResult.Success -> error("unexpected success: $value")
    is SkillContractResult.Failure -> error
}
