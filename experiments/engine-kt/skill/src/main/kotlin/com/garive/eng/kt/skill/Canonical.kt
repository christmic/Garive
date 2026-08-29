package com.garive.eng.kt.skill

import java.security.MessageDigest
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

internal fun sha256(value: ByteArray): String = MessageDigest.getInstance("SHA-256")
    .digest(value)
    .joinToString(separator = "") { byte -> "%02x".format(byte.toInt() and 0xff) }

internal fun canonicalDigest(value: JsonObject): SkillContractResult<String> = runCatching {
    JsonCanonicalizer(value.toString()).encodedUTF8
}.fold(
    onSuccess = { SkillContractResult.Success(sha256(it)) },
    onFailure = { failure(SkillErrorCode.INVALID_SKILL) },
)

internal fun definitionDigest(value: SkillDefinition): SkillContractResult<String> = canonicalDigest(
    JsonObject(
        mapOf(
            "contract" to JsonPrimitive(DEFINITION_CONTRACT),
            "version" to JsonPrimitive(CONTRACT_VERSION),
            "definition" to definitionJson(value),
        ),
    ),
)

internal fun definitionJson(value: SkillDefinition): JsonObject = JsonObject(
    mapOf(
        "skill_id" to JsonPrimitive(value.skillId),
        "skill_revision" to JsonPrimitive(value.skillRevision),
        "name" to JsonPrimitive(value.name),
        "description" to JsonPrimitive(value.description),
        "instructions" to JsonObject(
            mapOf(
                "digest" to JsonPrimitive(value.instructions.digest),
                "inline_utf8" to JsonPrimitive(value.instructions.inlineUtf8),
            ),
        ),
        "activation" to activationJson(value.activation),
        "required_capabilities" to JsonArray(value.requiredCapabilities.map(::capabilityJson)),
        "allowed_tool_references" to JsonArray(value.allowedToolReferences.map(::toolJson)),
        "max_instruction_bytes" to JsonPrimitive(value.maxInstructionBytes),
        "contract_version" to JsonPrimitive(value.contractVersion),
    ),
)

private fun activationJson(value: ActivationPolicy): JsonObject = when (value) {
    ActivationPolicy.ExplicitOnly -> JsonObject(mapOf("kind" to JsonPrimitive("explicit_only")))
    is ActivationPolicy.Tagged -> JsonObject(
        mapOf(
            "kind" to JsonPrimitive("tagged"),
            "tags" to JsonArray(value.tags.map(::JsonPrimitive)),
        ),
    )
}

private fun capabilityJson(value: CapabilityReference): JsonObject = JsonObject(
    mapOf(
        "kind" to JsonPrimitive(value.kind),
        "name" to JsonPrimitive(value.name),
        "exact_revision" to JsonPrimitive(value.exactRevision),
        "contract_version" to JsonPrimitive(value.contractVersion),
    ),
)

private fun toolJson(value: ExactToolReference): JsonObject = JsonObject(
    mapOf(
        "name" to JsonPrimitive(value.name),
        "exact_revision" to JsonPrimitive(value.exactRevision),
    ),
)
