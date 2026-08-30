package com.garive.android

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import com.garive.mobile.preferences.Theme
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
public class AndroidAppearanceStoreTest {
    private val context: Context = ApplicationProvider.getApplicationContext()

    @Before
    @After
    public fun clearPreferences(): Unit {
        assertTrue(
            context.getSharedPreferences("garive-client", Context.MODE_PRIVATE)
                .edit()
                .clear()
                .commit(),
        )
    }

    @Test
    public fun explicitThemesSurviveANewStoreInstance(): Unit {
        val first = AndroidAppearanceStore(context)
        assertEquals(Theme.SYSTEM, first.theme())

        first.setTheme(Theme.LIGHT)
        assertEquals(Theme.LIGHT, AndroidAppearanceStore(context).theme())

        first.setTheme(Theme.DARK)
        assertEquals(Theme.DARK, AndroidAppearanceStore(context).theme())
    }

    @Test
    public fun unknownStoredThemeFailsClosedToSystem(): Unit {
        context.getSharedPreferences("garive-client", Context.MODE_PRIVATE)
            .edit()
            .putString("theme", "future-theme")
            .commit()

        assertEquals(Theme.SYSTEM, AndroidAppearanceStore(context).theme())
    }
}
