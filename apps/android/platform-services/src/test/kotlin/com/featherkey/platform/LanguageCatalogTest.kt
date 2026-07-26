package com.featherkey.platform

import org.junit.Assert.assertEquals
import org.junit.Test

class LanguageCatalogTest {
    @Test fun luxembourgish_has_a_native_display_name() {
        assertEquals("Lëtzebuergesch", LanguageCatalog.displayName("lb"))
    }

    @Test fun unknown_tag_falls_back_to_the_tag() {
        assertEquals("xx", LanguageCatalog.displayName("xx"))
    }
}
