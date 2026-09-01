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
        val validated = validateIntent(intent)
        val (definition, arguments, normalized) = when (validated) {
            is ToolContractResult.Success -> validated.value
            is ToolContractResult.Failure -> return validated
        }
        if (definition.accessPolicy != null) return failure(PreparationErrorCode.EFFECT_ACCESS_INVALID)
        return preparedV1(intent, definition, arguments, normalized)
    }

    /** Prepares one v2 call using the exact frozen trusted resolver revision. */
    public fun prepareV2(
        intent: ToolIntent,
        resolver: ToolAccessResolver,
    ): ToolContractResult<PreparedToolCall> = prepareAccessible(intent, resolver, false)

    /** Prepares one v3 call bound to exact resources and F0 requirements. */
    public fun prepareV3(
        intent: ToolIntent,
        resolver: ToolAccessResolver,
    ): ToolContractResult<PreparedToolCall> = prepareAccessible(intent, resolver, true)

    private fun prepareAccessible(
        intent: ToolIntent,
        resolver: ToolAccessResolver,
        requireSandbox: Boolean,
    ): ToolContractResult<PreparedToolCall> {
        val validated = validateIntent(intent)
        val (definition, arguments, normalized) = when (validated) {
            is ToolContractResult.Success -> validated.value
            is ToolContractResult.Failure -> return validated
        }
        val policy = definition.accessPolicy ?: return failure(PreparationErrorCode.EFFECT_ACCESS_INVALID)
        val sandbox = definition.sandboxRequirements
        if (requireSandbox != (sandbox != null)) {
            return failure(PreparationErrorCode.SANDBOX_REQUIREMENT_INVALID)
        }
        if (resolver.revision != definition.accessResolverRevision) {
            return failure(PreparationErrorCode.EFFECT_ACCESS_INVALID)
        }
        val resolved = resolver.resolve(arguments)
        val accesses = when (resolved) {
            is ToolContractResult.Success -> resolved.value
            is ToolContractResult.Failure -> return resolved
        }
        val mutating = accesses.values.any { it.mode != AccessMode.READ }
        val requiresMutation = definition.requirements.capabilities.any {
            it in setOf(
                ExecutionCapability.FILESYSTEM_WRITE,
                ExecutionCapability.PROCESS,
                ExecutionCapability.BROWSER_ACT,
                ExecutionCapability.COMPUTER_ACT,
            )
        }
        if (!policy.covers(accesses) ||
            definition.replayClass == ReplayClass.READ_ONLY && mutating ||
            definition.replayClass != ReplayClass.READ_ONLY && requiresMutation && !mutating
        ) {
            return failure(PreparationErrorCode.EFFECT_ACCESS_INVALID)
        }
        val accessJson = JsonArray(accesses.values.map { access ->
            JsonObject(
                mapOf(
                    "namespace" to JsonPrimitive(access.namespace.wireName),
                    "resource_key" to JsonPrimitive(access.resourceKey),
                    "mode" to JsonPrimitive(access.mode.wireName),
                ),
            )
        })
        val sandboxDigest = when (val result = sandbox?.digest()) {
            is ToolContractResult.Success -> result.value
            is ToolContractResult.Failure -> return result
            null -> null
        }
        val version = if (sandbox == null) 2 else 3
        val additions = mutableMapOf<String, JsonElement>(
            "access_policy_revision" to JsonPrimitive(policy.policyRevision),
            "access_resolver_revision" to JsonPrimitive(resolver.revision),
            "invocation_accesses" to accessJson,
            "max_result_bytes" to JsonPrimitive(policy.maxResultBytes),
        )
        if (sandbox != null && sandboxDigest != null) {
            additions["sandbox_requirements"] = sandbox.canonicalJson()
            additions["sandbox_requirements_digest"] = JsonPrimitive(sandboxDigest)
        }
        val preimage = preparedPreimage(
            definition,
            arguments,
            version,
            additions,
        )
        return prepared(
            intent,
            definition,
            normalized,
            preimage,
            version,
            policy.policyRevision,
            resolver.revision,
            accesses,
            policy.maxResultBytes,
            sandbox,
            sandboxDigest,
        )
    }

    private fun validateIntent(intent: ToolIntent): ToolContractResult<ValidatedIntent> {
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
        return ToolContractResult.Success(ValidatedIntent(definition, arguments, normalized))
    }

    private fun preparedV1(
        intent: ToolIntent,
        definition: ToolDefinition,
        arguments: JsonElement,
        normalized: String,
    ): ToolContractResult<PreparedToolCall> = prepared(
        intent,
        definition,
        normalized,
        preparedPreimage(definition, arguments, 1),
        1,
        null,
        null,
        null,
        null,
    )

    @Suppress("LongParameterList")
    private fun prepared(
        intent: ToolIntent,
        definition: ToolDefinition,
        normalized: String,
        preimage: JsonObject,
        contractVersion: Int,
        policyRevision: String?,
        resolverRevision: String?,
        accesses: InvocationAccessSet?,
        maxResultBytes: Long?,
        sandboxRequirements: SandboxRequirementsV1? = null,
        sandboxRequirementsDigest: String? = null,
    ): ToolContractResult<PreparedToolCall> {
        val canonicalPreimage = canonicalize(preimage.toString())
            ?: return failure(PreparationErrorCode.NON_CANONICAL_VALUE)
        return ToolContractResult.Success(
            PreparedToolCall(
                intent.modelCallId,
                definition.name,
                definition.revision,
                normalized,
                sha256(canonicalPreimage.encodeToByteArray()),
                definition.requirements,
                definition.replayClass,
                contractVersion,
                policyRevision,
                resolverRevision,
                accesses,
                maxResultBytes,
                sandboxRequirements,
                sandboxRequirementsDigest,
            ),
        )
    }

    private fun preparedPreimage(
        definition: ToolDefinition,
        arguments: JsonElement,
        version: Int,
        additions: Map<String, JsonElement> = emptyMap(),
    ): JsonObject {
        val requirements = JsonObject(
            mapOf(
                "capabilities" to JsonArray(definition.requirements.capabilities.map { JsonPrimitive(it.wireName) }),
                "max_duration_ms" to JsonPrimitive(definition.requirements.maxDurationMs),
                "max_output_bytes" to JsonPrimitive(definition.requirements.maxOutputBytes),
            ),
        )
        return JsonObject(
            mapOf<String, JsonElement>(
                "contract" to JsonPrimitive("garive.prepared-tool-call"),
                "version" to JsonPrimitive(version),
                "tool_name" to JsonPrimitive(definition.name),
                "tool_revision" to JsonPrimitive(definition.revision),
                "arguments" to arguments,
                "requirements" to requirements,
                "replay_class" to JsonPrimitive(definition.replayClass.wireName),
            ) + additions,
        )
    }

    public companion object {
        /** Rejects duplicate names and constructs an immutable catalog. */
        public fun create(definitions: List<ToolDefinition>): ToolContractResult<ToolCatalog> {
            val byName = definitions.associateBy(ToolDefinition::name)
            if (byName.size != definitions.size) return failure(PreparationErrorCode.INVALID_TOOL_DEFINITION)
            return ToolContractResult.Success(ToolCatalog(byName))
        }

        /** Returns the canonical digest of one exact immutable Tool catalogue. */
        public fun digest(definitions: List<ToolDefinition>): ToolContractResult<String> {
            val ordered = definitions.sortedBy(ToolDefinition::name)
            if (ordered.zipWithNext().any { (left, right) -> left.name == right.name }) {
                return failure(PreparationErrorCode.INVALID_TOOL_DEFINITION)
            }
            val preimage = JsonObject(
                mapOf(
                    "contract" to JsonPrimitive("garive.tool-catalogue"),
                    "version" to JsonPrimitive(1),
                    "definitions" to JsonArray(ordered.map(::definitionJson)),
                ),
            )
            val canonical = canonicalize(preimage.toString())
                ?: return failure(PreparationErrorCode.NON_CANONICAL_VALUE)
            return ToolContractResult.Success(sha256(canonical.encodeToByteArray()))
        }
    }
}

private fun definitionJson(definition: ToolDefinition): JsonObject = JsonObject(
    buildMap {
        put("name", JsonPrimitive(definition.name))
        put("revision", JsonPrimitive(definition.revision))
        put("description", JsonPrimitive(definition.description))
        put("input_schema", definition.inputSchema)
        put(
            "requirements",
            JsonObject(
                mapOf(
                    "capabilities" to JsonArray(
                        definition.requirements.capabilities.map { JsonPrimitive(it.wireName) },
                    ),
                    "max_duration_ms" to JsonPrimitive(definition.requirements.maxDurationMs),
                    "max_output_bytes" to JsonPrimitive(definition.requirements.maxOutputBytes),
                ),
            ),
        )
        put("replay_class", JsonPrimitive(definition.replayClass.wireName))
        definition.accessPolicy?.let { policy ->
            put(
                "access_contract",
                JsonObject(
                    mapOf(
                        "policy" to accessPolicyJson(policy),
                        "resolver_revision" to JsonPrimitive(definition.accessResolverRevision!!),
                    ),
                ),
            )
        }
        definition.sandboxRequirements?.let { put("sandbox_requirements", it.canonicalJson()) }
    },
)

private fun accessPolicyJson(policy: ToolAccessPolicyV1): JsonObject = JsonObject(
    mapOf(
        "policy_revision" to JsonPrimitive(policy.policyRevision),
        "filesystem_roots" to policyEntriesJson(policy.filesystemRoots),
        "process_lanes" to policyEntriesJson(policy.processLanes),
        "network_origins" to policyEntriesJson(policy.networkOrigins),
        "runtime_lanes" to policyEntriesJson(policy.runtimeLanes),
        "max_accesses" to JsonPrimitive(policy.maxAccesses),
        "max_result_bytes" to JsonPrimitive(policy.maxResultBytes),
    ),
)

private fun policyEntriesJson(entries: List<AccessPolicyEntry>): JsonArray = JsonArray(
    entries.map { entry ->
        JsonObject(
            mapOf(
                "resource" to JsonPrimitive(entry.resource),
                "allowed_modes" to JsonArray(entry.allowedModes.map { JsonPrimitive(it.wireName) }),
            ),
        )
    },
)

private data class ValidatedIntent(
    val definition: ToolDefinition,
    val arguments: JsonElement,
    val normalized: String,
)

private fun canonicalize(value: String): String? = runCatching {
    JsonCanonicalizer(value).encodedString
}.getOrNull()

private fun sha256(value: ByteArray): String = MessageDigest.getInstance("SHA-256")
    .digest(value)
    .joinToString(separator = "") { byte -> "%02x".format(byte.toInt() and 0xff) }
