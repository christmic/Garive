package com.garive.android

import android.content.Context
import android.content.Intent
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onAllNodesWithText
import androidx.compose.ui.test.onNodeWithContentDescription
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import org.junit.Assume.assumeTrue
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
        assumeTrue(
            "Run through `just mobile-android-live-ui` so the loopback Host and adb reverse are explicit.",
            InstrumentationRegistry.getArguments().getString("gariveLiveHost") == "true",
        )
        val context = ApplicationProvider.getApplicationContext<Context>()
        context.getSharedPreferences("garive_mobile_pending_v1", Context.MODE_PRIVATE).edit().clear().commit()
        val intent = Intent(context, MainActivity::class.java)
            .putExtra("garive_walkthrough", true)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)

        ActivityScenario.launch<MainActivity>(intent).use {
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
}
