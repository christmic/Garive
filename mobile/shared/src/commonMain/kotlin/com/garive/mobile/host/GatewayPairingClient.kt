package com.garive.mobile.host

import io.ktor.client.HttpClient
import io.ktor.client.request.post
import io.ktor.client.request.setBody
import io.ktor.client.request.url
import io.ktor.client.statement.bodyAsChannel
import io.ktor.http.ContentType
import io.ktor.http.contentType
import io.ktor.utils.io.readRemaining
import kotlinx.coroutines.CancellationException
import kotlinx.io.readByteArray
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.serialization.json.put

/** Native platform identifier admitted by the Gateway pairing ceremony. */
public enum class MobilePlatform(public val wireName: String) { IOS("ios"), ANDROID("android") }

/** Device-scoped result whose debug form never contains the access grant. */
public class PairingGrant internal constructor(
    public val accessGrant: String,
    public val deviceId: String,
    public val expiresAt: String,
) {
    override fun toString(): String = "PairingGrant(accessGrant=<redacted>)"
}

/** Strict one-time pairing exchange for a public HTTPS Gateway origin. */
public class GatewayPairingClient internal constructor(
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

    /** Exchanges one operator code and native device public key for a revocable grant. */
    @Throws(HostClientException::class, CancellationException::class)
    public suspend fun exchange(
        code: String,
        deviceName: String,
        platform: MobilePlatform,
        devicePublicKey: String,
    ): PairingGrant {
        if (code.length !in 6..128 || deviceName.length !in 1..100 ||
            devicePublicKey.length !in 43..2_731 || !devicePublicKey.all(::isBase64Url)
        ) fail(HostClientError.INVALID_COMMAND)
        val body = buildJsonObject {
            put("api_version", API_VERSION)
            put("code", code)
            put("device_name", deviceName)
            put("platform", platform.wireName)
            put("device_public_key", devicePublicKey)
        }.toString()
        val response = try {
            client.post {
                url("$origin/v1/mobile/pair")
                contentType(ContentType.Application.Json)
                setBody(body)
            }
        } catch (error: CancellationException) {
            throw error
        } catch (_: Throwable) {
            fail(HostClientError.TRANSPORT_FAILURE)
        }
        val bytes = response.bodyAsChannel().readRemaining((maxResponseBytes + 1).toLong()).readByteArray()
        if (bytes.size > maxResponseBytes) fail(HostClientError.INVALID_EVENT)
        val value = try {
            Json.parseToJsonElement(bytes.decodeToString()).jsonObject
        } catch (_: Throwable) {
            fail(HostClientError.INVALID_EVENT)
        }
        if (response.status.value != 201) {
            fail(classifyServerError(value["code"]?.jsonPrimitive?.content.orEmpty()), response.status.value)
        }
        val version = value["api_version"]?.jsonPrimitive?.content.orEmpty()
        val grant = value["access_grant"]?.jsonPrimitive?.content.orEmpty()
        val deviceId = value["device_id"]?.jsonPrimitive?.content.orEmpty()
        val expiresAt = value["expires_at"]?.jsonPrimitive?.content.orEmpty()
        if (version != API_VERSION || grant.length !in 20..4_096 || deviceId.isEmpty() || expiresAt.isEmpty()) {
            fail(HostClientError.INVALID_EVENT)
        }
        return PairingGrant(grant, deviceId, expiresAt)
    }
}

private fun isBase64Url(character: Char): Boolean =
    character in 'a'..'z' || character in 'A'..'Z' || character in '0'..'9' || character == '-' || character == '_'
