package com.garive.android.ui

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Lock
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Icon
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import com.garive.android.PairingSuggestion

/** Secure first-run connection surface. */
@Composable
internal fun PairingScreen(
    errorCode: String?,
    pairing: Boolean,
    suggestion: PairingSuggestion?,
    onConnect: (String, String) -> Unit,
) {
    var origin by remember { mutableStateOf("") }
    var code by remember { mutableStateOf("") }
    LaunchedEffect(suggestion) {
        suggestion?.let { origin = it.origin; code = it.code }
    }
    Box(
        modifier = Modifier
            .fillMaxSize()
            .background(MaterialTheme.colorScheme.background)
            .padding(horizontal = 24.dp),
        contentAlignment = Alignment.Center,
    ) {
        Column(
            modifier = Modifier.fillMaxWidth(),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Surface(
                color = MaterialTheme.colorScheme.primary.copy(alpha = 0.14f),
                shape = RoundedCornerShape(20.dp),
            ) {
                Icon(
                    Icons.Rounded.Lock,
                    contentDescription = null,
                    tint = MaterialTheme.colorScheme.primary,
                    modifier = Modifier.padding(18.dp),
                )
            }
            Spacer(Modifier.height(4.dp))
            Text("Pair your server", style = MaterialTheme.typography.displaySmall)
            Text(
                "Keep Agent work moving securely when your computer is out of reach.",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (suggestion != null) {
                Text(
                    "Pairing with ${suggestion.serviceName}",
                    style = MaterialTheme.typography.titleMedium,
                    color = MaterialTheme.colorScheme.secondary,
                )
            }
            OutlinedTextField(
                value = origin,
                onValueChange = { origin = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Service address") },
                placeholder = { Text("https://agent.example.com") },
                singleLine = true,
            )
            OutlinedTextField(
                value = code,
                onValueChange = { code = it },
                modifier = Modifier.fillMaxWidth(),
                label = { Text("Access code") },
                supportingText = { Text("Stored with Android Keystore encryption") },
                visualTransformation = PasswordVisualTransformation(),
                singleLine = true,
            )
            if (errorCode != null) {
                Text(
                    "Connection could not be verified · $errorCode",
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodyMedium,
                )
            }
            Button(
                onClick = { onConnect(origin.trim(), code) },
                enabled = origin.isNotBlank() && code.isNotBlank() && !pairing,
                modifier = Modifier
                    .fillMaxWidth()
                    .height(54.dp),
                shape = RoundedCornerShape(16.dp),
                colors = ButtonDefaults.buttonColors(containerColor = MaterialTheme.colorScheme.primary),
            ) {
                if (pairing) CircularProgressIndicator(strokeWidth = 2.dp)
                else Text("Connect securely")
            }
            Text(
                "Remote connections require HTTPS. Garive never stores the access code in preferences or logs.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}
