package com.garive.eng.kt.config

import com.garive.eng.kt.tools.ToolDefinition
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer
import java.security.MessageDigest

private class ResolutionAbort(public val error: ResolutionError) : RuntimeException()

private fun abort(code: ResolutionErrorCode, path: String): Nothing =
    throw ResolutionAbort(ResolutionError(code, path))

private fun sha256(bytes: ByteArray): String =
    MessageDigest.getInstance("SHA-256").digest(bytes).joinToString("") { "%02x".format(it) }

/** Returns lowercase SHA-256 over RFC 8785 canonical JSON bytes. */
public fun digestCanonicalValue(value: JsonElement): DefinitionResult<String> =
    try {
        DefinitionResult.Success(sha256(JsonCanonicalizer(value.toString()).encodedUTF8))
    } catch (_: RuntimeException) {
        DefinitionResult.Failure(
            ResolutionError(ResolutionErrorCode.NON_CANONICAL_VALUE, "/canonical_value"),
        )
    }

private fun digest(value: JsonElement, path: String): String =
    when (val result = digestCanonicalValue(value)) {
        is DefinitionResult.Success -> result.value
        is DefinitionResult.Failure -> abort(result.error.code, path)
    }

private fun <T> exactOne(candidates: List<T>, required: Boolean, path: String): T? =
    when {
        candidates.isEmpty() && required -> abort(ResolutionErrorCode.REFERENCE_NOT_FOUND, path)
        candidates.isEmpty() -> null
        candidates.size == 1 -> candidates.single()
        else -> abort(ResolutionErrorCode.REFERENCE_AMBIGUOUS, path)
    }

private fun stringArray(values: Iterable<String>): JsonArray =
    JsonArray(values.map(::JsonPrimitive))

private fun referenceJson(value: InstructionReference): JsonObject = JsonObject(
    mapOf(
        "source_id" to JsonPrimitive(value.sourceId),
        "exact_revision" to JsonPrimitive(value.exactRevision),
        "required" to JsonPrimitive(value.required),
    ),
)

private fun limitsJson(value: DefaultLimits, omitNull: Boolean): JsonObject {
    val fields = linkedMapOf<String, JsonElement>("max_iterations" to JsonPrimitive(value.maxIterations))
    fun optional(name: String, item: Long?) {
        if (item != null) fields[name] = JsonPrimitive(item) else if (!omitNull) fields[name] = JsonNull
    }
    optional("max_input_tokens", value.maxInputTokens)
    optional("max_output_tokens", value.maxOutputTokens)
    optional("deadline_budget_ms", value.deadlineBudgetMs)
    return JsonObject(fields)
}

private fun effectiveLimitsJson(value: EffectiveLimits): JsonObject {
    val fields = linkedMapOf<String, JsonElement>("max_iterations" to JsonPrimitive(value.maxIterations))
    value.maxInputTokens?.let { fields["max_input_tokens"] = JsonPrimitive(it) }
    value.maxOutputTokens?.let { fields["max_output_tokens"] = JsonPrimitive(it) }
    value.deadlineBudgetMs?.let { fields["deadline_budget_ms"] = JsonPrimitive(it) }
    return JsonObject(fields)
}

private fun definitionJson(value: AgentDefinition): JsonObject = JsonObject(
    mapOf(
        "definition_id" to JsonPrimitive(value.definitionId),
        "revision" to JsonPrimitive(value.revision),
        "instruction_sources" to JsonArray(value.instructionSources.map(::referenceJson)),
        "model_roles" to JsonArray(value.modelRoles.map { role ->
            JsonObject(mapOf("role_id" to JsonPrimitive(role.roleId), "required_capabilities" to stringArray(role.requiredCapabilities), "required" to JsonPrimitive(role.required)))
        }),
        "capabilities" to JsonArray(value.capabilities.map { capability ->
            JsonObject(mapOf("kind" to JsonPrimitive(capability.kind.wireName), "name" to JsonPrimitive(capability.name), "exact_revision" to JsonPrimitive(capability.exactRevision), "contract_version" to JsonPrimitive(capability.contractVersion), "required" to JsonPrimitive(capability.required)))
        }),
        "governance" to JsonObject(mapOf("policy_id" to JsonPrimitive(value.governance.policyId), "exact_revision" to JsonPrimitive(value.governance.exactRevision), "allowed_requirement_capabilities" to stringArray(value.governance.allowedRequirementCapabilities), "interaction_modes" to stringArray(value.governance.interactionModes.map { it.wireName }), "default_unmatched" to JsonPrimitive(value.governance.defaultUnmatched.wireName))),
        "context_policy" to JsonObject(mapOf("policy_id" to JsonPrimitive(value.contextPolicy.policyId), "exact_revision" to JsonPrimitive(value.contextPolicy.exactRevision))),
        "limits" to limitsJson(value.limits, false),
        "contract_versions" to JsonObject(value.contractVersions.mapValues { JsonPrimitive(it.value) }),
    ),
)

private fun toolJson(value: ToolDefinition): JsonObject {
    val fields = linkedMapOf<String, JsonElement>(
        "name" to JsonPrimitive(value.name),
        "revision" to JsonPrimitive(value.revision),
        "description" to JsonPrimitive(value.description),
        "input_schema" to value.inputSchema,
        "requirements" to JsonObject(mapOf("capabilities" to stringArray(value.requirements.capabilities.map { it.wireName }), "max_duration_ms" to JsonPrimitive(value.requirements.maxDurationMs), "max_output_bytes" to JsonPrimitive(value.requirements.maxOutputBytes))),
        "replay_class" to JsonPrimitive(value.replayClass.wireName),
    )
    value.accessPolicy?.let { policy ->
        fun entries(values: List<com.garive.eng.kt.tools.AccessPolicyEntry>): JsonArray = JsonArray(
            values.map { entry ->
                JsonObject(
                    mapOf(
                        "resource" to JsonPrimitive(entry.resource),
                        "allowed_modes" to stringArray(entry.allowedModes.map { it.wireName }),
                    ),
                )
            },
        )
        fields["access_contract"] = JsonObject(
            mapOf(
                "policy" to JsonObject(
                    mapOf(
                        "policy_revision" to JsonPrimitive(policy.policyRevision),
                        "filesystem_roots" to entries(policy.filesystemRoots),
                        "process_lanes" to entries(policy.processLanes),
                        "network_origins" to entries(policy.networkOrigins),
                        "runtime_lanes" to entries(policy.runtimeLanes),
                        "max_accesses" to JsonPrimitive(policy.maxAccesses),
                        "max_result_bytes" to JsonPrimitive(policy.maxResultBytes),
                    ),
                ),
                "resolver_revision" to JsonPrimitive(value.accessResolverRevision!!),
            ),
        )
    }
    return JsonObject(fields)
}

private fun snapshotJson(value: EffectiveAgentSnapshot, includeDigest: Boolean): JsonObject {
    val fields = linkedMapOf<String, JsonElement>(
        "definition_id" to JsonPrimitive(value.definitionId),
        "definition_revision" to JsonPrimitive(value.definitionRevision),
        "definition_digest" to JsonPrimitive(value.definitionDigest),
        "instructions" to JsonArray(value.instructions.map { JsonObject(mapOf("source_id" to JsonPrimitive(it.sourceId), "exact_revision" to JsonPrimitive(it.exactRevision), "content_utf8" to JsonPrimitive(it.contentUtf8), "content_digest" to JsonPrimitive(it.contentDigest))) }),
        "model_roles" to JsonArray(value.modelRoles.map { JsonObject(mapOf("role_id" to JsonPrimitive(it.roleId), "capability_target_id" to JsonPrimitive(it.capabilityTargetId), "admitted_capabilities" to stringArray(it.admittedCapabilities))) }),
        "capabilities" to JsonObject(mapOf("tools" to JsonArray(value.capabilities.tools.map(::toolJson)), "descriptors" to JsonArray(value.capabilities.descriptors.map { descriptor -> JsonObject(mapOf("kind" to JsonPrimitive(descriptor.kind.wireName), "name" to JsonPrimitive(descriptor.name), "exact_revision" to JsonPrimitive(descriptor.exactRevision), "contract_version" to JsonPrimitive(descriptor.contractVersion), "descriptor_digest" to JsonPrimitive(descriptor.descriptorDigest))) }))),
        "governance" to JsonObject(mapOf("policy_id" to JsonPrimitive(value.governance.policyId), "exact_revision" to JsonPrimitive(value.governance.exactRevision), "allowed_requirement_capabilities" to stringArray(value.governance.allowedRequirementCapabilities), "interaction_modes" to stringArray(value.governance.interactionModes.map { it.wireName }), "default_unmatched" to JsonPrimitive(value.governance.defaultUnmatched.wireName))),
        "context_policy" to JsonObject(mapOf("policy_id" to JsonPrimitive(value.contextPolicy.policyId), "exact_revision" to JsonPrimitive(value.contextPolicy.exactRevision), "descriptor_digest" to JsonPrimitive(value.contextPolicy.descriptorDigest))),
        "limits" to effectiveLimitsJson(value.limits),
        "contract_versions" to JsonObject(value.contractVersions.mapValues { JsonPrimitive(it.value) }),
    )
    if (includeDigest) fields["snapshot_digest"] = JsonPrimitive(value.snapshotDigest)
    return JsonObject(fields)
}

private class InstructionExpansion(private val registry: ResolutionRegistry) {
    private val active: MutableSet<Pair<String, String>> = mutableSetOf()
    private val emitted: MutableSet<Pair<String, String>> = mutableSetOf()
    public val output: MutableList<ResolvedInstruction> = mutableListOf()

    public fun expand(reference: InstructionReference, path: String, cyclePath: String) {
        val key = reference.sourceId to reference.exactRevision
        if (key in emitted) return
        if (!active.add(key)) abort(ResolutionErrorCode.REFERENCE_CYCLE, cyclePath)
        val resource = exactOne(registry.instructions.filter { it.sourceId == reference.sourceId && it.exactRevision == reference.exactRevision }, reference.required, path)
        if (resource == null) { active.remove(key); return }
        resource.dependencies.forEachIndexed { index, dependency -> expand(dependency, "$path/dependencies/$index", cyclePath) }
        active.remove(key)
        emitted.add(key)
        output += ResolvedInstruction(resource.sourceId, resource.exactRevision, resource.contentUtf8, sha256(resource.contentUtf8.toByteArray(Charsets.UTF_8)))
    }
}

private fun resolveInstructions(definition: AgentDefinition, registry: ResolutionRegistry): List<ResolvedInstruction> {
    val expansion = InstructionExpansion(registry)
    definition.instructionSources.forEachIndexed { index, reference -> expansion.expand(reference, "/instruction_sources/$index", "/instruction_sources/$index") }
    return expansion.output.toList()
}

private fun resolveRoles(definition: AgentDefinition, registry: ResolutionRegistry): List<ResolvedModelRole> =
    definition.modelRoles.mapIndexedNotNull { index, requirement ->
        val path = "/model_roles/$index"
        val candidate = exactOne(registry.modelRoles.filter { it.roleId == requirement.roleId }, requirement.required, path) ?: return@mapIndexedNotNull null
        if (!candidate.admittedCapabilities.containsAll(requirement.requiredCapabilities)) {
            if (requirement.required) abort(ResolutionErrorCode.POLICY_INCOMPATIBLE, path)
            null
        } else ResolvedModelRole(requirement.roleId, candidate.capabilityTargetId, requirement.requiredCapabilities)
    }

private fun tightened(requested: Long?, cap: Long?, path: String): Long? = when {
    requested != null && cap != null && cap > requested -> abort(ResolutionErrorCode.POLICY_INCOMPATIBLE, path)
    cap != null -> cap
    else -> requested
}

/** Resolves exact Runtime candidates into one immutable Turn-bound snapshot. */
public fun resolveDefinition(definition: AgentDefinition, registry: ResolutionRegistry, policy: ProductPolicy): DefinitionResult<EffectiveAgentSnapshot> = try {
    definition.contractVersions.forEach { (name, version) -> if (version !in policy.admittedContractVersions[name].orEmpty()) abort(ResolutionErrorCode.UNSUPPORTED_CONTRACT_VERSION, "/contract_versions/$name") }
    val governance = exactOne(registry.governancePolicies.filter { it.policyId == definition.governance.policyId && it.exactRevision == definition.governance.exactRevision }, true, "/governance")!!
    val allowed = definition.governance.allowedRequirementCapabilities.filter { it in governance.allowedRequirementCapabilities && it in policy.allowedRequirementCapabilities }
    val modes = definition.governance.interactionModes.filter { it in governance.interactionModes && it in policy.interactionModes }
    val effectiveGovernance = EffectiveGovernancePolicy(governance.policyId, governance.exactRevision, allowed, modes, definition.governance.defaultUnmatched)
    val tools = mutableListOf<ToolDefinition>()
    val descriptors = mutableListOf<CapabilityDescriptor>()
    definition.capabilities.forEach { reference ->
        val path = "/capabilities/${reference.kind.wireName}/${reference.name}"
        if (reference.kind == CapabilityKind.TOOL) {
            val tool = exactOne(registry.tools.filter { it.name == reference.name && it.revision == reference.exactRevision }, reference.required, path)
            if (tool != null && tool.requirements.capabilities.all { it.wireName in allowed }) tools += tool else if (tool != null && reference.required) abort(ResolutionErrorCode.POLICY_INCOMPATIBLE, path)
        } else {
            exactOne(registry.capabilityDescriptors.filter { it.kind == reference.kind && it.name == reference.name && it.exactRevision == reference.exactRevision && it.contractVersion == reference.contractVersion }, reference.required, path)?.let(descriptors::add)
        }
    }
    val context = exactOne(registry.contextPolicies.filter { it.policyId == definition.contextPolicy.policyId && it.exactRevision == definition.contextPolicy.exactRevision }, true, "/context_policy")!!
    if (policy.limitCaps.maxIterations > definition.limits.maxIterations) abort(ResolutionErrorCode.POLICY_INCOMPATIBLE, "/limits/max_iterations")
    val limits = EffectiveLimits(policy.limitCaps.maxIterations, tightened(definition.limits.maxInputTokens, policy.limitCaps.maxInputTokens, "/limits/max_input_tokens"), tightened(definition.limits.maxOutputTokens, policy.limitCaps.maxOutputTokens, "/limits/max_output_tokens"), tightened(definition.limits.deadlineBudgetMs, policy.limitCaps.deadlineBudgetMs, "/limits/deadline_budget_ms"))
    val definitionEnvelope = JsonObject(mapOf("contract" to JsonPrimitive("garive.agent-definition"), "version" to JsonPrimitive(1), "definition" to definitionJson(definition)))
    val emptyDigest = EffectiveAgentSnapshot(definition.definitionId, definition.revision, digest(definitionEnvelope, "/definition"), resolveInstructions(definition, registry), resolveRoles(definition, registry), EffectiveCapabilitySnapshot(tools, descriptors), effectiveGovernance, ResolvedContextPolicy(context.policyId, context.exactRevision, context.descriptorDigest), limits, definition.contractVersions, "")
    val preimage = JsonObject(mapOf("contract" to JsonPrimitive("garive.effective-agent-snapshot"), "version" to JsonPrimitive(1)) + snapshotJson(emptyDigest, false))
    DefinitionResult.Success(emptyDigest.copy(snapshotDigest = digest(preimage, "/snapshot")))
} catch (failure: ResolutionAbort) {
    DefinitionResult.Failure(failure.error)
}

/** Converts an effective snapshot to its declared fixture JSON shape. */
public fun effectiveSnapshotJson(value: EffectiveAgentSnapshot): JsonObject = snapshotJson(value, true)
