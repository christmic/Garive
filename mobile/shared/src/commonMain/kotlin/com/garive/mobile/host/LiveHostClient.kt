package com.garive.mobile.host

import com.garive.host.v1.CreateSessionResponseV1
import com.garive.host.v1.DeliveredTurnResponseV1
import com.garive.host.v1.AgentDefinitionPageV1
import com.garive.host.v1.HostEventV1
import com.garive.host.v1.SessionPageV1
import com.garive.host.v1.SessionViewV1
import com.garive.host.v1.SessionMemberV1
import com.garive.host.v1.SessionMembershipV1
import com.garive.host.v1.StartTurnsResponseV1
import com.garive.host.v1.TurnDeliveryV1
import com.garive.host.v1.TurnCommandResponseV1
import com.garive.host.v1.TurnTimelinePageV1
import io.ktor.client.HttpClient
import io.ktor.client.engine.cio.CIO
import io.ktor.client.plugins.HttpTimeout
import io.ktor.client.plugins.sse.SSE
import io.ktor.client.plugins.sse.serverSentEvents
import io.ktor.client.request.accept
import io.ktor.client.request.header
import io.ktor.client.request.get
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.request.url
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsChannel
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.Url
import io.ktor.http.contentType
import io.ktor.http.encodeURLPathPart
import io.ktor.utils.io.readRemaining
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeout
import kotlinx.io.readByteArray
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.long
import kotlinx.serialization.json.put

/** Explicit transport and reduction bounds for one mobile H1 client. */
public data class HostClientLimits(
    public val maxCommandBytes: Int,
    public val maxEventBytes: Int,
    public val maxEvents: Int,
    public val followDeadlineMs: Long,
)

/** Bounded loopback H1 HTTP/SSE client shared by Android and iOS. */
public class LiveHostClient private constructor(
    baseUrl: String,
    private val authorization: RemoteAuthorization?,
    private val limits: HostClientLimits,
    private val client: HttpClient,
) : MobileHost {
    /** Creates a production client with fixed no-redirect CIO transport policy. */
    @Throws(HostClientException::class)
    public constructor(baseUrl: String, limits: HostClientLimits) :
        this(baseUrl, null, limits, defaultHostHttpClient())

    /** Creates an authenticated remote client that accepts only a public HTTPS DNS origin. */
    @Throws(HostClientException::class)
    public constructor(baseUrl: String, bearerToken: String, limits: HostClientLimits) :
        this(baseUrl, bearerAuthorization(bearerToken), limits, defaultHostHttpClient())

    internal constructor(baseUrl: String, limits: HostClientLimits, client: HttpClient) :
        this(baseUrl, null, limits, client)

    internal constructor(
        baseUrl: String,
        bearerToken: String,
        limits: HostClientLimits,
        client: HttpClient,
    ) : this(baseUrl, bearerAuthorization(bearerToken), limits, client)

    private val origin: String = validateBaseUrl(baseUrl, authorization != null)

    init {
        if (limits.maxCommandBytes <= 0 || limits.maxEventBytes <= 0 ||
            limits.maxEvents <= 0 || limits.followDeadlineMs <= 0
        ) fail(HostClientError.INVALID_CONFIGURATION)
    }

    /** Loads installed Agent definitions available for new Sessions. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun agentDefinitions(): AgentDefinitionPageV1 =
        decodeAgentDefinitionPage(read("/v1/agent-definitions"))

    /** Loads one bounded reverse-opened Session page. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun sessions(limit: Int): SessionPageV1 {
        if (limit <= 0 || limit > limits.maxEvents) fail(HostClientError.INVALID_COMMAND)
        return decodeSessionPage(read("/v1/sessions?limit=$limit"))
    }

    /** Loads one exact durable Session summary. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun session(sessionId: String): SessionViewV1 {
        if (sessionId.isEmpty()) fail(HostClientError.INVALID_COMMAND)
        val value = decodeSessionView(read("/v1/sessions/${sessionId.encodeURLPathPart()}"))
        if (value.session?.session_id != sessionId) fail(HostClientError.INVALID_EVENT)
        return value
    }

    /** Loads complete durable Turns changed after the supplied watermark. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun timeline(sessionId: String, afterPosition: Long, limit: Int): TurnTimelinePageV1 {
        if (sessionId.isEmpty() || afterPosition < 0 || limit <= 0 || limit > limits.maxEvents) {
            fail(HostClientError.INVALID_COMMAND)
        }
        val value = decodeTimelinePage(
            read(
                "/v1/sessions/${sessionId.encodeURLPathPart()}/timeline" +
                    "?after_position=$afterPosition&limit=$limit",
            ),
        )
        if (value.session_id != sessionId) fail(HostClientError.INVALID_EVENT)
        return value
    }

    /** Creates a Session with a caller-owned stable command identity. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun createSession(commandId: String, definitionId: String): CreateSessionResponseV1 {
        if (definitionId.isEmpty()) fail(HostClientError.INVALID_COMMAND)
        val value = post("/v1/sessions", commandId, buildJsonObject { put("agent_definition_id", definitionId) })
        return CreateSessionResponseV1(
            session_id = value.requiredText("session_id"),
            agent_instance_id = value.requiredText("agent_instance_id"),
            committed_position = value.requiredPosition("committed_position"),
        )
    }

    /** Starts one Turn with a caller-owned stable command identity. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun startTurn(commandId: String, sessionId: String, text: String): TurnCommandResponseV1 {
        if (sessionId.isEmpty() || text.isEmpty()) fail(HostClientError.INVALID_COMMAND)
        val agentId = session(sessionId).session?.agent_id.orEmpty()
        if (agentId.isEmpty()) fail(HostClientError.INVALID_EVENT)
        val routed = startTurnDirect(commandId, sessionId, agentId, text)
        val turn = routed.turns.singleOrNull() ?: fail(HostClientError.INVALID_EVENT)
        return TurnCommandResponseV1(
            session_id = routed.session_id,
            turn_id = turn.turn_id,
            execution_id = turn.execution_id,
            committed_position = turn.committed_position,
        )
    }

    /** Reads complete current Session membership metadata. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun sessionMembership(sessionId: String): SessionMembershipV1 {
        if (sessionId.isEmpty()) fail(HostClientError.INVALID_COMMAND)
        return decodeMembership(
            read("/v1/sessions/${sessionId.encodeURLPathPart()}/agents"),
            sessionId,
            limits.maxEvents,
        )
    }

    /** Adds one Agent identity to Session membership metadata. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun addSessionAgent(
        commandId: String,
        sessionId: String,
        agentId: String,
    ): SessionMembershipV1 {
        if (sessionId.isEmpty() || agentId.isEmpty()) fail(HostClientError.INVALID_COMMAND)
        return decodeMembership(
            post(
                "/v1/sessions/${sessionId.encodeURLPathPart()}/agents",
                commandId,
                buildJsonObject { put("agent_id", agentId) },
            ),
            sessionId,
            limits.maxEvents,
        )
    }

    /** Removes one Agent identity from Session membership metadata. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun removeSessionAgent(
        commandId: String,
        sessionId: String,
        agentId: String,
    ): SessionMembershipV1 {
        if (sessionId.isEmpty() || agentId.isEmpty()) fail(HostClientError.INVALID_COMMAND)
        return decodeMembership(
            post(
                "/v1/sessions/${sessionId.encodeURLPathPart()}/agents/${agentId.encodeURLPathPart()}/remove",
                commandId,
                buildJsonObject {},
            ),
            sessionId,
            limits.maxEvents,
        )
    }

    /** Starts exactly one current Session member Turn. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun startTurnDirect(
        commandId: String,
        sessionId: String,
        agentId: String,
        text: String,
    ): StartTurnsResponseV1 {
        if (sessionId.isEmpty() || agentId.isEmpty() || text.isEmpty()) fail(HostClientError.INVALID_COMMAND)
        val value = post(
            "/v1/sessions/${sessionId.encodeURLPathPart()}/turns",
            commandId,
            buildJsonObject {
                put("text", text); put("delivery", "direct"); put("agent_id", agentId)
            },
        )
        return decodeStartTurns(value, sessionId, "direct", agentId, limits.maxEvents)
    }

    /** Atomically starts one Turn for every current Session member. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun startTurnBroadcast(
        commandId: String,
        sessionId: String,
        text: String,
    ): StartTurnsResponseV1 {
        if (sessionId.isEmpty() || text.isEmpty()) fail(HostClientError.INVALID_COMMAND)
        val value = post(
            "/v1/sessions/${sessionId.encodeURLPathPart()}/turns",
            commandId,
            buildJsonObject { put("text", text); put("delivery", "broadcast") },
        )
        return decodeStartTurns(value, sessionId, "broadcast", null, limits.maxEvents)
    }

    /** Requests cancellation through one observed durable position. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun cancelTurn(
        commandId: String, sessionId: String, turnId: String, requestedThroughPosition: Long,
    ): TurnCommandResponseV1 {
        if (sessionId.isEmpty() || turnId.isEmpty() || requestedThroughPosition <= 0) {
            fail(HostClientError.INVALID_COMMAND)
        }
        return postTurn(
            eventsCancelPath(sessionId, turnId), commandId, sessionId, turnId,
            buildJsonObject { put("requested_through_position", requestedThroughPosition) },
        )
    }

    /** Steers one Open Turn with plain-text input. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun steerTurn(
        commandId: String, sessionId: String, turnId: String, text: String,
    ): TurnCommandResponseV1 {
        if (sessionId.isEmpty() || turnId.isEmpty() || text.isEmpty()) fail(HostClientError.INVALID_COMMAND)
        return postTurn(
            eventsInputPath(sessionId, turnId), commandId, sessionId, turnId,
            buildJsonObject {
                put("kind", "steer"); put("session_id", sessionId); put("text", text)
            },
        )
    }

    /** Submits an operator decision against one ApprovalRequired suspension. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun approvalEvent(
        commandId: String,
        sessionId: String,
        turnId: String,
        suspensionId: String,
        expectedSessionVersion: Long,
        approve: Boolean,
    ): TurnCommandResponseV1 {
        if (sessionId.isEmpty() || turnId.isEmpty() || suspensionId.isEmpty() || expectedSessionVersion <= 0) {
            fail(HostClientError.INVALID_COMMAND)
        }
        return postTurn(
            eventsInputPath(sessionId, turnId), commandId, sessionId, turnId,
            buildJsonObject {
                put("kind", "approval"); put("session_id", sessionId)
                put("suspension_id", suspensionId); put("expected_session_version", expectedSessionVersion)
                put("decision", if (approve) "approve" else "deny")
            },
        )
    }

    /** Submits an RFC 8785 typed JSON reply against a schema-bound ExternalInputRequired. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun askReplyEvent(
        commandId: String,
        sessionId: String,
        turnId: String,
        suspensionId: String,
        expectedSessionVersion: Long,
        inputJson: String,
    ): TurnCommandResponseV1 {
        if (sessionId.isEmpty() || turnId.isEmpty() || suspensionId.isEmpty() ||
            expectedSessionVersion <= 0 || inputJson.isEmpty()
        ) fail(HostClientError.INVALID_COMMAND)
        return postTurn(
            eventsInputPath(sessionId, turnId), commandId, sessionId, turnId,
            buildJsonObject {
                put("kind", "ask_reply"); put("session_id", sessionId)
                put("suspension_id", suspensionId); put("expected_session_version", expectedSessionVersion)
                put("input_json", inputJson)
            },
        )
    }

    /** Submits plain-text input against a schema-less ExternalInputRequired or PartialOutput. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun externalInputEvent(
        commandId: String,
        sessionId: String,
        turnId: String,
        suspensionId: String,
        expectedSessionVersion: Long,
        text: String,
    ): TurnCommandResponseV1 {
        if (sessionId.isEmpty() || turnId.isEmpty() || suspensionId.isEmpty() ||
            expectedSessionVersion <= 0 || text.isEmpty()
        ) fail(HostClientError.INVALID_COMMAND)
        return postTurn(
            eventsInputPath(sessionId, turnId), commandId, sessionId, turnId,
            buildJsonObject {
                put("kind", "external_input"); put("session_id", sessionId)
                put("suspension_id", suspensionId); put("expected_session_version", expectedSessionVersion)
                put("text", text)
            },
        )
    }

    private fun eventsInputPath(sessionId: String, turnId: String): String =
        "/v1/sessions/${sessionId.encodeURLPathPart()}/turns/${turnId.encodeURLPathPart()}/events"

    private fun eventsCancelPath(sessionId: String, turnId: String): String =
        "/v1/sessions/${sessionId.encodeURLPathPart()}/turns/${turnId.encodeURLPathPart()}/cancel"

    /** Follows committed events until an explicit durable terminal. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun followUntilTerminal(sessionId: String, afterPosition: Long): HostView {
        if (sessionId.isEmpty() || afterPosition < 0) fail(HostClientError.INVALID_COMMAND)
        try {
            return withTimeout(limits.followDeadlineMs) {
                var view = HostView(cursor = afterPosition)
                var count = 0
                client.serverSentEvents(
                    urlString = "$origin/v1/sessions/${sessionId.encodeURLPathPart()}/events?after_position=$afterPosition",
                    request = {
                        accept(ContentType.Text.EventStream)
                        authorization?.let { header(HttpHeaders.Authorization, it.header) }
                    },
                ) {
                    incoming.first { wire ->
                        val data = wire.data ?: return@first false
                        if (data.encodeToByteArray().size > limits.maxEventBytes || ++count > limits.maxEvents) {
                            fail(HostClientError.EVENT_LIMIT_EXCEEDED)
                        }
                        val value = decodeObject(data)
                        val event = HostEventV1(
                            api_version = value.requiredText("api_version"),
                            session_id = value.requiredText("session_id"),
                            position = value.requiredPosition("position"),
                            event = value.requiredText("event"),
                            turn_id = value.optionalText("turn_id"),
                            execution_id = value.optionalText("execution_id"),
                            text = value.optionalText("text"),
                        )
                        view = reduceHostEvents(sessionId, listOf(event), view, limits.maxEvents)
                        view.terminal != null
                    }
                }
                if (view.terminal == null) fail(HostClientError.TRANSPORT_FAILURE)
                view
            }
        } catch (error: HostClientException) {
            throw error
        } catch (error: TimeoutCancellationException) {
            fail(HostClientError.FOLLOW_DEADLINE)
        } catch (error: CancellationException) {
            throw error
        } catch (_: Throwable) {
            fail(HostClientError.TRANSPORT_FAILURE)
        }
    }

    private suspend fun postTurn(
        path: String, commandId: String, sessionId: String, turnId: String?, body: JsonObject,
    ): TurnCommandResponseV1 {
        val value = post(path, commandId, body)
        val response = TurnCommandResponseV1(
            session_id = value.requiredText("session_id"),
            turn_id = value.requiredText("turn_id"),
            execution_id = value.requiredText("execution_id"),
            committed_position = value.requiredPosition("committed_position"),
        )
        if (response.session_id != sessionId || turnId != null && response.turn_id != turnId) {
            fail(HostClientError.INVALID_EVENT)
        }
        return response
    }

    private suspend fun post(path: String, commandId: String, body: JsonObject): JsonObject {
        if (!validCommandId(commandId)) fail(HostClientError.INVALID_COMMAND)
        val encoded = body.toString()
        if (encoded.encodeToByteArray().size > limits.maxCommandBytes) fail(HostClientError.INVALID_COMMAND)
        val response = try {
            client.post { url(origin + path); contentType(ContentType.Application.Json)
                header(IDEMPOTENCY_KEY, commandId)
                authorization?.let { header(HttpHeaders.Authorization, it.header) }
                setBody(encoded) }
        } catch (_: Throwable) { fail(HostClientError.TRANSPORT_FAILURE) }
        return decodeResponse(response)
    }

    private suspend fun read(path: String): JsonObject {
        val response = try {
            client.get {
                url(origin + path)
                accept(ContentType.Application.Json)
                authorization?.let { header(HttpHeaders.Authorization, it.header) }
            }
        } catch (error: CancellationException) {
            throw error
        } catch (_: Throwable) {
            fail(HostClientError.TRANSPORT_FAILURE)
        }
        return decodeResponse(response)
    }

    private suspend fun decodeResponse(response: HttpResponse): JsonObject {
        if (response.status.value in 300..399) fail(HostClientError.TRANSPORT_FAILURE)
        val bytes = response.bodyAsChannel().readRemaining((limits.maxEventBytes + 1).toLong()).readByteArray()
        if (bytes.size > limits.maxEventBytes) fail(HostClientError.INVALID_EVENT)
        val raw = bytes.decodeToString()
        if (response.status.value !in 200..299) {
            val code = runCatching { decodeObject(raw).optionalText("code") }.getOrDefault("")
            fail(classifyServerError(code), response.status.value)
        }
        return decodeObject(raw)
    }
}

/** Creates the default no-retry CIO client; all addresses and bounds remain constructor inputs. */
internal fun defaultHostHttpClient(): HttpClient = HttpClient(CIO) {
    followRedirects = false
    install(HttpTimeout)
    install(SSE)
}

private fun decodeMembership(value: JsonObject, sessionId: String, maxMembers: Int): SessionMembershipV1 {
    val rawMembers = value["members"]?.jsonArray ?: fail(HostClientError.INVALID_EVENT)
    if (rawMembers.size > maxMembers) fail(HostClientError.INVALID_EVENT)
    val members = rawMembers.map { raw ->
        val member = raw.jsonObject
        SessionMemberV1(
            agent_id = member.requiredText("agent_id"),
            joined_position = member.requiredPosition("joined_position"),
        )
    }
    val observed = value.requiredPosition("observed_max_position")
    if (value.requiredText("api_version") != "v1" || value.requiredText("session_id") != sessionId ||
        members.map { it.agent_id }.toSet().size != members.size ||
        members.any { it.joined_position > observed } ||
        members.zipWithNext().any { (left, right) -> left.joined_position >= right.joined_position }
    ) fail(HostClientError.INVALID_EVENT)
    return SessionMembershipV1(
        api_version = "v1",
        session_id = sessionId,
        members = members,
        observed_max_position = observed,
    )
}

private fun decodeStartTurns(
    value: JsonObject,
    sessionId: String,
    delivery: String,
    directAgentId: String?,
    maxTurns: Int,
): StartTurnsResponseV1 {
    val rawTurns = value["turns"]?.jsonArray ?: fail(HostClientError.INVALID_EVENT)
    if (rawTurns.isEmpty() || rawTurns.size > maxTurns) fail(HostClientError.INVALID_EVENT)
    val turns = rawTurns.map { raw ->
        val turn = raw.jsonObject
        DeliveredTurnResponseV1(
            agent_id = turn.requiredText("agent_id"),
            turn_id = turn.requiredText("turn_id"),
            execution_id = turn.requiredText("execution_id"),
            committed_position = turn.requiredPosition("committed_position"),
        )
    }
    if (value.requiredText("api_version") != "v1" || value.requiredText("session_id") != sessionId ||
        value.requiredText("delivery") != delivery || turns.map { it.agent_id }.toSet().size != turns.size ||
        directAgentId != null && (turns.size != 1 || turns.single().agent_id != directAgentId)
    ) fail(HostClientError.INVALID_EVENT)
    return StartTurnsResponseV1(
        api_version = "v1",
        session_id = sessionId,
        delivery = if (delivery == "direct") {
            TurnDeliveryV1.TURN_DELIVERY_V1_DIRECT
        } else {
            TurnDeliveryV1.TURN_DELIVERY_V1_BROADCAST
        },
        turns = turns,
    )
}

private const val IDEMPOTENCY_KEY: String = "Idempotency-Key"
private val JSON: Json = Json { ignoreUnknownKeys = true }
private val KNOWN_HOST_ERRORS: Set<String> = setOf(
    "invalid_request", "not_found", "command_conflict", "concurrent_modification",
    "precondition_failed", "durability_unavailable", "corrupt_state",
)

internal fun classifyServerError(code: String): HostClientError = when (code) {
    "authentication_required" -> HostClientError.AUTHENTICATION_REQUIRED
    "actor_forbidden" -> HostClientError.ACTOR_FORBIDDEN
    "device_reauth_required" -> HostClientError.DEVICE_REAUTH_REQUIRED
    "rate_limited" -> HostClientError.RATE_LIMITED
    "runtime_unavailable" -> HostClientError.RUNTIME_UNAVAILABLE
    "pairing_rejected" -> HostClientError.PAIRING_REJECTED
    in KNOWN_HOST_ERRORS -> HostClientError.HOST_FAILURE
    else -> HostClientError.UNKNOWN_HOST_ERROR
}

internal fun validateBaseUrl(value: String, remote: Boolean): String {
    val url = runCatching { Url(value) }.getOrElse { fail(HostClientError.INVALID_CONFIGURATION) }
    val loopback = url.host in setOf("localhost", "127.0.0.1", "::1")
    val validRemoteHost = url.host.isNotEmpty() && !loopback && ':' !in url.host &&
        !url.host.endsWith(".local") && !url.host.all { it.isDigit() || it == '.' }
    if ((!remote && (url.protocol.name != "http" || !loopback)) ||
        (remote && (url.protocol.name != "https" || !validRemoteHost)) ||
        url.encodedPath != "/" || url.parameters.entries().isNotEmpty() || url.trailingQuery ||
        url.fragment.isNotEmpty() || !url.user.isNullOrEmpty() || !url.password.isNullOrEmpty()
    ) fail(HostClientError.INVALID_CONFIGURATION)
    val renderedHost = if (":" in url.host) "[${url.host}]" else url.host
    return "${url.protocol.name}://$renderedHost:${url.port}"
}

/** Validates and canonicalizes one public HTTPS Gateway origin before native UI displays it. */
@Throws(HostClientException::class)
public fun validateRemoteHostOrigin(value: String): String = validateBaseUrl(value, remote = true)

private class RemoteAuthorization(val header: String)

private fun bearerAuthorization(token: String): RemoteAuthorization {
    if (token.isEmpty() || token.length > 4_096 || token.any { it.code !in 0x21..0x7e }) {
        fail(HostClientError.INVALID_CONFIGURATION)
    }
    return RemoteAuthorization("Bearer $token")
}

private fun validCommandId(value: String): Boolean =
    value.isNotEmpty() && value.length <= 128 && value.all { it.code in 0x21..0x7e }

private fun decodeObject(value: String): JsonObject = try {
    JSON.parseToJsonElement(value).jsonObject
} catch (_: Throwable) { fail(HostClientError.INVALID_EVENT) }

private fun JsonObject.requiredText(key: String): String = optionalText(key).ifEmpty {
    fail(HostClientError.INVALID_EVENT)
}
private fun JsonObject.optionalText(key: String): String = this[key]?.jsonPrimitive?.content.orEmpty()
private fun JsonObject.requiredPosition(key: String): Long = try {
    this.getValue(key).jsonPrimitive.long.also { if (it <= 0) fail(HostClientError.INVALID_EVENT) }
} catch (error: HostClientException) { throw error
} catch (_: Throwable) { fail(HostClientError.INVALID_EVENT) }
