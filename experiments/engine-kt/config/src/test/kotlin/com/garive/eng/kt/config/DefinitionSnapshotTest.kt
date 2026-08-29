package com.garive.eng.kt.config

import com.garive.eng.kt.tools.ExecutionCapability
import com.garive.eng.kt.tools.ExecutionRequirements
import com.garive.eng.kt.tools.ReplayClass
import com.garive.eng.kt.tools.ToolContractResult
import com.garive.eng.kt.tools.ToolDefinition
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.io.path.Path
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertTrue

class DefinitionSnapshotTest {
    private val fixture: JsonObject by lazy {
        val root = Path(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(
            root.resolve("spec/fixtures/agent/agent-definition-snapshot.json").readText(),
        ).jsonObject
    }

    private fun success(result: DefinitionResult<Any>): Any =
        (result as DefinitionResult.Success).value

    private fun strings(value: JsonElement): List<String> =
        value.jsonArray.map { it.jsonPrimitive.content }

    private fun modes(value: JsonElement): List<InteractionMode> =
        strings(value).map {
            when (it) {
                "approval" -> InteractionMode.APPROVAL
                "external_input" -> InteractionMode.EXTERNAL_INPUT
                else -> error("unknown interaction mode: $it")
            }
        }

    private fun kind(value: String): CapabilityKind = CapabilityKind.entries.single { it.wireName == value }

    private fun reference(value: JsonObject): InstructionReference =
        success(
            InstructionReference.create(
                value.getValue("source_id").jsonPrimitive.content,
                value.getValue("exact_revision").jsonPrimitive.content,
                value.getValue("required").jsonPrimitive.content.toBoolean(),
            ),
        ) as InstructionReference

    private fun limits(value: JsonObject): DefaultLimits =
        success(
            DefaultLimits.create(
                value.getValue("max_iterations").jsonPrimitive.content.toLong(),
                value["max_input_tokens"]?.jsonPrimitive?.content?.toLong(),
                value["max_output_tokens"]?.jsonPrimitive?.content?.toLong(),
                value["deadline_budget_ms"]?.jsonPrimitive?.content?.toLong(),
            ),
        ) as DefaultLimits

    private fun definition(): AgentDefinition {
        val value = fixture.getValue("definition").jsonObject
        val governance = value.getValue("governance").jsonObject
        return success(
            AgentDefinition.create(
                value.getValue("definition_id").jsonPrimitive.content,
                value.getValue("revision").jsonPrimitive.content,
                value.getValue("instruction_sources").jsonArray.map { reference(it.jsonObject) },
                value.getValue("model_roles").jsonArray.map { element ->
                    val role = element.jsonObject
                    success(
                        ModelRoleRequirement.create(
                            role.getValue("role_id").jsonPrimitive.content,
                            strings(role.getValue("required_capabilities")),
                            role.getValue("required").jsonPrimitive.content.toBoolean(),
                        ),
                    ) as ModelRoleRequirement
                },
                value.getValue("capabilities").jsonArray.map { element ->
                    val capability = element.jsonObject
                    success(
                        CapabilityReference.create(
                            kind(capability.getValue("kind").jsonPrimitive.content),
                            capability.getValue("name").jsonPrimitive.content,
                            capability.getValue("exact_revision").jsonPrimitive.content,
                            capability.getValue("contract_version").jsonPrimitive.content.toLong(),
                            capability.getValue("required").jsonPrimitive.content.toBoolean(),
                        ),
                    ) as CapabilityReference
                },
                success(
                    GovernancePolicy.create(
                        governance.getValue("policy_id").jsonPrimitive.content,
                        governance.getValue("exact_revision").jsonPrimitive.content,
                        strings(governance.getValue("allowed_requirement_capabilities")),
                        modes(governance.getValue("interaction_modes")),
                    ),
                ) as GovernancePolicy,
                success(
                    ContextPolicyReference.create(
                        value.getValue("context_policy").jsonObject.getValue("policy_id").jsonPrimitive.content,
                        value.getValue("context_policy").jsonObject.getValue("exact_revision").jsonPrimitive.content,
                    ),
                ) as ContextPolicyReference,
                limits(value.getValue("limits").jsonObject),
                value.getValue("contract_versions").jsonObject.mapValues { it.value.jsonPrimitive.content.toLong() },
            ),
        ) as AgentDefinition
    }

    private fun tool(value: JsonObject): ToolDefinition {
        val requirements = value.getValue("requirements").jsonObject
        val executionRequirements = (
            ExecutionRequirements.create(
                strings(requirements.getValue("capabilities")).map { capability ->
                    ExecutionCapability.entries.single { it.wireName == capability }
                },
                requirements.getValue("max_duration_ms").jsonPrimitive.content.toLong(),
                requirements.getValue("max_output_bytes").jsonPrimitive.content.toLong(),
            ) as ToolContractResult.Success
        ).value
        return (
            ToolDefinition.create(
                value.getValue("name").jsonPrimitive.content,
                value.getValue("revision").jsonPrimitive.content,
                value.getValue("description").jsonPrimitive.content,
                value.getValue("input_schema"),
                executionRequirements,
                ReplayClass.entries.single {
                    it.wireName == value.getValue("replay_class").jsonPrimitive.content
                },
            ) as ToolContractResult.Success
        ).value
    }

    private fun registry(): ResolutionRegistry {
        val value = fixture.getValue("registry").jsonObject
        return ResolutionRegistry(
            value.getValue("instructions").jsonArray.map { element ->
                val item = element.jsonObject
                InstructionResource(
                    item.getValue("source_id").jsonPrimitive.content,
                    item.getValue("exact_revision").jsonPrimitive.content,
                    item.getValue("content_utf8").jsonPrimitive.content,
                    item.getValue("dependencies").jsonArray.map { reference(it.jsonObject) },
                )
            },
            value.getValue("model_roles").jsonArray.map { element ->
                val item = element.jsonObject
                ModelRoleCandidate(
                    item.getValue("role_id").jsonPrimitive.content,
                    item.getValue("capability_target_id").jsonPrimitive.content,
                    strings(item.getValue("admitted_capabilities")).toSet(),
                )
            },
            value.getValue("tools").jsonArray.map { tool(it.jsonObject) },
            emptyList(),
            value.getValue("governance_policies").jsonArray.map { element ->
                val item = element.jsonObject
                GovernancePolicyCandidate(
                    item.getValue("policy_id").jsonPrimitive.content,
                    item.getValue("exact_revision").jsonPrimitive.content,
                    strings(item.getValue("allowed_requirement_capabilities")).toSet(),
                    modes(item.getValue("interaction_modes")).toSet(),
                )
            },
            value.getValue("context_policies").jsonArray.map { element ->
                val item = element.jsonObject
                ContextPolicyCandidate(
                    item.getValue("policy_id").jsonPrimitive.content,
                    item.getValue("exact_revision").jsonPrimitive.content,
                    item.getValue("descriptor_digest").jsonPrimitive.content,
                )
            },
        )
    }

    private fun productPolicy(): ProductPolicy {
        val value = fixture.getValue("product_policy").jsonObject
        return ProductPolicy(
            strings(value.getValue("allowed_requirement_capabilities")).toSet(),
            modes(value.getValue("interaction_modes")).toSet(),
            limits(value.getValue("limit_caps").jsonObject),
            value.getValue("admitted_contract_versions").jsonObject.mapValues {
                it.value.jsonArray.map { version -> version.jsonPrimitive.content.toLong() }.toSet()
            },
        )
    }

    private fun rebuild(
        value: AgentDefinition,
        instructions: List<InstructionReference> = value.instructionSources,
        contracts: Map<String, Long> = value.contractVersions,
    ): DefinitionResult<AgentDefinition> = AgentDefinition.create(
        value.definitionId,
        value.revision,
        instructions,
        value.modelRoles,
        value.capabilities,
        value.governance,
        value.contextPolicy,
        value.limits,
        contracts,
    )

    @Test
    fun sharedExactResolutionMatchesSnapshot() {
        val result = resolveDefinition(definition(), registry(), productPolicy())
        val snapshot = (result as DefinitionResult.Success).value
        assertEquals(fixture.getValue("expected_snapshot"), effectiveSnapshotJson(snapshot))
    }

    @Test
    fun sharedFailureOperationsFailClosed() {
        fixture.getValue("failure_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            var definitionResult: DefinitionResult<AgentDefinition> = DefinitionResult.Success(definition())
            var registry = registry()
            var policy = productPolicy()
            case.getValue("operations").jsonArray.forEach { operationElement ->
                val operation = operationElement.jsonObject
                val operationKind = operation.getValue("kind").jsonPrimitive.content
                val current = (definitionResult as? DefinitionResult.Success)?.value
                when (operationKind) {
                    "remove_instruction" -> registry = registry.copy(instructions = registry.instructions.filterNot { it.sourceId == operation.getValue("source_id").jsonPrimitive.content && it.exactRevision == operation.getValue("exact_revision").jsonPrimitive.content })
                    "duplicate_instruction" -> registry = registry.copy(instructions = registry.instructions + registry.instructions.first { it.sourceId == operation.getValue("source_id").jsonPrimitive.content && it.exactRevision == operation.getValue("exact_revision").jsonPrimitive.content })
                    "add_dependency" -> registry = registry.copy(instructions = registry.instructions.map { resource -> if (resource.sourceId == operation.getValue("source_id").jsonPrimitive.content && resource.exactRevision == operation.getValue("exact_revision").jsonPrimitive.content) resource.copy(dependencies = resource.dependencies + (success(InstructionReference.create(operation.getValue("dependency_source_id").jsonPrimitive.content, operation.getValue("dependency_revision").jsonPrimitive.content, true)) as InstructionReference)) else resource })
                    "set_contract_version" -> definitionResult = rebuild(current!!, contracts = current.contractVersions + (operation.getValue("contract_name").jsonPrimitive.content to operation.getValue("version").jsonPrimitive.content.toLong()))
                    "remove_product_requirement_capability" -> policy = policy.copy(allowedRequirementCapabilities = policy.allowedRequirementCapabilities - operation.getValue("capability").jsonPrimitive.content)
                    "duplicate_instruction_root" -> definitionResult = rebuild(current!!, instructions = current.instructionSources + (success(InstructionReference.create(operation.getValue("source_id").jsonPrimitive.content, operation.getValue("exact_revision").jsonPrimitive.content, true)) as InstructionReference))
                    "set_product_max_iterations" -> policy = policy.copy(limitCaps = success(DefaultLimits.create(operation.getValue("value").jsonPrimitive.content.toLong(), policy.limitCaps.maxInputTokens, policy.limitCaps.maxOutputTokens, policy.limitCaps.deadlineBudgetMs)) as DefaultLimits)
                    else -> error("unknown fixture operation: $operationKind")
                }
            }
            val error = when (definitionResult) {
                is DefinitionResult.Failure -> definitionResult.error
                is DefinitionResult.Success -> (resolveDefinition(definitionResult.value, registry, policy) as DefinitionResult.Failure).error
            }
            assertEquals(case.getValue("expected_code").jsonPrimitive.content, error.code.wireName)
            assertEquals(case.getValue("expected_path").jsonPrimitive.content, error.path)
        }
    }

    @Test
    fun continuationAndLimitPropertiesHold() {
        val definition = definition()
        val registry = registry()
        var policy = productPolicy()
        val snapshot = (resolveDefinition(definition, registry, policy) as DefinitionResult.Success).value
        fixture.getValue("continuation_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            assertEquals(
                case.getValue("expected").jsonPrimitive.content == "accepted",
                snapshot.validateContinuation(case.getValue("definition_revision").jsonPrimitive.content, case.getValue("snapshot_digest").jsonPrimitive.content) is DefinitionResult.Success,
            )
        }
        for (cap in 1..definition.limits.maxIterations) {
            policy = policy.copy(limitCaps = success(DefaultLimits.create(cap, policy.limitCaps.maxInputTokens, policy.limitCaps.maxOutputTokens, policy.limitCaps.deadlineBudgetMs)) as DefaultLimits)
            val first = (resolveDefinition(definition, registry, policy) as DefinitionResult.Success).value
            val second = (resolveDefinition(definition, registry, policy) as DefinitionResult.Success).value
            assertEquals(first, second)
            assertTrue(first.limits.maxIterations <= definition.limits.maxIterations)
        }
    }
}
