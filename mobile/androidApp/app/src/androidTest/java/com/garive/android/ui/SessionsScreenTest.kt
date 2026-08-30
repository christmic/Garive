package com.garive.android.ui

import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.assertIsSelected
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import com.garive.mobile.model.MobileConnectionState
import com.garive.mobile.model.MobileAgentCard
import com.garive.mobile.model.MobileActivityItem
import com.garive.mobile.model.MobileDecision
import com.garive.mobile.model.MobileSessionCard
import com.garive.mobile.model.MobileTurnItem
import com.garive.mobile.model.MobileWorkState
import com.garive.mobile.model.MobileWorkStatus
import org.junit.Rule
import org.junit.Test
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse

public class SessionsScreenTest {
    @get:Rule
    public val compose = createComposeRule()

    @Test
    public fun localSearchAndStatusFilterComposeWithoutServerEffects(): Unit {
        val state = MobileWorkState(
            connection = MobileConnectionState.ONLINE,
            sessions = listOf(
                session("session-alpha", "Alpha Agent", MobileWorkStatus.WORKING),
                session("session-beta", "Beta Agent", MobileWorkStatus.COMPLETED),
            ),
        )
        compose.setContent { GariveTheme { SessionsScreen(state, {}, {}) } }

        compose.onNodeWithText("All").assertIsSelected()
        compose.onNodeWithText("Search Agent or Session").performTextInput("Beta")
        compose.onNodeWithText("Beta Agent").assertIsDisplayed()
        compose.onAllNodesWithText("Alpha Agent").assertCountEquals(0)
        compose.onNodeWithText("Working").performClick().assertIsSelected()
        compose.onAllNodesWithText("Beta Agent").assertCountEquals(0)
    }

    @Test
    public fun explicitShareTranscriptContainsOnlyRenderedConversation(): Unit {
        val state = MobileWorkState(
            selectedSessionId = "private-session-id",
            timeline = listOf(
                MobileTurnItem(
                    "private-turn-id", "Check release", "Release is healthy",
                    MobileWorkStatus.COMPLETED, 4, false, null, emptyList(),
                ),
            ),
        )

        assertEquals("You\nCheck release\n\nAgent\nRelease is healthy", conversationTranscript(state))
    }

    @Test
    public fun safeDiagnosticsExcludeServiceAndWorkContent(): Unit {
        val state = MobileWorkState(connection = MobileConnectionState.SECURITY_ERROR, draft = "private work")
        val copied = safeDiagnostics(state)

        assertFalse(copied.contains("private work"))
        assertFalse(copied.contains("http"))
        assertFalse(copied.contains("session", ignoreCase = true))
    }

    @Test
    public fun newTaskStarterWritesTheDesktopAlignedOutcome(): Unit {
        var draft = ""
        val agent = MobileAgentCard("agent-a", "Mobile Orchestrator", "revision-a", emptyList())
        compose.setContent {
            GariveTheme {
                NewTaskSheet(
                    agents = listOf(agent), selected = agent, draft = draft,
                    busy = false, online = true, onSelect = {}, onDraft = { draft = it },
                    onDismiss = {}, onStart = {},
                )
            }
        }

        compose.onNodeWithText("Synthesize").performClick()
        compose.runOnIdle {
            assertEquals("Turn notes into a clear decision memo", draft)
        }
    }

    @Test
    public fun approvalAndActivityRequireExplicitMobileActions(): Unit {
        val decision = MobileDecision(
            "suspension-a", 3, "approval_required", "Approval needed",
            "Approve the release after verified mobile checks?", "Approve once",
        )
        val turn = MobileTurnItem(
            "turn-a", "Finish the mobile release", "The release is ready for review.",
            MobileWorkStatus.NEEDS_INPUT, 4, false, decision,
            listOf(MobileActivityItem("activity-a", "Ran 4 checks", "completed", true, "verification_checked")),
        )
        val state = MobileWorkState(
            connection = MobileConnectionState.ONLINE,
            sessions = listOf(session("session-a", "Mobile Orchestrator", MobileWorkStatus.NEEDS_INPUT)),
            selectedSessionId = "session-a",
            timeline = listOf(turn),
        )
        val responses = mutableListOf<String>()
        compose.setContent {
            GariveTheme {
                ConversationScreen(state, {}, {}, {}, {}, {}, {}, { responses += it }, {}, {})
            }
        }

        compose.onNodeWithText("Decline").assertIsDisplayed().performClick()
        compose.onNodeWithText("Approve once").assertIsDisplayed().performClick()
        compose.onAllNodesWithText("Ran 4 checks").assertCountEquals(0)
        compose.onNodeWithText("Activity · 1").performClick()
        compose.onNodeWithText("Ran 4 checks").assertIsDisplayed()
        compose.onNodeWithText("Code · verification_checked").assertIsDisplayed()
        compose.runOnIdle { assertEquals(listOf("false", "true"), responses) }
    }

    @Test
    public fun stableFailureIsVisibleAndDismissible(): Unit {
        var dismissed = false
        compose.setContent {
            GariveTheme {
                MobileNoticeBanner("runtime_unavailable", false, { dismissed = true }, {}, {})
            }
        }

        compose.onNodeWithText("Runtime unavailable. Verified history is still shown.").assertIsDisplayed()
        compose.onNodeWithContentDescription("Dismiss notice").performClick()
        compose.runOnIdle { assertEquals(true, dismissed) }
    }

    private fun session(id: String, agent: String, status: MobileWorkStatus): MobileSessionCard =
        MobileSessionCard(id, agent, status, "2026-08-31T00:00:00Z", 1, 1)
}
