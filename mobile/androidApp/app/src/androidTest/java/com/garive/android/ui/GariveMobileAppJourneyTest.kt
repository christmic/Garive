package com.garive.android.ui

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.garive.host.v1.AgentDefinitionPageV1
import com.garive.host.v1.AgentDefinitionSummaryV1
import com.garive.host.v1.CreateSessionResponseV1
import com.garive.host.v1.SessionPageV1
import com.garive.host.v1.SessionViewV1
import com.garive.host.v1.TurnCommandResponseV1
import com.garive.host.v1.TurnTimelinePageV1
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
        val controller = MobileWorkController(
            host = JourneyHost,
            identities = CommandIdentitySource { "01k00000000000000000000000" },
            maxInputBytes = 16 * 1_024,
            persistence = EphemeralMobileWorkPersistence,
        )
        compose.setContent {
            GariveTheme(Theme.LIGHT) {
                GariveMobileApp(
                    origin = "https://demo.garive.local/",
                    controller = controller,
                    wakeRoute = null,
                    onWakeConsumed = {},
                    onSignOut = {},
                    theme = Theme.LIGHT,
                    onTheme = {},
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
    }
}

private object JourneyHost : MobileHost {
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

    override suspend fun sessions(limit: Int): SessionPageV1 = SessionPageV1(api_version = "v1")

    override suspend fun session(sessionId: String): SessionViewV1 = unsupported()

    override suspend fun timeline(
        sessionId: String,
        afterPosition: Long,
        limit: Int,
    ): TurnTimelinePageV1 = unsupported()

    override suspend fun createSession(commandId: String, definitionId: String): CreateSessionResponseV1 = unsupported()

    override suspend fun startTurn(commandId: String, sessionId: String, text: String): TurnCommandResponseV1 = unsupported()

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

    private fun <T> unsupported(): T = error("This read-only UI journey must not mutate the Host")
}
