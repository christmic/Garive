package com.garive.eng.kt.tools

import kotlinx.serialization.json.JsonElement

/** Typed success or failure returned by the portable tool contract. */
public sealed interface ToolContractResult<out T> {
    /** Successful immutable value. */
    public data class Success<T>(public val value: T) : ToolContractResult<T>

    /** Stable contract failure. */
    public data class Failure(public val error: PreparationError) : ToolContractResult<Nothing>
}

/** Stable C4 construction or preparation failure classification. */
public enum class PreparationErrorCode(public val wireName: String) {
    /** Model correlation identity is empty. */
    INVALID_MODEL_CALL_ID("invalid_model_call_id"),
    /** Proposed tool name is empty. */
    INVALID_TOOL_NAME("invalid_tool_name"),
    /** The frozen catalog does not admit the proposed tool. */
    TOOL_NOT_ADMITTED("tool_not_admitted"),
    /** Argument text is malformed, trailing, or duplicate-keyed. */
    INVALID_ARGUMENTS_JSON("invalid_arguments_json"),
    /** Arguments fail the exact Portable Tool Schema. */
    ARGUMENTS_SCHEMA_MISMATCH("arguments_schema_mismatch"),
    /** Definition or execution requirements are invalid. */
    INVALID_TOOL_DEFINITION("invalid_tool_definition"),
    /** Schema contains a keyword outside v1. */
    UNSUPPORTED_SCHEMA_KEYWORD("unsupported_schema_keyword"),
    /** Value cannot satisfy RFC 8785 canonicalization. */
    NON_CANONICAL_VALUE("non_canonical_value"),
    /** C5b access declaration, key, resolver result, or policy is invalid. */
    EFFECT_ACCESS_INVALID("effect_access_invalid"),
}

/** Deterministic JSON Schema assertion failure. */
public data class SchemaFailure(
    public val instancePath: String,
    public val schemaPath: String,
    public val keyword: String,
)

/** Typed preparation failure with optional ordered schema evidence. */
public data class PreparationError(
    public val code: PreparationErrorCode,
    public val failures: List<SchemaFailure> = emptyList(),
)

/** Neutral executor capability declared by one exact tool definition. */
public enum class ExecutionCapability(public val wireName: String) {
    /** Read from an admitted filesystem surface. */
    FILESYSTEM_READ("filesystem_read"),
    /** Mutate an admitted filesystem surface. */
    FILESYSTEM_WRITE("filesystem_write"),
    /** Start a bounded process. */
    PROCESS("process"),
    /** Access an admitted network surface. */
    NETWORK("network"),
}

/** Immutable executor requirements carried by a Prepared Call. */
public class ExecutionRequirements private constructor(
    capabilities: List<ExecutionCapability>,
    public val maxDurationMs: Long,
    public val maxOutputBytes: Long,
) {
    /** Unique capabilities in canonical declaration order. */
    public val capabilities: List<ExecutionCapability> = capabilities.toList()

    public companion object {
        /** Validates non-zero limits and unique capabilities. */
        public fun create(
            capabilities: List<ExecutionCapability>,
            maxDurationMs: Long,
            maxOutputBytes: Long,
        ): ToolContractResult<ExecutionRequirements> {
            if (maxDurationMs <= 0 || maxOutputBytes <= 0 || capabilities.distinct().size != capabilities.size) {
                return failure(PreparationErrorCode.INVALID_TOOL_DEFINITION)
            }
            return ToolContractResult.Success(
                ExecutionRequirements(capabilities.sortedBy { it.ordinal }, maxDurationMs, maxOutputBytes),
            )
        }
    }

    public override fun equals(other: Any?): Boolean =
        other is ExecutionRequirements &&
            capabilities == other.capabilities &&
            maxDurationMs == other.maxDurationMs &&
            maxOutputBytes == other.maxOutputBytes

    public override fun hashCode(): Int =
        31 * (31 * capabilities.hashCode() + maxDurationMs.hashCode()) + maxOutputBytes.hashCode()
}

/** Recovery safety declaration that Runtime must independently prove. */
public enum class ReplayClass(public val wireName: String) {
    /** Read-only operation eligible for a proven same-ID retry. */
    READ_ONLY("read_only"),
    /** Executor supports a proven idempotency identity. */
    IDEMPOTENT("idempotent"),
    /** Executor recovers from a committed receipt or journal. */
    RECEIPT_RECOVERABLE("receipt_recoverable"),
    /** Uncertain started operation always requires reconciliation. */
    NEVER_REPLAY("never_replay"),
}

/** Exact immutable definition admitted to one execution snapshot. */
public class ToolDefinition private constructor(
    public val name: String,
    public val revision: String,
    public val description: String,
    public val inputSchema: JsonElement,
    public val requirements: ExecutionRequirements,
    public val replayClass: ReplayClass,
    /** Frozen v2 policy, absent for C4 v1 definitions. */
    public val accessPolicy: ToolAccessPolicyV1?,
    /** Frozen trusted resolver revision, absent for C4 v1 definitions. */
    public val accessResolverRevision: String?,
) {
    /** Prepared Call contract version selected by this exact definition. */
    public val preparedContractVersion: Int = if (accessPolicy == null) 1 else 2

    public companion object {
        /** Validates and constructs one Portable Tool Schema v1 definition. */
        public fun create(
            name: String,
            revision: String,
            description: String,
            inputSchema: JsonElement,
            requirements: ExecutionRequirements,
            replayClass: ReplayClass,
        ): ToolContractResult<ToolDefinition> {
            return createInternal(
                name, revision, description, inputSchema, requirements, replayClass, null, null, false,
            )
        }

        /** Constructs a Prepared v2-capable definition with a frozen access contract. */
        @Suppress("LongParameterList")
        public fun createV2(
            name: String,
            revision: String,
            description: String,
            inputSchema: JsonElement,
            requirements: ExecutionRequirements,
            replayClass: ReplayClass,
            accessPolicy: ToolAccessPolicyV1,
            accessResolverRevision: String,
        ): ToolContractResult<ToolDefinition> = createInternal(
            name,
            revision,
            description,
            inputSchema,
            requirements,
            replayClass,
            accessPolicy,
            accessResolverRevision,
            true,
        )

        @Suppress("LongParameterList")
        private fun createInternal(
            name: String,
            revision: String,
            description: String,
            inputSchema: JsonElement,
            requirements: ExecutionRequirements,
            replayClass: ReplayClass,
            accessPolicy: ToolAccessPolicyV1?,
            accessResolverRevision: String?,
            v2AccessProof: Boolean,
        ): ToolContractResult<ToolDefinition> {
            if (name.isEmpty() || revision.isEmpty() || description.isEmpty()) {
                return failure(PreparationErrorCode.INVALID_TOOL_DEFINITION)
            }
            PortableSchema.validateDefinition(inputSchema)?.let { return ToolContractResult.Failure(it) }
            if (replayClass == ReplayClass.READ_ONLY && requirements.capabilities.any {
                    it != ExecutionCapability.FILESYSTEM_READ && !(v2AccessProof && it == ExecutionCapability.NETWORK)
                }
            ) {
                return failure(PreparationErrorCode.INVALID_TOOL_DEFINITION)
            }
            if ((accessPolicy == null) != (accessResolverRevision == null) || accessResolverRevision == "") {
                return failure(PreparationErrorCode.EFFECT_ACCESS_INVALID)
            }
            return ToolContractResult.Success(
                ToolDefinition(
                    name,
                    revision,
                    description,
                    inputSchema,
                    requirements,
                    replayClass,
                    accessPolicy,
                    accessResolverRevision,
                ),
            )
        }
    }
}

/** Untrusted model proposal supplied to C4. */
public data class ToolIntent(
    public val modelCallId: String,
    public val toolName: String,
    public val argumentsJson: String,
)

/** Immutable validated call carrying no invocation identity or authority. */
public data class PreparedToolCall(
    public val modelCallId: String,
    public val toolName: String,
    public val toolRevision: String,
    public val normalizedArguments: String,
    public val inputDigest: String,
    public val requirements: ExecutionRequirements,
    public val replayClass: ReplayClass,
    /** Immutable Prepared Call contract version. */
    public val contractVersion: Int,
    /** V2 access policy revision, absent for v1. */
    public val accessPolicyRevision: String?,
    /** V2 trusted resolver revision, absent for v1. */
    public val accessResolverRevision: String?,
    /** V2 exact canonical accesses, absent for v1. */
    public val invocationAccesses: InvocationAccessSet?,
    /** V2 result buffer charge, absent for v1. */
    public val maxResultBytes: Long?,
)

internal fun failure(code: PreparationErrorCode): ToolContractResult.Failure =
    ToolContractResult.Failure(PreparationError(code))
