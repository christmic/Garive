package com.garive.android

import android.content.Context
import android.content.Intent
import android.content.IntentFilter
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.hasClickAction
import androidx.compose.ui.test.hasScrollAction
import androidx.compose.ui.test.hasText
import androidx.compose.ui.test.assert
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
import org.junit.Assert.assertNotNull
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

/** Opt-in network journey against the repository's real loopback Debug Host. */
@RunWith(AndroidJUnit4::class)
public class LiveHostJourneyTest {
    @get:Rule
    public val compose = createEmptyComposeRule()

    @Test
    public fun createsCancelsAndAppendsThroughTheLiveHost(): Unit {
        requireLiveHost()
        val context = cleanContext()
        ActivityScenario.launch<MainActivity>(walkthroughIntent(context)).use {
            compose.waitUntil(8_000) {
                compose.onAllNodesWithText("Server connected").fetchSemanticsNodes().isNotEmpty()
            }
            compose.onNodeWithContentDescription("Open navigation").performClick()
            compose.onNodeWithText("Sessions").performClick()
            compose.onNodeWithContentDescription("New task").performClick()
            compose.onNodeWithText("Analyze").performClick()
            compose.onNodeWithText("Start on server").performClick()
            compose.waitUntil(8_000) {
                compose.onAllNodesWithText("Working · server work continues").fetchSemanticsNodes().isNotEmpty()
            }
            compose.onNodeWithText("Find the key patterns and recommend next steps")

            compose.onNodeWithContentDescription("Request cancellation", useUnmergedTree = true).performClick()
            compose.onNodeWithText("Request cancel").performClick()
            compose.waitUntil(8_000) {
                compose.onAllNodesWithText("Cancellation recorded. Committed work remains available.")
                    .fetchSemanticsNodes().isNotEmpty()
            }
            compose.onNodeWithText("Give the Agent direction").performTextInput("Prepare the Android handoff")
            compose.onNodeWithContentDescription("Send to Agent").performClick()
            compose.waitUntil(8_000) {
                compose.onAllNodesWithText("Prepare the Android handoff").fetchSemanticsNodes().isNotEmpty()
            }
        }
    }

    @Test
    public fun commitsApprovalThroughTheLiveHost(): Unit {
        requireLiveHost()
        val context = cleanContext()
        ActivityScenario.launch<MainActivity>(walkthroughIntent(context, "release-approval")).use {
            compose.waitUntil(8_000) {
                compose.onAllNodesWithText("Approve once").fetchSemanticsNodes().isNotEmpty()
            }
            compose.onNode(hasText("Approve once") and hasClickAction()).performClick()
            compose.waitUntil(8_000) {
                compose.onAllNodesWithText("Completed · server work continues").fetchSemanticsNodes().isNotEmpty()
            }
            compose.waitUntil(8_000) {
                compose.onAllNodesWithText(
                    "Approved. The agent resumed on the server and completed the release checks.",
                ).fetchSemanticsNodes().isNotEmpty()
            }
        }
    }

    @Test
    public fun commitsDeclineThroughTheLiveHost(): Unit {
        requireLiveHost()
        val context = cleanContext()
        ActivityScenario.launch<MainActivity>(walkthroughIntent(context, "release-decline")).use {
            compose.waitUntil(8_000) {
                compose.onAllNodesWithText("Decline").fetchSemanticsNodes().isNotEmpty()
            }
            compose.onNode(hasText("Decline") and hasClickAction()).performClick()
            compose.waitUntil(8_000) {
                compose.onAllNodesWithText("Completed · server work continues").fetchSemanticsNodes().isNotEmpty()
            }
            compose.waitUntil(8_000) {
                compose.onAllNodesWithText(
                    "Declined. The protected action was skipped and the decision was committed.",
                ).fetchSemanticsNodes().isNotEmpty()
            }
        }
    }

    @Test
    public fun opensTheNativeShareChooserFromAConversation(): Unit {
        requireLiveHost()
        val context = cleanContext()
        val instrumentation = InstrumentationRegistry.getInstrumentation()
        val chooser = instrumentation.addMonitor(IntentFilter(Intent.ACTION_CHOOSER), null, false)
        try {
            ActivityScenario.launch<MainActivity>(walkthroughIntent(context, "design-review")).use {
                compose.waitUntil(8_000) {
                    compose.onAllNodesWithText("Review the mobile interaction design.")
                        .fetchSemanticsNodes().isNotEmpty()
                }
                compose.onNodeWithTag("Agent code block").assert(hasScrollAction())
                compose.onNodeWithContentDescription("Share conversation").performClick()
                val chooserActivity = instrumentation.waitForMonitorWithTimeout(chooser, 3_000)
                assertNotNull(chooserActivity)
                instrumentation.runOnMainSync { chooserActivity?.finish() }
            }
        } finally {
            instrumentation.removeMonitor(chooser)
        }
    }

    private fun requireLiveHost(): Unit = assumeTrue(
        "Run through `just mobile-android-live-ui` so the loopback Host and adb reverse are explicit.",
        InstrumentationRegistry.getArguments().getString("gariveLiveHost") == "true",
    )

    private fun cleanContext(): Context = ApplicationProvider.getApplicationContext<Context>().also { context ->
        context.getSharedPreferences("garive_mobile_pending_v1", Context.MODE_PRIVATE).edit().clear().commit()
    }

    private fun walkthroughIntent(context: Context, sessionId: String? = null): Intent =
        Intent(context, MainActivity::class.java)
            .putExtra("garive_walkthrough", true)
            .apply { if (sessionId != null) putExtra("garive_walkthrough_session", sessionId) }
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
}
