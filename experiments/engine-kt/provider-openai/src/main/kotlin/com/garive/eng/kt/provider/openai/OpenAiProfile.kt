package com.garive.eng.kt.provider.openai

import com.garive.eng.kt.llm.RejectionKind
import com.garive.eng.kt.llm.UnavailableKind
import com.garive.eng.kt.openai.ProtocolHeader
import com.garive.eng.kt.openai.ResponsesAdapterConfig
import com.garive.eng.kt.provider.compatible.ErrorDisposition
import com.garive.eng.kt.provider.compatible.ErrorSignature
import com.garive.eng.kt.provider.compatible.ProtocolErrorPolicy
import com.garive.eng.kt.provider.profile.ConnectionInput
import com.garive.eng.kt.provider.profile.VendorProfileError
import com.garive.eng.kt.provider.profile.VendorProfileException

/** Adapter construction values and exact official P2-C error policy. */
public data class OpenAiProfile(
    public val adapterConfig: ResponsesAdapterConfig,
    public val errorPolicy: ProtocolErrorPolicy,
)

/** Builds the official profile from values supplied explicitly by Runtime. */
public fun buildOpenAiProfile(input: ConnectionInput): OpenAiProfile {
    val connection = input.resolve(Constants.DEFAULT_ENDPOINT, Constants.RESERVED_HEADERS)
    val headers = connection.extraHeaders.map { header ->
        ProtocolHeader.create(header.name, header.value, header.sensitive)
    } + ProtocolHeader.create(
        Constants.AUTHORIZATION,
        "Bearer ${connection.credential.exposeSecret()}",
        true,
    )
    val adapter = profileInvariant { ResponsesAdapterConfig(connection.endpoint, headers) }
    return OpenAiProfile(adapter, defaultOpenAiErrorPolicy())
}

/** Returns the pinned exact official error policy. */
public fun defaultOpenAiErrorPolicy(): ProtocolErrorPolicy = profileInvariant {
    ProtocolErrorPolicy.of(listOf(
        rule(400u, "invalid_request_error", "context_length_exceeded", ErrorDisposition.Rejected(RejectionKind.CONTEXT_OVERFLOW)),
        rule(401u, "invalid_request_error", "invalid_api_key", ErrorDisposition.Rejected(RejectionKind.AUTHENTICATION)),
        rule(429u, "rate_limit_error", "rate_limit_exceeded", ErrorDisposition.Unavailable(UnavailableKind.RATE_LIMITED)),
        rule(503u, "server_error", "server_error", ErrorDisposition.Unavailable(UnavailableKind.MODEL_UNAVAILABLE)),
    ))
}

private fun rule(
    status: UInt,
    type: String,
    code: String,
    disposition: ErrorDisposition,
): Pair<ErrorSignature, ErrorDisposition> = ErrorSignature(status.toUShort(), type, code) to disposition

private inline fun <T> profileInvariant(block: () -> T): T = try {
    block()
} catch (error: VendorProfileException) {
    throw error
} catch (_: IllegalArgumentException) {
    throw VendorProfileException(VendorProfileError.PROFILE_INVARIANT)
}
