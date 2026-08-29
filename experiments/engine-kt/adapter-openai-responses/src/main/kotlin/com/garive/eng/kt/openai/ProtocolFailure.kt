package com.garive.eng.kt.openai

/** Stable protocol-only failure code with no deployment or retry policy. */
public enum class ResponsesProtocolError {
    INVALID_ENDPOINT, INVALID_HEADER, INVALID_REQUEST, INVALID_JSON, INVALID_MEDIA_TYPE,
    INVALID_SSE, INVALID_LIFECYCLE, TRUNCATED_STREAM,
}

/** Typed failure exposed by the Kotlin Responses protocol adapter. */
public class ResponsesProtocolException(
    public val error: ResponsesProtocolError,
    cause: Throwable? = null,
) : IllegalArgumentException(error.name, cause)

internal inline fun <T> responseFailure(error: ResponsesProtocolError, block: () -> T): T = try {
    block()
} catch (failure: ResponsesProtocolException) {
    throw failure
} catch (failure: RuntimeException) {
    throw ResponsesProtocolException(error, failure)
}
