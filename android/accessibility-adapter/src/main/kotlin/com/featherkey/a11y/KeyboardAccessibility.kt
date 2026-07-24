package com.featherkey.a11y

/*
 * Accessibility surface for the keyboard (BR-33/BR-34-ish: usable with TalkBack).
 *
 * ⚠️ Authored, not compiled / not screen-reader-tested. This is a minimal hook:
 * it announces the committed/decoded key so an exploring user gets spoken
 * feedback. Full switch-access (BR-56) and richly-described virtual key nodes are
 * v1.x depth (IMPLEMENTATION_PLAN Wave 5 "deferred depth").
 */

import android.content.Context
import android.view.View
import android.view.accessibility.AccessibilityEvent
import android.view.accessibility.AccessibilityManager

class KeyboardAccessibility(context: Context) {

    private val manager =
        context.getSystemService(Context.ACCESSIBILITY_SERVICE) as AccessibilityManager

    val isEnabled: Boolean get() = manager.isEnabled

    /** Speak [description] (e.g. the decoded key) if a screen reader is active. */
    fun announce(host: View, description: CharSequence) {
        if (!manager.isEnabled) return
        host.announceForAccessibility(description)
    }

    /** Emit a low-level accessibility event for the decoded key. */
    fun sendKeyEvent(host: View, key: String) {
        if (!manager.isEnabled) return
        val event = AccessibilityEvent.obtain(AccessibilityEvent.TYPE_ANNOUNCEMENT)
        event.text.add(key)
        host.parent?.requestSendAccessibilityEvent(host, event)
    }
}
