package com.garive.android

import android.content.pm.ActivityInfo
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
public class MainActivityTest {
    @get:Rule
    public val compose = createAndroidComposeRule<MainActivity>()

    @Test
    public fun securePairingRendersCompleteInitialSurface(): Unit {
        listOf(
            "Pair your server",
            "Keep Agent work moving securely when your computer is out of reach.",
            "Service address",
            "Access code",
            "Connect securely",
            "Remote connections require HTTPS. Garive never stores the access code in preferences or logs.",
        ).forEach { label -> compose.onNodeWithText(label).performScrollTo().assertIsDisplayed() }
    }

    @Test
    public fun landscapePairingKeepsThePrimaryActionReachable(): Unit {
        compose.activityRule.scenario.onActivity {
            it.requestedOrientation = ActivityInfo.SCREEN_ORIENTATION_LANDSCAPE
        }
        compose.waitForIdle()

        compose.onNodeWithText("Connect securely").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Remote connections require HTTPS. Garive never stores the access code in preferences or logs.")
            .performScrollTo()
            .assertIsDisplayed()
    }
}
