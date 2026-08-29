package com.garive.android

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.garive.mobile.host.HostClientException
import com.garive.mobile.host.HostClientLimits
import com.garive.mobile.host.HostTerminalKind
import com.garive.mobile.host.LiveHostClient
import kotlinx.coroutines.launch

/** Android shell entry point for the shared live H1 client. */
public class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        setContent { GariveApp() }
    }
}

@Composable
private fun GariveApp() {
    var hostUrl by remember { mutableStateOf("http://127.0.0.1:4317/") }
    var definition by remember { mutableStateOf("definition-main") }
    var message by remember { mutableStateOf("hello") }
    var output by remember { mutableStateOf("Ready") }
    var running by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    MaterialTheme {
        Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text("Garive Agent", style = MaterialTheme.typography.headlineLarge)
            OutlinedTextField(hostUrl, { hostUrl = it }, label = { Text("Loopback Host URL") })
            OutlinedTextField(definition, { definition = it }, label = { Text("Agent definition") })
            OutlinedTextField(message, { message = it }, label = { Text("Message") })
            Button(enabled = !running, onClick = {
                running = true
                output = "running"
                scope.launch {
                    output = runCatching { runTurn(hostUrl, definition, message) }
                        .fold({ it }, { error ->
                            if (error is HostClientException) error.code.wireName else "transport_failure"
                        })
                    running = false
                }
            }) { Text("Run Agent") }
            Text(output)
        }
    }
}

private suspend fun runTurn(hostUrl: String, definition: String, message: String): String {
    val client = LiveHostClient(hostUrl, HostClientLimits(4_096, 8_192, 256, 120_000))
    val identity = "android-${android.os.SystemClock.elapsedRealtimeNanos()}"
    val session = client.createSession("create-$identity", definition)
    val turn = client.startTurn("turn-$identity", session.session_id, message)
    val view = client.followUntilTerminal(session.session_id, turn.committed_position)
    return if (view.terminal == HostTerminalKind.COMPLETED) view.text else view.terminal!!.name.lowercase()
}
