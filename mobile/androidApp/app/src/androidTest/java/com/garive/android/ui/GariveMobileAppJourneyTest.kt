package com.garive.android.ui

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performScrollTo
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
        compose.onNodeWithText("Analyze").performClick()
        compose.onNodeWithText("Start on server").assertIsEnabled()
        compose.onNodeWithText("Start on server").performClick()
        compose.waitUntil(5_000) {
            compose.onAllNodesWithText("Working · server work continues").fetchSemanticsNodes().isNotEmpty()
        }
        compose.onNodeWithText("Find the key patterns and recommend next steps").assertIsDisplayed()

        compose.onNodeWithContentDescription("Back to Work").performClick()
        compose.onNodeWithContentDescription("Open navigation").performClick()
        compose.onNodeWithText("Settings").performClick()
        compose.onNodeWithText("Light").performScrollTo().assertIsSelected()
        compose.onNodeWithText("Dark").performClick().assertIsSelected()
    }
}

private object JourneyHost : MobileHost {
    private var created: Boolean = false

    fun reset(): Unit { created = false }

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
        observed_max_position = 2,
    )

    override suspend fun timeline(
        sessionId: String,
        afterPosition: Long,
        limit: Int,
    ): TurnTimelinePageV1 = TurnTimelinePageV1(
        api_version = "v1",
        session_id = sessionId,
        items = if (created) listOf(
            TurnTimelineItemV1(
                turn_id = "turn-new",
                started_position = 2,
                latest_position = 2,
                state = "running",
                user_text = "Find the key patterns and recommend next steps",
            ),
        ) else emptyList(),
        scanned_through_position = 2,
        observed_max_position = 2,
        has_more = false,
    )

    override suspend fun createSession(commandId: String, definitionId: String): CreateSessionResponseV1 {
        created = true
        return CreateSessionResponseV1("session-new", "agent-1", 1)
    }

    override suspend fun startTurn(commandId: String, sessionId: String, text: String): TurnCommandResponseV1 =
        TurnCommandResponseV1(sessionId, "turn-new", "execution-new", 2)

    override suspend fun cancelTurn(
        commandId: String,
        sessionId: String,
        turnId: String,
        requestedThroughPosition: Long,
    ): TurnCommandResponseV1 = unsupported()

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
        latest_position = 2,
        latest_turn_id = "turn-new",
        latest_turn_state = "running",
        turn_count = 1,
    )

    private fun <T> unsupported(): T = error("This read-only UI journey must not mutate the Host")
}
