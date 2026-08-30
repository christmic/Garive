package com.garive.android

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
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
import com.garive.mobile.host.LiveHostClient
import java.util.UUID

/** Native Android entry point for secure remote Agent work. */
public class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val store = AndroidConnectionStore(this)
        setContent { GariveTheme { GariveRoot(store) } }
    }
}

@Composable
private fun GariveRoot(store: AndroidConnectionStore) {
    var connection by remember { mutableStateOf(store.load()) }
    var pairingError by remember { mutableStateOf<String?>(null) }
    val current = connection
    if (current == null) {
        PairingScreen(pairingError) { origin, accessGrant ->
            try {
                LiveHostClient(origin, accessGrant, limits())
                connection = store.save(origin, accessGrant)
                pairingError = null
            } catch (error: HostClientException) {
                pairingError = error.code.wireName
            }
        }
    } else {
        ConnectedRoot(current) {
            store.clear()
            connection = null
        }
    }
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
