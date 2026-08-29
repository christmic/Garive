package com.garive.eng.kt.provider.profile

import java.net.URI

/** Stable failure while validating explicit vendor connection values. */
public enum class VendorProfileError(public val code: String) {
    EMPTY_CREDENTIAL("empty_credential"),
    INVALID_CREDENTIAL("invalid_credential"),
    INVALID_ENDPOINT("invalid_endpoint"),
    INVALID_HEADER("invalid_header"),
    DUPLICATE_HEADER("duplicate_header"),
    RESERVED_HEADER("reserved_header"),
    PROFILE_INVARIANT("profile_invariant"),
}

/** Exception carrying one secret-free stable profile [error]. */
public class VendorProfileException(public val error: VendorProfileError) : IllegalArgumentException(error.code)

/** Runtime-supplied secret with redacted diagnostics and no implicit loaders. */
public class SecretValue private constructor(private val value: String) {
    /** Exposes the value only while constructing a sensitive protocol header. */
    public fun exposeSecret(): String = value

    override fun toString(): String = "SecretValue(<redacted>)"
    override fun equals(other: Any?): Boolean = other is SecretValue && value == other.value
    override fun hashCode(): Int = value.hashCode()

    public companion object {
        /** Validates one explicit secret value. */
        public fun create(value: String): SecretValue {
            if (value.isEmpty()) fail(VendorProfileError.EMPTY_CREDENTIAL)
            if (value.any { it == '\r' || it == '\n' || it == '\u0000' }) {
                fail(VendorProfileError.INVALID_CREDENTIAL)
            }
            return SecretValue(value)
        }
    }
}

/** Explicit default or Runtime-selected endpoint policy. */
public sealed interface EndpointSelection {
    /** Use the vendor profile's pinned default endpoint. */
    public data object Default : EndpointSelection
    /** Use an explicit absolute endpoint. */
    public data class Explicit(public val value: String) : EndpointSelection
}

/** One caller-supplied extra header with explicit sensitivity. */
public class ExplicitHeader private constructor(
    public val name: String,
    public val value: String,
    public val sensitive: Boolean,
) {
    override fun toString(): String =
        "ExplicitHeader(name=$name, value=${if (sensitive) "<redacted>" else value}, sensitive=$sensitive)"

    override fun equals(other: Any?): Boolean =
        other is ExplicitHeader && name == other.name && value == other.value && sensitive == other.sensitive

    override fun hashCode(): Int = 31 * (31 * name.hashCode() + value.hashCode()) + sensitive.hashCode()

    public companion object {
        /** Validates a header before vendor reservation policy is applied. */
        public fun create(name: String, value: String, sensitive: Boolean): ExplicitHeader {
            if (!name.matches(Regex("[!#$%&'*+.^_`|~0-9A-Za-z-]+")) ||
                value.any { it == '\r' || it == '\n' || it == '\u0000' }
            ) {
                fail(VendorProfileError.INVALID_HEADER)
            }
            return ExplicitHeader(name.lowercase(), value, sensitive)
        }
    }
}

/** Complete explicit connection input passed from Runtime to one profile. */
public data class ConnectionInput(
    public val endpoint: EndpointSelection,
    public val credential: SecretValue,
    public val extraHeaders: List<ExplicitHeader>,
) {
    /** Resolves and validates this input against one vendor's constants. */
    public fun resolve(defaultEndpoint: String, reservedHeaders: Set<String>): ResolvedConnection {
        val selected = when (endpoint) {
            EndpointSelection.Default -> defaultEndpoint
            is EndpointSelection.Explicit -> endpoint.value
        }
        val uri = runCatching { URI(selected) }.getOrNull()
        if (uri == null || !uri.isAbsolute || uri.host == null || uri.scheme !in setOf("http", "https") || uri.path.isEmpty()) {
            fail(VendorProfileError.INVALID_ENDPOINT)
        }
        val normalizedReserved = reservedHeaders.map(String::lowercase).toSet()
        if (extraHeaders.any { it.name in normalizedReserved }) fail(VendorProfileError.RESERVED_HEADER)
        if (extraHeaders.map(ExplicitHeader::name).distinct().size != extraHeaders.size) {
            fail(VendorProfileError.DUPLICATE_HEADER)
        }
        return ResolvedConnection(selected, credential, extraHeaders)
    }
}

/** Validated explicit values used during one profile construction. */
public data class ResolvedConnection(
    public val endpoint: String,
    public val credential: SecretValue,
    public val extraHeaders: List<ExplicitHeader>,
)

internal fun fail(error: VendorProfileError): Nothing = throw VendorProfileException(error)
