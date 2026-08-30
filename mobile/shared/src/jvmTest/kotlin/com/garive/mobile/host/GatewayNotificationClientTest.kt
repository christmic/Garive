package com.garive.mobile.host

import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.content.TextContent
import io.ktor.http.headersOf
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.runBlocking

public class GatewayNotificationClientTest {
    @Test
    public fun registersExactPlatformTokenWithGrant(): Unit = runBlocking {
        val engine = MockEngine { request ->
            assertEquals("https://agent.example.test/v1/mobile/push/registrations", request.url.toString())
            assertEquals("Bearer mobile-grant-at-least-twenty", request.headers[HttpHeaders.Authorization])
            val body = (request.body as TextContent).text
            assertEquals(
                "{\"api_version\":\"v1\",\"transport\":\"fcm\",\"registration_id\":\"provider-token-at-least-twenty\"}",
                body,
            )
            respond("", HttpStatusCode.NoContent)
        }
        val client = GatewayNotificationClient("https://agent.example.test/", 8_192, HttpClient(engine))
        client.register("mobile-grant-at-least-twenty", MobilePushTransport.FCM, "provider-token-at-least-twenty")
    }

    @Test
    public fun unregistersDedicatedRegistration(): Unit = runBlocking {
        val engine = MockEngine { request ->
            assertEquals("DELETE", request.method.value)
            assertEquals("/v1/mobile/push/registrations/self", request.url.encodedPath)
            assertEquals("Bearer mobile-grant-at-least-twenty", request.headers[HttpHeaders.Authorization])
            respond("", HttpStatusCode.NoContent)
        }
        GatewayNotificationClient("https://agent.example.test/", 8_192, HttpClient(engine))
            .unregister("mobile-grant-at-least-twenty")
    }

    @Test
    public fun resolvesOpaqueWakeOnlyAfterAuthentication(): Unit = runBlocking {
        val token = "r".repeat(43)
        val engine = MockEngine { request ->
            assertEquals("/v1/mobile/wake/$token:resolve", request.url.encodedPath)
            assertEquals("Bearer mobile-grant-at-least-twenty", request.headers[HttpHeaders.Authorization])
            respond(
                """{"api_version":"v1","destination":"session","session_id":"session_1","category":"attention"}""",
                HttpStatusCode.OK,
                headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString()),
            )
        }
        val route = GatewayNotificationClient("https://agent.example.test/", 8_192, HttpClient(engine))
            .resolve("mobile-grant-at-least-twenty", token)
        assertEquals("session", route.destination)
        assertEquals("session_1", route.sessionId)
        assertEquals("attention", route.category)
    }

    @Test
    public fun acceptsSettingsRouteWithoutSession(): Unit = runBlocking {
        val engine = MockEngine {
            respond(
                """{"api_version":"v1","destination":"settings","session_id":"","category":"connection_security"}""",
                HttpStatusCode.OK,
            )
        }
        val route = GatewayNotificationClient("https://agent.example.test/", 8_192, HttpClient(engine))
            .resolve("mobile-grant-at-least-twenty", "r".repeat(43))
        assertEquals("settings", route.destination)
        assertNull(route.sessionId)
    }

    @Test
    public fun rejectsMalformedOrExtendedWakeResponse(): Unit = runBlocking {
        val engine = MockEngine {
            respond(
                """{"api_version":"v1","destination":"session","session_id":"session_1","category":"attention","prompt":"secret"}""",
                HttpStatusCode.OK,
            )
        }
        val error = assertFailsWith<HostClientException> {
            GatewayNotificationClient("https://agent.example.test/", 8_192, HttpClient(engine))
                .resolve("mobile-grant-at-least-twenty", "r".repeat(43))
        }
        assertEquals(HostClientError.INVALID_EVENT, error.code)
        assertTrue("secret" !in error.toString())
    }

    @Test
    public fun rejectsTokenBeforeNetwork(): Unit = runBlocking {
        val client = GatewayNotificationClient(
            "https://agent.example.test/",
            8_192,
            HttpClient(MockEngine { error("network called") }),
        )
        val error = assertFailsWith<HostClientException> {
            client.resolve("mobile-grant-at-least-twenty", "not-a-token")
        }
        assertEquals(HostClientError.INVALID_COMMAND, error.code)
    }
}
