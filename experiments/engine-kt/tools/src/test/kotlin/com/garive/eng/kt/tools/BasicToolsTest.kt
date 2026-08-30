package com.garive.eng.kt.tools

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertTrue

class BasicToolsTest {
    @Test
    fun catalogueFreezesAllFiveV3Definitions() {
        val catalogue = catalogue()
        assertEquals(
            listOf(T1_PROCESS_RUN, T1_APPLY_PATCH, T1_LIST, T1_READ_TEXT, T1_SEARCH_TEXT),
            catalogue.definitions.map(ToolDefinition::name),
        )
        assertTrue(catalogue.definitions.all { it.revision == T1_TOOL_REVISION && it.preparedContractVersion == 3 })
        assertEquals(ReplayClass.NEVER_REPLAY, catalogue.definitions.first().replayClass)
        assertEquals(ReplayClass.RECEIPT_RECOVERABLE, catalogue.definitions[1].replayClass)
    }

    @Test
    fun readListAndSearchResolveExactWorkspaceAccess() {
        val catalogue = catalogue()
        listOf(
            Triple(T1_READ_TEXT, """{"path":"src/lib.rs","max_bytes":4096}""", "src/lib.rs"),
            Triple(T1_LIST, """{"path":".","max_entries":10,"include_hidden":false}""", "."),
            Triple(
                T1_SEARCH_TEXT,
                """{"path":"src","query":"needle","case_sensitive":true,"max_matches":10,"max_file_bytes":4096}""",
                "src",
            ),
        ).forEach { (name, arguments, key) ->
            val prepared = catalogue.prepare(ToolIntent("call", name, arguments)).value()
            val access = requireNotNull(prepared.invocationAccesses).values.single()
            assertEquals(AccessNamespace.FILESYSTEM, access.namespace)
            assertEquals(key, access.resourceKey)
            assertEquals(AccessMode.READ, access.mode)
        }
    }

    @Test
    fun patchAndProcessBindOnlyDeclaredResources() {
        val catalogue = catalogue()
        val digest = "a".repeat(64)
        val prepared = catalogue.prepare(
            ToolIntent(
                "call",
                T1_APPLY_PATCH,
                """{"patch":"*** Begin Patch\n*** Update File: src/lib.rs\n@@\n-old\n+new\n*** End Patch","expected_files":[{"path":"src/lib.rs","before_digest":"$digest"}]}""",
            ),
        ).value()
        assertEquals("src/lib.rs", requireNotNull(prepared.invocationAccesses).values.single().resourceKey)

        val process = catalogue.prepare(
            ToolIntent(
                "call",
                T1_PROCESS_RUN,
                """{"lane":"rust-toolchain","argv":["cargo","test"],"working_directory":".","max_output_bytes":4096,"timeout_ms":30000}""",
            ),
        ).value()
        assertEquals(
            listOf(AccessNamespace.FILESYSTEM, AccessNamespace.PROCESS),
            requireNotNull(process.invocationAccesses).values.map(ResourceAccess::namespace),
        )
    }

    @Test
    fun hostileRootsTargetsAndUnconfiguredLanesFailClosed() {
        val catalogue = catalogue()
        val rootRead = catalogue.prepare(
            ToolIntent("call", T1_READ_TEXT, """{"path":".","max_bytes":1}"""),
        )
        assertEquals(PreparationErrorCode.EFFECT_ACCESS_INVALID, rootRead.error())
        val unknownLane = catalogue.prepare(
            ToolIntent(
                "call",
                T1_PROCESS_RUN,
                """{"lane":"unknown","argv":["cargo"],"working_directory":".","max_output_bytes":1,"timeout_ms":1}""",
            ),
        )
        assertEquals(PreparationErrorCode.EFFECT_ACCESS_INVALID, unknownLane.error())
    }

    private fun catalogue(): BuiltinT1Catalogue =
        assertIs<ToolContractResult.Success<BuiltinT1Catalogue>>(
            BuiltinT1Catalogue.create("snapshot-1", listOf("rust-toolchain")),
        ).value

    private fun ToolContractResult<PreparedToolCall>.value(): PreparedToolCall =
        assertIs<ToolContractResult.Success<PreparedToolCall>>(this).value

    private fun ToolContractResult<PreparedToolCall>.error(): PreparationErrorCode =
        assertIs<ToolContractResult.Failure>(this).error.code
}
