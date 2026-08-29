package com.garive.eng.kt.observability

/** Stable portable O0 validation failure. */
public enum class AgentSignalErrorCode(public val wireName: String) {
    INVALID_SIGNAL("invalid_signal"),
    UNKNOWN_SIGNAL("unknown_signal"),
    ATTRIBUTE_NOT_ALLOWED("attribute_not_allowed"),
    ATTRIBUTE_LIMIT_EXCEEDED("attribute_limit_exceeded"),
    MEASUREMENT_INVALID("measurement_invalid"),
    REDACTION_VIOLATION("redaction_violation"),
}

/** Typed portable O0 construction result. */
public sealed interface AgentSignalResult<out T> {
    public data class Success<T>(public val value: T) : AgentSignalResult<T>
    public data class Failure(public val code: AgentSignalErrorCode) : AgentSignalResult<Nothing>
}

/** Diagnostic severity and queue priority. */
public enum class Severity(public val wireName: String) {
    TRACE("trace"), DEBUG("debug"), INFO("info"), WARN("warn"), ERROR("error"),
}

/** Source sensitivity; sinks may only narrow access. */
public enum class RedactionClass(public val wireName: String) {
    PUBLIC("public"), OPERATIONAL("operational"), RESTRICTED("restricted"),
}

/** Optional trace and durable-domain correlation, never metric labels. */
public data class Correlation(
    public val traceId: String? = null,
    public val spanId: String? = null,
    public val parentSpanId: String? = null,
    public val sessionId: String? = null,
    public val turnId: String? = null,
    public val executionId: String? = null,
    public val modelRequestId: String? = null,
    public val toolInvocationId: String? = null,
    public val durablePosition: ULong? = null,
)

/** Bounded allowlisted attribute value. */
public sealed interface AttributeValue {
    public data class StringValue(public val value: String) : AttributeValue
    public data class BoolValue(public val value: Boolean) : AttributeValue
    public data class IntegerValue(public val value: Long) : AttributeValue
}

/** One canonically ordered allowlisted attribute. */
public data class Attribute(public val name: String, public val value: AttributeValue)

/** Portable exact measurement unit. */
public enum class MeasurementUnit(public val wireName: String) {
    COUNT("count"), BYTES("bytes"), MILLISECONDS("milliseconds"), TOKENS("tokens"),
    BASIS_POINTS("basis_points"),
}

/** Known non-negative value or explicitly unknown token evidence. */
public sealed interface MeasurementValue {
    public data class Known(public val value: ULong) : MeasurementValue
    public data object Unknown : MeasurementValue
}

/** One canonically ordered catalogue measurement. */
public data class Measurement(
    public val name: String,
    public val value: MeasurementValue,
    public val unit: MeasurementUnit,
)

/** Canonical serialized signal and SHA-256 digest. */
public data class SignalBinding(public val digest: String, public val inlineUtf8: String)

internal fun <T> success(value: T): AgentSignalResult.Success<T> = AgentSignalResult.Success(value)
internal fun failure(code: AgentSignalErrorCode): AgentSignalResult.Failure = AgentSignalResult.Failure(code)
