package com.featherkey.onboarding

/*
 * BR-22 — the user's consent to on-device learning, persisted and withdrawable.
 *
 * On-device learning is OFF until the user explicitly opts in (privacy by
 * default). The flag is read by the IME before it learns and can be withdrawn
 * from settings at any time. Stored in plain (non-secret) DataStore preferences —
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

    /** Whether the user has opted into on-device learning. Default: false. */
    val learningEnabled: Flow<Boolean> =
        context.dataStore.data.map { it[learningEnabledKey] ?: false }

    suspend fun setLearningEnabled(enabled: Boolean) {
        context.dataStore.edit { it[learningEnabledKey] = enabled }
    }
}
