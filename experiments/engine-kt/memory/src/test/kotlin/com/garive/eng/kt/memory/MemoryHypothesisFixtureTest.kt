package com.garive.eng.kt.memory

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.contentOrNull
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse

public class MemoryHypothesisFixtureTest {
    private val root: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/memory-hypothesis-lifecycle-v1.json").readText(),
    ).jsonObject

    @Test
    public fun sharedRegistryAndImportsAreExact(): Unit {
        val registry = registry()
        root.values("imports").forEach { value ->
            val binding = MemoryAuthorityBinding.create(
                authority(value.text("authority")), value.optional("receipt_digest"),
            ).success()
            val imported = importM0Classification(role(value.text("m0_kind")), binding)
            assertEquals(type(value.text("expected_type")), imported.memoryType)
            assertEquals(role(value.text("expected_role")), imported.role)
            assertEquals(true, registry.admits(imported.memoryType, imported.role, binding.authority))
        }
    }

    @Test
    public fun sharedInvalidCasesFailClosed(): Unit {
        val registry = registry()
        root.values("invalid").forEach { value ->
            val actual = when (value.text("name")) {
                "user_without_receipt", "agent_with_receipt" -> MemoryAuthorityBinding.create(
                    authority(value.text("authority")), value.optional("receipt_digest"),
                ).failure().code.wireName
                "platform_without_policy" -> MemoryScopeBinding.create(
                    MemoryScopeClass.PLATFORM, null,
                ).failure().code.wireName
                "unsupported_pair" -> {
                    assertFalse(registry.admits(type(value.text("type")), role(value.text("role")), authority(value.text("authority"))))
                    MemoryErrorCode.UNKNOWN_MEMORY_TYPE.wireName
                }
                else -> error("unknown case")
            }
            assertEquals(value.text("expected"), actual, value.text("name"))
        }
    }

    @Test
    public fun registryOrderAndPlatformPolicyAreStrict(): Unit {
        val rows = descriptors().toMutableList().also { it[0] = it[1].also { row -> it[1] = it[0] } }
        assertEquals(MemoryErrorCode.UNKNOWN_MEMORY_TYPE, MemoryTypeRegistry.create("r", rows).failure().code)
        assertEquals(MemoryScopeClass.PLATFORM, MemoryScopeBinding.create(MemoryScopeClass.PLATFORM, "b".repeat(64)).success().scope)
        assertEquals(MemoryErrorCode.INVALID_MEMORY, MemoryScopeBinding.create(MemoryScopeClass.PROJECT, "b".repeat(64)).failure().code)
    }

    private fun registry(): MemoryTypeRegistry = MemoryTypeRegistry.create(
        root.getValue("registry").jsonObject.text("revision"), descriptors(),
    ).success()

    private fun descriptors(): List<MemoryTypeDescriptor> =
        root.getValue("registry").jsonObject.values("descriptors").map { value ->
            MemoryTypeDescriptor.create(
                type(value.text("type")), value.strings("roles").map(::role),
                value.strings("authorities").map(::authority), value.text("lifecycle"),
                value.text("recall"), value.text("retention"), value.text("surface_kind"),
            ).success()
        }
}

private fun type(value: String): MemoryType = MemoryType.entries.first { it.wireName == value }
private fun role(value: String): MemoryKind = MemoryKind.entries.first { it.wireName == value }
private fun authority(value: String): MemoryAuthority = MemoryAuthority.entries.first { it.wireName == value }
private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
private fun JsonObject.optional(key: String): String? = get(key)?.jsonPrimitive?.contentOrNull
private fun JsonObject.values(key: String): List<JsonObject> = getValue(key).jsonArray.map { it.jsonObject }
private fun JsonObject.strings(key: String): List<String> = getValue(key).jsonArray.map { it.jsonPrimitive.content }
private fun <T> MemoryContractResult<T>.success(): T = when (this) {
    is MemoryContractResult.Success -> value
    is MemoryContractResult.Failure -> error("unexpected failure: $error")
}
private fun MemoryContractResult<*>.failure(): MemoryError = when (this) {
    is MemoryContractResult.Success -> error("unexpected success: $value")
    is MemoryContractResult.Failure -> error
}
