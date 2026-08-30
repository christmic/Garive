package com.garive.mobile.host

import io.ktor.client.HttpClient
import io.ktor.client.request.delete
import io.ktor.client.request.header
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.request.url
import io.ktor.client.statement.HttpResponse
import io.ktor.client.statement.bodyAsChannel
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.contentType
import io.ktor.utils.io.readRemaining
import kotlinx.coroutines.CancellationException
import kotlinx.io.readByteArray
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** Provider transport selected by the native operating system. */
public enum class MobilePushTransport(public val wireName: String) { APNS("apns"), FCM("fcm") }

/** Authenticated destination resolved from a content-free wake hint. */
public class MobileWakeRoute internal constructor(
    public val destination: String,
    public val sessionId: String?,
    public val category: String,
)

/** Strict authenticated push-registration and wake-resolution client. */
public class GatewayNotificationClient internal constructor(
    baseUrl: String,
    private val maxResponseBytes: Int,
    private val client: HttpClient,
) {
    @Throws(HostClientException::class)
    public constructor(baseUrl: String, maxResponseBytes: Int = 8_192) :
        this(baseUrl, maxResponseBytes, defaultHostHttpClient())

    private val origin: String = validateBaseUrl(baseUrl, remote = true)

    init {
        if (maxResponseBytes !in 1..65_536) fail(HostClientError.INVALID_CONFIGURATION)
    }

    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun register(accessGrant: String, transport: MobilePushTransport, token: String): Unit {
        validateGrant(accessGrant)
        if (token.length !in 20..4_096 || token.any { it.code !in 0x21..0x7e }) {
            fail(HostClientError.INVALID_COMMAND)
        }
        val body = buildJsonObject {
            put("api_version", API_VERSION)
            put("transport", transport.wireName)
            put("token", token)
        }.toString()
        val response = request {
            client.post {
                url("$origin/v1/mobile/push/registrations")
                header(HttpHeaders.Authorization, "Bearer $accessGrant")
                contentType(ContentType.Application.Json)
                setBody(body)
            }
        }
        requireStatus(response, 204)
    }

    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun unregister(accessGrant: String): Unit {
        validateGrant(accessGrant)
        val response = request {
            client.delete {
                url("$origin/v1/mobile/push/registrations/self")
                header(HttpHeaders.Authorization, "Bearer $accessGrant")
            }
        }
        requireStatus(response, 204)
    }

    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun resolve(accessGrant: String, routeToken: String): MobileWakeRoute {
        validateGrant(accessGrant)
        if (routeToken.length != 43 || !routeToken.all(::isWakeTokenCharacter)) fail(HostClientError.INVALID_COMMAND)
        val response = request {
            client.post {
                url("$origin/v1/mobile/wake/$routeToken:resolve")
                header(HttpHeaders.Authorization, "Bearer $accessGrant")
            }
        }
        val value = readObject(response)
        if (response.status.value != 200) {
            fail(classifyServerError(value["code"]?.jsonPrimitive?.content.orEmpty()), response.status.value)
        }
        val allowed = setOf("api_version", "destination", "session_id", "category")
        if (value.keys != allowed || value["api_version"]?.jsonPrimitive?.content != API_VERSION) {
            fail(HostClientError.INVALID_EVENT)
        }
        val destination = value["destination"]?.jsonPrimitive?.content.orEmpty()
        val session = value["session_id"]?.jsonPrimitive?.content.orEmpty()
        val category = value["category"]?.jsonPrimitive?.content.orEmpty()
        val validCategory = category in setOf("attention", "completed", "failed", "connection_security")
        if (!validCategory || (destination == "settings" && session.isNotEmpty()) ||
            (destination == "session" && session.isEmpty()) || destination !in setOf("settings", "session")
        ) fail(HostClientError.INVALID_EVENT)
        return MobileWakeRoute(destination, session.ifEmpty { null }, category)
    }

    private suspend fun request(block: suspend () -> HttpResponse): HttpResponse = try {
        block()
    } catch (error: CancellationException) {
        throw error
    } catch (_: Throwable) {
        fail(HostClientError.TRANSPORT_FAILURE)
    }

    private suspend fun requireStatus(response: HttpResponse, expected: Int) {
        if (response.status.value == expected) return
        val value = readObject(response)
        fail(classifyServerError(value["code"]?.jsonPrimitive?.content.orEmpty()), response.status.value)
    }

    private suspend fun readObject(response: HttpResponse): JsonObject {
        val bytes = response.bodyAsChannel().readRemaining((maxResponseBytes + 1).toLong()).readByteArray()
        if (bytes.size > maxResponseBytes) fail(HostClientError.INVALID_EVENT)
        return try {
            Json.parseToJsonElement(bytes.decodeToString()).jsonObject
        } catch (_: Throwable) {
            fail(HostClientError.INVALID_EVENT)
        }
    }

    private fun validateGrant(value: String) {
        if (value.length !in 20..4_096 || value.any { it.code !in 0x21..0x7e }) {
            fail(HostClientError.INVALID_COMMAND)
        }
    }
}

private fun isWakeTokenCharacter(character: Char): Boolean =
    character in 'a'..'z' || character in 'A'..'Z' || character in '0'..'9' || character == '-' || character == '_'
