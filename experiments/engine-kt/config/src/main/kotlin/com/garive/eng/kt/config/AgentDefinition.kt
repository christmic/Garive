package com.garive.eng.kt.config

/** Complete immutable portable Agent intent. */
public class AgentDefinition private constructor(
    public val definitionId: String,
    public val revision: String,
    instructionSources: List<InstructionReference>,
    modelRoles: List<ModelRoleRequirement>,
    capabilities: List<CapabilityReference>,
    public val governance: GovernancePolicy,
    public val contextPolicy: ContextPolicyReference,
    public val limits: DefaultLimits,
    contractVersions: Map<String, Long>,
) {
    /** Ordered instruction roots from low to high precedence. */
    public val instructionSources: List<InstructionReference> = instructionSources.toList()

    /** Ordered neutral model roles. */
    public val modelRoles: List<ModelRoleRequirement> = modelRoles.toList()

    /** Exact capability references. */
    public val capabilities: List<CapabilityReference> = capabilities.toList()

    /** Required named portable contract versions in canonical key order. */
    public val contractVersions: Map<String, Long> = contractVersions.toSortedMap()

    public companion object {
        /** Validates all definition-local identity and uniqueness invariants. */
        public fun create(
            definitionId: String,
            revision: String,
            instructionSources: List<InstructionReference>,
            modelRoles: List<ModelRoleRequirement>,
            capabilities: List<CapabilityReference>,
            governance: GovernancePolicy,
            contextPolicy: ContextPolicyReference,
            limits: DefaultLimits,
            contractVersions: Map<String, Long>,
        ): DefinitionResult<AgentDefinition> {
            val failurePath = when {
                definitionId.isEmpty() -> "/definition_id"
                revision.isEmpty() -> "/revision"
                instructionSources.distinctBy { it.sourceId }.size != instructionSources.size ->
                    "/instruction_sources"
                modelRoles.distinctBy { it.roleId }.size != modelRoles.size -> "/model_roles"
                capabilities.distinctBy { it.kind to it.name }.size != capabilities.size ->
                    "/capabilities"
                contractVersions.any { it.key.isEmpty() || it.value <= 0 } -> "/contract_versions"
                else -> null
            }
            return if (failurePath != null) {
                invalid(failurePath)
            } else {
                DefinitionResult.Success(
                    AgentDefinition(
                        definitionId,
                        revision,
                        instructionSources,
                        modelRoles,
                        capabilities,
                        governance,
                        contextPolicy,
                        limits,
                        contractVersions,
                    ),
                )
            }
        }
    }
}
