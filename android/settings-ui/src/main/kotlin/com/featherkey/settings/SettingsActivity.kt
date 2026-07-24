package com.featherkey.settings

/*
 * Settings + first-run consent host (BR-22, BR-48). Launcher activity: on first
 * run it shows the consent screen; thereafter it shows settings — choose active
 * languages (several at once), toggle on-device learning (withdrawable consent),
 * and clear learned data.
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
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.Checkbox
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.featherkey.onboarding.ConsentStore
import com.featherkey.platform.KeyboardLanguage
import com.featherkey.platform.LanguageCatalog
import com.featherkey.platform.LanguagePrefs
import java.io.File
import kotlinx.coroutines.launch

class SettingsActivity : ComponentActivity() {

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val consent = ConsentStore(applicationContext)
        val langPrefs = LanguagePrefs(applicationContext)
        val languages = LanguageCatalog.all(applicationContext)
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
                        languages = languages,
                        initialActive = langPrefs.activeTags(),
                        onActiveChanged = { langPrefs.setActiveTags(it) },
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
    languages: List<KeyboardLanguage>,
    initialActive: List<String>,
    onActiveChanged: (List<String>) -> Unit,
) {
    Column(
        modifier = Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        Text("FeatherKey", style = MaterialTheme.typography.headlineSmall)
        Button(onClick = onEnableKeyboard) { Text("Enable FeatherKey in system settings") }

        LanguageSection(languages, initialActive, onActiveChanged)

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

/**
 * Multi-select language list. Every checked language is active at the same time;
 * the keyboard's globe key cycles which one is primary. At least one language
 * stays selected. Languages with no bundled word list are still selectable —
 * they contribute predictions only once a list is added.
 */
@Composable
private fun LanguageSection(
    languages: List<KeyboardLanguage>,
    initialActive: List<String>,
    onActiveChanged: (List<String>) -> Unit,
) {
    var active by remember { mutableStateOf(initialActive.toSet()) }
    Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
        Text("Languages", style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.Bold)
        Text(
            "Choose one or more. Several can be active at once; the globe key on " +
                "the keyboard cycles between them.",
            style = MaterialTheme.typography.bodySmall,
        )
        languages.forEach { lang ->
            val checked = active.contains(lang.tag)
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Checkbox(
                    checked = checked,
                    onCheckedChange = { want ->
                        val next = if (want) active + lang.tag else active - lang.tag
                        if (next.isNotEmpty()) { // keep at least one selected
                            active = next
                            // Persist in catalog order (primary = first active).
                            onActiveChanged(languages.filter { next.contains(it.tag) }.map { it.tag })
                        }
                    },
                )
                Column(modifier = Modifier.padding(start = 8.dp)) {
                    Text(lang.displayName)
                    if (!lang.hasLexicon) {
                        Text(
                            "no word list yet",
                            style = MaterialTheme.typography.labelSmall,
                            color = MaterialTheme.colorScheme.outline,
                        )
                    }
                }
            }
        }
    }
}
