package com.garive.android.security

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import android.util.Base64
import java.security.KeyStore
import java.security.KeyPairGenerator
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** Remote connection material with its access grant held only in encrypted storage. */
internal data class StoredConnection(val origin: String, val accessGrant: String)

/** Android Keystore-backed connection store used by the mobile composition root. */
internal class AndroidConnectionStore(context: Context) {
    private val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    fun load(): StoredConnection? = runCatching {
        val origin = preferences.getString(ORIGIN, null) ?: return null
        val iv = preferences.getString(IV, null)?.decode() ?: return null
        val ciphertext = preferences.getString(CIPHERTEXT, null)?.decode() ?: return null
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv))
        StoredConnection(origin, cipher.doFinal(ciphertext).decodeToString())
    }.getOrElse {
        clear()
        null
    }

    fun save(origin: String, accessGrant: String): StoredConnection {
        require(origin.isNotBlank() && accessGrant.isNotBlank())
        val cipher = Cipher.getInstance(TRANSFORMATION)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val ciphertext = cipher.doFinal(accessGrant.encodeToByteArray())
        check(
            preferences.edit()
                .putString(ORIGIN, origin)
                .putString(IV, cipher.iv.encode())
                .putString(CIPHERTEXT, ciphertext.encode())
                .commit(),
        )
        return StoredConnection(origin, accessGrant)
    }

    fun clear() {
        preferences.edit().clear().commit()
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        if (store.containsAlias(KEY_ALIAS)) store.deleteEntry(KEY_ALIAS)
        if (store.containsAlias(DEVICE_KEY_ALIAS)) store.deleteEntry(DEVICE_KEY_ALIAS)
    }

    /** Stable device public key retained by Android Keystore for pairing identity. */
    fun devicePublicKey(): String {
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        val existing = store.getCertificate(DEVICE_KEY_ALIAS)?.publicKey
        val key = existing ?: KeyPairGenerator.getInstance(KeyProperties.KEY_ALGORITHM_EC, KEYSTORE).run {
            initialize(
                KeyGenParameterSpec.Builder(
                    DEVICE_KEY_ALIAS,
                    KeyProperties.PURPOSE_SIGN or KeyProperties.PURPOSE_VERIFY,
                )
                    .setDigests(KeyProperties.DIGEST_SHA256)
                    .build(),
            )
            generateKeyPair().public
        }
        return Base64.encodeToString(key.encoded, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
    }

    private fun key(): SecretKey {
        val store = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (store.getKey(KEY_ALIAS, null) as? SecretKey)?.let { return it }
        val generator = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        generator.init(
            KeyGenParameterSpec.Builder(
                KEY_ALIAS,
                KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
            )
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return generator.generateKey()
    }

    private fun ByteArray.encode(): String = Base64.encodeToString(this, Base64.NO_WRAP)
    private fun String.decode(): ByteArray = Base64.decode(this, Base64.NO_WRAP)

    private companion object {
        const val PREFERENCES = "garive_mobile_connection_v1"
        const val ORIGIN = "origin"
        const val IV = "iv"
        const val CIPHERTEXT = "ciphertext"
        const val KEYSTORE = "AndroidKeyStore"
        const val KEY_ALIAS = "garive.mobile.remote.access.v1"
        const val DEVICE_KEY_ALIAS = "garive.mobile.remote.device.v1"
        const val TRANSFORMATION = "AES/GCM/NoPadding"
    }
}
