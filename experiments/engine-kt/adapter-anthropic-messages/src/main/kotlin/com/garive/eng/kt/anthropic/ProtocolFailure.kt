package com.garive.eng.kt.anthropic

/** Stable protocol-only failure code with no deployment or retry policy. */
public enum class MessagesProtocolError {
    INVALID_ENDPOINT, INVALID_HEADER, INVALID_REQUEST, INVALID_JSON, INVALID_MEDIA_TYPE,
    INVALID_SSE, INVALID_LIFECYCLE, TRUNCATED_STREAM,
}

/** Typed failure exposed by the Kotlin Messages protocol adapter. */
public class MessagesProtocolException(
    public val error: MessagesProtocolError,
    cause: Throwable? = null,
) : IllegalArgumentException(error.name, cause)

internal inline fun <T> messageFailure(error: MessagesProtocolError, block: () -> T): T = try {
    block()
} catch (failure: MessagesProtocolException) {
    throw failure
} catch (failure: RuntimeException) {
    throw MessagesProtocolException(error, failure)
}
