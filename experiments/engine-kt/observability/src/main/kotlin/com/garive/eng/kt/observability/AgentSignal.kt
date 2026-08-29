package com.garive.eng.kt.observability

import java.security.MessageDigest
import java.time.Instant
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonArray
import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import org.erdtman.jcs.JsonCanonicalizer

/** Validated immutable portable v1 Agent signal. */
@ConsistentCopyVisibility
public data class AgentSignal private constructor(
    public val signalName: String,
    public val schemaVersion: UInt,
    public val observedAtUtc: String,
    public val severity: Severity,
    public val correlation: Correlation,
    public val attributes: List<Attribute>,
    public val measurements: List<Measurement>,
    public val redactionClass: RedactionClass,
) {
    /** Returns RFC 8785 canonical JSON and its SHA-256 digest. */
    public fun binding(): AgentSignalResult<SignalBinding> = runCatching {
        val bytes = JsonCanonicalizer(toJson().toString()).encodedUTF8
        SignalBinding(sha256(bytes), bytes.decodeToString())
    }.fold(::success) { failure(AgentSignalErrorCode.INVALID_SIGNAL) }

    private fun toJson(): JsonObject = JsonObject(
        mapOf(
            "signal_name" to JsonPrimitive(signalName),
            "schema_version" to number(schemaVersion.toString()),
            "observed_at_utc" to JsonPrimitive(observedAtUtc),
            "severity" to JsonPrimitive(severity.wireName),
            "correlation" to correlationJson(correlation),
            "attributes" to JsonArray(attributes.map(::attributeJson)),
            "measurements" to JsonArray(measurements.map(::measurementJson)),
            "redaction_class" to JsonPrimitive(redactionClass.wireName),
        ),
    )

    public companion object {
        /** Validates catalogue, ordering, bounds, correlation, measurement and redaction rules. */
        public fun create(
            signalName: String,
            schemaVersion: UInt,
            observedAtUtc: String,
            severity: Severity,
            correlation: Correlation,
            attributes: List<Attribute>,
            measurements: List<Measurement>,
            redactionClass: RedactionClass,
        ): AgentSignalResult<AgentSignal> {
            if (schemaVersion != 1u || !canonicalUtc(observedAtUtc) || !validCorrelation(correlation) ||
                !strictNames(attributes.map(Attribute::name)) || !strictNames(measurements.map(Measurement::name))
            ) return failure(AgentSignalErrorCode.INVALID_SIGNAL)
            if (attributes.size > MAX_ATTRIBUTES || measurements.size > MAX_MEASUREMENTS) {
                return failure(AgentSignalErrorCode.ATTRIBUTE_LIMIT_EXCEEDED)
            }
            val schema = SignalCatalogue.schemas[signalName]
                ?: return failure(AgentSignalErrorCode.UNKNOWN_SIGNAL)
            if (redactionClass < schema.minimumRedaction) {
                return failure(AgentSignalErrorCode.REDACTION_VIOLATION)
            }
            for (attribute in attributes) {
                val category = schema.attributes[attribute.name]
                    ?: return failure(AgentSignalErrorCode.ATTRIBUTE_NOT_ALLOWED)
                if (!attributeValid(category, attribute.value)) {
                    return failure(AgentSignalErrorCode.ATTRIBUTE_NOT_ALLOWED)
                }
            }
            for (measurement in measurements) {
                if (schema.measurements[measurement.name] != measurement.unit ||
                    measurement.value is MeasurementValue.Unknown &&
                    (measurement.unit != MeasurementUnit.TOKENS || measurement.name !in TOKEN_NAMES)
                ) return failure(AgentSignalErrorCode.MEASUREMENT_INVALID)
            }
            return success(
                AgentSignal(
                    signalName, schemaVersion, observedAtUtc, severity, correlation,
                    attributes.toList(), measurements.toList(), redactionClass,
                ),
            )
        }
    }
}

private const val MAX_ATTRIBUTES: Int = 8
private const val MAX_MEASUREMENTS: Int = 8
private const val MAX_STRING_BYTES: Int = 64
private const val MAX_ID_BYTES: Int = 128
private val TOKEN_NAMES: Set<String> = setOf("input_tokens", "output_tokens")

private fun attributeValid(category: String, value: AttributeValue): Boolean = when (value) {
    is AttributeValue.BoolValue -> category == "bool"
    is AttributeValue.StringValue -> validText(value.value, MAX_STRING_BYTES) &&
        value.value in SignalCatalogue.enumValues.getOrDefault(category, emptyList())
    is AttributeValue.IntegerValue -> false
}

private fun validCorrelation(value: Correlation): Boolean =
    value.durablePosition != 0uL &&
        listOf(value.traceId to 32, value.spanId to 16, value.parentSpanId to 16)
            .all { (id, length) -> id == null || hexId(id, length) } &&
        listOf(value.sessionId, value.turnId, value.executionId, value.modelRequestId, value.toolInvocationId)
            .filterNotNull().all { validText(it, MAX_ID_BYTES) }

private fun hexId(value: String, length: Int): Boolean = value.length == length &&
    value.all { it in '0'..'9' || it in 'a'..'f' } && value.any { it != '0' }

private fun strictNames(values: List<String>): Boolean = values.all { validText(it, MAX_STRING_BYTES) } &&
    values.zipWithNext().all { (left, right) -> left < right }

private fun validText(value: String, maxBytes: Int): Boolean =
    value.isNotEmpty() && value.trim() == value && value.encodeToByteArray().size <= maxBytes

private fun canonicalUtc(value: String): Boolean = runCatching { Instant.parse(value).toString() == value }.getOrDefault(false)

private fun correlationJson(value: Correlation): JsonObject = JsonObject(buildMap {
    value.traceId?.let { put("trace_id", JsonPrimitive(it)) }
    value.spanId?.let { put("span_id", JsonPrimitive(it)) }
    value.parentSpanId?.let { put("parent_span_id", JsonPrimitive(it)) }
    value.sessionId?.let { put("session_id", JsonPrimitive(it)) }
    value.turnId?.let { put("turn_id", JsonPrimitive(it)) }
    value.executionId?.let { put("execution_id", JsonPrimitive(it)) }
    value.modelRequestId?.let { put("model_request_id", JsonPrimitive(it)) }
    value.toolInvocationId?.let { put("tool_invocation_id", JsonPrimitive(it)) }
    value.durablePosition?.let { put("durable_position", number(it.toString())) }
})

private fun attributeJson(value: Attribute): JsonObject = JsonObject(
    mapOf(
        "name" to JsonPrimitive(value.name),
        "value" to when (val attribute = value.value) {
            is AttributeValue.StringValue -> JsonObject(mapOf("kind" to JsonPrimitive("string"), "value" to JsonPrimitive(attribute.value)))
            is AttributeValue.BoolValue -> JsonObject(mapOf("kind" to JsonPrimitive("bool"), "value" to JsonPrimitive(attribute.value)))
            is AttributeValue.IntegerValue -> JsonObject(mapOf("kind" to JsonPrimitive("integer"), "value" to JsonPrimitive(attribute.value)))
        },
    ),
)

private fun measurementJson(value: Measurement): JsonObject = JsonObject(
    mapOf(
        "name" to JsonPrimitive(value.name),
        "value" to when (val measurement = value.value) {
            is MeasurementValue.Known -> JsonObject(mapOf("kind" to JsonPrimitive("known"), "value" to number(measurement.value.toString())))
            MeasurementValue.Unknown -> JsonObject(mapOf("kind" to JsonPrimitive("unknown")))
        },
        "unit" to JsonPrimitive(value.unit.wireName),
    ),
)

private fun number(value: String): JsonElement = Json.parseToJsonElement(value)
private fun sha256(value: ByteArray): String = MessageDigest.getInstance("SHA-256")
    .digest(value).joinToString("") { "%02x".format(it) }
