package com.garive.android.security

import com.garive.mobile.application.CommandIdentitySource
import java.math.BigInteger
import java.security.SecureRandom

/** Produces 26-character lowercase ULID-shaped command identities. */
internal class SortableCommandIdentitySource(
    private val clockMillis: () -> Long = System::currentTimeMillis,
    private val fillRandom: (ByteArray) -> Unit = SecureRandom()::nextBytes,
) : CommandIdentitySource {
    override fun nextId(): String {
        val timestamp = clockMillis()
        require(timestamp in 0..MAX_TIMESTAMP)
        val bytes = ByteArray(16)
        repeat(6) { index -> bytes[index] = (timestamp ushr (40 - index * 8)).toByte() }
        ByteArray(10).also { random ->
            fillRandom(random)
            random.copyInto(bytes, destinationOffset = 6)
        }
        var value = BigInteger(1, bytes)
        return CharArray(26) { '0' }.also { output ->
            for (index in output.indices.reversed()) {
                val divided = value.divideAndRemainder(BASE)
                output[index] = ALPHABET[divided[1].toInt()]
                value = divided[0]
            }
        }.concatToString()
    }

    private companion object {
        const val MAX_TIMESTAMP: Long = 0xffff_ffff_ffffL
        val BASE: BigInteger = BigInteger.valueOf(32)
        const val ALPHABET: String = "0123456789abcdefghjkmnpqrstvwxyz"
    }
}
