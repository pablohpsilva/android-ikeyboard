package com.featherkey.onboarding

/*
 * BR-22 — the user's consent to on-device learning, persisted and withdrawable.
 *
 * On-device learning is ON by default (the adaptive keyboard learns from what
 * the user types out of the box). The flag is read by the IME before it learns
 * and can be withdrawn from settings at any time. Sensitivity gating (BR-26)
 * still applies unconditionally — password/OTP fields are never learned from,
 * regardless of this flag. Stored in plain (non-secret) DataStore preferences —
 * it is a boolean preference, not personal data.
 *
 * ⚠️ Authored, not compiled.
 */

import android.content.Context
import androidx.datastore.core.DataStore
import androidx.datastore.preferences.core.Preferences
import androidx.datastore.preferences.core.booleanPreferencesKey
import androidx.datastore.preferences.core.edit
import androidx.datastore.preferences.preferencesDataStore
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map

private val Context.dataStore: DataStore<Preferences> by preferencesDataStore(name = "featherkey_consent")

class ConsentStore(private val context: Context) {

    private val learningEnabledKey = booleanPreferencesKey("learning_enabled")
    private val onboardingCompleteKey = booleanPreferencesKey("onboarding_complete")

    /** Whether on-device learning is enabled. Default: true (learn by default). */
    val learningEnabled: Flow<Boolean> =
        context.dataStore.data.map { it[learningEnabledKey] ?: true }

    suspend fun setLearningEnabled(enabled: Boolean) {
        context.dataStore.edit { it[learningEnabledKey] = enabled }
    }

    /**
     * Whether first-run onboarding has been completed. Default: false. This is
     * kept separate from [learningEnabled] on purpose: "learning is off" and
     * "we have never asked" are different states, and only the latter should show
     * the consent flow. The launcher activity reads this to decide whether to open
     * onboarding or settings.
     */
    val onboardingComplete: Flow<Boolean> =
        context.dataStore.data.map { it[onboardingCompleteKey] ?: false }

    suspend fun setOnboardingComplete(complete: Boolean) {
        context.dataStore.edit { it[onboardingCompleteKey] = complete }
    }
}
