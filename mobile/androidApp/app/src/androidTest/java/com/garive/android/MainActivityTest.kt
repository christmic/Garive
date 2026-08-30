package com.garive.android

import android.content.pm.ActivityInfo
import android.content.Intent
import android.net.Uri
import androidx.compose.ui.test.assertIsEnabled
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createAndroidComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performScrollTo
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Rule
import org.junit.Test
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
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

    @Test
    public fun pairingLinksRejectUnverifiedOriginsBeforePresentation(): Unit {
        val expiry = System.currentTimeMillis() / 1_000 + 300
        fun link(origin: String, extra: String = ""): Uri = Uri.parse(
            "garive://pair?origin=${Uri.encode(origin)}&code=one-time-code&exp=$expiry&name=Test%20service$extra",
        )

        assertEquals(
            "https://agent.example.test:443",
            parsePairingLink(link("https://agent.example.test/"))?.origin,
        )
        assertNull(parsePairingLink(link("http://agent.example.test/")))
        assertNull(parsePairingLink(link("https://user@agent.example.test/")))
        assertNull(parsePairingLink(link("https://agent.example.test/path")))
        assertNull(parsePairingLink(link("https://agent.example.test/", "&code=duplicate")))

        compose.activityRule.scenario.onActivity {
            it.startActivity(
                Intent(Intent.ACTION_VIEW, link("https://agent.example.test/"), it, MainActivity::class.java)
                    .addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            )
        }
        compose.onNodeWithText("Pairing with Test service").performScrollTo().assertIsDisplayed()
        compose.onNodeWithText("Connect securely").performScrollTo().assertIsEnabled()
    }
}
