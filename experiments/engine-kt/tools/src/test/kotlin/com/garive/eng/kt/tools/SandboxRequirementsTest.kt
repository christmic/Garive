package com.garive.eng.kt.tools

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertIs
import kotlin.test.assertTrue

class SandboxRequirementsTest {
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
}
