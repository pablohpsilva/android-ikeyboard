package com.featherkey.app

/*
 * The Android composition root (ARCH §9.3, Kotlin side). Process-level setup
 * lives here; the IME service and Activities are the entry points.
 *
 * ⚠️ Authored, not compiled. Deliberately minimal — the native core is opened
 * per IME session inside FeatherKeyImeService (which owns its lifecycle), not
 * here, so an Application crash can never strand a native handle.
 */

import android.app.Application

class FeatherKeyApplication : Application()
