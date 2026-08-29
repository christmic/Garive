package com.garive.eng.kt.tools

import java.security.MessageDigest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

/** Frozen exact-name catalog for one Kernel Execution. */
public class ToolCatalog private constructor(definitions: Map<String, ToolDefinition>) {
    private val definitions: Map<String, ToolDefinition> = definitions.toMap()

    /** Validates one untrusted intent and returns an immutable authority-free call. */
    public fun prepare(intent: ToolIntent): ToolContractResult<PreparedToolCall> {
        if (intent.modelCallId.isEmpty()) return failure(PreparationErrorCode.INVALID_MODEL_CALL_ID)
        if (intent.toolName.isEmpty()) return failure(PreparationErrorCode.INVALID_TOOL_NAME)
        val definition = definitions[intent.toolName] ?: return failure(PreparationErrorCode.TOOL_NOT_ADMITTED)
        val normalized = canonicalize(intent.argumentsJson)
            ?: return failure(PreparationErrorCode.INVALID_ARGUMENTS_JSON)
        val arguments = runCatching { Json.parseToJsonElement(normalized) }.getOrNull()
            ?: return failure(PreparationErrorCode.INVALID_ARGUMENTS_JSON)
        val failures = PortableSchema.validateArguments(definition.inputSchema, arguments)
        if (failures.isNotEmpty()) {
            return ToolContractResult.Failure(
                PreparationError(PreparationErrorCode.ARGUMENTS_SCHEMA_MISMATCH, failures),
            )
        }
        val requirements = JsonObject(
            mapOf(
                "capabilities" to JsonArray(definition.requirements.capabilities.map { JsonPrimitive(it.wireName) }),
                "max_duration_ms" to JsonPrimitive(definition.requirements.maxDurationMs),
                "max_output_bytes" to JsonPrimitive(definition.requirements.maxOutputBytes),
            ),
        )
        val preimage = JsonObject(
            mapOf(
                "contract" to JsonPrimitive("garive.prepared-tool-call"),
                "version" to JsonPrimitive(1),
                "tool_name" to JsonPrimitive(definition.name),
                "tool_revision" to JsonPrimitive(definition.revision),
                "arguments" to arguments,
                "requirements" to requirements,
                "replay_class" to JsonPrimitive(definition.replayClass.wireName),
            ),
        )
        val canonicalPreimage = canonicalize(preimage.toString())
            ?: return failure(PreparationErrorCode.NON_CANONICAL_VALUE)
        return ToolContractResult.Success(
            PreparedToolCall(
                modelCallId = intent.modelCallId,
                toolName = definition.name,
                toolRevision = definition.revision,
                normalizedArguments = normalized,
                inputDigest = sha256(canonicalPreimage.encodeToByteArray()),
                requirements = definition.requirements,
                replayClass = definition.replayClass,
            ),
        )
    }

    public companion object {
        /** Rejects duplicate names and constructs an immutable catalog. */
        public fun create(definitions: List<ToolDefinition>): ToolContractResult<ToolCatalog> {
            val byName = definitions.associateBy(ToolDefinition::name)
            if (byName.size != definitions.size) return failure(PreparationErrorCode.INVALID_TOOL_DEFINITION)
            return ToolContractResult.Success(ToolCatalog(byName))
        }
    }
}

private fun canonicalize(value: String): String? = runCatching {
    JsonCanonicalizer(value).encodedString
}.getOrNull()

private fun sha256(value: ByteArray): String = MessageDigest.getInstance("SHA-256")
    .digest(value)
    .joinToString(separator = "") { byte -> "%02x".format(byte.toInt() and 0xff) }
