package com.garive.eng.kt.provider.openai

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

public class OpenAiProfileTest {
    private val fixture: JsonObject = Json.parseToJsonElement(
        Path.of(System.getProperty("garive.repo.root"), "spec/fixtures/providers/vendor-connection-profiles-v1.json")
            .toFile().readText(),
    ).jsonObject

    @Test
    public fun `shared profiles construct exact redacted adapter config`(): Unit {
        fixture["profiles"]!!.jsonArray.map { it.jsonObject }
            .filter { it["vendor"]!!.jsonPrimitive.content == "openai" }
            .forEach { case ->
                val profile = buildOpenAiProfile(connection(case))
                assertEquals(case["expected"]!!.jsonObject["endpoint"]!!.jsonPrimitive.content, profile.adapterConfig.endpoint)
                if (case["endpoint"]!!.jsonObject["kind"]!!.jsonPrimitive.content == "default") {
                    val auth = profile.adapterConfig.headers.single { it.name == "authorization" }
                    assertEquals(case["expected"]!!.jsonObject["credential_value"]!!.jsonPrimitive.content, auth.value)
                    assertEquals(true, auth.sensitive)
                    assertFalse(profile.toString().contains(case["credential"]!!.jsonPrimitive.content))
                }
            }
    }

    @Test
    public fun `shared exact error rules match neutral dispositions`(): Unit {
        val policy = defaultOpenAiErrorPolicy()
        fixture["error_rules"]!!.jsonObject["openai"]!!.jsonArray.forEach { element ->
            val rule = element.jsonObject
            val actual = policy.classify(signature(rule))
            val expected = when (rule["expected"]!!.jsonPrimitive.content) {
                "context_overflow" -> ErrorDisposition.Rejected(RejectionKind.CONTEXT_OVERFLOW)
                "authentication" -> ErrorDisposition.Rejected(RejectionKind.AUTHENTICATION)
                "rate_limited" -> ErrorDisposition.Unavailable(UnavailableKind.RATE_LIMITED)
                "model_unavailable" -> ErrorDisposition.Unavailable(UnavailableKind.MODEL_UNAVAILABLE)
                else -> error("unknown fixture expectation")
            }
            assertEquals(expected, actual)
        }
    }

    @Test
    public fun `shared reserved auth failure returns stable code`(): Unit {
        val case = fixture["failure_cases"]!!.jsonArray.map { it.jsonObject }
            .single { it["name"]!!.jsonPrimitive.content == "openai-reserved-auth" }
        val error = assertFailsWith<VendorProfileException> {
            buildOpenAiProfile(ConnectionInput(
                EndpointSelection.Default,
                SecretValue.create("secret"),
                listOf(ExplicitHeader.create("Authorization", "caller", true)),
            ))
        }
        assertEquals(VendorProfileError.RESERVED_HEADER, error.error)
        assertEquals(case["code"]!!.jsonPrimitive.content, error.error.code)
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

    private fun signature(rule: JsonObject): ErrorSignature = ErrorSignature(
        rule["status"]!!.jsonPrimitive.content.toUShort(),
        rule["type"]!!.jsonPrimitive.content,
        rule["code"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content,
    )
}
