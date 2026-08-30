package com.garive.eng.kt.tools

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue

class AccessTypesTest {
    @Test
    fun workspaceRootIsTheOnlyAdmittedDotFilesystemIdentity() {
        val root = assertIs<ToolContractResult.Success<ResourceAccess>>(
            ResourceAccess.create(AccessNamespace.FILESYSTEM, ".", AccessMode.READ),
        )
        assertEquals(".", root.value.resourceKey)
        assertTrue(
            ResourceAccess.create(AccessNamespace.FILESYSTEM, "src/./secret", AccessMode.READ)
                is ToolContractResult.Failure,
        )
        assertTrue(
            ResourceAccess.create(AccessNamespace.FILESYSTEM, "..", AccessMode.READ)
                is ToolContractResult.Failure,
        )
    }

    @Test
    fun workspaceRootPolicyCoversEveryValidRelativeKey() {
        val root = assertIs<ToolContractResult.Success<AccessPolicyEntry>>(
            AccessPolicyEntry.create(".", listOf(AccessMode.READ)),
        ).value
        val policy = assertIs<ToolContractResult.Success<ToolAccessPolicyV1>>(
            ToolAccessPolicyV1.create("workspace-v1", listOf(root), emptyList(), emptyList(), emptyList(), 1, 4096),
        ).value
        listOf(".", "src", "src/lib.rs").forEach { key ->
            val resource = assertIs<ToolContractResult.Success<ResourceAccess>>(
                ResourceAccess.create(AccessNamespace.FILESYSTEM, key, AccessMode.READ),
            ).value
            val accesses = assertIs<ToolContractResult.Success<InvocationAccessSet>>(
                InvocationAccessSet.create(listOf(resource)),
            ).value
            assertTrue(policy.covers(accesses), key)
        }
    }
}
