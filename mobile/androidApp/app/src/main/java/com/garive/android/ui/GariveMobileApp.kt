package com.garive.android.ui

import android.content.Intent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.verticalScroll
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.ViewList
import androidx.compose.material.icons.rounded.Home
import androidx.compose.material.icons.rounded.Lock
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.Menu
import androidx.compose.material.icons.rounded.PeopleAlt
import androidx.compose.material.icons.rounded.Settings
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.IconButton
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.NavigationDrawerItem
import androidx.compose.material3.NavigationDrawerItemDefaults
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.TopAppBarDefaults
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.Alignment
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.material3.rememberDrawerState
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.lifecycle.Lifecycle
import androidx.lifecycle.compose.LifecycleEventEffect
import com.garive.android.MOBILE_MAX_INPUT_BYTES
import com.garive.mobile.application.MobileWorkController
import com.garive.mobile.host.MobileWakeRoute
import com.garive.mobile.model.MobileAgentCard
import com.garive.mobile.model.MobileDestination
import com.garive.mobile.model.MobileConnectionState
import com.garive.mobile.model.MobileWorkState
import com.garive.mobile.preferences.Theme
import kotlinx.coroutines.launch

/** Connected Android product shell backed only by the shared controller. */
@OptIn(ExperimentalMaterial3Api::class)
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
    walkthroughSessionId: String? = null,
    forcePrivacyShield: Boolean = false,
) {
    var state by remember(controller) { mutableStateOf(controller.state()) }
    var showNewTask by remember { mutableStateOf(false) }
    var selectedAgent by remember { mutableStateOf<MobileAgentCard?>(null) }
    var confirmCancel by remember { mutableStateOf(false) }
    var confirmUnpair by remember { mutableStateOf(false) }
    var confirmAbandonRetry by remember { mutableStateOf(false) }
    var privacyCovered by remember(forcePrivacyShield) { mutableStateOf(forcePrivacyShield) }
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val drawerState = rememberDrawerState(DrawerValue.Closed)

    LaunchedEffect(controller, wakeRoute, walkthroughSessionId) {
        state = controller.boot()
        state = controller.setTheme(theme.wireName)
        if (walkthroughSessionId != null) {
            state = controller.openSession(walkthroughSessionId)
        }
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
        privacyCovered = forcePrivacyShield
        if (state.connection.name != "CONNECTING") scope.launch { state = controller.refresh() }
    }
    LifecycleEventEffect(Lifecycle.Event.ON_PAUSE) { privacyCovered = true }
    LaunchedEffect(state.connection) {
        if (state.connection == MobileConnectionState.SIGNED_OUT) {
            controller.signOut()
            onSignOut()
        }
    }

    if (privacyCovered) {
        MobilePrivacyShield()
    } else if (state.destination == MobileDestination.CONVERSATION) {
        ConversationScreen(
            state = state,
            onBack = { state = controller.selectDestination(MobileDestination.WORK) },
            onDraft = { state = controller.editDraft(it) },
            onSend = { scope.launch { state = controller.sendTurn(state.draft) } },
            onCancel = { confirmCancel = true },
            onShare = {
                val intent = Intent(Intent.ACTION_SEND).apply {
                    type = "text/plain"
                    putExtra(Intent.EXTRA_TEXT, conversationTranscript(state))
                }
                context.startActivity(Intent.createChooser(intent, "Share Agent work"))
            },
            onDismissNotice = { state = controller.dismissNotice() },
            onContinue = { input -> scope.launch { state = controller.continueLatest(input) } },
            onRetry = { scope.launch { state = controller.retryExact() } },
            onAbandonRetry = { confirmAbandonRetry = true },
        )
    } else {
        BoxWithConstraints {
            val expanded = maxWidth >= 700.dp
            val navigationContent: @Composable () -> Unit = {
                RemoteNavigationContent(
                    origin = origin,
                    state = state,
                    onDestination = {
                        state = controller.selectDestination(it)
                        if (!expanded) scope.launch { drawerState.close() }
                    },
                    onSession = {
                        scope.launch {
                            state = controller.openSession(it)
                            if (!expanded) drawerState.close()
                        }
                    },
                )
            }
            val workspace: @Composable () -> Unit = {
            Scaffold(
                topBar = {
                    TopAppBar(
                        title = {
                            Column {
                                Text("Remote", style = MaterialTheme.typography.titleMedium)
                                Text(
                                    origin.remoteHost(),
                                    color = if (state.connection == MobileConnectionState.ONLINE) {
                                        GariveMint
                                    } else {
                                        MaterialTheme.colorScheme.onSurfaceVariant
                                    },
                                    style = MaterialTheme.typography.labelSmall,
                                )
                            }
                        },
                        navigationIcon = if (expanded) {{}} else {{
                            IconButton(onClick = { scope.launch { drawerState.open() } }) {
                                Icon(Icons.Rounded.Menu, contentDescription = "Open navigation")
                            }
                        }},
                        actions = {
                            IconButton(onClick = {
                                selectedAgent = state.agents.firstOrNull()
                                state = controller.beginTask()
                                showNewTask = true
                            }) { Icon(Icons.Rounded.Add, contentDescription = "New task") }
                        },
                        colors = TopAppBarDefaults.topAppBarColors(containerColor = MaterialTheme.colorScheme.background),
                    )
                },
            ) { padding ->
                    Column(Modifier.padding(padding)) {
                        state.noticeCode?.let { code ->
                            MobileNoticeBanner(
                                code = code,
                                pending = state.pendingCommand != null,
                                onDismiss = { state = controller.dismissNotice() },
                                onRetry = { scope.launch { state = controller.retryExact() } },
                                onAbandonRetry = { confirmAbandonRetry = true },
                            )
                        }
                        when (state.destination) {
                            MobileDestination.WORK -> WorkScreen(
                                state,
                                onOpen = { scope.launch { state = controller.openSession(it) } },
                                onNewTask = {
                                    selectedAgent = state.agents.firstOrNull()
                                    state = controller.beginTask()
                                    showNewTask = true
                                },
                                onRefresh = { scope.launch { state = controller.refresh() } },
                            )
                            MobileDestination.SESSIONS -> SessionsScreen(
                                state,
                                onOpen = { scope.launch { state = controller.openSession(it) } },
                                onRefresh = { scope.launch { state = controller.refresh() } },
                            )
                            MobileDestination.AGENTS -> AgentsScreen(
                                state,
                                onStart = {
                                    selectedAgent = it
                                    state = controller.beginTask()
                                    showNewTask = true
                                },
                                onRefresh = { scope.launch { state = controller.refresh() } },
                            )
                            MobileDestination.SETTINGS -> SettingsScreen(
                                origin, state, theme,
                                onTheme = {
                                    state = controller.setTheme(it.wireName)
                                    onTheme(it)
                                },
                                openNotificationSettings,
                                onSignOut = { confirmUnpair = true },
                            )
                            MobileDestination.CONVERSATION -> Unit
                        }
                        if (state.refreshing && state.sessions.isEmpty()) {
                            CircularProgressIndicator(modifier = Modifier.padding(24.dp))
                        }
                    }
                }
            }
            if (expanded) {
                Row(Modifier.fillMaxSize()) {
                    Surface(
                        modifier = Modifier.width(300.dp).fillMaxHeight(),
                        color = MaterialTheme.colorScheme.background,
                        tonalElevation = 1.dp,
                    ) { navigationContent() }
                    Box(Modifier.weight(1f).fillMaxHeight()) { workspace() }
                }
            } else {
                ModalNavigationDrawer(
                    drawerState = drawerState,
                    drawerContent = {
                        ModalDrawerSheet(
                            modifier = Modifier.width(300.dp).fillMaxHeight(),
                            drawerContainerColor = MaterialTheme.colorScheme.background,
                        ) { navigationContent() }
                    },
                ) { workspace() }
            }
        }
    }

    if (showNewTask) {
        NewTaskSheet(
            agents = state.agents,
            selected = selectedAgent,
            draft = state.draft,
            busy = state.pendingCommand != null,
            online = state.connection == MobileConnectionState.ONLINE,
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

    if (confirmUnpair) {
        UnpairConfirmation(
            onDismiss = { confirmUnpair = false },
            onConfirm = {
                confirmUnpair = false
                controller.signOut()
                onSignOut()
            },
        )
    }

    if (confirmAbandonRetry) {
        AlertDialog(
            onDismissRequest = { confirmAbandonRetry = false },
            title = { Text("Forget exact retry?") },
            text = { Text("The server may already have accepted this command. Refresh history before starting replacement work.") },
            confirmButton = {
                Button(onClick = {
                    confirmAbandonRetry = false
                    state = controller.abandonPending()
                }) { Text("Forget retry") }
            },
            dismissButton = {
                OutlinedButton(onClick = { confirmAbandonRetry = false }) { Text("Keep retry") }
            },
        )
    }
}

@Composable
internal fun MobilePrivacyShield() {
    Surface(
        modifier = Modifier.fillMaxSize(),
        color = MaterialTheme.colorScheme.background,
        contentColor = MaterialTheme.colorScheme.onBackground,
    ) {
        Column(
            modifier = Modifier.fillMaxSize().padding(32.dp),
            horizontalAlignment = Alignment.CenterHorizontally,
            verticalArrangement = Arrangement.Center,
        ) {
            Surface(
                shape = RoundedCornerShape(22.dp),
                color = MaterialTheme.colorScheme.primary.copy(alpha = 0.12f),
            ) {
                Icon(
                    Icons.Rounded.Lock,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.padding(20.dp),
                )
            }
            Text(
                "Remote work is private",
                style = MaterialTheme.typography.headlineSmall,
                modifier = Modifier.padding(top = 20.dp),
            )
            Text(
                "Return to Garive to view your Agent activity.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                style = MaterialTheme.typography.bodyMedium,
                modifier = Modifier.padding(top = 8.dp),
            )
        }
    }
}

@Composable
private fun RemoteNavigationContent(
    origin: String,
    state: MobileWorkState,
    onDestination: (MobileDestination) -> Unit,
    onSession: (String) -> Unit,
) {
    Column(Modifier.fillMaxHeight().padding(horizontal = 14.dp, vertical = 24.dp)) {
        Text(
            "Garive",
            style = MaterialTheme.typography.headlineSmall,
            modifier = Modifier.padding(horizontal = 14.dp),
        )
        Text(
            "Remote · ${origin.remoteHost()}",
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 14.dp, vertical = 8.dp),
        )
        navigationItems.forEach { item ->
            NavigationDrawerItem(
                label = { Text(item.label) },
                selected = state.destination == item.destination,
                icon = { Icon(item.icon, contentDescription = null) },
                colors = NavigationDrawerItemDefaults.colors(
                    selectedContainerColor = MaterialTheme.colorScheme.surfaceVariant,
                    selectedIconColor = MaterialTheme.colorScheme.onSurface,
                    selectedTextColor = MaterialTheme.colorScheme.onSurface,
                ),
                onClick = { onDestination(item.destination) },
            )
        }
        Text(
            "Recent",
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.padding(start = 14.dp, top = 24.dp, bottom = 8.dp),
        )
        state.sessions.take(4).forEach { session ->
            NavigationDrawerItem(
                label = {
                    Column {
                        Text(session.agentName, maxLines = 1)
                        Text(
                            session.status.label(),
                            color = statusColor(session.status),
                            style = MaterialTheme.typography.labelSmall,
                        )
                    }
                },
                selected = state.selectedSessionId == session.sessionId,
                colors = NavigationDrawerItemDefaults.colors(
                    selectedContainerColor = MaterialTheme.colorScheme.surfaceVariant,
                    selectedTextColor = MaterialTheme.colorScheme.onSurface,
                ),
                onClick = { onSession(session.sessionId) },
            )
        }
    }
}

@Composable
internal fun UnpairConfirmation(onDismiss: () -> Unit, onConfirm: () -> Unit) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Unpair this device?") },
        text = { Text("This removes access from this phone. Agent work and history remain on your service.") },
        confirmButton = { Button(onClick = onConfirm) { Text("Unpair device") } },
        dismissButton = { OutlinedButton(onClick = onDismiss) { Text("Keep paired") } },
    )
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun NewTaskSheet(
    agents: List<MobileAgentCard>,
    selected: MobileAgentCard?,
    draft: String,
    busy: Boolean,
    online: Boolean,
    onSelect: (MobileAgentCard) -> Unit,
    onDraft: (String) -> Unit,
    onDismiss: () -> Unit,
    onStart: () -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(onDismissRequest = onDismiss, sheetState = sheetState) {
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
            Text("Agent", style = MaterialTheme.typography.titleMedium)
            LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                items(agents, key = { it.definitionId }) { agent ->
                    FilterChip(
                        selected = selected == agent,
                        onClick = { onSelect(agent) },
                        label = { Text(agent.displayName) },
                    )
                }
            }
            Text("Start with a clear outcome", style = MaterialTheme.typography.titleMedium)
            LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                items(mobileGoalStarters, key = { it.label }) { starter ->
                    OutlinedButton(
                        onClick = { onDraft(starter.prompt) },
                        modifier = Modifier.width(224.dp),
                        shape = RoundedCornerShape(14.dp),
                    ) {
                        Column(Modifier.padding(vertical = 4.dp)) {
                            Text(starter.label, color = MaterialTheme.colorScheme.primary)
                            Text(
                                starter.prompt,
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                                style = MaterialTheme.typography.bodySmall,
                                maxLines = 2,
                            )
                        }
                    }
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
                isError = draft.encodeToByteArray().size > MOBILE_MAX_INPUT_BYTES,
                supportingText = if (draft.encodeToByteArray().size > MOBILE_MAX_INPUT_BYTES) {
                    { Text("Goal is larger than the 16 KiB service limit") }
                } else null,
            )
            Button(
                onClick = onStart,
                modifier = Modifier.fillMaxWidth(),
                enabled = selected != null && draft.isNotBlank() &&
                    draft.encodeToByteArray().size <= MOBILE_MAX_INPUT_BYTES && !busy && online,
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


private fun String.remoteHost(): String = removePrefix("https://").removePrefix("http://").substringBefore('/').ifBlank { "service" }

internal data class MobileGoalStarter(val label: String, val prompt: String)

internal val mobileGoalStarters: List<MobileGoalStarter> = listOf(
    MobileGoalStarter("Synthesize", "Turn notes into a clear decision memo"),
    MobileGoalStarter("Analyze", "Find the key patterns and recommend next steps"),
    MobileGoalStarter("Create", "Draft a polished project brief from my outline"),
)
