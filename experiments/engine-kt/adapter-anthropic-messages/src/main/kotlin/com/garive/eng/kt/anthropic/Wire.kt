package com.garive.eng.kt.anthropic

/** Internal Messages HTTP vocabulary. */
internal object HttpWire {
    const val METHOD_POST: String = "POST"
    const val HEADER_CONTENT_TYPE: String = "content-type"
    const val HEADER_ACCEPT: String = "accept"
    const val MEDIA_JSON: String = "application/json"
    const val MEDIA_SSE: String = "text/event-stream"
    const val MEDIA_PDF: String = "application/pdf"
    const val MEDIA_TEXT: String = "text/plain"
}

/** Internal Messages JSON field vocabulary used across codecs. */
internal object MessageFields {
    const val TYPE: String = "type"
    const val ERROR: String = "error"
    const val DELTA: String = "delta"
    const val CONTENT_BLOCK: String = "content_block"
    const val PARTIAL_JSON: String = "partial_json"

    val CREATE: Set<String> = setOf(
        "model", "max_tokens", "messages", "stream", "system", "stop_sequences",
        "temperature", "top_p", "top_k", "tools", "tool_choice", "output_config",
        "thinking", "metadata",
    )
}

/** Internal Messages discriminators shared by parsing and lifecycle checks. */
internal object MessageKinds {
    const val MESSAGE: String = "message"
    const val ERROR: String = "error"
    const val ASSISTANT: String = "assistant"
    const val TEXT: String = "text"
    const val IMAGE: String = "image"
    const val DOCUMENT: String = "document"
    const val THINKING: String = "thinking"
    const val REDACTED_THINKING: String = "redacted_thinking"
    const val TOOL_USE: String = "tool_use"
    const val TOOL_RESULT: String = "tool_result"
    const val BASE64: String = "base64"
    const val URL: String = "url"
    const val CONTENT: String = "content"
    const val EPHEMERAL: String = "ephemeral"
    const val JSON_SCHEMA: String = "json_schema"
    const val AUTO: String = "auto"
    const val ANY: String = "any"
    const val TOOL: String = "tool"
    const val NONE: String = "none"
    const val DISABLED: String = "disabled"
    const val ENABLED: String = "enabled"
    const val ADAPTIVE: String = "adaptive"
}
