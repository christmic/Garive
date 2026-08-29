package com.garive.mobile.host

import com.garive.host.v1.HostEventV1

/** Durable terminal kinds recognized by the A1 mobile reducer. */
public enum class HostTerminalKind { COMPLETED, SUSPENDED, STOPPED, FAILED }

/** Exact portable A1 client failure vocabulary. */
public enum class HostClientError(public val wireName: String) {
    INVALID_CONFIGURATION("invalid_configuration"),
    INVALID_COMMAND("invalid_command"),
    INVALID_EVENT("invalid_event"),
    EVENT_ORDER_VIOLATION("event_order_violation"),
    EVENT_LIMIT_EXCEEDED("event_limit_exceeded"),
    HOST_FAILURE("host_failure"),
    UNKNOWN_HOST_ERROR("unknown_host_error"),
    TRANSPORT_FAILURE("transport_failure"),
    FOLLOW_DEADLINE("follow_deadline"),
}

/** Safe failure without request, header, event, or response content. */
public class HostClientException(
    public val code: HostClientError,
    public val status: Int? = null,
) : Exception(if (status == null) code.wireName else "${code.wireName} (HTTP $status)")

/** Ephemeral mobile projection; durable truth remains in H1. */
public data class HostView(
    public val cursor: Long = 0,
    public val terminal: HostTerminalKind? = null,
    public val text: String = "",
    public val unknownEvents: List<String> = emptyList(),
    public val fingerprints: Map<Long, HostEventV1> = emptyMap(),
)

/** Reduces ordered replay/follow events without treating stream EOF as terminal. */
public fun reduceHostEvents(
    sessionId: String,
    events: List<HostEventV1>,
    initial: HostView = HostView(),
    maxEvents: Int = 16,
): HostView {
    if (sessionId.isEmpty() || maxEvents <= 0) fail(HostClientError.INVALID_CONFIGURATION)
    if (events.size > maxEvents) fail(HostClientError.EVENT_LIMIT_EXCEEDED)
    var cursor = initial.cursor
    var terminal = initial.terminal
    var text = initial.text
    val savedCursor = initial.cursor
    val unknown = initial.unknownEvents.toMutableList()
    val fingerprints = initial.fingerprints.toMutableMap()
    events.forEach { event ->
        if (event.api_version != API_VERSION || event.session_id != sessionId || event.position <= 0) {
            fail(HostClientError.INVALID_EVENT)
        }
        val prior = fingerprints[event.position]
        if (prior != null) {
            if (prior != event) fail(HostClientError.EVENT_ORDER_VIOLATION)
            return@forEach
        }
        if (event.position <= savedCursor) return@forEach
        if (event.position <= cursor || terminal != null) fail(HostClientError.EVENT_ORDER_VIOLATION)
        cursor = event.position
        fingerprints[event.position] = event
        when (event.event) {
            "turn.completed" -> { terminal = HostTerminalKind.COMPLETED; text = event.text }
            "turn.suspended" -> terminal = HostTerminalKind.SUSPENDED
            "turn.stopped" -> terminal = HostTerminalKind.STOPPED
            "turn.failed" -> terminal = HostTerminalKind.FAILED
            !in KNOWN_EVENTS -> if (event.event !in unknown) unknown += event.event
        }
    }
    return HostView(cursor, terminal, text, unknown, fingerprints)
}

internal const val API_VERSION: String = "v1"
internal val KNOWN_EVENTS: Set<String> = setOf(
    "session.created", "turn.started", "turn.completed", "turn.suspended", "turn.stopped", "turn.failed",
)

internal fun fail(code: HostClientError, status: Int? = null): Nothing =
    throw HostClientException(code, status)
