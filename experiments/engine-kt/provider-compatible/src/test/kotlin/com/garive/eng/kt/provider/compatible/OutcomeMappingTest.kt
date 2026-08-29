package com.garive.eng.kt.provider.compatible

import com.garive.eng.kt.anthropic.MessageResponse
import com.garive.eng.kt.llm.InterruptionKind
import com.garive.eng.kt.llm.InvokeOutcome
import com.garive.eng.kt.llm.ModelItem
import com.garive.eng.kt.llm.ModelStopReason
import com.garive.eng.kt.llm.RejectionKind
import com.garive.eng.kt.llm.TokenCount
import com.garive.eng.kt.llm.UnavailableKind
import com.garive.eng.kt.openai.ResponseEnvelope
import java.nio.file.Path
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlin.time.Duration.Companion.seconds
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonNull
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

public class OutcomeMappingTest {
    private val fixture: JsonObject = Json.parseToJsonElement(
        Path.of(System.getProperty("garive.repo.root"), "spec/fixtures/providers/compatible-mapping-v1.json")
            .toFile().readText(),
    ).jsonObject

    @Test
    public fun `shared responses terminals preserve items usage and interruption`(): Unit {
        val cases = fixture["outcome_cases"]!!.jsonArray
        val completed = normalizeResponses(ResponseEnvelope.parse(responsesWire(cases[0].jsonObject)), false)
        val success = assertIs<InvokeOutcome.Completed>(completed)
        assertEquals(ModelStopReason.ToolUse, success.stopReason)
        assertIs<ModelItem.Text>(success.items[0])
        assertIs<ModelItem.ToolIntent>(success.items[1])
        assertEquals(TokenCount.Known(10u), success.usage.inputTokens)
        assertEquals(TokenCount.Known(4u), success.usage.outputTokens)

        val incomplete = assertIs<InvokeOutcome.Interrupted>(
            normalizeResponses(ResponseEnvelope.parse(responsesWire(cases[1].jsonObject)), false),
        )
        assertEquals(InterruptionKind.OUTPUT_LIMIT, incomplete.reason)
    }

    @Test
    public fun `shared messages terminals distinguish refusal and context rejection`(): Unit {
        val cases = fixture["outcome_cases"]!!.jsonArray
        val refusal = assertIs<InvokeOutcome.Completed>(
            normalizeMessages(MessageResponse.parse(messagesWire(cases[2].jsonObject)), false),
        )
        assertEquals(ModelStopReason.Refusal, refusal.stopReason)
        assertIs<ModelItem.Refusal>(refusal.items.single())

        val rejected = assertIs<InvokeOutcome.Rejected>(
            normalizeMessages(MessageResponse.parse(messagesWire(cases[3].jsonObject)), false),
        )
        assertEquals(RejectionKind.CONTEXT_OVERFLOW, rejected.reason)
    }

    @Test
    public fun `shared exact errors never guess from message text`(): Unit {
        val policy = ProtocolErrorPolicy.of(listOf(
            ErrorSignature(401u, "authentication_error", null) to ErrorDisposition.Rejected(RejectionKind.AUTHENTICATION),
            ErrorSignature(429u, "rate_limit_error", "rate_limit") to ErrorDisposition.Unavailable(UnavailableKind.RATE_LIMITED),
        ))
        fixture["error_cases"]!!.jsonArray.forEach { element ->
            val case = element.jsonObject
            val signature = ErrorSignature(
                case["status"]!!.jsonPrimitive.content.toUShort(),
                case["type"]!!.jsonPrimitive.content,
                case["code"]?.takeUnless { it is JsonNull }?.jsonPrimitive?.content,
            )
            when (case["expected"]!!.jsonPrimitive.content) {
                "authentication" -> assertIs<InvokeOutcome.Rejected>(classifyProtocolError(policy, signature, 2.seconds))
                "rate_limited" -> assertIs<InvokeOutcome.Unavailable>(classifyProtocolError(policy, signature, 2.seconds))
                "unclassified_protocol_error" -> assertEquals(
                    CompatibleProviderError.UNCLASSIFIED_PROTOCOL_ERROR,
                    kotlin.runCatching { classifyProtocolError(policy, signature, 2.seconds) }
                        .exceptionOrNull().let { assertIs<CompatibleProviderException>(it).error },
                )
            }
        }
    }

    private fun responsesWire(case: JsonObject): JsonObject {
        val response = case["response"]!!.jsonObject
        val output = response["items"]!!.jsonArray.mapIndexed { index, element ->
            val item = element.jsonObject
            when (item["kind"]!!.jsonPrimitive.content) {
                "text" -> buildJsonObject {
                    put("type", "message"); put("id", "msg-$index"); put("role", "assistant"); put("status", "completed")
                    put("content", JsonArray(listOf(buildJsonObject {
                        put("type", "output_text"); put("text", item["text"]!!); put("annotations", JsonArray(emptyList()))
                    })))
                }
                "tool" -> buildJsonObject {
                    put("type", "function_call"); put("call_id", item["model_call_id"]!!); put("name", item["tool_name"]!!)
                    put("arguments", item["arguments"]!!.toString()); put("status", "completed")
                }
                else -> error("unsupported fixture item")
            }
        }
        val usage = response["usage"]?.takeUnless { it is JsonNull }?.jsonObject?.let { value ->
            val input = value["input"]!!.jsonPrimitive.content.toLong()
            val generated = value["output"]!!.jsonPrimitive.content.toLong()
            buildJsonObject {
                put("input_tokens", input); put("output_tokens", generated); put("total_tokens", input + generated)
                put("input_tokens_details", buildJsonObject {
                    put("cached_tokens", value["cache_read"]!!); put("cache_write_tokens", value["cache_write"]!!)
                })
                put("output_tokens_details", buildJsonObject { put("reasoning_tokens", 0) })
            }
        }
        return buildJsonObject {
            put("id", "response-1"); put("created_at", 1.0); put("error", JsonNull)
            put("incomplete_details", response["reason"]?.takeUnless { it is JsonNull }?.let { reason ->
                buildJsonObject { put("reason", reason) }
            } ?: JsonNull)
            put("instructions", JsonNull); put("metadata", JsonNull); put("model", "fixture"); put("object", "response")
            put("output", JsonArray(output)); put("parallel_tool_calls", false); put("temperature", JsonNull)
            put("tool_choice", "auto"); put("tools", JsonArray(emptyList())); put("top_p", JsonNull)
            put("status", response["status"]!!); put("usage", usage ?: JsonNull)
        }
    }

    private fun messagesWire(case: JsonObject): JsonObject {
        val response = case["response"]!!.jsonObject
        val usage = response["usage"]!!.jsonObject
        return buildJsonObject {
            put("id", "message-1"); put("type", "message"); put("role", "assistant"); put("model", "fixture")
            put("content", JsonArray(response["items"]!!.jsonArray.map { item -> buildJsonObject {
                put("type", "text"); put("text", item.jsonObject["text"]!!)
            }}))
            put("stop_reason", response["stop_reason"]!!); put("stop_sequence", JsonNull)
            put("usage", buildJsonObject {
                put("input_tokens", usage["input"]!!); put("output_tokens", usage["output"]!!)
                put("cache_read_input_tokens", usage["cache_read"]!!); put("cache_creation_input_tokens", usage["cache_write"]!!)
            })
        }
    }
}
