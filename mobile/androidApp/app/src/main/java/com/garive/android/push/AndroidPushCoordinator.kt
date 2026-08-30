package com.garive.android.push

import android.Manifest
import android.app.Application
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.os.Build
import androidx.core.app.NotificationCompat
import androidx.core.app.NotificationManagerCompat
import com.garive.android.BuildConfig
import com.garive.android.MainActivity
import com.garive.android.R
import com.garive.android.security.AndroidConnectionStore
import com.garive.android.security.StoredConnection
import com.garive.mobile.host.GatewayNotificationClient
import com.garive.mobile.host.MobilePushTransport
import com.google.firebase.FirebaseApp
import com.google.firebase.FirebaseOptions
import com.google.firebase.installations.FirebaseInstallations
import com.google.firebase.messaging.FirebaseMessaging
import com.google.firebase.messaging.FirebaseMessagingService
import com.google.firebase.messaging.RemoteMessage
import kotlin.coroutines.resume
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.launch
import kotlinx.coroutines.suspendCancellableCoroutine

internal const val WAKE_ACTION = "com.garive.android.WAKE"
internal const val WAKE_TOKEN = "route_token"

public class GariveApplication : Application() {
    override fun onCreate() {
        super.onCreate()
        AndroidPushCoordinator.initialize(this)
    }
}

internal object AndroidPushCoordinator {
    fun initialize(context: Context): Boolean {
        if (FirebaseApp.getApps(context).isNotEmpty()) return true
        if (listOf(
                BuildConfig.FIREBASE_APP_ID, BuildConfig.FIREBASE_API_KEY,
                BuildConfig.FIREBASE_PROJECT_ID, BuildConfig.FIREBASE_SENDER_ID,
            ).any(String::isBlank)
        ) return false
        val options = FirebaseOptions.Builder()
            .setApplicationId(BuildConfig.FIREBASE_APP_ID)
            .setApiKey(BuildConfig.FIREBASE_API_KEY)
            .setProjectId(BuildConfig.FIREBASE_PROJECT_ID)
            .setGcmSenderId(BuildConfig.FIREBASE_SENDER_ID)
            .build()
        FirebaseApp.initializeApp(context, options)
        return true
    }

    suspend fun register(context: Context, connection: StoredConnection) {
        if (!initialize(context)) return
        val token = suspendCancellableCoroutine { continuation ->
            FirebaseMessaging.getInstance().register()
                .addOnSuccessListener {
                    FirebaseInstallations.getInstance().id
                        .addOnSuccessListener { continuation.resume(it) }
                        .addOnFailureListener { continuation.resume("") }
                }
                .addOnFailureListener { continuation.resume("") }
        }
        registerToken(connection, token)
    }

    suspend fun registerToken(connection: StoredConnection, token: String) {
        if (token.length !in 20..4_096) return
        GatewayNotificationClient(connection.origin).register(
            connection.accessGrant, MobilePushTransport.FCM, token,
        )
    }

    suspend fun unregister(connection: StoredConnection) {
        runCatching { GatewayNotificationClient(connection.origin).unregister(connection.accessGrant) }
        runCatching { FirebaseMessaging.getInstance().unregister() }
    }

    fun show(context: Context, data: Map<String, String>) {
        val allowed = setOf("schema_version", "route_token", "category", "collapse_key")
        val token = data[WAKE_TOKEN].orEmpty()
        val category = data["category"].orEmpty()
        if (data.keys != allowed || data["schema_version"] != "1" || token.length != 43 ||
            category !in setOf("attention", "completed", "failed", "connection_security")
        ) return
        val manager = context.getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, "Remote work", NotificationManager.IMPORTANCE_DEFAULT).apply {
                description = "Content-free status updates from your Garive service"
                lockscreenVisibility = android.app.Notification.VISIBILITY_PRIVATE
            },
        )
        val intent = Intent(context, MainActivity::class.java).setAction(WAKE_ACTION).putExtra(WAKE_TOKEN, token)
        val pending = PendingIntent.getActivity(
            context, token.hashCode(), intent,
            PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
        )
        val notification = NotificationCompat.Builder(context, CHANNEL)
            .setSmallIcon(R.drawable.ic_garive).setContentTitle("Garive update")
            .setContentText("Open Garive to refresh verified server state")
            .setContentIntent(pending).setAutoCancel(true).setCategory(NotificationCompat.CATEGORY_STATUS)
            .setVisibility(NotificationCompat.VISIBILITY_PRIVATE).build()
        if (Build.VERSION.SDK_INT < 33 || context.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED) {
            NotificationManagerCompat.from(context).notify(data["collapse_key"].hashCode(), notification)
        }
    }

    private const val CHANNEL = "garive_remote_work_v1"
}

public class GariveFirebaseService : FirebaseMessagingService() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)

    override fun onRegistered(installationId: String) {
        AndroidConnectionStore(this).load()?.let { connection ->
            scope.launch { runCatching { AndroidPushCoordinator.registerToken(connection, installationId) } }
        }
    }

    override fun onMessageReceived(message: RemoteMessage) {
        AndroidPushCoordinator.show(this, message.data)
    }
}
