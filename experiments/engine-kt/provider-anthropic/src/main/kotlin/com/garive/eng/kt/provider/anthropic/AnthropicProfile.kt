package com.garive.eng.kt.provider.anthropic

import com.garive.eng.kt.anthropic.MessagesAdapterConfig
import com.garive.eng.kt.anthropic.ProtocolHeader
import com.garive.eng.kt.llm.RejectionKind
import com.garive.eng.kt.llm.UnavailableKind
import com.garive.eng.kt.provider.compatible.ErrorDisposition
import com.garive.eng.kt.provider.compatible.ErrorSignature
import com.garive.eng.kt.provider.compatible.ProtocolErrorPolicy
import com.garive.eng.kt.provider.profile.ConnectionInput
import com.garive.eng.kt.provider.profile.VendorProfileError
import com.garive.eng.kt.provider.profile.VendorProfileException

/** Adapter construction values and exact official P2-C error policy. */
public data class AnthropicProfile(
    public val adapterConfig: MessagesAdapterConfig,
    public val errorPolicy: ProtocolErrorPolicy,
)

/** Builds the official API-key profile from Runtime-supplied values. */
public fun buildAnthropicProfile(input: ConnectionInput): AnthropicProfile {
    val connection = input.resolve(Constants.DEFAULT_ENDPOINT, Constants.RESERVED_HEADERS)
    val headers = connection.extraHeaders.map { header ->
        ProtocolHeader.create(header.name, header.value, header.sensitive)
    } + ProtocolHeader.create(Constants.API_KEY, connection.credential.exposeSecret(), true)
    val adapter = profileInvariant {
        MessagesAdapterConfig(
            endpoint = connection.endpoint,
            headers = headers,
            versionHeaderName = Constants.VERSION_HEADER,
            protocolVersion = Constants.PROTOCOL_VERSION,
        )
    }
    return AnthropicProfile(adapter, defaultAnthropicErrorPolicy())
}

/** Returns the pinned exact official error policy. */
public fun defaultAnthropicErrorPolicy(): ProtocolErrorPolicy = profileInvariant {
    ProtocolErrorPolicy.of(listOf(
        rule(401u, "authentication_error", ErrorDisposition.Rejected(RejectionKind.AUTHENTICATION)),
        rule(429u, "rate_limit_error", ErrorDisposition.Unavailable(UnavailableKind.RATE_LIMITED)),
        rule(529u, "overloaded_error", ErrorDisposition.Unavailable(UnavailableKind.MODEL_UNAVAILABLE)),
    ))
}

private fun rule(
    status: UInt,
    type: String,
    disposition: ErrorDisposition,
): Pair<ErrorSignature, ErrorDisposition> = ErrorSignature(status.toUShort(), type, null) to disposition

private inline fun <T> profileInvariant(block: () -> T): T = try {
    block()
} catch (error: VendorProfileException) {
    throw error
} catch (_: IllegalArgumentException) {
    throw VendorProfileException(VendorProfileError.PROFILE_INVARIANT)
}
