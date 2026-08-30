package com.garive.android

import android.os.Bundle
import android.os.Build
import android.content.Intent
import android.net.Uri
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
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
import java.util.UUID
import kotlinx.coroutines.launch

/** Native Android entry point for secure remote Agent work. */
public class MainActivity : ComponentActivity() {
    private val pairingSuggestion = mutableStateOf<PairingSuggestion?>(null)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        pairingSuggestion.value = parsePairingLink(intent?.data)
        val store = AndroidConnectionStore(this)
        setContent { GariveTheme { GariveRoot(store, pairingSuggestion.value) } }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        pairingSuggestion.value = parsePairingLink(intent.data)
    }
}

@Composable
private fun GariveRoot(store: AndroidConnectionStore, suggestion: PairingSuggestion?) {
    var connection by remember { mutableStateOf(store.load()) }
    var pairingError by remember { mutableStateOf<String?>(null) }
    var pairing by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
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
                    pairingError = null
                } catch (error: HostClientException) {
                    pairingError = error.code.wireName
                } finally {
                    pairing = false
                }
            }
        }
    } else {
        ConnectedRoot(current) {
            store.clear()
            connection = null
            scope.launch {
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
private fun ConnectedRoot(connection: StoredConnection, onSignOut: () -> Unit) {
    val controller = remember(connection) {
        MobileWorkController(
            host = LiveHostClient(connection.origin, connection.accessGrant, limits()),
            identities = CommandIdentitySource { UUID.randomUUID().toString() },
        )
    }
    GariveMobileApp(connection.origin, controller, onSignOut)
}

private fun limits(): HostClientLimits = HostClientLimits(
    maxCommandBytes = 4_096,
    maxEventBytes = 64 * 1_024,
    maxEvents = 256,
    followDeadlineMs = 120_000,
)
