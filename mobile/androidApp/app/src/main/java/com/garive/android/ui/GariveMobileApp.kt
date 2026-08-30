package com.garive.android.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ViewList
import androidx.compose.material.icons.rounded.Home
import androidx.compose.material.icons.rounded.PeopleAlt
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import com.garive.mobile.application.MobileWorkController
import com.garive.mobile.host.MobileWakeRoute
import com.garive.mobile.model.MobileAgentCard
import com.garive.mobile.model.MobileDestination
import com.garive.mobile.model.MobileWorkState
import com.garive.mobile.preferences.Theme
import kotlinx.coroutines.launch

/** Connected Android product shell backed only by the shared controller. */
@Composable
internal fun GariveMobileApp(
    origin: String,
    controller: MobileWorkController,
    wakeRoute: MobileWakeRoute?,
    onWakeConsumed: () -> Unit,
    onSignOut: () -> Unit,
    theme: Theme,
    onTheme: (Theme) -> Unit,
    openNotificationSettings: () -> Unit,
) {
    var state by remember(controller) { mutableStateOf(controller.state()) }
    var showNewTask by remember { mutableStateOf(false) }
    var selectedAgent by remember { mutableStateOf<MobileAgentCard?>(null) }
    var confirmCancel by remember { mutableStateOf(false) }
    val scope = rememberCoroutineScope()
    val showNavigationLabels = LocalDensity.current.fontScale < 1.6f

    LaunchedEffect(controller, wakeRoute) {
        state = controller.boot()
        wakeRoute?.let { route ->
            state = if (route.destination == "session" && route.sessionId != null) {
                controller.openSession(route.sessionId!!)
            } else {
                controller.selectDestination(MobileDestination.SETTINGS)
            }
            onWakeConsumed()
        }
    }
    LifecycleEventEffect(Lifecycle.Event.ON_RESUME) {
        if (state.connection.name != "CONNECTING") scope.launch { state = controller.refresh() }
    }

    if (state.destination == MobileDestination.CONVERSATION) {
        ConversationScreen(
            state = state,
            onBack = { state = controller.selectDestination(MobileDestination.WORK) },
            onDraft = { state = controller.editDraft(it) },
            onSend = { scope.launch { state = controller.sendTurn(state.draft) } },
            onCancel = { confirmCancel = true },
            onContinue = { scope.launch { state = controller.continueLatest(state.draft.ifBlank { "approved" }) } },
            onRetry = { scope.launch { state = controller.retryExact() } },
        )
    } else {
        Scaffold(
            bottomBar = {
                NavigationBar {
                    navigationItems.forEach { item ->
                        val label: (@Composable () -> Unit)? = if (showNavigationLabels) {
                            { Text(item.label, maxLines = 1, overflow = TextOverflow.Clip) }
                        } else {
                            null
                        }
                        NavigationBarItem(
                            selected = state.destination == item.destination,
                            onClick = { state = controller.selectDestination(item.destination) },
                            icon = { Icon(item.icon, contentDescription = item.label) },
                            label = label,
                            alwaysShowLabel = showNavigationLabels,
                        )
                    }
                }
            },
        ) { padding ->
            Column(Modifier.padding(padding)) {
                when (state.destination) {
                    MobileDestination.WORK -> WorkScreen(
                        state,
                        onOpen = { scope.launch { state = controller.openSession(it) } },
                        onNewTask = { selectedAgent = state.agents.firstOrNull(); showNewTask = true },
                        onRefresh = { scope.launch { state = controller.refresh() } },
                    )
                    MobileDestination.SESSIONS -> SessionsScreen(
                        state,
                        onOpen = { scope.launch { state = controller.openSession(it) } },
                        onRefresh = { scope.launch { state = controller.refresh() } },
                    )
                    MobileDestination.AGENTS -> AgentsScreen(
                        state,
                        onStart = { selectedAgent = it; showNewTask = true },
                        onRefresh = { scope.launch { state = controller.refresh() } },
                    )
                    MobileDestination.SETTINGS -> SettingsScreen(
                        origin, state, theme, onTheme, openNotificationSettings,
                    ) {
                        controller.signOut()
                        onSignOut()
                    }
                    MobileDestination.CONVERSATION -> Unit
                }
                if (state.refreshing && state.sessions.isEmpty()) {
                    CircularProgressIndicator(modifier = Modifier.padding(24.dp))
                }
            }
        }
    }

    if (showNewTask) {
        NewTaskSheet(
            agents = state.agents,
            selected = selectedAgent,
            draft = state.draft,
            busy = state.pendingCommand != null,
            onSelect = { selectedAgent = it },
            onDraft = { state = controller.editDraft(it) },
            onDismiss = { showNewTask = false },
            onStart = {
                selectedAgent?.let { agent ->
                    scope.launch {
                        state = controller.startTask(agent.definitionId, state.draft)
                        if (state.selectedSessionId != null) showNewTask = false
                    }
                }
            },
        )
    }

    if (confirmCancel) {
        AlertDialog(
            onDismissRequest = { confirmCancel = false },
            title = { Text("Request cancellation?") },
            text = { Text("The Agent may finish current durable work before the server records a stop.") },
            confirmButton = {
                Button(onClick = {
                    confirmCancel = false
                    scope.launch { state = controller.cancelLatest() }
                }) { Text("Request cancel") }
            },
            dismissButton = {
                OutlinedButton(onClick = { confirmCancel = false }) { Text("Keep working") }
            },
        )
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun NewTaskSheet(
    agents: List<MobileAgentCard>,
    selected: MobileAgentCard?,
    draft: String,
    busy: Boolean,
    onSelect: (MobileAgentCard) -> Unit,
    onDraft: (String) -> Unit,
    onDismiss: () -> Unit,
    onStart: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(
            Modifier.fillMaxWidth()
                .verticalScroll(rememberScrollState())
                .navigationBarsPadding()
                .padding(horizontal = 20.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(14.dp),
        ) {
            Text("Start remote work", style = MaterialTheme.typography.headlineSmall)
            Text(
                "The Agent keeps running on your server after you leave the app.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            agents.forEach { agent ->
                OutlinedButton(
                    onClick = { onSelect(agent) },
                    modifier = Modifier.fillMaxWidth(),
                    shape = RoundedCornerShape(14.dp),
                ) {
                    Text(if (selected == agent) "✓ ${agent.displayName}" else agent.displayName)
                }
            }
            OutlinedTextField(
                value = draft,
                onValueChange = onDraft,
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Outcome for the Agent") },
                placeholder = { Text("Investigate, build, verify, and report…") },
                minLines = 4,
                maxLines = 8,
                enabled = !busy,
                shape = RoundedCornerShape(18.dp),
            )
            Button(
                onClick = onStart,
                modifier = Modifier.fillMaxWidth(),
                enabled = selected != null && draft.isNotBlank() && !busy,
            ) { Text(if (busy) "Sending securely…" else "Start on server") }
            OutlinedButton(onClick = onDismiss, modifier = Modifier.fillMaxWidth()) { Text("Cancel") }
        }
    }
}

private data class NavigationItem(
    val destination: MobileDestination,
    val label: String,
    val icon: ImageVector,
)

private val navigationItems = listOf(
    NavigationItem(MobileDestination.WORK, "Work", Icons.Rounded.Home),
    NavigationItem(MobileDestination.SESSIONS, "Sessions", Icons.AutoMirrored.Rounded.ViewList),
    NavigationItem(MobileDestination.AGENTS, "Agents", Icons.Rounded.PeopleAlt),
    NavigationItem(MobileDestination.SETTINGS, "Settings", Icons.Rounded.Settings),
)
