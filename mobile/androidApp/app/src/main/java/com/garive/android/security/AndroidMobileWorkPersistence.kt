package com.garive.android.security

import android.content.Context
import com.garive.mobile.application.MobileWorkPersistence

/** App-private restart storage for one bounded non-credential pending mutation. */
internal class AndroidMobileWorkPersistence(context: Context) : MobileWorkPersistence {
    private val preferences = context.getSharedPreferences(PREFERENCES, Context.MODE_PRIVATE)

    override fun readPendingRecord(): String? = preferences.getString(RECORD, null)

    override fun writePendingRecord(value: String?) {
        preferences.edit().apply {
            if (value == null) remove(RECORD) else putString(RECORD, value)
        }.commit()
    }

    override fun readPendingPayload(): String? = preferences.getString(PAYLOAD, null)

    override fun writePendingPayload(value: String?) {
        preferences.edit().apply {
            if (value == null) remove(PAYLOAD) else putString(PAYLOAD, value)
        }.commit()
    }

    private companion object {
        const val PREFERENCES = "garive_mobile_pending_v1"
        const val RECORD = "record"
        const val PAYLOAD = "payload"
    }
}
