package com.garive.mobile.host

import com.garive.host.v1.HostEventV1
import com.garive.host.v1.TurnDeliveryV1
import io.ktor.client.HttpClient
import io.ktor.client.engine.mock.MockEngine
import io.ktor.client.engine.mock.respond
import io.ktor.client.plugins.sse.SSE
import io.ktor.http.ContentType
import io.ktor.http.HttpHeaders
import io.ktor.http.HttpStatusCode
import io.ktor.http.headersOf
import io.ktor.http.content.TextContent
import com.sun.net.httpserver.HttpServer
import java.net.InetSocketAddress
import java.nio.file.Path
import java.util.concurrent.atomic.AtomicInteger
import kotlin.io.path.readText
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertFailsWith
import kotlin.test.assertNull
import kotlin.test.assertTrue
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.*

public class LiveHostClientTest {
    private val fixture: JsonObject by lazy {
        val root = Path.of(System.getProperty("garive.repo.root"))
        Json.parseToJsonElement(root.resolve("spec/fixtures/host/live-host-client-v1.json").readText()).jsonObject
    }

    @Test
    public fun remoteOriginIsCanonicalBeforeNativePresentation(): Unit {
        assertEquals("https://agent.example.test:443", validateRemoteHostOrigin("https://agent.example.test/"))
        listOf(
            "http://agent.example.test/",
            "https://localhost/",
            "https://192.0.2.1/",
            "https://agent.example.test/path",
            "https://agent.example.test/?query=yes",
            "https://user@agent.example.test/",
        ).forEach { value ->
            assertFailsWith<HostClientException>(value) { validateRemoteHostOrigin(value) }
        }
    }

    @Test
    public fun sharedFixtureCoversGapsReconnectUnknownAndFailures(): Unit {
        val session = fixture.text("session_id")
        val valid = fixture.getValue("valid_stream").jsonArray.map { it.jsonObject.toEvent() }
        val view = reduceHostEvents(session, valid)
        val expected = fixture.getValue("expected").jsonObject
        assertEquals(expected.long("cursor"), view.cursor)
        assertEquals(HostTerminalKind.COMPLETED, view.terminal)
        assertEquals(expected.text("text"), view.text)
        assertEquals(expected.getValue("unknown_events").jsonArray.map { it.jsonPrimitive.content }, view.unknownEvents)

        val prefix = reduceHostEvents(session, valid.take(2))
        val reconnect = fixture.getValue("reconnect").jsonObject
        val resumed = reduceHostEvents(
            session, reconnect.getValue("events").jsonArray.map { it.jsonObject.toEvent() }, prefix,
        )
        assertEquals(listOf(5L, 9L), resumed.fingerprints.keys.filter { it > prefix.cursor })
        val disconnected = reduceHostEvents(
            session, fixture.getValue("disconnect_before_terminal").jsonArray.map { it.jsonObject.toEvent() },
        )
        assertNull(disconnected.terminal)
        assertEquals(
            fixture.getValue("failure_codes").jsonArray.map { it.jsonPrimitive.content },
            HostClientError.entries.map { it.wireName },
        )
    }

    @Test
    public fun sharedInvalidMutationsFailExactly(): Unit {
        val session = fixture.text("session_id")
        val valid = fixture.getValue("valid_stream").jsonArray.map { it.jsonObject.toEvent() }
        fixture.getValue("invalid_streams").jsonArray.forEach { raw ->
            val test = raw.jsonObject
            val events = mutation(test.text("mutation"), valid)
            val error = assertFailsWith<HostClientException> { reduceHostEvents(session, events) }
            assertEquals(test.text("expected"), error.code.wireName, test.text("name"))
        }
    }

    @Test
    public fun generatedHostEventRoundTripsWire(): Unit {
        val event = fixture.getValue("valid_stream").jsonArray.last().jsonObject.toEvent()
        assertEquals(event, HostEventV1.ADAPTER.decode(event.encode()))
    }

    @Test
    public fun realLoopbackTransportUsesH1AndStopsOnlyAtDurableTerminal(): Unit = runBlocking {
        val seen = mutableListOf<String>()
        val events = fixture.getValue("valid_stream").jsonArray.map { it.jsonObject.toEvent() }
        val call = AtomicInteger()
        val server = HttpServer.create(InetSocketAddress("127.0.0.1", 0), 0)
        server.createContext("/") { exchange ->
            seen += "${exchange.requestMethod} ${exchange.requestURI.path} ${exchange.requestHeaders.getFirst("Idempotency-Key").orEmpty()}"
            val (contentType, body) = when (call.getAndIncrement()) {
                0 -> "application/json" to """{"session_id":"session-client","agent_instance_id":"agent-1","committed_position":1}"""
                1 -> "application/json" to """{"api_version":"v1","session_id":"session-client","delivery":"direct","turns":[{"agent_id":"definition-main","turn_id":"turn-client","execution_id":"execution-client","committed_position":2}]}"""
                else -> "text/event-stream" to events.joinToString("") {
                    "id: ${it.position}\nevent: host\ndata: ${it.toJson()}\n\n"
                }
            }
            val bytes = body.encodeToByteArray()
            exchange.responseHeaders.add("Content-Type", contentType)
            exchange.sendResponseHeaders(200, bytes.size.toLong())
            exchange.responseBody.use { it.write(bytes) }
        }
        server.start()
        try {
            val client = LiveHostClient("http://127.0.0.1:${server.address.port}/", limits())
            val session = client.createSession("create-stable", "definition-main")
            val turn = client.startTurnDirect("turn-stable", session.session_id, "definition-main", "hello")
            val view =
            client.followUntilTerminal(session.session_id, turn.turns.single().committed_position)
            assertEquals(HostTerminalKind.COMPLETED, view.terminal)
            assertEquals("durable answer", view.text)
            assertEquals(listOf(
                "POST /v1/sessions create-stable",
                "POST /v1/sessions/session-client/turns turn-stable",
                "GET /v1/sessions/session-client/events ",
            ), seen)
        } finally {
            server.stop(0)
        }
    }

    @Test
    public fun rejectsNonLoopbackAndRedactsKnownHostFailure(): Unit = runBlocking {
        assertEquals(
            HostClientError.INVALID_CONFIGURATION,
            assertFailsWith<HostClientException> { LiveHostClient("http://example.com/", limits()) }.code,
        )
        val engine = MockEngine {
            respondJson("""{"code":"not_found","secret":"must-not-leak"}""", HttpStatusCode.NotFound)
        }
        val client = LiveHostClient("http://localhost:4317/", limits(), HttpClient(engine))
        val error = assertFailsWith<HostClientException> {
            client.createSession("create-stable", "definition-main")
        }
        assertEquals(HostClientError.HOST_FAILURE, error.code)
        assertEquals(404, error.status)
        assertTrue("must-not-leak" !in error.toString())
    }

    @Test
    public fun remoteHttpsSendsBearerWithoutLeakingIt(): Unit = runBlocking {
        var authorization: String? = null
        val engine = MockEngine { request ->
            authorization = request.headers[HttpHeaders.Authorization]
            respondJson(
                """{"session_id":"session-client","agent_instance_id":"agent-1","committed_position":1}""",
            )
        }
        val client = LiveHostClient("https://agent.example.test/", "mobile-secret", limits(), HttpClient(engine))
        client.createSession("create-stable", "definition-main")
        assertEquals("Bearer mobile-secret", authorization)
        assertTrue("mobile-secret" !in client.toString())
    }

    @Test
    public fun remoteConfigurationFailsBeforeTransport(): Unit {
        assertEquals(
            HostClientError.INVALID_CONFIGURATION,
            assertFailsWith<HostClientException> {
                LiveHostClient("https://127.0.0.1/", "token", limits())
            }.code,
        )
        assertEquals(
            HostClientError.INVALID_CONFIGURATION,
            assertFailsWith<HostClientException> {
                LiveHostClient("https://agent.example.test/path", "token", limits())
            }.code,
        )
        assertEquals(
            HostClientError.INVALID_CONFIGURATION,
            assertFailsWith<HostClientException> {
                LiveHostClient("https://agent.example.test/", "", limits())
            }.code,
        )
    }

    @Test
    public fun remoteReadModelsUseExactH2RoutesAndGeneratedValues(): Unit = runBlocking {
        val seen = mutableListOf<String>()
        val engine = MockEngine { request ->
            seen += "${request.method.value} ${request.url.encodedPath}?${request.url.encodedQuery} ${request.headers[HttpHeaders.Authorization]}"
            val body = when (request.url.encodedPath) {
                "/v1/agent-definitions" ->
                    """{"api_version":"v1","definitions":[{"api_version":"v1","definition_id":"definition-main","definition_revision":"revision-1","capabilities":["work"]}]}"""
                "/v1/sessions" ->
                    """{"api_version":"v1","sessions":[{"api_version":"v1","session_id":"session-client","agent_instance_id":"agent-1","definition_id":"definition-main","definition_revision":"revision-1","opened_at":"2026-08-30T00:00:00Z","latest_position":9,"latest_turn_id":"turn-client","latest_turn_state":"suspended","turn_count":1}]}"""
                "/v1/sessions/session-client" ->
                    """{"api_version":"v1","session":{"api_version":"v1","session_id":"session-client","agent_instance_id":"agent-1","definition_id":"definition-main","definition_revision":"revision-1","opened_at":"2026-08-30T00:00:00Z","latest_position":9,"latest_turn_id":"turn-client","latest_turn_state":"suspended","turn_count":1},"observed_max_position":9}"""
                "/v1/sessions/session-client/timeline" ->
                    """{"api_version":"v1","session_id":"session-client","items":[{"turn_id":"turn-client","started_position":2,"latest_position":9,"state":"suspended","user_text":"ship mobile","suspension":{"suspension_id":"suspension-1","session_version":3,"kind":"approval_required","prompt_schema":"garive.public-suspension-prompt.v1","prompt_json":"{\"message\":\"Approve?\",\"schema_version\":1}","prompt_digest":"885cfe3367b0344b40518f34170c6b4e81e64722ade58a0a9e61bc0e136e6b86","response_schema_json":"{\"type\":\"boolean\"}","response_schema_digest":"7cb541e84f226754a46c21c79f131fa2898354e1242456e6fd1c162bce319553"},"content_truncated":false,"activities":[{"api_version":"v1","activity_id":"activity-1","kind":"tool","label_key":"agent.activity.work","state":"waiting_for_input","source_position":8,"terminal":false}]}],"scanned_through_position":9,"observed_max_position":9,"has_more":false}"""
                else -> error("unexpected path ${request.url.encodedPath}")
            }
            respondJson(body)
        }
        val client = LiveHostClient("https://agent.example.test/", "mobile-secret", limits(), HttpClient(engine))

        assertEquals("definition-main", client.agentDefinitions().definitions.single().definition_id)
        assertEquals("suspended", client.sessions(8).sessions.single().latest_turn_state)
        assertEquals(9, client.session("session-client").observed_max_position)
        val timeline = client.timeline("session-client", 0, 8)
        assertEquals("approval_required", timeline.items.single().suspension?.kind)
        assertEquals("agent.activity.work", timeline.items.single().activities.single().label_key)
        assertEquals(
            listOf(
                "GET /v1/agent-definitions? Bearer mobile-secret",
                "GET /v1/sessions?limit=8 Bearer mobile-secret",
                "GET /v1/sessions/session-client? Bearer mobile-secret",
                "GET /v1/sessions/session-client/timeline?after_position=0&limit=8 Bearer mobile-secret",
            ),
            seen,
        )
    }

    @Test
    public fun everySharedHostFailureIsTyped(): Unit = runBlocking {
        fixture.getValue("host_errors").jsonArray.forEach { raw ->
            val hostError = raw.jsonObject
            val engine = MockEngine {
                respondJson(
                    buildJsonObject { put("code", hostError.text("code")); put("secret", "must-not-leak") }.toString(),
                    HttpStatusCode.fromValue(hostError.long("status").toInt()),
                )
            }
            val client = LiveHostClient("http://localhost:4317/", limits(), HttpClient(engine))
            val error = assertFailsWith<HostClientException> {
                client.createSession("create-stable", "definition-main")
            }
            assertEquals(HostClientError.HOST_FAILURE, error.code)
            assertEquals(hostError.long("status").toInt(), error.status)
            assertTrue("must-not-leak" !in error.toString())
        }
    }

    @Test
    public fun turnMutationsUseExactH1Paths(): Unit = runBlocking {
        val paths = mutableListOf<String>()
        val bodies = mutableListOf<String>()
        val response = """{"session_id":"session-client","turn_id":"turn-client","execution_id":"execution-client","committed_position":12}"""
        val engine = MockEngine { request ->
            paths += request.url.encodedPath
            if (request.body is TextContent) bodies += (request.body as TextContent).text
            respondJson(response)
        }
        val client = LiveHostClient("http://127.0.0.1:4317/", limits(), HttpClient(engine))
        client.cancelTurn("cancel-stable", "session-client", "turn-client", 9)
        client.steerTurn("steer-stable", "session-client", "turn-client", "follow-up input")
        client.approvalEvent(
            "approval-stable", "session-client", "turn-client", "suspension-client", 4, true,
        )
        client.externalInputEvent(
            "external-input-stable", "session-client", "turn-client", "suspension-client", 4, "approved input",
        )
        client.askReplyEvent(
            "ask-reply-stable", "session-client", "turn-client", "suspension-client", 4, """{"approved":true}""",
        )
        assertEquals(
            listOf(
                "/v1/sessions/session-client/turns/turn-client/cancel",
                "/v1/sessions/session-client/turns/turn-client/events",
                "/v1/sessions/session-client/turns/turn-client/events",
                "/v1/sessions/session-client/turns/turn-client/events",
                "/v1/sessions/session-client/turns/turn-client/events",
            ),
            paths,
        )
        assertTrue("\"session_id\"" !in bodies[0], "cancel body must drop session_id, got ${bodies[0]}")
        assertTrue("\"requested_through_position\":9" in bodies[0], "cancel body must carry requested_through_position, got ${bodies[0]}")
        assertTrue("\"kind\":\"steer\"" in bodies[1], "steer body must carry kind=steer, got ${bodies[1]}")
        assertTrue("\"kind\":\"approval\"" in bodies[2] && "\"decision\":\"approve\"" in bodies[2], "approval body must carry kind=approval + decision=approve, got ${bodies[2]}")
        assertTrue("\"kind\":\"external_input\"" in bodies[3] && "\"text\":\"approved input\"" in bodies[3], "external_input body must carry text, got ${bodies[3]}")
        assertTrue("\"kind\":\"ask_reply\"" in bodies[4] && "input_json" in bodies[4] && "approved" in bodies[4], "ask_reply body must carry canonical input_json bytes, got ${bodies[4]}")
    }

    @Test
    public fun mobileInputBoundaryFitsTheConfiguredCommandEnvelope(): Unit = runBlocking {
        val input = "a".repeat(16_384)
        var encodedBytes = 0
        val engine = MockEngine { request ->
            encodedBytes = (request.body as TextContent).text.encodeToByteArray().size
            respondJson(
                """{"api_version":"v1","session_id":"session-client","delivery":"direct","turns":[{"agent_id":"definition-main","turn_id":"turn-client","execution_id":"execution-client","committed_position":2}]}""",
            )
        }
        val mobileLimits = HostClientLimits(65_536, 65_536, 1_024, 120_000)
        val client = LiveHostClient("http://127.0.0.1:4317/", mobileLimits, HttpClient(engine))

        client.startTurnDirect("turn-stable", "session-client", "definition-main", input)

        assertTrue(encodedBytes > input.length)
        assertTrue(encodedBytes <= mobileLimits.maxCommandBytes)
    }

    @Test
    public fun membershipAndBroadcastUseTheExplicitSessionContract(): Unit = runBlocking {
        val paths = mutableListOf<String>()
        val bodies = mutableListOf<String>()
        var call = 0
        val roster =
            """{"api_version":"v1","session_id":"session-client","members":[{"agent_id":"alpha","joined_position":2}],"observed_max_position":2}"""
        val broadcast =
            """{"api_version":"v1","session_id":"session-client","delivery":"broadcast","turns":[{"agent_id":"alpha","turn_id":"turn-alpha","execution_id":"execution-alpha","committed_position":5}]}"""
        val engine = MockEngine { request ->
            paths += request.url.encodedPath
            if (request.body is TextContent) bodies += (request.body as TextContent).text
            respondJson(if (call++ < 3) roster else broadcast)
        }
        val client = LiveHostClient("http://127.0.0.1:4317/", limits(), HttpClient(engine))

        assertEquals("alpha", client.sessionMembership("session-client").members.single().agent_id)
        client.addSessionAgent("join-alpha", "session-client", "alpha")
        client.removeSessionAgent("leave-alpha", "session-client", "alpha")
        val started = client.startTurnBroadcast("broadcast-alpha", "session-client", "hello")

        assertEquals(TurnDeliveryV1.TURN_DELIVERY_V1_BROADCAST, started.delivery)
        assertEquals(
            listOf(
                "/v1/sessions/session-client/agents",
                "/v1/sessions/session-client/agents",
                "/v1/sessions/session-client/agents/alpha/remove",
                "/v1/sessions/session-client/turns",
            ),
            paths,
        )
        assertTrue(bodies.last().contains("\"delivery\":\"broadcast\""))
    }

    private fun mutation(name: String, source: List<HostEventV1>): List<HostEventV1> = when (name) {
        "api_version_v2" -> listOf(source.first().copy(api_version = "v2"))
        "session_other" -> listOf(source.first().copy(session_id = "other"))
        "position_zero" -> listOf(source.first().copy(position = 0))
        "position_backward" -> listOf(source.first().copy(position = 2), source.first().copy(position = 1))
        "duplicate_conflict" -> listOf(source.first().copy(position = 2), source.first().copy(position = 2, event = "turn.started"))
        "event_count_17" -> List(17) { source.first().copy(position = (it + 1).toLong()) }
        else -> error("unknown mutation $name")
    }

    private fun limits(): HostClientLimits = HostClientLimits(4_096, 8_192, 16, 2_000)

    private fun io.ktor.client.engine.mock.MockRequestHandleScope.respondJson(
        body: String,
        status: HttpStatusCode = HttpStatusCode.OK,
    ) = respond(body, status, headersOf(HttpHeaders.ContentType, ContentType.Application.Json.toString()))

    private fun JsonObject.toEvent(): HostEventV1 = HostEventV1(
        api_version = text("api_version"), session_id = text("session_id"), position = long("position"),
        event = text("event"), turn_id = optional("turn_id"), execution_id = optional("execution_id"),
        text = optional("text"),
    )

    private fun HostEventV1.toJson(): String = buildJsonObject {
        put("api_version", api_version); put("session_id", session_id); put("position", position)
        put("event", event); put("turn_id", turn_id); put("execution_id", execution_id); put("text", text)
    }.toString()

    private fun JsonObject.text(key: String): String = getValue(key).jsonPrimitive.content
    private fun JsonObject.long(key: String): Long = getValue(key).jsonPrimitive.long
    private fun JsonObject.optional(key: String): String = get(key)?.jsonPrimitive?.content.orEmpty()
}
