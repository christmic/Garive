package com.garive.eng.kt.memory

/** Cognitive lifecycle class independent of content role. */
public enum class MemoryType(public val wireName: String) {
    SEMANTIC("semantic"), EPISODIC("episodic"), LESSON("lesson"), PROCEDURAL("procedural"),
}

/** Provenance authority of a memory hypothesis. */
public enum class MemoryAuthority(public val wireName: String) {
    USER_DECLARED("user_declared"), AGENT_LEARNED("agent_learned"),
    ORGANISATION_PUBLISHED("organisation_published"),
}

/** Receipt-shaped authority claim; Runtime remains responsible for verification. */
@ConsistentCopyVisibility
public data class MemoryAuthorityBinding private constructor(
    public val authority: MemoryAuthority,
    public val receiptDigest: String?,
) {
    public companion object {
        /** Applies exact receipt-presence and digest rules. */
        public fun create(
            authority: MemoryAuthority,
            receiptDigest: String?,
        ): MemoryContractResult<MemoryAuthorityBinding> {
            val requires = authority != MemoryAuthority.AGENT_LEARNED
            return when {
                requires && receiptDigest == null -> failure(MemoryErrorCode.AUTHORITY_RECEIPT_REQUIRED)
                requires != (receiptDigest != null) || receiptDigest?.let(::validDigest) == false ->
                    failure(MemoryErrorCode.INVALID_MEMORY)
                else -> MemoryContractResult.Success(MemoryAuthorityBinding(authority, receiptDigest))
            }
        }
    }
}

/** Privacy/ownership class whose identifiers remain Runtime-opaque. */
public enum class MemoryScopeClass(public val wireName: String) {
    SESSION("session"), AGENT_INSTANCE("agent_instance"), USER("user"), PROJECT("project"), PLATFORM("platform"),
}

/** Scope class plus the aggregation policy required only by Platform. */
@ConsistentCopyVisibility
public data class MemoryScopeBinding private constructor(
    public val scope: MemoryScopeClass,
    public val aggregationPolicyDigest: String?,
) {
    public companion object {
        /** Applies exact Platform policy-binding rules. */
        public fun create(
            scope: MemoryScopeClass,
            aggregationPolicyDigest: String?,
        ): MemoryContractResult<MemoryScopeBinding> {
            val platform = scope == MemoryScopeClass.PLATFORM
            return when {
                platform && aggregationPolicyDigest == null -> failure(MemoryErrorCode.SCOPE_POLICY_DENIED)
                platform != (aggregationPolicyDigest != null) ||
                    aggregationPolicyDigest?.let(::validDigest) == false -> failure(MemoryErrorCode.INVALID_MEMORY)
                else -> MemoryContractResult.Success(MemoryScopeBinding(scope, aggregationPolicyDigest))
            }
        }
    }
}

/** Immutable registry row selecting admitted policy revisions. */
@ConsistentCopyVisibility
public data class MemoryTypeDescriptor private constructor(
    public val memoryType: MemoryType,
    public val roles: List<MemoryKind>,
    public val authorities: List<MemoryAuthority>,
    public val lifecycleRevision: String,
    public val recallRevision: String,
    public val retentionRevision: String,
    public val surfaceKind: String,
) {
    /** Tests an exact role and authority combination. */
    public fun admits(role: MemoryKind, authority: MemoryAuthority): Boolean =
        role in roles && authority in authorities

    public companion object {
        /** Validates non-empty canonical sets and bounded policy identities. */
        public fun create(
            memoryType: MemoryType,
            roles: List<MemoryKind>,
            authorities: List<MemoryAuthority>,
            lifecycleRevision: String,
            recallRevision: String,
            retentionRevision: String,
            surfaceKind: String,
        ): MemoryContractResult<MemoryTypeDescriptor> {
            val texts = listOf(lifecycleRevision, recallRevision, retentionRevision, surfaceKind)
            return if (roles.isEmpty() || !orderedEnums(roles) || authorities.isEmpty() ||
                !orderedEnums(authorities) || texts.any { !validText(it, MAX_REFERENCE_BYTES) }
            ) failure(MemoryErrorCode.INVALID_MEMORY)
            else MemoryContractResult.Success(
                MemoryTypeDescriptor(memoryType, roles, authorities, lifecycleRevision, recallRevision, retentionRevision, surfaceKind),
            )
        }
    }
}

/** Frozen complete M1 type registry. */
@ConsistentCopyVisibility
public data class MemoryTypeRegistry private constructor(
    public val revision: String,
    public val descriptors: List<MemoryTypeDescriptor>,
) {
    /** Tests admission under this exact registry. */
    public fun admits(type: MemoryType, role: MemoryKind, authority: MemoryAuthority): Boolean =
        descriptors.find { it.memoryType == type }?.admits(role, authority) == true

    public companion object {
        /** Requires canonical rows covering every M1 type exactly once. */
        public fun create(
            revision: String,
            descriptors: List<MemoryTypeDescriptor>,
        ): MemoryContractResult<MemoryTypeRegistry> =
            if (!validText(revision, MAX_REFERENCE_BYTES) || descriptors.map { it.memoryType } != MemoryType.entries) {
                failure(MemoryErrorCode.UNKNOWN_MEMORY_TYPE)
            } else MemoryContractResult.Success(MemoryTypeRegistry(revision, descriptors))
    }
}

/** Explicit result of importing one M0 role. */
public data class ImportedMemoryClassification(
    public val memoryType: MemoryType,
    public val role: MemoryKind,
    public val authority: MemoryAuthorityBinding,
)

/** Maps M0 meaning without inspecting content or inferring authority. */
public fun importM0Classification(
    kind: MemoryKind,
    authority: MemoryAuthorityBinding,
): ImportedMemoryClassification = ImportedMemoryClassification(
    if (kind == MemoryKind.SUMMARY) MemoryType.EPISODIC else MemoryType.SEMANTIC,
    kind,
    authority,
)

/** One registry-admitted initial classification for a newly committed M0 revision. */
@ConsistentCopyVisibility
public data class MemoryRevisionClassification private constructor(
    public val memoryType: MemoryType,
    public val role: MemoryKind,
    public val authority: MemoryAuthorityBinding,
    public val scope: MemoryRevisionScope,
    public val lifecycle: HypothesisState,
    public val policyRevision: String,
) {
    public companion object {
        /** Validates role mapping, authority admission, initial lifecycle and policy identity. */
        public fun create(
            role: MemoryKind,
            authority: MemoryAuthorityBinding,
            scope: MemoryRevisionScope,
            lifecycle: HypothesisState,
            policyRevision: String,
            registry: MemoryTypeRegistry,
        ): MemoryContractResult<MemoryRevisionClassification> {
            val imported = importM0Classification(role, authority)
            return if (lifecycle !in setOf(HypothesisState.CANDIDATE, HypothesisState.ACTIVE) ||
                !validText(policyRevision, MAX_REFERENCE_BYTES) ||
                !registry.admits(imported.memoryType, imported.role, imported.authority.authority)
            ) MemoryContractResult.Failure(MemoryError(MemoryErrorCode.INVALID_MEMORY))
            else MemoryContractResult.Success(MemoryRevisionClassification(
                imported.memoryType, imported.role, imported.authority, scope, lifecycle, policyRevision,
            ))
        }
    }
}

/** Runtime-authorized M1 scope frozen for one committed revision. */
@ConsistentCopyVisibility
public data class MemoryRevisionScope private constructor(
    public val scope: MemoryScopeClass,
    public val ownerId: String,
    public val aggregationPolicyDigest: String?,
) {
    public companion object {
        /** Validates the opaque owner plus the Platform-only aggregation policy. */
        public fun create(
            scope: MemoryScopeClass,
            ownerId: String,
            aggregationPolicyDigest: String?,
        ): MemoryContractResult<MemoryRevisionScope> = when (
            val binding = MemoryScopeBinding.create(scope, aggregationPolicyDigest)
        ) {
            is MemoryContractResult.Failure -> binding
            is MemoryContractResult.Success -> if (!validText(ownerId, MAX_REFERENCE_BYTES)) {
                MemoryContractResult.Failure(MemoryError(MemoryErrorCode.INVALID_MEMORY))
            } else MemoryContractResult.Success(MemoryRevisionScope(
                binding.value.scope, ownerId, binding.value.aggregationPolicyDigest,
            ))
        }
    }
}

private fun <T : Enum<T>> orderedEnums(values: List<T>): Boolean =
    values.zipWithNext().all { (left, right) -> left.ordinal < right.ordinal }
