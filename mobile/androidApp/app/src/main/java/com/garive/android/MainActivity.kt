package com.garive.android
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.*
import androidx.compose.material3.*
import androidx.compose.runtime.*
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
class MainActivity : ComponentActivity() { override fun onCreate(state: Bundle?) { super.onCreate(state); setContent { GariveApp() } } }
@Composable fun GariveApp() { var output by remember { mutableStateOf("") }; MaterialTheme { Column(Modifier.padding(24.dp), verticalArrangement = Arrangement.spacedBy(16.dp)) {
    Text("Garive Agent", style = MaterialTheme.typography.headlineLarge); Text("You: hello")
    Button(onClick = { output = "hello from Garive · completed" }) { Text("Run embedded host") }; Text(output.ifEmpty { "Ready" }) } } }
