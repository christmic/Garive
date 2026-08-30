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
}
