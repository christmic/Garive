package com.garive.android.security

import androidx.test.ext.junit.runners.AndroidJUnit4
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
public class SortableCommandIdentitySourceTest {
    @Test
    public fun identitiesHaveExactLowercaseSortableShape(): Unit {
        var now = 1_700_000_000_000L
        var randomByte: Byte = 1
        val source = SortableCommandIdentitySource(
            clockMillis = { now++ },
            fillRandom = { bytes -> bytes.fill(randomByte++) },
        )

        val first = source.nextId()
        val second = source.nextId()

        assertEquals(26, first.length)
        assertTrue(first.matches(Regex("[0-9abcdefghjkmnpqrstvwxyz]{26}")))
        assertTrue(first < second)
        assertTrue(first != second)
    }
}
