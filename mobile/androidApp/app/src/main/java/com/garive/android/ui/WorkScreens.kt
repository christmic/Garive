package com.garive.android.ui

import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyRow
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.rounded.Logout
import androidx.compose.material.icons.rounded.Add
import androidx.compose.material.icons.rounded.CheckCircle
import androidx.compose.material.icons.rounded.ChevronRight
import androidx.compose.material.icons.rounded.CloudDone
import androidx.compose.material.icons.rounded.ErrorOutline
import androidx.compose.material.icons.rounded.HourglassTop
import androidx.compose.material.icons.rounded.NotificationsActive
import androidx.compose.material.icons.rounded.PlayArrow
import androidx.compose.material3.FilterChip
import androidx.compose.material3.Button
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.garive.mobile.model.MobileAgentCard
import com.garive.mobile.model.MobileConnectionState
import com.garive.mobile.model.MobileSessionCard
import com.garive.mobile.model.MobileWorkState
import com.garive.mobile.model.MobileWorkStatus
import com.garive.mobile.preferences.Theme
import com.garive.android.BuildConfig
import android.os.Build
import android.net.Uri

@Composable
internal fun WorkScreen(
    state: MobileWorkState,
    onOpen: (String) -> Unit,
    onNewTask: () -> Unit,
    onRefresh: () -> Unit,
) {
    LazyColumn(
        modifier = Modifier.padding(horizontal = 20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            DestinationHeader("Work", "Your Agent command center", state.connection, onRefresh)
        }
        if (state.sessions.isEmpty()) {
            item { EmptyWork(onNewTask) }
        } else {
            if (state.attention.isNotEmpty()) {
                item { SectionTitle("Needs you", "A quick decision keeps work moving") }
                items(state.attention, key = { it.sessionId }) { WorkCard(it, onOpen) }
            }
            if (state.running.isNotEmpty()) {
                item { SectionTitle("In progress", "Running safely on your server") }
                items(state.running, key = { it.sessionId }) { WorkCard(it, onOpen) }
            }
            if (state.recent.isNotEmpty()) {
                item { SectionTitle("Recent", null) }
                items(state.recent, key = { it.sessionId }) { WorkCard(it, onOpen) }
            }
        }
        item { Spacer(Modifier.height(100.dp)) }
    }
}

@Composable
internal fun SessionsScreen(state: MobileWorkState, onOpen: (String) -> Unit, onRefresh: () -> Unit) {
    var filter by remember { mutableStateOf("All") }
    var search by remember { mutableStateOf("") }
    val visible = state.sessions.filter { session ->
        val matchesFilter = when (filter) {
            "Working" -> session.status == MobileWorkStatus.WORKING
            "Needs you" -> session.status == MobileWorkStatus.NEEDS_INPUT
            "Done" -> session.status in setOf(MobileWorkStatus.COMPLETED, MobileWorkStatus.STOPPED)
            else -> true
        }
        matchesFilter && (search.isBlank() || session.agentName.contains(search, ignoreCase = true) ||
            session.sessionId.contains(search, ignoreCase = true))
    }
    LazyColumn(
        modifier = Modifier.padding(horizontal = 20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item {
            DestinationHeader("Sessions", "Durable work, ready anywhere", state.connection, onRefresh)
            OutlinedTextField(
                value = search,
                onValueChange = { search = it },
                label = { Text("Search Agent or Session") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Spacer(Modifier.height(8.dp))
            LazyRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                items(listOf("All", "Working", "Needs you", "Done")) { label ->
                    FilterChip(
                        selected = filter == label,
                        onClick = { filter = label },
                        label = { Text(label) },
                    )
                }
            }
            Spacer(Modifier.height(4.dp))
        }
        items(visible, key = { it.sessionId }) { WorkCard(it, onOpen) }
        if (visible.isEmpty()) {
            item {
                Text(
                    "No Sessions in this view.",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(vertical = 32.dp),
                )
            }
        }
        item { Spacer(Modifier.height(100.dp)) }
    }
}

@Composable
internal fun AgentsScreen(state: MobileWorkState, onStart: (MobileAgentCard) -> Unit, onRefresh: () -> Unit) {
    val largeText = LocalDensity.current.fontScale >= 1.6f
    LazyColumn(
        modifier = Modifier.padding(horizontal = 20.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        item { DestinationHeader("Agents", "Choose the right specialist", state.connection, onRefresh) }
        items(state.agents, key = { it.definitionId }) { agent ->
            var showingDetails by remember(agent.definitionId) { mutableStateOf(false) }
            Surface(
                shape = RoundedCornerShape(20.dp),
                color = MaterialTheme.colorScheme.surface,
                border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline.copy(alpha = 0.32f)),
                modifier = Modifier.fillMaxWidth().clickable { onStart(agent) },
            ) {
                Column(Modifier.padding(18.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                    Row(verticalAlignment = Alignment.CenterVertically, modifier = Modifier.fillMaxWidth()) {
                        Surface(shape = CircleShape, color = MaterialTheme.colorScheme.primary.copy(alpha = 0.14f)) {
                            Icon(
                                Icons.Rounded.PlayArrow,
                                contentDescription = null,
                                tint = MaterialTheme.colorScheme.primary,
                                modifier = Modifier.padding(10.dp),
                            )
                        }
                        if (!largeText) {
                            Column(Modifier.weight(1f).padding(start = 12.dp)) {
                                Text(agent.displayName, style = MaterialTheme.typography.titleLarge)
                                Text(
                                    "Ready on your server",
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        } else {
                            Spacer(Modifier.weight(1f))
                        }
                        Icon(Icons.Rounded.ChevronRight, contentDescription = "Start with ${agent.displayName}")
                    }
                    if (largeText) {
                        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text(agent.displayName, style = MaterialTheme.typography.titleLarge)
                            Text(
                                "Ready on your server",
                                color = MaterialTheme.colorScheme.onSurfaceVariant,
                            )
                        }
                    }
                    if (agent.capabilities.isNotEmpty()) {
                        Text(
                            agent.capabilities.joinToString(" · ") { it.replace('_', ' ') },
                            style = MaterialTheme.typography.bodySmall,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                        )
                    }
                    TextButton(onClick = { showingDetails = !showingDetails }) {
                        Text(if (showingDetails) "Hide details" else "Details")
                    }
                    if (showingDetails) {
                        Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                            Text("Revision ${agent.revision}", style = MaterialTheme.typography.labelMedium)
                            SelectionContainer {
                                Text(
                                    agent.definitionId,
                                    style = MaterialTheme.typography.bodySmall,
                                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                                )
                            }
                        }
                    }
                }
            }
        }
        item { Spacer(Modifier.height(100.dp)) }
    }
}

@Composable
internal fun SettingsScreen(
    origin: String,
    state: MobileWorkState,
    theme: Theme,
    onTheme: (Theme) -> Unit,
    openNotificationSettings: () -> Unit,
    onSignOut: () -> Unit,
) {
    val context = LocalContext.current
    var diagnosticsCopied by remember { mutableStateOf(false) }
    LazyColumn(
        modifier = Modifier.padding(horizontal = 20.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        item { DestinationHeader("Settings", "This device and service", state.connection, null) }
        item {
            SettingsCard {
                Text("Paired service", style = MaterialTheme.typography.titleMedium)
                Text(origin, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                Text(
                    "Verified host · ${Uri.parse(origin).host ?: "—"}",
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                HorizontalDivider(Modifier.padding(vertical = 14.dp))
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Rounded.CloudDone, contentDescription = null, tint = GariveMint)
                    Text("HTTPS remote control", modifier = Modifier.padding(start = 10.dp))
                }
            }
        }
        item {
            SettingsCard {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Icon(Icons.Rounded.NotificationsActive, contentDescription = null)
                    Column(Modifier.padding(start = 12.dp)) {
                        Text("Action notifications", style = MaterialTheme.typography.titleMedium)
                        Text("Status only on lock screen", color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
                OutlinedButton(onClick = openNotificationSettings, modifier = Modifier.padding(top = 14.dp)) {
                    Text("Open notification settings")
                }
            }
        }
        item {
            SettingsCard {
                Text("Appearance", style = MaterialTheme.typography.titleMedium)
                SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth().padding(top = 12.dp)) {
                    Theme.entries.forEachIndexed { index, choice ->
                        SegmentedButton(
                            selected = choice == theme,
                            onClick = { onTheme(choice) },
                            shape = SegmentedButtonDefaults.itemShape(index, Theme.entries.size),
                        ) { Text(choice.wireName.replaceFirstChar(Char::uppercase)) }
                    }
                }
            }
        }
        item {
            SettingsCard {
                Text("Diagnostics", style = MaterialTheme.typography.titleMedium)
                Text("Device · ${Build.MODEL}", color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text("Android · ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})", color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text("Garive · ${BuildConfig.VERSION_NAME}", color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text("Connection · ${state.connection.name.lowercase()}", color = MaterialTheme.colorScheme.onSurfaceVariant)
                OutlinedButton(
                    onClick = {
                        context.getSystemService(Context.CLIPBOARD_SERVICE)
                            .let { it as ClipboardManager }
                            .setPrimaryClip(ClipData.newPlainText("Garive diagnostics", safeDiagnostics(state)))
                        diagnosticsCopied = true
                    },
                    modifier = Modifier.padding(top = 10.dp),
                ) {
                    Text(if (diagnosticsCopied) "Diagnostics copied" else "Copy safe diagnostics")
                }
            }
        }
        item {
            OutlinedButton(
                onClick = onSignOut,
                modifier = Modifier.fillMaxWidth().heightIn(min = 52.dp),
                shape = RoundedCornerShape(16.dp),
            ) {
                Icon(Icons.AutoMirrored.Rounded.Logout, contentDescription = null)
                Text("Unpair this device", modifier = Modifier.padding(start = 8.dp))
            }
        }
        item { Spacer(Modifier.height(100.dp)) }
    }
}

internal fun safeDiagnostics(state: MobileWorkState): String = listOf(
    "Garive ${BuildConfig.VERSION_NAME}",
    "Android ${Build.VERSION.RELEASE} (API ${Build.VERSION.SDK_INT})",
    "Connection ${state.connection.name.lowercase()}",
).joinToString("\n")

@Composable
private fun DestinationHeader(
    title: String,
    subtitle: String,
    connection: MobileConnectionState,
    onRefresh: (() -> Unit)?,
) {
    Spacer(Modifier.height(14.dp))
    Row(verticalAlignment = Alignment.Top) {
        Column(Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.headlineSmall)
            Text(subtitle, color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodyMedium)
        }
        if (onRefresh != null) {
            FilledIconButton(onClick = onRefresh) {
                Icon(Icons.Rounded.CloudDone, contentDescription = "Refresh")
            }
        }
    }
    Surface(
        shape = CircleShape,
        color = statusColor(connection).copy(alpha = 0.13f),
    ) {
        Text(
            connection.label(),
            color = statusColor(connection),
            style = MaterialTheme.typography.labelMedium,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
        )
    }
    Spacer(Modifier.height(10.dp))
}

@Composable
private fun WorkCard(session: MobileSessionCard, onOpen: (String) -> Unit) {
    val largeText = LocalDensity.current.fontScale >= 1.6f
    Surface(
        shape = RoundedCornerShape(12.dp),
        color = Color.Transparent,
        modifier = Modifier.fillMaxWidth().clickable { onOpen(session.sessionId) },
    ) {
        if (largeText) {
            Column(Modifier.padding(17.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
                Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                    WorkStatusIcon(session.status)
                    Spacer(Modifier.weight(1f))
                    Icon(Icons.Rounded.ChevronRight, contentDescription = "Open Session")
                }
                WorkCardText(session)
            }
        } else {
            Row(Modifier.padding(horizontal = 6.dp, vertical = 13.dp), verticalAlignment = Alignment.CenterVertically) {
                WorkStatusIcon(session.status)
                Column(Modifier.weight(1f).padding(horizontal = 12.dp)) {
                    WorkCardText(session)
                }
                Icon(Icons.Rounded.ChevronRight, contentDescription = "Open Session")
            }
        }
    }
}

@Composable
private fun WorkStatusIcon(status: MobileWorkStatus) {
    Surface(shape = CircleShape, color = Color.Transparent) {
        Icon(
            statusIcon(status),
            contentDescription = null,
            tint = statusColor(status),
            modifier = Modifier.padding(5.dp).size(20.dp),
        )
    }
}

@Composable
private fun WorkCardText(session: MobileSessionCard) {
    Text(session.agentName, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
    Text(
        "${session.status.label()} · ${session.turnCount} ${if (session.turnCount == 1L) "turn" else "turns"}",
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        style = MaterialTheme.typography.bodyMedium,
    )
}

@Composable
private fun EmptyWork(onNewTask: () -> Unit) {
    Surface(shape = RoundedCornerShape(24.dp), color = MaterialTheme.colorScheme.surfaceVariant.copy(alpha = 0.5f)) {
        Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(10.dp)) {
            Text("Your Agents are ready", style = MaterialTheme.typography.headlineSmall)
            Text(
                "Start work now, put your phone away, and return only when the Agent needs you.",
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            OutlinedButton(onClick = onNewTask) { Text("Choose an Agent") }
        }
    }
}

@Composable
private fun SectionTitle(title: String, subtitle: String?) {
    Column(Modifier.padding(top = 12.dp, bottom = 2.dp)) {
        Text(title, style = MaterialTheme.typography.titleLarge, fontWeight = FontWeight.SemiBold)
        if (subtitle != null) Text(subtitle, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

@Composable
private fun SettingsCard(content: @Composable ColumnScope.() -> Unit) {
    Surface(shape = RoundedCornerShape(20.dp), color = MaterialTheme.colorScheme.surface) {
        Column(Modifier.fillMaxWidth().padding(18.dp), content = content)
    }
}

@Composable
internal fun statusColor(status: MobileWorkStatus): Color = when (status) {
    MobileWorkStatus.WORKING -> MaterialTheme.colorScheme.primary
    MobileWorkStatus.NEEDS_INPUT -> GariveAmber
    MobileWorkStatus.COMPLETED -> GariveMint
    MobileWorkStatus.FAILED -> MaterialTheme.colorScheme.error
    else -> MaterialTheme.colorScheme.onSurfaceVariant
}

@Composable
private fun statusColor(connection: MobileConnectionState): Color = when (connection) {
    MobileConnectionState.ONLINE -> GariveMint
    MobileConnectionState.CONNECTING, MobileConnectionState.RECONNECTING -> GariveAmber
    MobileConnectionState.OFFLINE, MobileConnectionState.SECURITY_ERROR -> MaterialTheme.colorScheme.error
    MobileConnectionState.SIGNED_OUT -> MaterialTheme.colorScheme.onSurfaceVariant
}

private fun statusIcon(status: MobileWorkStatus) = when (status) {
    MobileWorkStatus.WORKING -> Icons.Rounded.HourglassTop
    MobileWorkStatus.NEEDS_INPUT -> Icons.Rounded.NotificationsActive
    MobileWorkStatus.COMPLETED -> Icons.Rounded.CheckCircle
    MobileWorkStatus.FAILED -> Icons.Rounded.ErrorOutline
    else -> Icons.Rounded.CloudDone
}

internal fun MobileWorkStatus.label(): String = when (this) {
    MobileWorkStatus.READY -> "Ready"
    MobileWorkStatus.WORKING -> "Working"
    MobileWorkStatus.NEEDS_INPUT -> "Needs you"
    MobileWorkStatus.COMPLETED -> "Completed"
    MobileWorkStatus.STOPPED -> "Stopped"
    MobileWorkStatus.FAILED -> "Failed"
    MobileWorkStatus.UPDATED -> "Updated"
}

private fun MobileConnectionState.label(): String = when (this) {
    MobileConnectionState.ONLINE -> "Server connected"
    MobileConnectionState.CONNECTING -> "Connecting"
    MobileConnectionState.RECONNECTING -> "Reconnecting"
    MobileConnectionState.OFFLINE -> "Offline · verified history"
    MobileConnectionState.SIGNED_OUT -> "Not paired"
    MobileConnectionState.SECURITY_ERROR -> "Security check required"
}
