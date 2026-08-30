package com.garive.android

import android.content.Context
import com.garive.mobile.preferences.Theme

internal class AndroidAppearanceStore(context: Context) {
    private val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    internal fun theme(): Theme = Theme.entries.firstOrNull {
        it.wireName == preferences.getString(THEME, null)
    } ?: Theme.SYSTEM

    internal fun setTheme(theme: Theme): Unit {
        preferences.edit().putString(THEME, theme.wireName).apply()
    }

    private companion object {
        const val PREFERENCES: String = "garive-client"
        const val THEME: String = "theme"
    }
}
