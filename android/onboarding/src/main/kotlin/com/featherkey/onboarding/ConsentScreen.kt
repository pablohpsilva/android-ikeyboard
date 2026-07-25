package com.featherkey.onboarding

/*
 * BR-22 — first-run onboarding. Two steps, in the order that earns trust before
 * it asks for anything:
 *
 *   1. The promise + the (opt-in, off-by-default) learning consent. The copy
 *      states exactly what is and is not learned and that everything stays on
 *      device, encrypted.
 *   2. Turning FeatherKey on — the genuinely hard step a single consent screen
 *      used to skip: enabling the keyboard in system settings and picking it as
 *      the active one. The step self-updates to a done state once we are default.
 *
 * Colours come from the active Material 3 scheme (see settings-ui FeatherKeyTheme),
 * so light/dark and dynamic colour are all handled without hard-coded values.
 *
 * ⚠️ Authored, not compiled / not design-reviewed.
 */

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ElevatedCard
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp

/**
 * The first-run flow. Ends by calling [onFinish] with the user's learning choice;
 * the host activity persists both that choice and the onboarding-complete flag.
 *
 * @param isDefaultKeyboard whether FeatherKey is already the active keyboard
 *   (refreshed by the host on resume, so the turn-on step reflects reality live).
 * @param onEnableInSettings open the system keyboard list (step one of turning on).
 * @param onPickKeyboard open the input-method picker (step two).
 * @param onFinish user is done onboarding; carries the learning opt-in.
 */
@Composable
fun OnboardingFlow(
    isDefaultKeyboard: Boolean,
    onEnableInSettings: () -> Unit,
    onPickKeyboard: () -> Unit,
    onFinish: (learningEnabled: Boolean) -> Unit,
) {
    var step by remember { mutableIntStateOf(0) }
    var learningEnabled by remember { mutableStateOf(false) }

    Surface(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 24.dp, vertical = 32.dp),
        ) {
            StepDots(current = step, total = 2)
            Spacer(Modifier.size(28.dp))
            when (step) {
                0 -> ConsentStep(
                    learningEnabled = learningEnabled,
                    onLearningChanged = { learningEnabled = it },
                    onContinue = { step = 1 },
                )
                else -> TurnOnStep(
                    isDefaultKeyboard = isDefaultKeyboard,
                    onEnableInSettings = onEnableInSettings,
                    onPickKeyboard = onPickKeyboard,
                    onDone = { onFinish(learningEnabled) },
                )
            }
        }
    }
}

/** Backwards-compatible entry point: a bare consent screen with no turn-on step. */
@Composable
fun ConsentScreen(onContinue: (learningEnabled: Boolean) -> Unit) {
    var learningEnabled by remember { mutableStateOf(false) }
    Surface(modifier = Modifier.fillMaxSize()) {
        Column(
            modifier = Modifier.fillMaxSize().padding(24.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            ConsentStep(
                learningEnabled = learningEnabled,
                onLearningChanged = { learningEnabled = it },
                onContinue = { onContinue(learningEnabled) },
            )
        }
    }
}

@Composable
private fun ConsentStep(
    learningEnabled: Boolean,
    onLearningChanged: (Boolean) -> Unit,
    onContinue: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
        Text(
            "Your typing never leaves this device",
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.SemiBold,
        )
        Text(
            "FeatherKey can learn the words you use so suggestions fit how you " +
                "write. That learning happens here, on your phone, in encrypted " +
                "storage. It’s never uploaded, shared or synced — and passwords " +
                "and payment fields are always skipped.",
            style = MaterialTheme.typography.bodyLarge,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        // The opt-in itself, raised into a card so the one thing being asked for
        // reads as a distinct decision rather than another line of body text.
        ElevatedCard {
            Row(
                modifier = Modifier.fillMaxWidth().padding(20.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text("Learn from what I type", style = MaterialTheme.typography.titleMedium)
                    Text(
                        "Off by default. The keyboard works fully without it.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                Spacer(Modifier.size(12.dp))
                Switch(checked = learningEnabled, onCheckedChange = onLearningChanged)
            }
        }
        Button(onClick = onContinue, modifier = Modifier.fillMaxWidth()) {
            Text("Continue")
        }
    }
}

@Composable
private fun TurnOnStep(
    isDefaultKeyboard: Boolean,
    onEnableInSettings: () -> Unit,
    onPickKeyboard: () -> Unit,
    onDone: () -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(20.dp)) {
        Text(
            "Turn on FeatherKey",
            style = MaterialTheme.typography.headlineMedium,
            fontWeight = FontWeight.SemiBold,
        )
        if (isDefaultKeyboard) {
            Text(
                "FeatherKey is your keyboard. You’re all set — you can fine-tune " +
                    "languages and typing in settings whenever you like.",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            Text(
                "Two quick steps in your system settings: switch FeatherKey on in " +
                    "the keyboard list, then pick it as your keyboard. You can change " +
                    "back at any time.",
                style = MaterialTheme.typography.bodyLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            StepRow(1, "Switch FeatherKey on", onEnableInSettings)
            StepRow(2, "Choose FeatherKey as your keyboard", onPickKeyboard)
        }
        Button(onClick = onDone, modifier = Modifier.fillMaxWidth()) {
            Text(if (isDefaultKeyboard) "Done" else "I’ll do this later")
        }
    }
}

/** A numbered, tappable step: [n] · label · action button. */
@Composable
private fun StepRow(n: Int, label: String, onClick: () -> Unit) {
    ElevatedCard {
        Row(
            modifier = Modifier.fillMaxWidth().padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            NumberBadge(n)
            Spacer(Modifier.size(14.dp))
            Text(
                label,
                modifier = Modifier.weight(1f),
                style = MaterialTheme.typography.bodyLarge,
            )
            Spacer(Modifier.size(8.dp))
            OutlinedButton(onClick = onClick) { Text("Open") }
        }
    }
}

@Composable
private fun NumberBadge(n: Int) {
    Surface(
        shape = CircleShape,
        color = MaterialTheme.colorScheme.primaryContainer,
        modifier = Modifier.size(28.dp),
    ) {
        Box(contentAlignment = Alignment.Center) {
            Text(
                n.toString(),
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onPrimaryContainer,
            )
        }
    }
}

/** Progress dots for the [total]-step flow; the [current] step's dot is filled. */
@Composable
private fun StepDots(current: Int, total: Int) {
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        repeat(total) { i ->
            val on = i == current
            Surface(
                shape = CircleShape,
                color = if (on) MaterialTheme.colorScheme.primary
                else MaterialTheme.colorScheme.surfaceVariant,
                content = {},
                modifier = Modifier.size(if (on) 10.dp else 8.dp),
            )
        }
    }
}
