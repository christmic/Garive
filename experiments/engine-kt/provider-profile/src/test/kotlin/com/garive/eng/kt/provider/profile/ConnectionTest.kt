package com.garive.eng.kt.provider.profile

import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

public class ConnectionTest {
    private val fixture: JsonObject = Json.parseToJsonElement(
        Path.of(System.getProperty("garive.repo.root"), "spec/fixtures/providers/vendor-connection-profiles-v1.json")
            .toFile().readText(),
    ).jsonObject

    @Test
    public fun `secret validation and diagnostics fail closed`(): Unit {
        assertEquals(
            VendorProfileError.EMPTY_CREDENTIAL,
            assertFailsWith<VendorProfileException> { SecretValue.create("") }.error,
        )
        assertEquals(
            VendorProfileError.INVALID_CREDENTIAL,
            assertFailsWith<VendorProfileException> { SecretValue.create("secret\nvalue") }.error,
        )
        assertFalse(SecretValue.create("fixture-secret").toString().contains("fixture-secret"))
    }

    @Test
    public fun `endpoint duplicate and reserved headers are rejected`(): Unit {
        val secret = SecretValue.create("fixture-secret")
        assertEquals(
            VendorProfileError.INVALID_ENDPOINT,
            assertFailsWith<VendorProfileException> {
                ConnectionInput(EndpointSelection.Explicit("/responses"), secret, emptyList())
                    .resolve("https://default.test/responses", emptySet())
            }.error,
        )
        val duplicate = listOf(
            ExplicitHeader.create("x-extra", "one", false),
            ExplicitHeader.create("X-Extra", "two", false),
        )
        assertEquals(
            VendorProfileError.DUPLICATE_HEADER,
            assertFailsWith<VendorProfileException> {
                ConnectionInput(EndpointSelection.Default, secret, duplicate)
                    .resolve("https://default.test/responses", emptySet())
            }.error,
        )
        assertEquals(
            VendorProfileError.RESERVED_HEADER,
            assertFailsWith<VendorProfileException> {
                ConnectionInput(EndpointSelection.Default, secret, duplicate.take(1))
                    .resolve("https://default.test/responses", setOf("x-extra"))
            }.error,
        )
    }

    @Test
    public fun `shared generic failures return stable codes`(): Unit {
        fixture["failure_cases"]!!.jsonArray.map { it.jsonObject }.forEach { case ->
            val name = case["name"]!!.jsonPrimitive.content
            val error = when (name) {
                "empty-credential" -> failure { SecretValue.create("") }
                "credential-line-break" -> failure { SecretValue.create("secret\nvalue") }
                "relative-endpoint" -> failure {
                    ConnectionInput(
                        EndpointSelection.Explicit("/responses"),
                        SecretValue.create("secret"),
                        emptyList(),
                    ).resolve("https://default.test/responses", emptySet())
                }
                "invalid-header-name" -> failure { ExplicitHeader.create("bad header", "value", false) }
                "duplicate-header" -> failure {
                    ConnectionInput(
                        EndpointSelection.Default,
                        SecretValue.create("secret"),
                        listOf(
                            ExplicitHeader.create("x-extra", "one", false),
                            ExplicitHeader.create("X-Extra", "two", false),
                        ),
                    ).resolve("https://default.test/responses", emptySet())
                }
                else -> return@forEach
            }
            assertEquals(case["code"]!!.jsonPrimitive.content, error.error.code, name)
        }
    }

    private fun failure(block: () -> Unit): VendorProfileException =
        assertFailsWith<VendorProfileException>(block = block)
}
