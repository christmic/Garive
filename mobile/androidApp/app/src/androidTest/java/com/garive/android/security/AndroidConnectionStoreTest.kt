package com.garive.android.security

import android.content.Context
import androidx.test.core.app.ApplicationProvider
import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Before
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
public class AndroidConnectionStoreTest {
    private val context: Context = ApplicationProvider.getApplicationContext()
    private val store = AndroidConnectionStore(context)

    @Before
    public fun setUp(): Unit = store.clear()

    @After
    public fun tearDown(): Unit = store.clear()

    @Test
    public fun grantIsEncryptedAndClearRotatesDeviceIdentity(): Unit {
        val secret = "grant-that-must-never-appear-in-preferences"
        val firstDeviceKey = store.devicePublicKey()

        assertEquals(
            StoredConnection("https://agent.example.test", secret),
            store.save("https://agent.example.test", secret),
        )
        assertEquals(secret, store.load()?.accessGrant)
        assertFalse(
            context.getSharedPreferences("garive_mobile_connection_v1", Context.MODE_PRIVATE)
                .all.values.any { it.toString().contains(secret) },
        )

        store.clear()

        assertNull(store.load())
        assertNotEquals(firstDeviceKey, store.devicePublicKey())
    }

    @Test
    public fun pendingMutationRoundTripsAndClearsIndependently(): Unit {
        val pending = AndroidMobileWorkPersistence(context)

        pending.writePendingPayload("exact input")
        pending.writePendingRecord("{\"schema_version\":1}")

        assertEquals("exact input", pending.readPendingPayload())
        assertEquals("{\"schema_version\":1}", pending.readPendingRecord())
        pending.writePendingRecord(null)
        pending.writePendingPayload(null)
        assertNull(pending.readPendingRecord())
        assertNull(pending.readPendingPayload())
    }
}
