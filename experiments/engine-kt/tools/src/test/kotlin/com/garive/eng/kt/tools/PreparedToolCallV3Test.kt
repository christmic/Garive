package com.garive.eng.kt.tools

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class PreparedToolCallV3Test {
    private val expected = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/sandbox-safety-v1.json").readText(),
    ).jsonObject.getValue("prepared_v3").jsonObject

    private object PathResolver : ToolAccessResolver {
        override val revision: String = "path-resolver-v1"

        override fun resolve(arguments: JsonElement): ToolContractResult<InvocationAccessSet> =
            InvocationAccessSet.create(
                listOf(
                    ResourceAccess.create(
                        AccessNamespace.FILESYSTEM,
                        arguments.jsonObject.getValue("path").jsonPrimitive.content,
                        AccessMode.READ,
                    ).value(),
                ),
            )
    }

    @Test
    fun preparedV3MatchesRustBindingsAndDigest() {
        val catalog = ToolCatalog.create(listOf(definition(filesystemProfile()))).value()
        val prepared = catalog.prepareV3(
            ToolIntent("call-1", "read_file", """{"path":"src/lib.rs"}"""),
            PathResolver,
        ).value()
        assertEquals(3, prepared.contractVersion)
        assertEquals(
            expected.getValue("sandbox_requirements_digest").jsonPrimitive.content,
            prepared.sandboxRequirementsDigest,
        )
        assertEquals(
            expected.getValue("prepared_digest").jsonPrimitive.content,
            prepared.inputDigest,
        )
    }

    @Test
    fun definitionRevalidatesProfileAndV2CannotPrepareV3() {
        val processProfile = SandboxRequirementsV1.create(
            listOf(ExecutionCapability.PROCESS),
            listOf(
                SandboxControl.PROCESS_CONTAINMENT,
                SandboxControl.STRUCTURED_ARGUMENTS,
                SandboxControl.ENVIRONMENT_ALLOWLIST,
                SandboxControl.RESOURCE_LIMITS,
            ),
            1,
            8,
        ).value()
        assertEquals(
            PreparationErrorCode.SANDBOX_REQUIREMENT_INVALID,
            assertIs<ToolContractResult.Failure>(definitionResult(processProfile)).error.code,
        )
        val v3 = ToolCatalog.create(listOf(definition(filesystemProfile()))).value()
        assertEquals(
            PreparationErrorCode.SANDBOX_REQUIREMENT_INVALID,
            assertIs<ToolContractResult.Failure>(
                v3.prepareV2(
                    ToolIntent("call-1", "read_file", """{"path":"src/lib.rs"}"""),
                    PathResolver,
                ),
            ).error.code,
        )
    }

    private fun filesystemProfile(): SandboxRequirementsV1 = SandboxRequirementsV1.create(
        listOf(ExecutionCapability.FILESYSTEM_READ),
        listOf(
            SandboxControl.FILESYSTEM_SCOPE,
            SandboxControl.SYMLINK_CONTAINMENT,
            SandboxControl.RESOURCE_LIMITS,
        ),
        null,
        8,
    ).value()

    private fun definition(profile: SandboxRequirementsV1): ToolDefinition = definitionResult(profile).value()

    private fun definitionResult(profile: SandboxRequirementsV1): ToolContractResult<ToolDefinition> =
        ToolDefinition.createV3(
            "read_file",
            "read-file-v3",
            "Read one file",
            Json.parseToJsonElement(
                """{"type":"object","properties":{"path":{"type":"string"}},"required":["path"],"additionalProperties":false}""",
            ),
            ExecutionRequirements.create(listOf(ExecutionCapability.FILESYSTEM_READ), 1_000, 4_096).value(),
            ReplayClass.READ_ONLY,
            ToolAccessPolicyV1.create(
                "read-policy-v1",
                listOf(AccessPolicyEntry.create("src", listOf(AccessMode.READ)).value()),
                emptyList(),
                emptyList(),
                emptyList(),
                1,
                2_048,
            ).value(),
            "path-resolver-v1",
            profile,
        )
}

private fun <T> ToolContractResult<T>.value(): T = assertIs<ToolContractResult.Success<T>>(this).value
