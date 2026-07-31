package com.featherkey.settings

/*
 * Settings + first-run onboarding host (BR-22, BR-48). Launcher activity: on first
 * run it shows the onboarding flow (promise + consent, then turning the keyboard
 * on); thereafter it shows settings, grouped into clear sections —
 *
 *   Setup       enable / set FeatherKey as the default keyboard
 *   Languages   active set (reorderable primary) + available to add
 *   Typing      keyboard height, key outlines, haptics (drive KeyboardView)
 *   Privacy     opt-in learning, the privacy promise, and a guarded data wipe
 *
 * The structure, the guarded destructive action, the active/available language
 * split and the appearance section all come from the design brief's "known gaps".
 *
 * ⚠️ Authored, not compiled / not design-reviewed.
 */

import android.content.Context
import android.content.Intent
import android.os.Bundle
import android.provider.Settings
import android.view.inputmethod.InputMethodManager
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.Card
import androidx.compose.material3.CardDefaults
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Snackbar
import androidx.compose.material3.SnackbarHost
import androidx.compose.material3.SnackbarHostState
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
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
import androidx.compose.ui.unit.sp
import com.featherkey.onboarding.ConsentStore
import com.featherkey.onboarding.OnboardingFlow
import com.featherkey.platform.DefaultImeStatus
import com.featherkey.platform.KeyboardAppearancePrefs
import com.featherkey.platform.KeyboardHeight
import com.featherkey.platform.KeyboardLanguage
import com.featherkey.platform.KeyboardLayoutChoice
import com.featherkey.platform.KeyboardLayoutPrefs
import com.featherkey.platform.LanguageCatalog
import com.featherkey.platform.LanguagePrefs
import java.io.File
import kotlinx.coroutines.launch

class SettingsActivity : ComponentActivity() {

    /**
     * Whether FeatherKey is the system's selected keyboard. Refreshed in
     * [onResume] so returning from system input-method settings (where the user
     * may have just switched to us) updates the setup card and onboarding step live.
     */
    private val isDefaultKeyboard = mutableStateOf(false)

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        val consent = ConsentStore(applicationContext)
        val langPrefs = LanguagePrefs(applicationContext)
        val appearance = KeyboardAppearancePrefs(applicationContext)
        val layoutPrefs = KeyboardLayoutPrefs(applicationContext)
        val languages = LanguageCatalog.all(applicationContext)
        setContent {
            FeatherKeyTheme {
                val scope = rememberCoroutineScope()
                // null = still loading the flag; blank frame avoids flashing the
                // wrong screen (onboarding vs settings) for returning users.
                val onboarded by consent.onboardingComplete.collectAsState(initial = null)
                when (onboarded) {
                    null -> Surface(modifier = Modifier.fillMaxSize()) {}
                    false -> OnboardingFlow(
                        isDefaultKeyboard = isDefaultKeyboard.value,
                        onEnableInSettings = { openImeSettings() },
                        onPickKeyboard = { openImePicker() },
                        onFinish = { learning ->
                            scope.launch {
                                consent.setLearningEnabled(learning)
                                consent.setOnboardingComplete(true)
                            }
                        },
                    )
                    else -> {
                        val learning by consent.learningEnabled.collectAsState(initial = false)
                        SettingsScreen(
                            isDefaultKeyboard = isDefaultKeyboard.value,
                            onEnableKeyboard = { openImeSettings() },
                            languages = languages,
                            initialActive = langPrefs.activeTags(),
                            onActiveChanged = { langPrefs.setActiveTags(it); langPrefs.activeTags() },
                            learningEnabled = learning,
                            onLearningChanged = { scope.launch { consent.setLearningEnabled(it) } },
                            appearance = appearance,
                            layoutPrefs = layoutPrefs,
                            hasLearnedData = hasLearnedData(),
                            onClearLearned = { clearLearnedData() },
                        )
                    }
                }
            }
        }
    }

    override fun onResume() {
        super.onResume()
        val selected = Settings.Secure.getString(contentResolver, Settings.Secure.DEFAULT_INPUT_METHOD)
        isDefaultKeyboard.value = DefaultImeStatus.isDefault(selected, packageName, IME_SERVICE_CLASS)
    }

    private fun openImeSettings() {
        startActivity(Intent(Settings.ACTION_INPUT_METHOD_SETTINGS))
    }

    /** The system "choose input method" sheet — step two of turning the keyboard on. */
    private fun openImePicker() {
        (getSystemService(Context.INPUT_METHOD_SERVICE) as? InputMethodManager)
            ?.showInputMethodPicker()
    }

    /** True if any learned-data file exists — drives the "nothing to clear" state. */
    private fun hasLearnedData(): Boolean =
        LEARNED_FILES.any { File(filesDir, it).exists() }

    /**
     * Clear learned data by deleting the encrypted store and the shell-side
     * learning files. The next IME session re-provisions a fresh (empty) store.
     * This is the BR-22 "withdraw + erase" escape hatch. (A finer-grained per-word
     * manage UI is BR-9/BR-14, v1.x.)
     */
    private fun clearLearnedData() {
        LEARNED_FILES.forEach { File(filesDir, it).delete() }
    }

    private companion object {
        /** The IME service class, used to build our flattened default-IME component. */
        const val IME_SERVICE_CLASS = "com.featherkey.ime.FeatherKeyImeService"

        /** Everything a "clear learned data" wipes: encrypted store + shell TSVs. */
        val LEARNED_FILES = listOf("featherkey.redb", "usage.tsv", "context.tsv")
    }
}

// ---------------------------------------------------------------------------
// Screen
// ---------------------------------------------------------------------------

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun SettingsScreen(
    isDefaultKeyboard: Boolean,
    onEnableKeyboard: () -> Unit,
    languages: List<KeyboardLanguage>,
    initialActive: List<String>,
    onActiveChanged: (List<String>) -> List<String>,
    learningEnabled: Boolean,
    onLearningChanged: (Boolean) -> Unit,
    appearance: KeyboardAppearancePrefs,
    layoutPrefs: KeyboardLayoutPrefs,
    hasLearnedData: Boolean,
    onClearLearned: () -> Unit,
) {
    val snackbar = remember { SnackbarHostState() }
    val scope = rememberCoroutineScope()

    Scaffold(
        topBar = { TopAppBar(title = { Text("FeatherKey") }) },
        snackbarHost = { SnackbarHost(snackbar) { Snackbar(it) } },
    ) { inner ->
        Column(
            modifier = Modifier
                .padding(inner)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 20.dp, vertical = 8.dp),
            verticalArrangement = Arrangement.spacedBy(28.dp),
        ) {
            SetupSection(isDefaultKeyboard, onEnableKeyboard)
            LanguagesSection(languages, initialActive, onActiveChanged)
            TypingSection(appearance, layoutPrefs)
            PrivacySection(
                learningEnabled = learningEnabled,
                onLearningChanged = onLearningChanged,
                hasLearnedData = hasLearnedData,
                onClearLearned = onClearLearned,
                onCleared = { scope.launch { snackbar.showSnackbar("Learned data cleared") } },
            )
            Spacer(Modifier.size(12.dp))
        }
    }
}

// ---------------------------------------------------------------------------
// Setup
// ---------------------------------------------------------------------------

@Composable
private fun SetupSection(isDefaultKeyboard: Boolean, onEnableKeyboard: () -> Unit) {
    Section("Setup") {
        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(14.dp)) {
                if (isDefaultKeyboard) {
                    StatusRow(ok = true, text = "FeatherKey is your default keyboard")
                    Text(
                        "You can switch keyboards any time from your phone’s keyboard settings.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                } else {
                    StatusRow(ok = false, text = "FeatherKey isn’t your keyboard yet")
                    Text(
                        "Turn FeatherKey on in your system keyboard list, then choose it as " +
                            "your keyboard.",
                        style = MaterialTheme.typography.bodyMedium,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                    Button(onClick = onEnableKeyboard, modifier = Modifier.fillMaxWidth()) {
                        Text("Open keyboard settings")
                    }
                }
            }
        }
    }
}

/** A status line: a filled/hollow dot in the semantic colour + the state text. */
@Composable
private fun StatusRow(ok: Boolean, text: String) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Surface(
            shape = androidx.compose.foundation.shape.CircleShape,
            color = if (ok) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline,
            content = {},
            modifier = Modifier.size(10.dp),
        )
        Spacer(Modifier.size(10.dp))
        Text(text, style = MaterialTheme.typography.titleMedium)
    }
}

// ---------------------------------------------------------------------------
// Languages
// ---------------------------------------------------------------------------

@Composable
private fun LanguagesSection(
    languages: List<KeyboardLanguage>,
    initialActive: List<String>,
    onActiveChanged: (List<String>) -> List<String>,
) {
    // Ordered active tags (first = primary). All edits go through [update], which
    // keeps at least one active and reflects the *persisted* result — setActiveTags
    // may expand/reorder the set (e.g. the Luxembourgish companion bundle), so we
    // adopt what it actually saved rather than the raw request.
    var active by remember { mutableStateOf(initialActive.filter { tag -> languages.any { it.tag == tag } }) }
    fun update(next: List<String>) {
        if (next.isEmpty()) return // never leave zero languages selected
        active = onActiveChanged(next).filter { tag -> languages.any { it.tag == tag } }
    }

    val byTag = languages.associateBy { it.tag }
    val activeLangs = active.mapNotNull { byTag[it] }
    val available = languages.filter { it.tag !in active }

    Section("Languages") {
        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(vertical = 8.dp)) {
                Text(
                    "Active",
                    style = MaterialTheme.typography.labelLarge,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(start = 20.dp, top = 12.dp, bottom = 4.dp),
                )
                activeLangs.forEachIndexed { index, lang ->
                    LanguageRow(
                        lang = lang,
                        isPrimary = index == 0,
                        canRemove = activeLangs.size > 1,
                        onMakePrimary = { update(listOf(lang.tag) + active.filter { it != lang.tag }) },
                        onRemove = { update(active.filter { it != lang.tag }) },
                    )
                }

                if (available.isNotEmpty()) {
                    HorizontalDivider(modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp))
                    Text(
                        "Add a language",
                        style = MaterialTheme.typography.labelLarge,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(start = 20.dp, bottom = 4.dp),
                    )
                    available.forEach { lang ->
                        AvailableLanguageRow(lang = lang, onAdd = { update(active + lang.tag) })
                    }
                }
            }
        }
        Text(
            "Several languages can be active at once — the keyboard blends them and " +
                "follows what you’re writing in. The primary language leads the space bar.",
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(start = 4.dp, top = 8.dp),
        )
    }
}

@Composable
private fun LanguageRow(
    lang: KeyboardLanguage,
    isPrimary: Boolean,
    canRemove: Boolean,
    onMakePrimary: () -> Unit,
    onRemove: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Text(lang.displayName, style = MaterialTheme.typography.bodyLarge)
                if (isPrimary) {
                    Spacer(Modifier.size(8.dp))
                    Pill("Primary")
                }
            }
            if (!lang.hasLexicon) LayoutOnlyCaption()
        }
        if (isPrimary) {
            if (canRemove) TextButton(onClick = onRemove) { Text("Remove") }
        } else {
            TextButton(onClick = onMakePrimary) { Text("Make primary") }
            TextButton(onClick = onRemove) { Text("Remove") }
        }
    }
}

@Composable
private fun AvailableLanguageRow(lang: KeyboardLanguage, onAdd: () -> Unit) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(horizontal = 20.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(modifier = Modifier.weight(1f)) {
            Text(
                lang.displayName,
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            if (!lang.hasLexicon) LayoutOnlyCaption()
        }
        OutlinedButton(onClick = onAdd) { Text("Add") }
    }
}

/** Honest treatment of a language we ship a layout but no word list for. */
@Composable
private fun LayoutOnlyCaption() {
    Text(
        "Layout only — no predictions yet",
        style = MaterialTheme.typography.labelSmall,
        color = MaterialTheme.colorScheme.outline,
    )
}

// ---------------------------------------------------------------------------
// Typing (keyboard appearance)
// ---------------------------------------------------------------------------

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun TypingSection(appearance: KeyboardAppearancePrefs, layoutPrefs: KeyboardLayoutPrefs) {
    var height by remember { mutableStateOf(appearance.height()) }
    var outlines by remember { mutableStateOf(appearance.keyOutlines()) }
    var haptics by remember { mutableStateOf(appearance.haptics()) }
    var layout by remember { mutableStateOf(layoutPrefs.choice()) }

    Section("Typing") {
        Card(modifier = Modifier.fillMaxWidth()) {
            Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(18.dp)) {
                Text("Keyboard height", style = MaterialTheme.typography.titleMedium)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    HeightOption("Compact", height == KeyboardHeight.COMPACT) {
                        height = KeyboardHeight.COMPACT; appearance.setHeight(height)
                    }
                    HeightOption("Standard", height == KeyboardHeight.STANDARD) {
                        height = KeyboardHeight.STANDARD; appearance.setHeight(height)
                    }
                    HeightOption("Tall", height == KeyboardHeight.TALL) {
                        height = KeyboardHeight.TALL; appearance.setHeight(height)
                    }
                }
                HorizontalDivider()
                Text("Keyboard layout", style = MaterialTheme.typography.titleMedium)
                Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    LayoutOption("Auto", layout == KeyboardLayoutChoice.AUTO) {
                        layout = KeyboardLayoutChoice.AUTO; layoutPrefs.setChoice(layout)
                    }
                    LayoutOption("QWERTY", layout == KeyboardLayoutChoice.QWERTY) {
                        layout = KeyboardLayoutChoice.QWERTY; layoutPrefs.setChoice(layout)
                    }
                    LayoutOption("QWERTZ", layout == KeyboardLayoutChoice.QWERTZ) {
                        layout = KeyboardLayoutChoice.QWERTZ; layoutPrefs.setChoice(layout)
                    }
                    LayoutOption("AZERTY", layout == KeyboardLayoutChoice.AZERTY) {
                        layout = KeyboardLayoutChoice.AZERTY; layoutPrefs.setChoice(layout)
                    }
                }
                HorizontalDivider()
                ToggleRow(
                    title = "Key outlines",
                    subtitle = "Add a hairline border around every key.",
                    checked = outlines,
                    onChange = { outlines = it; appearance.setKeyOutlines(it) },
                )
                ToggleRow(
                    title = "Haptic feedback",
                    subtitle = "A gentle tick when you press a key.",
                    checked = haptics,
                    onChange = { haptics = it; appearance.setHaptics(it) },
                )
                Text(
                    "Changes apply the next time the keyboard opens.",
                    style = MaterialTheme.typography.labelSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun HeightOption(label: String, selected: Boolean, onSelect: () -> Unit) {
    FilterChip(selected = selected, onClick = onSelect, label = { Text(label) })
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun LayoutOption(label: String, selected: Boolean, onSelect: () -> Unit) {
    FilterChip(selected = selected, onClick = onSelect, label = { Text(label) })
}

// ---------------------------------------------------------------------------
// Privacy & data
// ---------------------------------------------------------------------------

@Composable
private fun PrivacySection(
    learningEnabled: Boolean,
    onLearningChanged: (Boolean) -> Unit,
    hasLearnedData: Boolean,
    onClearLearned: () -> Unit,
    onCleared: () -> Unit,
) {
    var confirmClear by remember { mutableStateOf(false) }
    // Optimistic local mirror so the "nothing to clear" state updates immediately
    // after a wipe without needing to re-read the filesystem.
    var dataPresent by remember { mutableStateOf(hasLearnedData) }

    Section("Privacy & data") {
        // The learning opt-in, raised into a primary-tinted card: the strongest
        // trust control shouldn't read as quiet helper text (a brief "known gap").
        ElevatedCard(
            modifier = Modifier.fillMaxWidth(),
            colors = CardDefaults.elevatedCardColors(
                containerColor = MaterialTheme.colorScheme.primaryContainer,
                contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
            ),
        ) {
            Column(modifier = Modifier.padding(20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Column(modifier = Modifier.weight(1f)) {
                        Text("Learn from what I type", style = MaterialTheme.typography.titleMedium)
                        Text(
                            "Learned words stay on this device. Nothing is uploaded, ever.",
                            style = MaterialTheme.typography.bodyMedium,
                        )
                    }
                    Spacer(Modifier.size(12.dp))
                    Switch(checked = learningEnabled, onCheckedChange = onLearningChanged)
                }
                if (!learningEnabled) {
                    Text(
                        "FeatherKey isn’t learning anything right now. Turn this on for " +
                            "suggestions that adapt to how you write.",
                        style = MaterialTheme.typography.bodySmall,
                    )
                }
            }
        }

        Spacer(Modifier.size(12.dp))

        // Clear learned data — guarded. Disabled (with an honest empty state) when
        // there is nothing to clear.
        if (dataPresent) {
            OutlinedButton(
                onClick = { confirmClear = true },
                modifier = Modifier.fillMaxWidth(),
                colors = ButtonDefaults.outlinedButtonColors(
                    contentColor = MaterialTheme.colorScheme.error,
                ),
            ) { Text("Clear learned data") }
        } else {
            Text(
                "Nothing learned yet — as you type, your common words will start " +
                    "appearing sooner.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = 4.dp),
            )
        }
    }

    if (confirmClear) {
        ClearDataDialog(
            onConfirm = {
                confirmClear = false
                onClearLearned()
                dataPresent = false
                onCleared()
            },
            onDismiss = { confirmClear = false },
        )
    }
}

@Composable
private fun ClearDataDialog(onConfirm: () -> Unit, onDismiss: () -> Unit) {
    androidx.compose.material3.AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Clear learned data?") },
        text = {
            Text(
                "This erases everything FeatherKey has learned on this device — your " +
                    "added words, typing habits and next-word patterns. It can’t be " +
                    "undone, and nothing is backed up anywhere to restore from.",
            )
        },
        confirmButton = {
            TextButton(
                onClick = onConfirm,
                colors = ButtonDefaults.textButtonColors(contentColor = MaterialTheme.colorScheme.error),
            ) { Text("Clear everything") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("Keep it") } },
    )
}

// ---------------------------------------------------------------------------
// Shared building blocks
// ---------------------------------------------------------------------------

/** A titled section: an eyebrow label + its content, spaced consistently. */
@Composable
private fun Section(title: String, content: @Composable () -> Unit) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Text(
            title.uppercase(),
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.SemiBold,
            letterSpacing = 1.4.sp,
            color = MaterialTheme.colorScheme.primary,
        )
        content()
    }
}

/** A title + subtitle row with a trailing switch. */
@Composable
private fun ToggleRow(title: String, subtitle: String, checked: Boolean, onChange: (Boolean) -> Unit) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Column(modifier = Modifier.weight(1f)) {
            Text(title, style = MaterialTheme.typography.titleMedium)
            Text(
                subtitle,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        Spacer(Modifier.size(12.dp))
        Switch(checked = checked, onCheckedChange = onChange)
    }
}

/** A small rounded label chip (e.g. the "Primary" language badge). */
@Composable
private fun Pill(text: String) {
    Surface(
        shape = androidx.compose.foundation.shape.RoundedCornerShape(50),
        color = MaterialTheme.colorScheme.primaryContainer,
        contentColor = MaterialTheme.colorScheme.onPrimaryContainer,
    ) {
        Text(
            text,
            style = MaterialTheme.typography.labelSmall,
            modifier = Modifier.padding(horizontal = 10.dp, vertical = 3.dp),
        )
    }
}
