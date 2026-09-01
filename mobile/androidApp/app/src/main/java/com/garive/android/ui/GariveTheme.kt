package com.garive.android.ui

import androidx.compose.foundation.isSystemInDarkTheme
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.material3.lightColorScheme
import androidx.compose.runtime.Composable
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.garive.mobile.preferences.Theme

internal val GariveCoral = Color(0xFF315FCF)
internal val GariveMint = Color(0xFF27825A)
internal val GariveAmber = Color(0xFFA46816)
internal val GariveFailure = Color(0xFFD85B63)
internal val GariveInk = Color(0xFF000000)
internal val GariveRaised = Color(0xFF111111)
internal val GariveIvory = Color(0xFFF7F5EF)

/** Native mappings of the shared client geometry tokens. */
internal object GariveMobileMetrics {
    val composerRadius = 24.dp
    val userPromptRadius = 22.dp
    val decisionRadius = 20.dp
    val touchTarget = 48.dp
    val attentionEdge = 2.dp
}

private val DarkColors = darkColorScheme(
    primary = Color(0xFF4F7CF7),
    onPrimary = Color.White,
    secondary = Color(0xFF70D5A7),
    tertiary = Color(0xFFE7B46C),
    error = GariveFailure,
    background = GariveInk,
    onBackground = GariveIvory,
    surface = GariveRaised,
    onSurface = GariveIvory,
    surfaceVariant = Color(0xFF1C1C1E),
    onSurfaceVariant = Color(0xFFC2C3C8),
    outline = Color(0xFF4C4F58),
)

private val LightColors = lightColorScheme(
    primary = GariveCoral,
    onPrimary = Color.White,
    secondary = GariveMint,
    tertiary = GariveAmber,
    error = Color(0xFFB32635),
    background = Color(0xFFFBFAF6),
    onBackground = Color(0xFF272723),
    surface = Color(0xFFFFFEFB),
    onSurface = Color(0xFF272723),
    surfaceVariant = Color(0xFFF0EEE7),
    onSurfaceVariant = Color(0xFF686760),
    outline = Color(0xFFD4D1C8),
)

/** Garive's native Material 3 visual system. */
@Composable
internal fun GariveTheme(theme: Theme = Theme.SYSTEM, content: @Composable () -> Unit) {
    val dark = when (theme) {
        Theme.SYSTEM -> isSystemInDarkTheme()
        Theme.LIGHT -> false
        Theme.DARK -> true
    }
    MaterialTheme(
        colorScheme = if (dark) DarkColors else LightColors,
        typography = MaterialTheme.typography.copy(
            displaySmall = TextStyle(
                fontFamily = FontFamily.SansSerif,
                fontWeight = FontWeight.SemiBold,
                fontSize = 34.sp,
                lineHeight = 40.sp,
            ),
            headlineSmall = TextStyle(
                fontFamily = FontFamily.SansSerif,
                fontWeight = FontWeight.SemiBold,
                fontSize = 24.sp,
                lineHeight = 30.sp,
            ),
            bodyLarge = TextStyle(
                fontFamily = FontFamily.SansSerif,
                fontWeight = FontWeight.Normal,
                fontSize = 17.sp,
                lineHeight = 25.sp,
            ),
            labelLarge = TextStyle(
                fontFamily = FontFamily.SansSerif,
                fontWeight = FontWeight.SemiBold,
                fontSize = 15.sp,
                lineHeight = 20.sp,
            ),
        ),
        content = content,
    )
}
