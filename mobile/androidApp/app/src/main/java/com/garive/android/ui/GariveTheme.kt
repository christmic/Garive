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
import androidx.compose.ui.unit.sp

internal val GariveCoral = Color(0xFFFF745F)
internal val GariveMint = Color(0xFF6ED6B2)
internal val GariveAmber = Color(0xFFF2BC62)
internal val GariveFailure = Color(0xFFFF6B73)
internal val GariveInk = Color(0xFF101315)
internal val GariveRaised = Color(0xFF1A1F22)
internal val GariveIvory = Color(0xFFF8F5EE)

private val DarkColors = darkColorScheme(
    primary = GariveCoral,
    onPrimary = Color(0xFF24100C),
    secondary = GariveMint,
    tertiary = GariveAmber,
    error = GariveFailure,
    background = GariveInk,
    onBackground = GariveIvory,
    surface = GariveRaised,
    onSurface = GariveIvory,
    surfaceVariant = Color(0xFF252B2E),
    onSurfaceVariant = Color(0xFFB9C1C3),
    outline = Color(0xFF465055),
)

private val LightColors = lightColorScheme(
    primary = Color(0xFFB83C2C),
    onPrimary = Color.White,
    secondary = Color(0xFF176B54),
    tertiary = Color(0xFF7C5200),
    error = Color(0xFFB32635),
    background = Color(0xFFF6F3EC),
    onBackground = Color(0xFF171A1C),
    surface = Color(0xFFFFFCF5),
    onSurface = Color(0xFF171A1C),
    surfaceVariant = Color(0xFFEAE6DD),
    onSurfaceVariant = Color(0xFF545C60),
    outline = Color(0xFFABB1B2),
)

/** Garive's native Material 3 visual system. */
@Composable
internal fun GariveTheme(content: @Composable () -> Unit) {
    MaterialTheme(
        colorScheme = if (isSystemInDarkTheme()) DarkColors else LightColors,
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
