package com.featherkey.onboarding

/*
 * BR-22 — first-run, plain-language consent. Learning is opt-in; the copy states
 * exactly what is (and is not) collected and that everything stays on device.
 *
 * ⚠️ Authored, not compiled / not design-reviewed.
 */

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp

/**
 * @param onContinue invoked with the user's learning choice once they proceed.
 */
@Composable
fun ConsentScreen(onContinue: (learningEnabled: Boolean) -> Unit) {
    var learningEnabled by remember { mutableStateOf(false) }

    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("Your typing stays on this device")
        Text(
            "FeatherKey learns your words on-device to improve suggestions. " +
                "Nothing you type is ever sent off your phone. Password and secure " +
                "fields are never learned. You can review or clear what it has " +
                "learned any time in Settings.",
        )
        Row(verticalAlignment = Alignment.CenterVertically) {
            Switch(checked = learningEnabled, onCheckedChange = { learningEnabled = it })
            Text("Let FeatherKey learn from my typing", modifier = Modifier.padding(start = 12.dp))
        }
        Button(onClick = { onContinue(learningEnabled) }) { Text("Continue") }
    }
}
