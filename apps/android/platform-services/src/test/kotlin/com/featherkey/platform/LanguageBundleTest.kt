package com.featherkey.platform

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class LanguageBundleTest {

    @Test fun adding_lb_first_time_appends_missing_companions_lb_stays_first() {
        val r = LanguageBundle.withCompanions(
            current = listOf("en"),
            requested = listOf("lb", "en"),
            alreadyApplied = false,
        )
        assertEquals(listOf("lb", "en", "de", "fr"), r.tags)
        assertTrue(r.bundleApplied)
    }

    @Test fun does_not_duplicate_already_active_companions() {
        val r = LanguageBundle.withCompanions(
            current = listOf("fr"),
            requested = listOf("lb", "fr"),
            alreadyApplied = false,
        )
        assertEquals(listOf("lb", "fr", "de", "en"), r.tags)
        assertTrue(r.bundleApplied)
    }

    @Test fun already_applied_is_a_noop() {
        val r = LanguageBundle.withCompanions(
            current = listOf("lb", "de", "fr", "en"),
            requested = listOf("lb"),
            alreadyApplied = true,
        )
        assertEquals(listOf("lb"), r.tags)
        assertTrue(r.bundleApplied)
    }

    @Test fun lb_already_active_does_not_retrigger() {
        // A reorder/rotate where lb is in both current and requested must not fire.
        val r = LanguageBundle.withCompanions(
            current = listOf("lb", "de"),
            requested = listOf("de", "lb"),
            alreadyApplied = false,
        )
        assertEquals(listOf("de", "lb"), r.tags)
        assertFalse(r.bundleApplied)
    }

    @Test fun no_lb_requested_is_a_noop() {
        val r = LanguageBundle.withCompanions(
            current = listOf("en"),
            requested = listOf("en", "pt"),
            alreadyApplied = false,
        )
        assertEquals(listOf("en", "pt"), r.tags)
        assertFalse(r.bundleApplied)
    }

    @Test fun adding_lb_to_an_existing_set_promotes_it_to_primary() {
        // The "Add" action appends lb to the end; the bundle must move it to
        // primary so its QWERTZ layout and momentum head-start take effect.
        val r = LanguageBundle.withCompanions(
            current = listOf("en", "pt", "es", "fr"),
            requested = listOf("en", "pt", "es", "fr", "lb"),
            alreadyApplied = false,
        )
        assertEquals(listOf("lb", "en", "pt", "es", "fr", "de"), r.tags)
        assertTrue(r.bundleApplied)
    }
}
