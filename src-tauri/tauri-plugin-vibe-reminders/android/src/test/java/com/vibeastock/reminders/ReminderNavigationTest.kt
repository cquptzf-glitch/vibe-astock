package com.vibeastock.reminders

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ReminderNavigationTest {
    @Test
    fun acceptsOnlyKnownNotificationDestinations() {
        for (route in listOf(
            "/mobile/journal/daily", "/mobile/reminders",
            "/mobile/stock/SH600519", "/mobile/stock/SZ000001", "/mobile/stock/BJ430047",
        )) {
            assertEquals(route, validReminderRoute(route))
        }
    }

    @Test
    fun rejectsExternalAndMalformedDestinations() {
        for (route in listOf(
            null, "", "https://example.com", "/mobile/settings",
            "/mobile/stock/600519", "/mobile/stock/SH600519/extra",
            "/mobile/stock/SH600519?key=secret", "/mobile/stock/%2e%2e",
        )) {
            assertNull(validReminderRoute(route))
        }
    }
}
