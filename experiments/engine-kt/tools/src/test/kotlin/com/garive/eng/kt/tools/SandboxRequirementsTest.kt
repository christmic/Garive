package com.garive.eng.kt.tools

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class SandboxRequirementsTest {
    private val fixture: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/sandbox-safety-v1.json").readText(),
    ).jsonObject

    @Test
    fun filesystemProfileRequiresEveryControl() {
        val result = SandboxRequirementsV1.create(
            listOf(ExecutionCapability.FILESYSTEM_READ),
            listOf(SandboxControl.FILESYSTEM_SCOPE, SandboxControl.RESOURCE_LIMITS),
            null,
            8,
        )
        assertEquals(
            PreparationErrorCode.SANDBOX_REQUIREMENT_INVALID,
            assertIs<ToolContractResult.Failure>(result).error.code,
        )
    }

    @Test
    fun canonicalProfileMatchesRustDigestAndCoverage() {
        val requested = filesystemProfile(16)
        val executor = filesystemProfile(8)
        assertTrue(requested.isCoveredBy(executor))
        assertFalse(executor.isCoveredBy(requested))
        assertEquals(
            "ee3658a7b9788d184f0f97b9b611826416cf546b0786a775f9ba339c18d9e611",
            assertIs<ToolContractResult.Success<String>>(executor.digest()).value,
        )
    }

    @Test
    fun sharedFixtureHasCanonicalProfileAndFailures() {
        assertEquals(1, fixture.getValue("schema_version").jsonPrimitive.int)
        fixture.getValue("profiles").jsonArray.forEach {
            val profile = it.jsonObject
            val value = assertIs<ToolContractResult.Success<SandboxRequirementsV1>>(profileFromJson(profile)).value
            assertEquals(
                profile.getValue("canonical_controls").jsonArray.map { control -> control.jsonPrimitive.content },
                value.controls.map(SandboxControl::wireName),
            )
            assertEquals(
                profile.getValue("digest").jsonPrimitive.content,
                assertIs<ToolContractResult.Success<String>>(value.digest()).value,
            )
        }
        fixture.getValue("invalid_profiles").jsonArray.forEach {
            assertEquals(
                PreparationErrorCode.SANDBOX_REQUIREMENT_INVALID,
                assertIs<ToolContractResult.Failure>(profileFromJson(it.jsonObject)).error.code,
            )
        }
    }

    private fun filesystemProfile(maxOpenFiles: Int): SandboxRequirementsV1 =
        assertIs<ToolContractResult.Success<SandboxRequirementsV1>>(
            SandboxRequirementsV1.create(
                listOf(ExecutionCapability.FILESYSTEM_READ),
                listOf(
                    SandboxControl.RESOURCE_LIMITS,
                    SandboxControl.SYMLINK_CONTAINMENT,
                    SandboxControl.FILESYSTEM_SCOPE,
                ),
                null,
                maxOpenFiles,
            ),
        ).value

    private fun profileFromJson(value: JsonObject): ToolContractResult<SandboxRequirementsV1> =
        SandboxRequirementsV1.create(
            value.getValue("capabilities").jsonArray.map {
                when (it.jsonPrimitive.content) {
                    "filesystem_read" -> ExecutionCapability.FILESYSTEM_READ
                    "process" -> ExecutionCapability.PROCESS
                    "browser_observe" -> ExecutionCapability.BROWSER_OBSERVE
                    "computer_act" -> ExecutionCapability.COMPUTER_ACT
                    else -> error("unknown fixture capability")
                }
            },
            value.getValue("controls").jsonArray.map {
                SandboxControl.entries.single { control -> control.wireName == it.jsonPrimitive.content }
            },
            value["max_processes"]?.jsonPrimitive?.contentOrNull?.toInt(),
            value.getValue("max_open_files").jsonPrimitive.int,
        )
}
