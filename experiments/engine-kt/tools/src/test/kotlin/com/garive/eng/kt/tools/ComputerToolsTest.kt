package com.garive.eng.kt.tools

import java.io.File
import kotlin.test.Test
import kotlin.test.assertEquals
import kotlin.test.assertIs
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive

class ComputerToolsTest {
    @Test
    fun observeAndSemanticActBindOneExactTarget() {
        val catalogue = catalogue()
        val observe = catalogue.prepare(
            ToolIntent(
                "observe",
                T2_COMPUTER_OBSERVE,
                """{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","max_nodes":100,"max_text_bytes":4096,"capture":"none","max_capture_bytes":1024,"max_capture_pixels":1000}""",
            ),
        ).success()
        assertEquals("computer:desktop-1:app-1:window-1", requireNotNull(observe.invocationAccesses).values.single().resourceKey)
        assertEquals(AccessMode.READ, requireNotNull(observe.invocationAccesses).values.single().mode)

        val act = catalogue.prepare(
            ToolIntent(
                "press",
                T2_COMPUTER_ACT,
                """{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"press","node_ref":"node-1"}""",
            ),
        ).success()
        assertEquals(AccessMode.WRITE, requireNotNull(act.invocationAccesses).values.single().mode)
    }

    @Test
    fun coordinateActionsRequireCompleteVisibleGeometry() {
        val valid = """{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"click_point","display_id":"display-1","point_x":200,"point_y":200,"snapshot_pixel_width":1000,"snapshot_pixel_height":800,"scale_milli":2000,"visible_frame_x":100,"visible_frame_y":100,"visible_frame_width":500,"visible_frame_height":400}"""
        catalogue().prepare(ToolIntent("click", T2_COMPUTER_ACT, valid)).success()
        for (arguments in listOf(
            valid.replace("\"point_x\":200", "\"point_x\":600"),
            valid.replace("\"visible_frame_width\":500", "\"visible_frame_width\":901"),
            valid.replace("\"display_id\":\"display-1\"", "\"display_id\":\"bad:id\""),
        )) {
            assertEquals(
                PreparationErrorCode.EFFECT_ACCESS_INVALID,
                assertIs<ToolContractResult.Failure>(
                    catalogue().prepare(ToolIntent("bad", T2_COMPUTER_ACT, arguments)),
                ).error.code,
            )
        }
    }

    @Test
    fun mixedSemanticFieldsZeroMotionAndUnadmittedTargetsFailClosed() {
        for (arguments in listOf(
            """{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"press","node_ref":"node-1","point_x":10}""",
            """{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"window-1","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"scroll","node_ref":"node-1","delta_x":0,"delta_y":0}""",
            """{"desktop_session_id":"desktop-1","application_id":"app-1","window_id":"other","expected_snapshot_id":"snapshot-1","target_revision":"window-rev-1","action":"press","node_ref":"node-1"}""",
        )) {
            assertEquals(
                PreparationErrorCode.EFFECT_ACCESS_INVALID,
                assertIs<ToolContractResult.Failure>(
                    catalogue().prepare(ToolIntent("bad", T2_COMPUTER_ACT, arguments)),
                ).error.code,
            )
        }
    }

    @Test
    fun sharedFixtureMatchesRustDigestsAccessesAndFailures() {
        val fixture = Json.parseToJsonElement(
            File(System.getProperty("garive.repo.root"), "spec/fixtures/agent/computer-tools-v1.json").readText(),
        ).jsonObject
        val catalogue = assertIs<ToolContractResult.Success<BuiltinT2ComputerCatalogue>>(
            BuiltinT2ComputerCatalogue.create(
                fixture.getValue("policy_revision").jsonPrimitive.content,
                fixture.getValue("targets").jsonArray.map { target ->
                    val value = target.jsonObject
                    assertIs<ToolContractResult.Success<ComputerTargetScope>>(
                        ComputerTargetScope.create(
                            value.getValue("desktop_session_id").jsonPrimitive.content,
                            value.getValue("application_id").jsonPrimitive.content,
                            value.getValue("window_id").jsonPrimitive.content,
                        ),
                    ).value
                },
            ),
        ).value
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
            val expected = case.getValue("accesses").jsonArray.single().jsonObject
            val access = requireNotNull(prepared.invocationAccesses).values.single()
            assertEquals(expected.getValue("namespace").jsonPrimitive.content, access.namespace.wireName)
            assertEquals(expected.getValue("resource_key").jsonPrimitive.content, access.resourceKey)
            assertEquals(expected.getValue("mode").jsonPrimitive.content, access.mode.wireName)
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

    private fun catalogue(): BuiltinT2ComputerCatalogue =
        assertIs<ToolContractResult.Success<BuiltinT2ComputerCatalogue>>(
            BuiltinT2ComputerCatalogue.create(
                "computer-policy-1",
                listOf(
                    assertIs<ToolContractResult.Success<ComputerTargetScope>>(
                        ComputerTargetScope.create("desktop-1", "app-1", "window-1"),
                    ).value,
                ),
            ),
        ).value

    private fun ToolContractResult<PreparedToolCall>.success(): PreparedToolCall =
        assertIs<ToolContractResult.Success<PreparedToolCall>>(this).value
}
