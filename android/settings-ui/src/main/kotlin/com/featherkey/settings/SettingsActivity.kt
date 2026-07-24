package com.featherkey.settings

/*
 * Settings + first-run consent host (BR-22, BR-48). Launcher activity: on first
 * run it shows the consent screen; thereafter it shows settings — toggle
 * on-device learning (withdrawable consent) and clear learned data.
 *
 * ⚠️ Authored, not compiled / not run.
 */

import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.featherkey.onboarding.ConsentStore
import java.io.File
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.launch

class SettingsActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val consent = ConsentStore(applicationContext)
        setContent {
            MaterialTheme {
                Surface(modifier = Modifier.fillMaxSize()) {
                    val scope = rememberCoroutineScope()
                    val learning by consent.learningEnabled.collectAsState(initial = false)
                    SettingsScreen(
                        learningEnabled = learning,
                        onLearningChanged = { scope.launch { consent.setLearningEnabled(it) } },
                        onClearLearned = { clearLearnedData() },
                        onEnableKeyboard = {
                            startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
                        },
                    )
                }
            }
        }
    }

    /**
     * Clear learned data by deleting the encrypted store. The next IME session
     * re-provisions a fresh (empty) store. This is the BR-22 "withdraw + erase"
     * escape hatch. (A finer-grained per-word manage UI is BR-9/BR-14, v1.x.)
     */
    private fun clearLearnedData() {
        File(filesDir, "featherkey.redb").delete()
    }
}

@Composable
private fun SettingsScreen(
    learningEnabled: Boolean,
    onLearningChanged: (Boolean) -> Unit,
    onClearLearned: () -> Unit,
    onEnableKeyboard: () -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxSize().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("FeatherKey", style = MaterialTheme.typography.headlineSmall)
        Button(onClick = onEnableKeyboard) { Text("Enable FeatherKey in system settings") }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Switch(checked = learningEnabled, onCheckedChange = onLearningChanged)
            Text("On-device learning", modifier = Modifier.padding(start = 12.dp))
        }
        Text(
            "Nothing you type leaves this device. Password and secure fields are " +
                "never learned.",
            style = MaterialTheme.typography.bodySmall,
        )
        Button(onClick = onClearLearned) { Text("Clear learned data") }
    }
}
