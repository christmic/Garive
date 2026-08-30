package com.garive.mobile.host

import com.garive.host.v1.CreateSessionResponseV1
import com.garive.host.v1.AgentDefinitionPageV1
import com.garive.host.v1.HostEventV1
import com.garive.host.v1.SessionPageV1
import com.garive.host.v1.SessionViewV1
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
        return postTurn(
            "/v1/sessions/${sessionId.encodeURLPathPart()}/turns", commandId, sessionId, null,
            buildJsonObject { put("text", text) },
        )
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
            "/v1/turns/${turnId.encodeURLPathPart()}:cancel", commandId, sessionId, turnId,
            buildJsonObject {
                put("session_id", sessionId); put("requested_through_position", requestedThroughPosition)
            },
        )
    }

    /** Continues one exact durable suspension. */
    @Throws(HostClientException::class, CancellationException::class)
    override suspend fun continueTurn(
        commandId: String,
        sessionId: String,
        turnId: String,
        suspensionId: String,
        expectedSessionVersion: Long,
        input: String,
    ): TurnCommandResponseV1 {
        if (sessionId.isEmpty() || turnId.isEmpty() || suspensionId.isEmpty() ||
            expectedSessionVersion <= 0 || input.isEmpty()
        ) fail(HostClientError.INVALID_COMMAND)
        return postTurn(
            "/v1/turns/${turnId.encodeURLPathPart()}:continue", commandId, sessionId, turnId,
            buildJsonObject {
                put("session_id", sessionId); put("suspension_id", suspensionId)
                put("expected_session_version", expectedSessionVersion); put("input", input)
            },
        )
    }

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
            fail(if (code in KNOWN_HOST_ERRORS) HostClientError.HOST_FAILURE else HostClientError.UNKNOWN_HOST_ERROR,
                response.status.value)
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

private const val IDEMPOTENCY_KEY: String = "Idempotency-Key"
private val JSON: Json = Json { ignoreUnknownKeys = true }
private val KNOWN_HOST_ERRORS: Set<String> = setOf(
    "invalid_request", "not_found", "command_conflict", "concurrent_modification",
    "precondition_failed", "durability_unavailable", "corrupt_state",
)

private fun validateBaseUrl(value: String, remote: Boolean): String {
    val url = runCatching { Url(value) }.getOrElse { fail(HostClientError.INVALID_CONFIGURATION) }
    val loopback = url.host in setOf("localhost", "127.0.0.1", "::1")
    val validRemoteHost = url.host.isNotEmpty() && !loopback && ':' !in url.host &&
        !url.host.endsWith(".local") && !url.host.all { it.isDigit() || it == '.' }
    if ((!remote && (url.protocol.name != "http" || !loopback)) ||
        (remote && (url.protocol.name != "https" || !validRemoteHost)) ||
        url.encodedPath != "/" || url.parameters.entries().isNotEmpty() || url.fragment.isNotEmpty()
    ) fail(HostClientError.INVALID_CONFIGURATION)
    val renderedHost = if (":" in url.host) "[${url.host}]" else url.host
    return "${url.protocol.name}://$renderedHost:${url.port}"
}

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
