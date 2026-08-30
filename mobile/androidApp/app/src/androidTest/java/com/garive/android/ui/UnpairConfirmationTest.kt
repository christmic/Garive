package com.garive.android.ui

import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test

public class UnpairConfirmationTest {
    @get:Rule
    public val compose = createComposeRule()

    @Test
    public fun explainsScopeAndRequiresExplicitConfirmation(): Unit {
        var confirmed = false
        compose.setContent {
            GariveTheme {
                UnpairConfirmation(onDismiss = {}, onConfirm = { confirmed = true })
            }
        }

        compose.onNodeWithText("Unpair this device?").assertIsDisplayed()
        compose.onNodeWithText(
            "This removes access from this phone. Agent work and history remain on your service.",
        ).assertIsDisplayed()
        compose.onNodeWithText("Keep paired").assertIsDisplayed()
        compose.onNodeWithText("Unpair device").performClick()
        compose.runOnIdle { assertTrue(confirmed) }
    }
}
