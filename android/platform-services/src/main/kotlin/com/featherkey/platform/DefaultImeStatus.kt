package com.featherkey.platform

/**
 * Whether this keyboard is the system's currently-selected input method.
 *
 * The framework stores the selected IME as a flattened component name "pkg/cls"
 * under Settings.Secure.DEFAULT_INPUT_METHOD. Crucially the class half may be in
 * either form: absolute ("com.featherkey/com.featherkey.ime.FeatherKeyImeService")
 * or package-relative with a leading dot ("com.featherkey/.ime.FeatherKeyImeService")
 * — the platform commonly stores the short form. Reading the value needs a
 * ContentResolver (the caller's job); the match itself is pure so it is
 * unit-testable without an Android runtime (mirrors [SessionPlan]).
 */
object DefaultImeStatus {
    /**
     * True iff [currentDefault] — the raw DEFAULT_INPUT_METHOD value, or null when
     * no IME is selected — denotes the input method [pkg]/[serviceClass]. Accepts
     * both the absolute and the leading-dot package-relative class forms.
     */
    fun isDefault(currentDefault: String?, pkg: String, serviceClass: String): Boolean {
        val value = currentDefault ?: return false
        val slash = value.indexOf('/')
        if (slash < 0) return false
        val curPkg = value.substring(0, slash)
        val rawCls = value.substring(slash + 1)
        // Expand the package-relative ".Foo" form to its absolute class name.
        val curCls = if (rawCls.startsWith(".")) curPkg + rawCls else rawCls
        return curPkg == pkg && curCls == serviceClass
    }
}
