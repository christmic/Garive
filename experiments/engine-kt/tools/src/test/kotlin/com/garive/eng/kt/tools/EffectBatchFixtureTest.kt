package com.garive.eng.kt.tools

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import org.junit.jupiter.api.Assertions.assertEquals
import org.junit.jupiter.api.Assertions.assertFalse
import org.junit.jupiter.api.Assertions.assertTrue
import org.junit.jupiter.api.Test

class EffectBatchFixtureTest {
    private val fixture: JsonObject = Json.parseToJsonElement(
        File(
            System.getProperty("garive.repo.root"),
            "spec/fixtures/agent/deterministic-effect-batches-v1.json",
        ).readText(),
    ).jsonObject

    private class ExactResolver(private val mode: AccessMode) : ToolAccessResolver {
        override val revision: String = "exact-v1"

        override fun resolve(arguments: JsonElement): ToolContractResult<InvocationAccessSet> {
            val access = ResourceAccess.create(
                AccessNamespace.FILESYSTEM,
                arguments.jsonObject.getValue("path").jsonPrimitive.content,
                mode,
            ).value()
            return InvocationAccessSet.create(listOf(access))
        }
    }

    private fun definition(path: String, mode: AccessMode, replay: ReplayClass): ToolDefinition {
        val template = fixture.getValue("definition_template").jsonObject
        val requirements = template.getValue("requirements").jsonObject
        val policy = template.getValue("access_policy").jsonObject
        val capability = if (mode == AccessMode.READ) {
            ExecutionCapability.FILESYSTEM_READ
        } else {
            ExecutionCapability.FILESYSTEM_WRITE
        }
        return ToolDefinition.createV2(
            "tool_$path",
            template.getValue("tool_revision").jsonPrimitive.content,
            template.getValue("description").jsonPrimitive.content,
            template.getValue("input_schema"),
            ExecutionRequirements.create(
                listOf(capability),
                requirements.getValue("max_duration_ms").jsonPrimitive.long,
                requirements.getValue("max_output_bytes").jsonPrimitive.long,
            ).value(),
            replay,
            ToolAccessPolicyV1.create(
                policy.getValue("policy_revision").jsonPrimitive.content,
                listOf(AccessPolicyEntry.create(policy.getValue("filesystem_root").jsonPrimitive.content, listOf(mode)).value()),
                emptyList(),
                emptyList(),
                emptyList(),
                policy.getValue("max_accesses").jsonPrimitive.int,
                policy.getValue("max_result_bytes").jsonPrimitive.long,
            ).value(),
            template.getValue("resolver_revision").jsonPrimitive.content,
        ).value()
    }

    private fun prepared(path: String, mode: AccessMode, replay: ReplayClass): PreparedToolCall =
        ToolCatalog.create(listOf(definition(path, mode, replay))).value().prepareV2(
            ToolIntent("call-$path", "tool_$path", "{\"path\":\"$path\"}"),
            ExactResolver(mode),
        ).value()

    @Test
    fun `shared fixture has strict canonical access semantics`() {
        assertKeys(
            fixture,
            "schema_version",
            "definition_template",
            "normalization_cases",
            "policy_cases",
            "plan_case",
            "failure_cases",
        )
        assertEquals(1, fixture.getValue("schema_version").jsonPrimitive.int)
        val template = fixture.getValue("definition_template").jsonObject
        assertKeys(
            template,
            "tool_revision",
            "description",
            "input_schema",
            "requirements",
            "access_policy",
            "resolver_revision",
        )
        assertKeys(template.getValue("requirements").jsonObject, "max_duration_ms", "max_output_bytes")
        assertKeys(
            template.getValue("access_policy").jsonObject,
            "policy_revision",
            "filesystem_root",
            "max_accesses",
            "max_result_bytes",
        )
        fixture.getValue("normalization_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            assertKeys(case, "name", "namespace", "resource_key", "mode", "valid")
            val namespace = when (case.getValue("namespace").jsonPrimitive.content) {
                "filesystem" -> AccessNamespace.FILESYSTEM
                "network" -> AccessNamespace.NETWORK
                else -> error("unknown fixture namespace")
            }
            val result = ResourceAccess.create(
                namespace,
                case.getValue("resource_key").jsonPrimitive.content,
                AccessMode.READ,
            )
            assertEquals(case.getValue("valid").jsonPrimitive.boolean, result is ToolContractResult.Success)
        }

        val policy = ToolAccessPolicyV1.create(
            "policy-v1",
            listOf(AccessPolicyEntry.create("src", listOf(AccessMode.READ)).value()),
            emptyList(),
            emptyList(),
            emptyList(),
            1,
            512,
        ).value()
        fixture.getValue("policy_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            assertKeys(case, "name", "resource_key", "covered")
            val accesses = InvocationAccessSet.create(
                listOf(
                    ResourceAccess.create(
                        AccessNamespace.FILESYSTEM,
                        case.getValue("resource_key").jsonPrimitive.content,
                        AccessMode.READ,
                    ).value(),
                ),
            ).value()
            assertEquals(case.getValue("covered").jsonPrimitive.boolean, policy.covers(accesses))
        }
    }

    @Test
    fun `shared fixture produces byte-identical graph and plan digests`() {
        val case = fixture.getValue("plan_case").jsonObject
        assertKeys(
            case,
            "name",
            "calls",
            "limits",
            "conflict_graph_bytes",
            "conflict_graph_digest",
            "steps",
            "plan_digest",
        )
        val calls = case.getValue("calls").jsonArray.map { element ->
            val call = element.jsonObject
            assertKeys(call, "path", "mode", "replay_class", "prepared_digest")
            val mode = if (call.getValue("mode").jsonPrimitive.content == "read") AccessMode.READ else AccessMode.WRITE
            val replay = if (call.getValue("replay_class").jsonPrimitive.content == "read_only") {
                ReplayClass.READ_ONLY
            } else {
                ReplayClass.IDEMPOTENT
            }
            prepared(call.getValue("path").jsonPrimitive.content, mode, replay).also {
                assertEquals(call.getValue("prepared_digest").jsonPrimitive.content, it.inputDigest)
            }
        }
        val limits = case.getValue("limits").jsonObject
        assertKeys(
            limits,
            "max_intents",
            "max_accesses_per_intent",
            "max_total_accesses",
            "max_parallel_reads",
            "max_buffered_result_bytes",
        )
        val plan = planEffectBatch(
            calls,
            EffectBatchLimitsV1.create(
                limits.getValue("max_intents").jsonPrimitive.int,
                limits.getValue("max_accesses_per_intent").jsonPrimitive.int,
                limits.getValue("max_total_accesses").jsonPrimitive.int,
                limits.getValue("max_parallel_reads").jsonPrimitive.int,
                limits.getValue("max_buffered_result_bytes").jsonPrimitive.long,
            ).value(),
        ).value()
        assertEquals(
            case.getValue("conflict_graph_bytes").jsonArray.map { it.jsonPrimitive.int },
            plan.conflictGraphBytes,
        )
        assertEquals(case.getValue("conflict_graph_digest").jsonPrimitive.content, plan.conflictGraphDigest)
        assertEquals(case.getValue("plan_digest").jsonPrimitive.content, plan.planDigest)
        assertEquals(
            listOf(
                EffectBatchStep.ParallelReadGroup(listOf(0, 1, 2)),
                EffectBatchStep.SequentialStep(3),
            ),
            plan.steps,
        )
    }

    @Test
    fun `shared failure families stay closed`() {
        val expected = fixture.getValue("failure_cases").jsonArray.map { element ->
            element.jsonObject.also { assertKeys(it, "name", "expected_code") }
                .getValue("expected_code").jsonPrimitive.content
        }
        val actual = mutableListOf<String>()
        actual += EffectBatchLimitsV1.create(0, 1, 1, 1, 1).failure().error.code.wireName

        val legacy = ToolDefinition.create(
            "legacy",
            "v1",
            "Legacy",
            Json.parseToJsonElement(
                """{"type":"object","properties":{},"required":[],"additionalProperties":false}""",
            ),
            ExecutionRequirements.create(listOf(ExecutionCapability.FILESYSTEM_READ), 1, 1).value(),
            ReplayClass.READ_ONLY,
        ).value()
        val legacyCall = ToolCatalog.create(listOf(legacy)).value()
            .prepare(ToolIntent("call", "legacy", "{}")).value()
        actual += planEffectBatch(
            listOf(legacyCall),
            EffectBatchLimitsV1.create(1, 1, 1, 1, 1).value(),
        ).failure().error.code.wireName

        val wrongResolver = object : ToolAccessResolver {
            override val revision: String = "wrong"
            override fun resolve(arguments: JsonElement): ToolContractResult<InvocationAccessSet> =
                error("resolver must not run after revision mismatch")
        }
        val wrong = ToolCatalog.create(listOf(definition("src/a", AccessMode.READ, ReplayClass.READ_ONLY)))
            .value()
            .prepareV2(
                ToolIntent("call", "tool_src/a", "{\"path\":\"src/a\"}"),
                wrongResolver,
            ) as ToolContractResult.Failure
        actual += wrong.error.code.wireName

        assertEquals(expected, actual)
        assertEquals(
            listOf("effect_batch_bound_exceeded", "effect_access_invalid", "effect_access_invalid"),
            expected,
        )
        assertTrue(expected.all { it in setOf("effect_batch_bound_exceeded", "effect_access_invalid") })
        assertFalse(expected.isEmpty())
    }
}

private fun assertKeys(value: JsonObject, vararg expected: String) {
    assertEquals(expected.toSet(), value.keys)
}

private fun <T> ToolContractResult<T>.value(): T = (this as ToolContractResult.Success).value

private fun <T> EffectBatchResult<T>.value(): T = (this as EffectBatchResult.Success).value

private fun EffectBatchResult<*>.failure(): EffectBatchResult.Failure = this as EffectBatchResult.Failure
