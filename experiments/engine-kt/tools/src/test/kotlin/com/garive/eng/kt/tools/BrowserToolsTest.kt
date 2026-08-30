package com.garive.eng.kt.tools

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class BrowserToolsTest {
    private val fixture: JsonObject = Json.parseToJsonElement(
        File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/browser-tools-v1.json").readText(),
    ).jsonObject

    @Test
    fun catalogueFreezesObserveAndNeverReplayActions() {
        val definitions = catalogue().definitions
        assertEquals(
            listOf(T2_BROWSER_ACT, T2_BROWSER_NAVIGATE, T2_BROWSER_OBSERVE),
            definitions.map(ToolDefinition::name),
        )
        assertEquals(ReplayClass.NEVER_REPLAY, definitions[0].replayClass)
        assertEquals(ReplayClass.READ_ONLY, definitions[2].replayClass)
    }

    @Test
    fun sharedFixtureMatchesRustDigestsAccessesAndFailures() {
        val catalogue = catalogue()
        fixture.getValue("valid_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            val prepared = catalogue.prepare(
                ToolIntent(
                    "fixture-call",
                    case.getValue("tool_name").jsonPrimitive.content,
                    case.getValue("arguments").toString(),
                ),
            ).success()
            assertEquals(case.getValue("prepared_digest").jsonPrimitive.content, prepared.inputDigest)
            assertEquals(
                case.getValue("accesses").jsonArray.map { access ->
                    val value = access.jsonObject
                    Triple(
                        value.getValue("namespace").jsonPrimitive.content,
                        value.getValue("resource_key").jsonPrimitive.content,
                        value.getValue("mode").jsonPrimitive.content,
                    )
                },
                requireNotNull(prepared.invocationAccesses).values.map { access ->
                    Triple(access.namespace.wireName, access.resourceKey, access.mode.wireName)
                },
            )
        }
        fixture.getValue("invalid_cases").jsonArray.forEach { element ->
            val case = element.jsonObject
            assertEquals(
                case.getValue("error").jsonPrimitive.content,
                assertIs<ToolContractResult.Failure>(
                    catalogue.prepare(
                        ToolIntent(
                            "fixture-bad",
                            case.getValue("tool_name").jsonPrimitive.content,
                            case.getValue("arguments").toString(),
                        ),
                    ),
                ).error.code.wireName,
            )
        }
    }

    @Test
    fun unadmittedPageOriginMismatchAndInvalidActionShapeFailClosed() {
        val catalogue = catalogue()
        for (arguments in listOf(
            """{"session_id":"session-1","page_id":"other","expected_snapshot_id":"snapshot-1","target_revision":"nav-1","action":"reload","allowed_navigation_origins":[]}""",
            """{"session_id":"session-1","page_id":"page-1","expected_snapshot_id":"snapshot-1","target_revision":"nav-1","action":"scroll","delta_x":0,"delta_y":0,"allowed_navigation_origins":[]}""",
            """{"session_id":"session-1","page_id":"page-1","expected_snapshot_id":"snapshot-1","target_revision":"nav-1","action":"click","allowed_navigation_origins":[]}""",
        )) {
            assertEquals(
                PreparationErrorCode.EFFECT_ACCESS_INVALID,
                assertIs<ToolContractResult.Failure>(
                    catalogue.prepare(ToolIntent("bad", T2_BROWSER_ACT, arguments)),
                ).error.code,
            )
        }
    }

    private fun catalogue(): BuiltinT2BrowserCatalogue =
        assertIs<ToolContractResult.Success<BuiltinT2BrowserCatalogue>>(
            BuiltinT2BrowserCatalogue.create(
                fixture.getValue("policy_revision").jsonPrimitive.content,
                fixture.getValue("pages").jsonArray.map { page ->
                    val value = page.jsonObject
                    assertIs<ToolContractResult.Success<BrowserPageScope>>(
                        BrowserPageScope.create(
                            value.getValue("session_id").jsonPrimitive.content,
                            value.getValue("page_id").jsonPrimitive.content,
                        ),
                    ).value
                },
                fixture.getValue("origins").jsonArray.map { it.jsonPrimitive.content },
            ),
        ).value

    private fun ToolContractResult<PreparedToolCall>.success(): PreparedToolCall =
        assertIs<ToolContractResult.Success<PreparedToolCall>>(this).value
}
