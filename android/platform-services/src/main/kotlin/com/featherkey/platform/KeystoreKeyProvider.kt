package com.featherkey.platform

/*
 * BR-62 — device-bound key provisioning for the encrypted store.
 *
 * ⚠️ AUTHORED, NOT COMPILED / NOT SECURITY-REVIEWED on device. This is the
 * security-critical seam; treat it as a careful first draft and audit it against
 * a real Keystore before shipping (BR-28 security review is a v1.x gate).
 *
 * Design — envelope encryption. The Rust core (`secure-store`) needs a raw
 * 32-byte key for AES-256-GCM at rest, but Android Keystore keys are
 * non-exportable. So:
 *   1. A non-exportable AES-256-GCM **master** key lives in the AndroidKeyStore
 *      (StrongBox-backed where the hardware supports it).
 *   2. A random 32-byte **data** key is generated once, wrapped (encrypted) by
 *      the master key, and the wrapped blob is persisted in app-private storage.
 *   3. On each launch the blob is unwrapped in memory to recover the data key,
 *      which is handed to Rust and then zeroized on the Rust side.
 * The raw data key never touches disk unencrypted, and unwrapping requires the
 * hardware-held master key — so it is bound to this device (BR-62, BR-8).
 */

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyProperties
import java.io.File
import java.security.KeyStore
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

class KeystoreKeyProvider(private val context: Context) {

    private companion object {
        const val MASTER_ALIAS = "featherkey.master.v1"
        const val KEYSTORE = "AndroidKeyStore"
        const val WRAPPED_KEY_FILE = "featherkey.datakey.wrapped"
        // The encrypted store the data key opens; dropped when the key is reset so a
        // fresh key never faces an old, now-undecryptable store. Must match the path
        // FeatherKeyImeService opens (File(filesDir, "featherkey.redb")).
        const val STORE_FILE = "featherkey.redb"
        const val DATA_KEY_BYTES = 32
        const val GCM_TAG_BITS = 128
        const val GCM_IV_BYTES = 12
    }

    /**
     * Return the 32-byte data key for the encrypted store, provisioning it on
     * first run. Caller must hand it straight to the native core and hold no
     * long-lived copy.
     *
     * Self-healing: if a persisted wrapped key cannot be unwrapped — the
     * Keystore master key was invalidated (device restore/migration, secure-lock
     * reset) or the blob on disk is corrupt — we do NOT crash the IME (which
     * would leave the user unable to type). We discard the unusable material and
     * provision a fresh data key. The trade-off is explicit: the prior encrypted
     * store becomes undecryptable and its learned data is effectively reset,
     * which is the correct failure mode for a device-bound key (BR-62) — better a
     * fresh, empty, still-encrypted store than a dead keyboard.
     */
    fun provisionDataKey(): ByteArray {
        val blobFile = File(context.filesDir, WRAPPED_KEY_FILE)
        if (blobFile.exists()) {
            runCatching { unwrap(blobFile.readBytes()) }.getOrNull()?.let { return it }
            // Unwrap failed: the master key or blob is no longer usable. Drop both
            // the wrapped key and the now-unreadable store so we start clean.
            runCatching { blobFile.delete() }
            runCatching { deleteMasterKey() }
            runCatching { File(context.filesDir, STORE_FILE).delete() }
        }
        val dataKey = ByteArray(DATA_KEY_BYTES).also { java.security.SecureRandom().nextBytes(it) }
        runCatching { blobFile.writeBytes(wrap(dataKey)) }
        return dataKey
    }

    /** Remove the master key so the next [masterKey] call regenerates a fresh one. */
    private fun deleteMasterKey() {
        KeyStore.getInstance(KEYSTORE).apply { load(null) }.deleteEntry(MASTER_ALIAS)
    }

    private fun masterKey(): SecretKey {
        val ks = KeyStore.getInstance(KEYSTORE).apply { load(null) }
        (ks.getKey(MASTER_ALIAS, null) as? SecretKey)?.let { return it }

        // Deliberately NOT user-authentication- or unlocked-device-bound: the IME
        // provisions this key at service startup, which can occur on the lock
        // screen (e.g. a direct-reply field), so requiring an unlocked device
        // would make the keyboard unusable exactly when it is needed. Confidentiality
        // rests on the non-exportable, hardware-held (StrongBox where available)
        // master key plus the app sandbox and allowBackup=false, not on lock state.
        val builder = KeyGenParameterSpec.Builder(
            MASTER_ALIAS,
            KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT,
        )
            .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
            .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
            .setKeySize(256)
        // Prefer hardware-backed StrongBox; fall back if the device lacks it.
        runCatching { builder.setIsStrongBoxBacked(true) }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, KEYSTORE)
        return try {
            gen.init(builder.build())
            gen.generateKey()
        } catch (e: Exception) {
            // StrongBox unavailable on this device — retry without it.
            gen.init(builder.setIsStrongBoxBacked(false).build())
            gen.generateKey()
        }
    }

    /** wrapped = iv(12) || ciphertext||tag. */
    private fun wrap(dataKey: ByteArray): ByteArray {
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.ENCRYPT_MODE, masterKey())
        val ct = cipher.doFinal(dataKey)
        return cipher.iv + ct
    }

    private fun unwrap(blob: ByteArray): ByteArray {
        require(blob.size > GCM_IV_BYTES) { "corrupt wrapped key blob" }
        val iv = blob.copyOfRange(0, GCM_IV_BYTES)
        val ct = blob.copyOfRange(GCM_IV_BYTES, blob.size)
        val cipher = Cipher.getInstance("AES/GCM/NoPadding")
        cipher.init(Cipher.DECRYPT_MODE, masterKey(), GCMParameterSpec(GCM_TAG_BITS, iv))
        return cipher.doFinal(ct)
    }
}
