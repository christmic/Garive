package com.garive.android.security

import android.content.Context
import com.garive.mobile.application.MobileWorkPersistence

/** App-private restart storage for bounded navigation/drafts and one pending mutation. */
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

    override fun readPreferencesRecord(): String? = preferences.getString(PREFERENCES_RECORD, null)

    override fun writePreferencesRecord(value: String?) {
        preferences.edit().apply {
            if (value == null) remove(PREFERENCES_RECORD) else putString(PREFERENCES_RECORD, value)
        }.commit()
    }

    private companion object {
        const val PREFERENCES = "garive_mobile_pending_v1"
        const val RECORD = "record"
        const val PAYLOAD = "payload"
        const val PREFERENCES_RECORD = "preferences"
    }
}
