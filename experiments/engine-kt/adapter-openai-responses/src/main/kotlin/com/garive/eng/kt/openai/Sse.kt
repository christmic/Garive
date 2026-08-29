package com.garive.eng.kt.openai

/** Incremental UTF-8 Server-Sent Events frame. */
public data class SseFrame(
    public val event: String?,
    public val data: String,
    public val id: String?,
    public val retry: ULong?,
)

/** Byte-chunk incremental SSE decoder. */
public class SseDecoder {
    private var buffer: ByteArray = byteArrayOf()

    /** Appends arbitrary transport bytes and emits complete frames immediately. */
    public fun push(bytes: ByteArray): List<SseFrame> = responseFailure(ResponsesProtocolError.INVALID_SSE) {
        buffer += bytes
        val frames = mutableListOf<SseFrame>()
        while (true) {
            val boundary = findBoundary(buffer) ?: break
            val frame = parseFrame(buffer.copyOfRange(0, boundary.first))
            buffer = buffer.copyOfRange(boundary.first + boundary.second, buffer.size)
            if (frame != null) frames += frame
        }
        return frames
    }

    /** Requires EOF to follow a complete frame or comments only. */
    public fun finish(): Unit = responseFailure(ResponsesProtocolError.TRUNCATED_STREAM) {
        val trailing = runCatching {
            buffer.decodeToString(throwOnInvalidSequence = true)
        }.getOrNull()
        require(trailing != null && trailing.lineSequence().all { it.isBlank() || it.startsWith(':') })
        buffer = byteArrayOf()
    }
}

private fun findBoundary(bytes: ByteArray): Pair<Int, Int>? {
    for (index in bytes.indices) {
        if (index + 1 < bytes.size && bytes[index] == 10.toByte() && bytes[index + 1] == 10.toByte()) {
            return index to 2
        }
        if (index + 3 < bytes.size && bytes[index] == 13.toByte() &&
            bytes[index + 1] == 10.toByte() && bytes[index + 2] == 13.toByte() &&
            bytes[index + 3] == 10.toByte()
        ) {
            return index to 4
        }
    }
    return null
}

private fun parseFrame(bytes: ByteArray): SseFrame? {
    val text = bytes.decodeToString(throwOnInvalidSequence = true)
    var event: String? = null
    var id: String? = null
    var retry: ULong? = null
    val data = mutableListOf<String>()
    text.lineSequence().forEach { raw ->
        val line = raw.removeSuffix("\r")
        if (line.isNotEmpty() && !line.startsWith(':')) {
            val field = line.substringBefore(':')
            val value = line.substringAfter(':', "").removePrefix(" ")
            when (field) {
                "event" -> event = value
                "data" -> data += value
                "id" -> if ('\u0000' !in value) id = value
                "retry" -> retry = value.toULong()
            }
        }
    }
    return if (data.isEmpty()) null else SseFrame(event, data.joinToString("\n"), id, retry)
}
