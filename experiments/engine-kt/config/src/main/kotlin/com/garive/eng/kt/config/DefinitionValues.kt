package com.garive.eng.kt.config

/** Stable D0 definition or resolution failure classification. */
public enum class ResolutionErrorCode(public val wireName: String) {
    /** Exact definition identity is absent. */
    DEFINITION_NOT_FOUND("definition_not_found"),
    /** A required exact reference is absent. */
    REFERENCE_NOT_FOUND("reference_not_found"),
    /** More than one candidate matches an exact reference. */
    REFERENCE_AMBIGUOUS("reference_ambiguous"),
    /** Exact-reference expansion contains a cycle. */
    REFERENCE_CYCLE("reference_cycle"),
    /** A required contract version is not admitted. */
    UNSUPPORTED_CONTRACT_VERSION("unsupported_contract_version"),
    /** Product authority cannot satisfy the definition. */
    POLICY_INCOMPATIBLE("policy_incompatible"),
    /** A definition or cross-field invariant is invalid. */
    INVALID_DEFINITION("invalid_definition"),
    /** A digest input cannot satisfy canonicalization. */
    NON_CANONICAL_VALUE("non_canonical_value"),
}

/** Secret-free D0 failure with a stable JSON-pointer path. */
public data class ResolutionError(
    public val code: ResolutionErrorCode,
    public val path: String,
)

/** Typed success or failure returned by the D0 contract. */
public sealed interface DefinitionResult<out T> {
    /** Successful immutable value. */
    public data class Success<T>(public val value: T) : DefinitionResult<T>

    /** Stable secret-free failure. */
    public data class Failure(public val error: ResolutionError) : DefinitionResult<Nothing>
}

internal fun invalid(path: String): DefinitionResult.Failure =
    DefinitionResult.Failure(ResolutionError(ResolutionErrorCode.INVALID_DEFINITION, path))

/** Exact instruction source reference. */
public class InstructionReference private constructor(
    public val sourceId: String,
    public val exactRevision: String,
    public val required: Boolean,
) {
    public companion object {
        /** Validates one exact instruction reference. */
        public fun create(
            sourceId: String,
            exactRevision: String,
            required: Boolean,
        ): DefinitionResult<InstructionReference> =
            if (sourceId.isEmpty()) {
                invalid("/instruction_sources/source_id")
            } else if (exactRevision.isEmpty()) {
                invalid("/instruction_sources/exact_revision")
            } else {
                DefinitionResult.Success(InstructionReference(sourceId, exactRevision, required))
            }
    }
}

/** Neutral model role requirement. */
public class ModelRoleRequirement private constructor(
    public val roleId: String,
    requiredCapabilities: List<String>,
    public val required: Boolean,
) {
    /** Canonically sorted unique capabilities required from the target. */
    public val requiredCapabilities: List<String> = requiredCapabilities.toList()

    public companion object {
        /** Validates one model role and rejects duplicate capabilities. */
        public fun create(
            roleId: String,
            requiredCapabilities: List<String>,
            required: Boolean,
        ): DefinitionResult<ModelRoleRequirement> =
            if (
                roleId.isEmpty() ||
                requiredCapabilities.any(String::isEmpty) ||
                requiredCapabilities.distinct().size != requiredCapabilities.size
            ) {
                invalid("/model_roles/required_capabilities")
            } else {
                DefinitionResult.Success(
                    ModelRoleRequirement(roleId, requiredCapabilities.sorted(), required),
                )
            }
    }
}

/** Portable capability kind admitted by D0. */
public enum class CapabilityKind(public val wireName: String) {
    /** C4 tool definition. */
    TOOL("tool"),
    /** Skill descriptor. */
    SKILL("skill"),
    /** Memory descriptor. */
    MEMORY("memory"),
    /** Knowledge descriptor. */
    KNOWLEDGE("knowledge"),
    /** Delegation descriptor. */
    DELEGATION("delegation"),
}

/** Exact capability reference. */
public class CapabilityReference private constructor(
    public val kind: CapabilityKind,
    public val name: String,
    public val exactRevision: String,
    public val contractVersion: Long,
    public val required: Boolean,
) {
    public companion object {
        /** Validates one exact capability reference. */
        public fun create(
            kind: CapabilityKind,
            name: String,
            exactRevision: String,
            contractVersion: Long,
            required: Boolean,
        ): DefinitionResult<CapabilityReference> =
            if (name.isEmpty()) {
                invalid("/capabilities/name")
            } else if (exactRevision.isEmpty()) {
                invalid("/capabilities/exact_revision")
            } else if (contractVersion <= 0) {
                invalid("/capabilities/contract_version")
            } else {
                DefinitionResult.Success(
                    CapabilityReference(kind, name, exactRevision, contractVersion, required),
                )
            }
    }
}

/** Interaction mode that effective governance may admit. */
public enum class InteractionMode(public val wireName: String) {
    /** Human or product authority approval. */
    APPROVAL("approval"),
    /** Typed external input request. */
    EXTERNAL_INPUT("external_input"),
}

/** Required unmatched-policy behavior in D0 v1. */
public enum class DefaultUnmatched(public val wireName: String) {
    /** Reject an unmatched request. */
    DENY("deny"),
}

/** Requested governance policy and portable authority surface. */
public class GovernancePolicy private constructor(
    public val policyId: String,
    public val exactRevision: String,
    allowedRequirementCapabilities: List<String>,
    interactionModes: List<InteractionMode>,
    public val defaultUnmatched: DefaultUnmatched,
) {
    /** Canonically sorted executor capabilities the definition may request. */
    public val allowedRequirementCapabilities: List<String> = allowedRequirementCapabilities.toList()

    /** Canonically ordered interaction modes. */
    public val interactionModes: List<InteractionMode> = interactionModes.toList()

    public companion object {
        /** Validates policy identity and unique requested sets. */
        public fun create(
            policyId: String,
            exactRevision: String,
            allowedRequirementCapabilities: List<String>,
            interactionModes: List<InteractionMode>,
        ): DefinitionResult<GovernancePolicy> =
            if (
                policyId.isEmpty() || exactRevision.isEmpty() ||
                allowedRequirementCapabilities.any(String::isEmpty) ||
                allowedRequirementCapabilities.distinct().size != allowedRequirementCapabilities.size ||
                interactionModes.distinct().size != interactionModes.size
            ) {
                invalid("/governance")
            } else {
                DefinitionResult.Success(
                    GovernancePolicy(
                        policyId,
                        exactRevision,
                        allowedRequirementCapabilities.sorted(),
                        interactionModes.sortedBy { it.ordinal },
                        DefaultUnmatched.DENY,
                    ),
                )
            }
    }
}

/** Exact context policy reference. */
public class ContextPolicyReference private constructor(
    public val policyId: String,
    public val exactRevision: String,
) {
    public companion object {
        /** Validates an exact context policy reference. */
        public fun create(
            policyId: String,
            exactRevision: String,
        ): DefinitionResult<ContextPolicyReference> =
            if (policyId.isEmpty()) {
                invalid("/context_policy/policy_id")
            } else if (exactRevision.isEmpty()) {
                invalid("/context_policy/exact_revision")
            } else {
                DefinitionResult.Success(ContextPolicyReference(policyId, exactRevision))
            }
    }
}

/** Definition defaults that Runtime may only tighten. */
public class DefaultLimits private constructor(
    public val maxIterations: Long,
    public val maxInputTokens: Long?,
    public val maxOutputTokens: Long?,
    public val deadlineBudgetMs: Long?,
) {
    public companion object {
        /** Rejects zero values while retaining explicit optional bounds. */
        public fun create(
            maxIterations: Long,
            maxInputTokens: Long?,
            maxOutputTokens: Long?,
            deadlineBudgetMs: Long?,
        ): DefinitionResult<DefaultLimits> =
            if (
                maxIterations <= 0 ||
                listOfNotNull(maxInputTokens, maxOutputTokens, deadlineBudgetMs).any { it <= 0 }
            ) {
                invalid("/limits")
            } else {
                DefinitionResult.Success(
                    DefaultLimits(maxIterations, maxInputTokens, maxOutputTokens, deadlineBudgetMs),
                )
            }
    }
}
