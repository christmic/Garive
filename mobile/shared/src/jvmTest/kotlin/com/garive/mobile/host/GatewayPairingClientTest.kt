package com.garive.mobile.host

import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.ContentType
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.TextContent
import io.ktor.http.headersOf
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertFalse
import kotlin.test.assertTrue
import kotlinx.coroutines.runBlocking

public class GatewayPairingClientTest {
    @Test
    public fun exchangesStrictDeviceCeremonyWithoutAuthorization(): Unit = runBlocking {
        var path = ""
        var body = ""
        var authorizationPresent = false
        val engine = MockEngine { request ->
            path = request.url.encodedPath
            body = (request.body as TextContent).text
            authorizationPresent = request.headers.contains("Authorization")
            respond(
                content = """{"api_version":"v1","access_grant":"grant-at-least-twenty-characters","device_id":"device-1","expires_at":"2026-09-29T00:00:00Z"}""",
                status = HttpStatusCode.Created,
                headers = headersOf("Content-Type", ContentType.Application.Json.toString()),
            )
        }
        val client = GatewayPairingClient("https://agent.example.test/", 8_192, HttpClient(engine))
        val grant = client.exchange("one-time-code", "Test phone", MobilePlatform.ANDROID, publicKey())

        assertEquals("/v1/mobile/pair", path)
        assertTrue("\"platform\":\"android\"" in body)
        assertTrue("\"code\":\"one-time-code\"" in body)
        assertFalse(authorizationPresent)
        assertEquals("device-1", grant.deviceId)
        assertTrue("grant-at-least" !in grant.toString())
    }

    @Test
    public fun rejectsUnsafeOriginAndMalformedKeyBeforeTransport(): Unit {
        assertEquals(
            HostClientError.INVALID_CONFIGURATION,
            assertFailsWith<HostClientException> { GatewayPairingClient("http://agent.example.test/") }.code,
        )
        val client = GatewayPairingClient("https://agent.example.test/", 8_192, HttpClient(MockEngine { error("called") }))
        assertEquals(
            HostClientError.INVALID_COMMAND,
            assertFailsWith<HostClientException> {
                runBlocking { client.exchange("one-time-code", "Phone", MobilePlatform.IOS, "not+a+base64/key") }
            }.code,
        )
    }

    @Test
    public fun mapsRejectedPairingWithoutLeakingResponse(): Unit = runBlocking {
        val engine = MockEngine {
            respond(
                content = """{"code":"pairing_rejected","secret":"must-not-leak"}""",
                status = HttpStatusCode.Unauthorized,
                headers = headersOf("Content-Type", ContentType.Application.Json.toString()),
            )
        }
        val client = GatewayPairingClient("https://agent.example.test/", 8_192, HttpClient(engine))
        val error = assertFailsWith<HostClientException> {
            client.exchange("wrong-code", "Phone", MobilePlatform.IOS, publicKey())
        }
        assertEquals(HostClientError.PAIRING_REJECTED, error.code)
        assertFalse("must-not-leak" in error.toString())
    }

    private fun publicKey(): String = "A".repeat(43)
}
