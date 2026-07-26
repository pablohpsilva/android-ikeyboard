package com.featherkey.platform

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class DefaultImeStatusTest {

    private val pkg = "com.featherkey"
    private val cls = "com.featherkey.ime.FeatherKeyImeService"

    @Test
    fun is_default_when_stored_in_absolute_class_form() {
        val stored = "com.featherkey/com.featherkey.ime.FeatherKeyImeService"
        assertTrue(DefaultImeStatus.isDefault(stored, pkg, cls))
    }

    @Test
    fun is_default_when_stored_in_package_relative_short_form() {
        // The platform commonly stores the leading-dot short form.
        val stored = "com.featherkey/.ime.FeatherKeyImeService"
        assertTrue(DefaultImeStatus.isDefault(stored, pkg, cls))
    }

    @Test
    fun not_default_when_another_keyboard_is_selected() {
        val other = "com.samsung.android.honeyboard/.service.HoneyBoardService"
        assertFalse(DefaultImeStatus.isDefault(other, pkg, cls))
    }

    @Test
    fun not_default_when_no_ime_is_selected() {
        // Settings.Secure.DEFAULT_INPUT_METHOD can be null before any IME is set.
        assertFalse(DefaultImeStatus.isDefault(null, pkg, cls))
    }

    @Test
    fun not_default_when_same_short_class_belongs_to_a_different_package() {
        // A relative ".ime.FeatherKeyImeService" under a foreign package must not
        // match — the leading dot expands against THAT package, not ours.
        val stored = "com.evil.clone/.ime.FeatherKeyImeService"
        assertFalse(DefaultImeStatus.isDefault(stored, pkg, cls))
    }

    @Test
    fun not_default_when_value_has_no_component_separator() {
        assertFalse(DefaultImeStatus.isDefault("garbage-no-slash", pkg, cls))
    }
}
