package com.garive.eng.kt.provider.anthropic

import com.garive.eng.kt.llm.RejectionKind
import com.garive.eng.kt.llm.UnavailableKind
import com.garive.eng.kt.provider.compatible.ErrorDisposition
import com.garive.eng.kt.provider.compatible.ErrorSignature
import com.garive.eng.kt.provider.profile.ConnectionInput
import com.garive.eng.kt.provider.profile.EndpointSelection
import com.garive.eng.kt.provider.profile.ExplicitHeader
import com.garive.eng.kt.provider.profile.SecretValue
import com.garive.eng.kt.provider.profile.VendorProfileError
import com.garive.eng.kt.provider.profile.VendorProfileException
import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFalse
import kotlin.test.assertFailsWith
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

public class AnthropicProfileTest {
    private val fixture: JsonObject = Json.parseToJsonElement(
        Path.of(System.getProperty("garive.repo.root"), "spec/fixtures/providers/vendor-connection-profiles-v1.json")
            .toFile().readText(),
    ).jsonObject

    @Test
    public fun `shared profiles construct exact redacted adapter config`(): Unit {
        fixture["profiles"]!!.jsonArray.map { it.jsonObject }
            .filter { it["vendor"]!!.jsonPrimitive.content == "anthropic" }
            .forEach { case ->
                val profile = buildAnthropicProfile(connection(case))
                val expected = case["expected"]!!.jsonObject
                assertEquals(expected["endpoint"]!!.jsonPrimitive.content, profile.adapterConfig.endpoint)
                if (case["endpoint"]!!.jsonObject["kind"]!!.jsonPrimitive.content == "default") {
                    val auth = profile.adapterConfig.headers.single { it.name == "x-api-key" }
                    assertEquals(expected["credential_value"]!!.jsonPrimitive.content, auth.value)
                    assertEquals(expected["version_header"]!!.jsonPrimitive.content, profile.adapterConfig.versionHeaderName)
                    assertEquals(expected["protocol_version"]!!.jsonPrimitive.content, profile.adapterConfig.protocolVersion)
                    assertFalse(profile.toString().contains(case["credential"]!!.jsonPrimitive.content))
                }
            }
    }

    @Test
    public fun `shared exact errors leave message-only context unclassified`(): Unit {
        val policy = defaultAnthropicErrorPolicy()
        fixture["error_rules"]!!.jsonObject["anthropic"]!!.jsonArray.forEach { element ->
            val rule = element.jsonObject
            val actual = policy.classify(ErrorSignature(
                rule["status"]!!.jsonPrimitive.content.toUShort(),
                rule["type"]!!.jsonPrimitive.content,
                rule["code"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content,
            ))
            val expected = when (rule["expected"]!!.jsonPrimitive.content) {
                "authentication" -> ErrorDisposition.Rejected(RejectionKind.AUTHENTICATION)
                "rate_limited" -> ErrorDisposition.Unavailable(UnavailableKind.RATE_LIMITED)
                "model_unavailable" -> ErrorDisposition.Unavailable(UnavailableKind.MODEL_UNAVAILABLE)
                "unclassified_protocol_error" -> null
                else -> error("unknown fixture expectation")
            }
            assertEquals(expected, actual)
        }
    }

    @Test
    public fun `shared reserved headers return stable codes`(): Unit {
        val cases = fixture["failure_cases"]!!.jsonArray.map { it.jsonObject }
        mapOf(
            "anthropic-reserved-version" to "Anthropic-Version",
            "anthropic-reserved-bearer" to "Authorization",
        ).forEach { (name, header) ->
            val case = cases.single { it["name"]!!.jsonPrimitive.content == name }
            val error = assertFailsWith<VendorProfileException> {
                buildAnthropicProfile(ConnectionInput(
                    EndpointSelection.Default,
                    SecretValue.create("secret"),
                    listOf(ExplicitHeader.create(header, "caller", true)),
                ))
            }
            assertEquals(VendorProfileError.RESERVED_HEADER, error.error)
            assertEquals(case["code"]!!.jsonPrimitive.content, error.error.code, name)
        }
    }

    private fun connection(case: JsonObject): ConnectionInput {
        val endpoint = case["endpoint"]!!.jsonObject.let {
            if (it["kind"]!!.jsonPrimitive.content == "default") EndpointSelection.Default
            else EndpointSelection.Explicit(it["value"]!!.jsonPrimitive.content)
        }
        val headers = case["extra_headers"]!!.jsonArray.map { element ->
            val header = element.jsonObject
            ExplicitHeader.create(
                header["name"]!!.jsonPrimitive.content,
                header["value"]!!.jsonPrimitive.content,
                header["sensitive"]!!.jsonPrimitive.boolean,
            )
        }
        return ConnectionInput(endpoint, SecretValue.create(case["credential"]!!.jsonPrimitive.content), headers)
    }
}
