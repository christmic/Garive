package com.garive.eng.kt.config

import com.garive.eng.kt.tools.ToolDefinition

/** Exact instruction registry resource with ordered dependencies. */
public data class InstructionResource(
    public val sourceId: String,
    public val exactRevision: String,
    public val contentUtf8: String,
    public val dependencies: List<InstructionReference>,
)

/** Neutral model target candidate supplied by Runtime. */
public data class ModelRoleCandidate(
    public val roleId: String,
    public val capabilityTargetId: String,
    public val admittedCapabilities: Set<String>,
)

/** Exact non-tool capability descriptor supplied by Runtime. */
public data class CapabilityDescriptor(
    public val kind: CapabilityKind,
    public val name: String,
    public val exactRevision: String,
    public val contractVersion: Long,
    public val descriptorDigest: String,
)

/** Exact governance registry candidate. */
public data class GovernancePolicyCandidate(
    public val policyId: String,
    public val exactRevision: String,
    public val allowedRequirementCapabilities: Set<String>,
    public val interactionModes: Set<InteractionMode>,
)

/** Exact context policy registry candidate. */
public data class ContextPolicyCandidate(
    public val policyId: String,
    public val exactRevision: String,
    public val descriptorDigest: String,
)

/** One exact Tool-to-public-label mapping used only by H3 projection. */
public data class PublicToolActivityDescriptor(
    public val toolName: String,
    public val toolRevision: String,
    public val labelKey: String,
)

/** Complete immutable H3 label catalogue included in snapshot v2. */
public data class PublicToolActivityCatalogue(
    public val schemaVersion: Int,
    public val catalogueRevision: String,
    public val descriptors: List<PublicToolActivityDescriptor>,
)

/** Frozen Runtime registry view used for one resolution attempt. */
public data class ResolutionRegistry(
    public val instructions: List<InstructionResource>,
    public val modelRoles: List<ModelRoleCandidate>,
    public val tools: List<ToolDefinition>,
    public val capabilityDescriptors: List<CapabilityDescriptor>,
    public val governancePolicies: List<GovernancePolicyCandidate>,
    public val contextPolicies: List<ContextPolicyCandidate>,
    public val publicToolActivityCatalogue: PublicToolActivityCatalogue? = null,
)

/** Product and actor ceilings applied without mutating the definition. */
public data class ProductPolicy(
    public val allowedRequirementCapabilities: Set<String>,
    public val interactionModes: Set<InteractionMode>,
    public val limitCaps: DefaultLimits,
    public val admittedContractVersions: Map<String, Set<Long>>,
)

/** Resolved exact instruction included in execution precedence order. */
public data class ResolvedInstruction(
    public val sourceId: String,
    public val exactRevision: String,
    public val contentUtf8: String,
    public val contentDigest: String,
)

/** Resolved neutral model role. */
public data class ResolvedModelRole(
    public val roleId: String,
    public val capabilityTargetId: String,
    public val admittedCapabilities: List<String>,
)

/** Exact enabled tool definitions and other public capability descriptors. */
public data class EffectiveCapabilitySnapshot(
    public val tools: List<ToolDefinition>,
    public val descriptors: List<CapabilityDescriptor>,
)

/** Governance policy after intersection with Runtime authority. */
public data class EffectiveGovernancePolicy(
    public val policyId: String,
    public val exactRevision: String,
    public val allowedRequirementCapabilities: List<String>,
    public val interactionModes: List<InteractionMode>,
    public val defaultUnmatched: DefaultUnmatched,
)

/** Resolved exact context policy descriptor. */
public data class ResolvedContextPolicy(
    public val policyId: String,
    public val exactRevision: String,
    public val descriptorDigest: String,
)

/** Effective limits after monotonic Runtime tightening. */
public data class EffectiveLimits(
    public val maxIterations: Long,
    public val maxInputTokens: Long?,
    public val maxOutputTokens: Long?,
    public val deadlineBudgetMs: Long?,
)

/** Deeply immutable exact execution meaning bound to one durable Turn. */
public data class EffectiveAgentSnapshot(
    public val definitionId: String,
    public val definitionRevision: String,
    public val definitionDigest: String,
    public val instructions: List<ResolvedInstruction>,
    public val modelRoles: List<ResolvedModelRole>,
    public val capabilities: EffectiveCapabilitySnapshot,
    public val governance: EffectiveGovernancePolicy,
    public val contextPolicy: ResolvedContextPolicy,
    public val limits: EffectiveLimits,
    public val contractVersions: Map<String, Long>,
    public val publicToolActivityCatalogue: PublicToolActivityCatalogue? = null,
    public val snapshotDigest: String,
) {
    /** Validates that continuation reuses the exact durable binding. */
    public fun validateContinuation(
        definitionRevision: String,
        snapshotDigest: String,
    ): DefinitionResult<Unit> =
        if (this.definitionRevision == definitionRevision && this.snapshotDigest == snapshotDigest) {
            DefinitionResult.Success(Unit)
        } else {
            DefinitionResult.Failure(
                ResolutionError(ResolutionErrorCode.INVALID_DEFINITION, "/continuation_binding"),
            )
        }
}
