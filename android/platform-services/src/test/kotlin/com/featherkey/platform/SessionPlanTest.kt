package com.featherkey.platform
import org.junit.Assert.assertEquals
import org.junit.Test

class SessionPlanTest {
    @Test fun opens_new_closes_removed_keeps_order() {
        val plan = SessionPlan.of(openNow = setOf("en", "ru"), desiredTags = listOf("en-US", "es"))
        assertEquals(listOf("en", "es"), plan.order)
        assertEquals(listOf("es"), plan.open)
        assertEquals(listOf("ru"), plan.close)
    }
    @Test fun dedupes_by_language() {
        val plan = SessionPlan.of(openNow = emptySet(), desiredTags = listOf("en-US", "en-GB", "es"))
        assertEquals(listOf("en", "es"), plan.order)
    }
    @Test fun empty_desired_closes_everything() {
        val plan = SessionPlan.of(openNow = setOf("en"), desiredTags = emptyList())
        assertEquals(emptyList<String>(), plan.order)
        assertEquals(listOf("en"), plan.close)
    }
}
