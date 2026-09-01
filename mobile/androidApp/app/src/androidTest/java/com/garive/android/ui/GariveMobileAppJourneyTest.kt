package com.garive.android.ui

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.hasScrollAction
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
import androidx.compose.ui.test.performTextInput
import androidx.compose.ui.test.performTouchInput
import androidx.compose.ui.test.swipeUp
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.garive.host.v1.AgentDefinitionPageV1
import com.garive.host.v1.AgentDefinitionSummaryV1
import com.garive.host.v1.CreateSessionResponseV1
import com.garive.host.v1.SessionPageV1
import com.garive.host.v1.SessionSummaryV1
import com.garive.host.v1.SessionViewV1
import com.garive.host.v1.TurnCommandResponseV1
import com.garive.host.v1.TurnTimelinePageV1
import com.garive.host.v1.TurnTimelineItemV1
import com.garive.mobile.application.CommandIdentitySource
import com.garive.mobile.application.EphemeralMobileWorkPersistence
import com.garive.mobile.application.MobileWorkController
import com.garive.mobile.host.HostView
import com.garive.mobile.host.MobileHost
import com.garive.mobile.preferences.Theme
import org.junit.Rule
import org.junit.Test
import org.junit.Assert.assertTrue
import org.junit.Assert.assertFalse
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
public class GariveMobileAppJourneyTest {
    @get:Rule
    public val compose = createComposeRule()

    @Test
    public fun connectedShellNavigatesAndBuildsANewTask(): Unit {
        JourneyHost.reset()
        var commandSequence = 0
        val controller = MobileWorkController(
            host = JourneyHost,
            identities = CommandIdentitySource { "01k${commandSequence++.toString().padStart(23, '0')}" },
            maxInputBytes = 16 * 1_024,
            persistence = EphemeralMobileWorkPersistence,
        )
        compose.setContent {
            var theme by remember { mutableStateOf(Theme.LIGHT) }
            GariveTheme(theme) {
                GariveMobileApp(
                    origin = "https://demo.garive.local/",
                    controller = controller,
                    wakeRoute = null,
                    onWakeConsumed = {},
                    onSignOut = {},
                    theme = theme,
                    onTheme = { theme = it },
                    openNotificationSettings = {},
                )
            }
        }

        compose.waitUntil(5_000) {
            compose.onAllNodesWithText("Your Agents are ready").fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText("Your Agents are ready").assertIsDisplayed()

        compose.onNodeWithContentDescription("Open navigation").performClick()
        compose.onNodeWithText("Sessions").performClick()
        compose.onNodeWithText("Durable work, ready anywhere").assertIsDisplayed()

        compose.onNodeWithContentDescription("New task").performClick()
        compose.onNodeWithText("Start with a clear outcome").assertIsDisplayed()
        compose.onNodeWithText("Start on server").assertIsDisplayed()
        compose.onNodeWithText("Analyze").performClick()
        assertTrue(compose.onAllNodesWithText("Start with a clear outcome").fetchSemanticsNodes().isEmpty())
        compose.onNodeWithText("Start on server").assertIsEnabled()
        compose.onNodeWithText("Start on server").performClick()
        compose.waitUntil(5_000) {
            compose.onAllNodesWithText("Working · server work continues").fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText("Find the key patterns and recommend next steps").assertIsDisplayed()

        compose.onNodeWithContentDescription("Stop current work").performClick()
        compose.onNodeWithText("Request cancel").performClick()
        compose.waitUntil(5_000) {
            compose.onAllNodesWithText("Cancellation recorded. Committed work remains available.")
                .fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText("Give the Agent direction").performTextInput("Prepare the final mobile handoff")
        compose.onNodeWithContentDescription("Send to Agent").performClick()
        compose.waitUntil(5_000) {
            compose.onAllNodesWithText("Prepare the final mobile handoff").fetchSemanticsNodes().isNotEmpty()
        }

        compose.onNodeWithContentDescription("Back to Work").performClick()
        compose.onNodeWithContentDescription("Open navigation").performClick()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithText("Light").performScrollTo().assertIsSelected()
        compose.onNodeWithText("Dark").performClick().assertIsSelected()
    }

    @Test
    public fun starterDisclosureTracksTheEditableDraft(): Unit {
        assertTrue(showMobileGoalStarters(""))
        assertFalse(showMobileGoalStarters(" "))
        assertFalse(showMobileGoalStarters(mobileGoalStarters.first().prompt))
    }

    @Test
    public fun inactiveWorkspaceReplacesAllRemoteContentWithPrivacyShield(): Unit {
        JourneyHost.reset()
        val controller = MobileWorkController(
            host = JourneyHost,
            identities = CommandIdentitySource { "01k000000000000000000000" },
            maxInputBytes = 16 * 1_024,
            persistence = EphemeralMobileWorkPersistence,
        )
        compose.setContent {
            GariveTheme(Theme.DARK) {
                GariveMobileApp(
                    origin = "https://demo.garive.local/",
                    controller = controller,
                    wakeRoute = null,
                    onWakeConsumed = {},
                    onSignOut = {},
                    theme = Theme.DARK,
                    onTheme = {},
                    openNotificationSettings = {},
                    forcePrivacyShield = true,
                )
            }
        }

        compose.onNodeWithText("Remote work is private").assertIsDisplayed()
        assertTrue(compose.onAllNodesWithText("Your Agents are ready").fetchSemanticsNodes().isEmpty())
    }

    @Test
    public fun confirmedUnpairReturnsToSecurePairing(): Unit {
        JourneyHost.reset()
        val controller = MobileWorkController(
            host = JourneyHost,
            identities = CommandIdentitySource { "01k000000000000000000000" },
            maxInputBytes = 16 * 1_024,
            persistence = EphemeralMobileWorkPersistence,
        )
        compose.setContent {
            var paired by remember { mutableStateOf(true) }
            GariveTheme(Theme.LIGHT) {
                if (paired) {
                    GariveMobileApp(
                        origin = "https://demo.garive.local/",
                        controller = controller,
                        wakeRoute = null,
                        onWakeConsumed = {},
                        onSignOut = { paired = false },
                        theme = Theme.LIGHT,
                        onTheme = {},
                        openNotificationSettings = {},
                    )
                } else {
                    PairingScreen(errorCode = null, pairing = false, suggestion = null) { _, _ -> }
                }
            }
        }

        compose.waitUntil(5_000) {
            compose.onAllNodesWithText("Your Agents are ready").fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithContentDescription("Open navigation").performClick()
        compose.onNodeWithText("Settings").performClick()
        repeat(4) { compose.onNode(hasScrollAction()).performTouchInput { swipeUp() } }
        compose.onNodeWithText("Unpair this device").performClick()
        compose.onNodeWithText("Unpair device").performClick()

        compose.waitUntil(3_000) {
            compose.onAllNodesWithText("Pair your server").fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText("Pair your server").assertIsDisplayed()
        compose.onNodeWithText("Connect securely").assertIsDisplayed()
    }
}

private object JourneyHost : MobileHost {
    private var created: Boolean = false
    private var position: Long = 1
    private val turns: MutableList<TurnTimelineItemV1> = mutableListOf()

    fun reset(): Unit {
        created = false
        position = 1
        turns.clear()
    }

    override suspend fun agentDefinitions(): AgentDefinitionPageV1 = AgentDefinitionPageV1(
        api_version = "v1",
        definitions = listOf(
            AgentDefinitionSummaryV1(
                api_version = "v1",
                definition_id = "mobile-orchestrator",
                definition_revision = "revision-1",
                capabilities = listOf("work"),
            ),
        ),
    )

    override suspend fun sessions(limit: Int): SessionPageV1 = SessionPageV1(
        api_version = "v1",
        sessions = if (created) listOf(summary()) else emptyList(),
    )

    override suspend fun session(sessionId: String): SessionViewV1 = SessionViewV1(
        api_version = "v1",
        session = summary(),
        observed_max_position = position,
    )

    override suspend fun timeline(
        sessionId: String,
        afterPosition: Long,
        limit: Int,
    ): TurnTimelinePageV1 = TurnTimelinePageV1(
        api_version = "v1",
        session_id = sessionId,
        items = turns.toList(),
        scanned_through_position = position,
        observed_max_position = position,
        has_more = false,
    )

    override suspend fun createSession(commandId: String, definitionId: String): CreateSessionResponseV1 {
        created = true
        return CreateSessionResponseV1("session-new", "agent-1", 1)
    }

    override suspend fun startTurn(commandId: String, sessionId: String, text: String): TurnCommandResponseV1 {
        position += 2
        val turnId = "turn-${turns.size + 1}"
        turns += TurnTimelineItemV1(
            turn_id = turnId,
            started_position = position - 1,
            latest_position = position,
            state = "running",
            user_text = text,
        )
        return TurnCommandResponseV1(sessionId, turnId, "execution-${turns.size}", position)
    }

    override suspend fun cancelTurn(
        commandId: String,
        sessionId: String,
        turnId: String,
        requestedThroughPosition: Long,
    ): TurnCommandResponseV1 {
        position++
        val latest = turns.last()
        turns[turns.lastIndex] = latest.copy(
            latest_position = position,
            state = "stopped",
            completion_text = "Cancellation recorded. Committed work remains available.",
        )
        return TurnCommandResponseV1(sessionId, turnId, "execution-${turns.size}", position)
    }

    override suspend fun continueTurn(
        commandId: String,
        sessionId: String,
        turnId: String,
        suspensionId: String,
        expectedSessionVersion: Long,
        input: String,
        inputJson: Boolean,
    ): TurnCommandResponseV1 = unsupported()

    override suspend fun followUntilTerminal(sessionId: String, afterPosition: Long): HostView = unsupported()

    private fun summary(): SessionSummaryV1 = SessionSummaryV1(
        api_version = "v1",
        session_id = "session-new",
        agent_instance_id = "agent-1",
        definition_id = "mobile-orchestrator",
        definition_revision = "revision-1",
        opened_at = "2026-08-31T00:00:00Z",
        latest_position = position,
        latest_turn_id = turns.lastOrNull()?.turn_id,
        latest_turn_state = turns.lastOrNull()?.state,
        turn_count = turns.size.toLong(),
    )

    private fun <T> unsupported(): T = error("This UI journey does not exercise this Host action")
}
