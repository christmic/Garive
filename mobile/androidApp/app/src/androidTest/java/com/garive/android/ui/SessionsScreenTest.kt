package com.garive.android.ui

import androidx.compose.ui.test.assertCountEquals
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import com.garive.mobile.model.MobileConnectionState
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

        compose.onNodeWithText("Search Agent or Session").performTextInput("Beta")
        compose.onNodeWithText("Beta Agent").assertIsDisplayed()
        compose.onAllNodesWithText("Alpha Agent").assertCountEquals(0)
        compose.onNodeWithText("Working").performClick()
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

    private fun session(id: String, agent: String, status: MobileWorkStatus): MobileSessionCard =
        MobileSessionCard(id, agent, status, "2026-08-31T00:00:00Z", 1, 1)
}
