package com.garive.eng.kt.openai

/** Internal Responses HTTP vocabulary. */
internal object HttpWire {
    const val METHOD_POST: String = "POST"
    const val HEADER_CONTENT_TYPE: String = "content-type"
    const val HEADER_ACCEPT: String = "accept"
    const val MEDIA_JSON: String = "application/json"
    const val MEDIA_SSE: String = "text/event-stream"
}

/** Internal Responses JSON field vocabulary used across codecs. */
internal object ResponseFields {
    const val TYPE: String = "type"
    const val RESPONSE: String = "response"
    const val ERROR: String = "error"
    const val ROLE: String = "role"
    const val SEQUENCE_NUMBER: String = "sequence_number"

    val CREATE: Set<String> = setOf(
        "model", "input", "stream", "max_output_tokens", "temperature", "top_p",
        "truncation", "tools", "tool_choice", "parallel_tool_calls", "text",
        "reasoning", "metadata", "stream_options",
    )
}

/** Internal Responses discriminators shared by parsing and lifecycle checks. */
internal object ResponseKinds {
    const val RESPONSE: String = "response"
    const val MESSAGE: String = "message"
    const val FUNCTION_CALL: String = "function_call"
    const val REASONING: String = "reasoning"
    const val OUTPUT_TEXT: String = "output_text"
    const val REFUSAL: String = "refusal"
    const val ASSISTANT: String = "assistant"
    const val FUNCTION_CALL_OUTPUT: String = "function_call_output"
    const val INPUT_TEXT: String = "input_text"
    const val INPUT_IMAGE: String = "input_image"
    const val FUNCTION: String = "function"
    const val TEXT: String = "text"
    const val JSON_OBJECT: String = "json_object"
    const val JSON_SCHEMA: String = "json_schema"
}
