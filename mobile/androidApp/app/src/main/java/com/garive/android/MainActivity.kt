package com.garive.android

import android.os.Bundle
import android.os.Build
import android.content.Intent
import android.net.Uri
import android.Manifest
import android.provider.Settings
import android.content.pm.PackageManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.ui.platform.LocalContext
import com.garive.android.push.AndroidPushCoordinator
import com.garive.android.push.WAKE_ACTION
import com.garive.android.push.WAKE_TOKEN
import com.garive.android.security.AndroidConnectionStore
import com.garive.android.security.StoredConnection
import com.garive.android.ui.GariveMobileApp
import com.garive.android.ui.GariveTheme
import com.garive.android.ui.PairingScreen
import com.garive.mobile.application.CommandIdentitySource
import com.garive.mobile.application.MobileWorkController
import com.garive.mobile.host.HostClientException
import com.garive.mobile.host.HostClientLimits
import com.garive.mobile.host.GatewayPairingClient
import com.garive.mobile.host.LiveHostClient
import com.garive.mobile.host.MobilePlatform
import com.garive.mobile.host.GatewayNotificationClient
import com.garive.mobile.host.MobileWakeRoute
import com.garive.mobile.preferences.Theme
import java.util.UUID
import kotlinx.coroutines.launch

/** Native Android entry point for secure remote Agent work. */
public class MainActivity : ComponentActivity() {
    private val pairingSuggestion = mutableStateOf<PairingSuggestion?>(null)
    private val wakeToken = mutableStateOf<String?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val walkthrough = BuildConfig.DEBUG && intent.getBooleanExtra(WALKTHROUGH_EXTRA, false)
        val appearance = getSharedPreferences("garive-client", MODE_PRIVATE)
        pairingSuggestion.value = parsePairingLink(intent?.data)
        val store = AndroidConnectionStore(this)
        wakeToken.value = intent?.takeIf { it.action == WAKE_ACTION }?.getStringExtra(WAKE_TOKEN)
        if (!walkthrough && store.load() != null) requestNotificationPermission()
        setContent {
            var theme by remember {
                mutableStateOf(
                    Theme.entries.firstOrNull { it.wireName == appearance.getString("theme", null) }
                        ?: Theme.SYSTEM,
                )
            }
            val selectTheme: (Theme) -> Unit = {
                theme = it
                appearance.edit().putString("theme", it.wireName).apply()
            }
            GariveTheme(theme) {
                if (walkthrough) {
                    GariveWalkthroughRoot(theme, selectTheme, ::openNotificationSettings)
                } else {
                    GariveRoot(
                        store, pairingSuggestion.value, wakeToken.value,
                        onWakeConsumed = { wakeToken.value = null },
                        requestNotifications = ::requestNotificationPermission,
                        theme = theme,
                        onTheme = selectTheme,
                        openNotificationSettings = ::openNotificationSettings,
                    )
                }
            }
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        pairingSuggestion.value = parsePairingLink(intent.data)
        wakeToken.value = intent.takeIf { it.action == WAKE_ACTION }?.getStringExtra(WAKE_TOKEN)
    }

    private fun requestNotificationPermission() {
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(arrayOf(Manifest.permission.POST_NOTIFICATIONS), NOTIFICATION_PERMISSION_REQUEST)
        }
    }

    private fun openNotificationSettings() {
        startActivity(
            Intent(Settings.ACTION_APP_NOTIFICATION_SETTINGS)
                .putExtra(Settings.EXTRA_APP_PACKAGE, packageName),
        )
    }

    private companion object {
        const val NOTIFICATION_PERMISSION_REQUEST = 41
        const val WALKTHROUGH_EXTRA = "garive_walkthrough"
    }
}

@Composable
private fun GariveWalkthroughRoot(
    theme: Theme,
    onTheme: (Theme) -> Unit,
    openNotificationSettings: () -> Unit,
) {
    val origin = "http://127.0.0.1:4318/"
    val controller = remember {
        MobileWorkController(
            host = LiveHostClient(origin, limits()),
            identities = CommandIdentitySource { UUID.randomUUID().toString() },
        )
    }
    GariveMobileApp(origin, controller, null, {}, {}, theme, onTheme, openNotificationSettings)
}

@Composable
private fun GariveRoot(
    store: AndroidConnectionStore,
    suggestion: PairingSuggestion?,
    wakeToken: String?,
    onWakeConsumed: () -> Unit,
    requestNotifications: () -> Unit = {},
    theme: Theme,
    onTheme: (Theme) -> Unit,
    openNotificationSettings: () -> Unit,
) {
    var connection by remember { mutableStateOf(store.load()) }
    var pairingError by remember { mutableStateOf<String?>(null) }
    var pairing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val current = connection
    if (current == null) {
        PairingScreen(pairingError, pairing, suggestion) { origin, code ->
            scope.launch {
                pairing = true
                try {
                    val grant = GatewayPairingClient(origin).exchange(
                        code = code,
                        deviceName = Build.MODEL.take(100),
                        platform = MobilePlatform.ANDROID,
                        devicePublicKey = store.devicePublicKey(),
                    )
                    connection = store.save(origin, grant.accessGrant)
                    requestNotifications()
                    AndroidPushCoordinator.register(context, connection!!)
                    pairingError = null
                } catch (error: HostClientException) {
                    pairingError = error.code.wireName
                } finally {
                    pairing = false
                }
            }
        }
    } else {
        LaunchedEffect(current) { runCatching { AndroidPushCoordinator.register(context, current) } }
        ConnectedRoot(current, wakeToken, onWakeConsumed, theme, onTheme, openNotificationSettings) {
            store.clear()
            connection = null
            scope.launch {
                AndroidPushCoordinator.unregister(current)
                runCatching { GatewayPairingClient(current.origin).revoke(current.accessGrant) }
            }
        }
    }
}

internal data class PairingSuggestion(val origin: String, val code: String, val serviceName: String)

private fun parsePairingLink(uri: Uri?): PairingSuggestion? {
    if (uri?.scheme != "garive" || uri.host != "pair") return null
    val allowed = setOf("origin", "code", "exp", "name")
    if (uri.queryParameterNames != allowed || allowed.any { uri.getQueryParameters(it).size != 1 }) return null
    val expiry = uri.getQueryParameter("exp")?.toLongOrNull() ?: return null
    val now = System.currentTimeMillis() / 1_000
    val origin = uri.getQueryParameter("origin") ?: return null
    val code = uri.getQueryParameter("code") ?: return null
    val name = uri.getQueryParameter("name") ?: return null
    if (expiry <= now || expiry > now + 600 || code.length !in 6..128 || name.length !in 1..100) return null
    return PairingSuggestion(origin, code, name)
}

@Composable
private fun ConnectedRoot(
    connection: StoredConnection,
    wakeToken: String?,
    onWakeConsumed: () -> Unit,
    theme: Theme,
    onTheme: (Theme) -> Unit,
    openNotificationSettings: () -> Unit,
    onSignOut: () -> Unit,
) {
    val controller = remember(connection) {
        MobileWorkController(
            host = LiveHostClient(connection.origin, connection.accessGrant, limits()),
            identities = CommandIdentitySource { UUID.randomUUID().toString() },
        )
    }
    var wakeRoute by remember { mutableStateOf<MobileWakeRoute?>(null) }
    LaunchedEffect(connection, wakeToken) {
        wakeRoute = try {
            wakeToken?.let { GatewayNotificationClient(connection.origin).resolve(connection.accessGrant, it) }
        } catch (_: HostClientException) {
            null
        } finally {
            if (wakeToken != null) onWakeConsumed()
        }
    }
    GariveMobileApp(
        connection.origin, controller, wakeRoute, { wakeRoute = null }, onSignOut,
        theme, onTheme, openNotificationSettings,
    )
}

private fun limits(): HostClientLimits = HostClientLimits(
    maxCommandBytes = 4_096,
    maxEventBytes = 64 * 1_024,
    maxEvents = 256,
    followDeadlineMs = 120_000,
)
