package com.garive.eng.kt.tools

import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.test.assertNotEquals
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put

class ToolCatalogueDigestTest {
    @Test
    fun catalogueDigestMatchesRustAndRejectsDuplicateNames() {
        val alpha = definition("alpha", "Read alpha")
        val beta = definition("beta", "Read beta")
        val forward = ToolCatalog.digest(listOf(alpha, beta)).value()
        assertEquals("92f7d0a98d5654275256bb4079da8a554992e1488b5f77ec95c297ca20bc7d93", forward)
        assertEquals(forward, ToolCatalog.digest(listOf(beta, alpha)).value())
        assertNotEquals(forward, ToolCatalog.digest(listOf(alpha, definition("beta", "Changed meaning"))).value())
        assertEquals(
            PreparationErrorCode.INVALID_TOOL_DEFINITION,
            assertIs<ToolContractResult.Failure>(
                ToolCatalog.digest(listOf(definition("duplicate", "One"), definition("duplicate", "Two"))),
            ).error.code,
        )
    }

    private fun definition(name: String, description: String): ToolDefinition =
        ToolDefinition.create(
            name,
            "v1",
            description,
            buildJsonObject {
                put("type", "object")
                put("properties", buildJsonObject {})
                put("required", kotlinx.serialization.json.JsonArray(emptyList()))
                put("additionalProperties", false)
            },
            ExecutionRequirements.create(listOf(ExecutionCapability.FILESYSTEM_READ), 1_000L, 1_000L).value(),
            ReplayClass.READ_ONLY,
        ).value()

    private fun <T> ToolContractResult<T>.value(): T = assertIs<ToolContractResult.Success<T>>(this).value
}
