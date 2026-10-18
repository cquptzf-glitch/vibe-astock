package com.vibeastock.reminders

import org.junit.Assert.*
import org.junit.Test

class MarketClockTest {
    @Test fun usesChinaTimeAndRejectsMalformedDates() {
        val millis=quoteMillis("20261016093000")!!
        val clock=marketClock(millis)
        assertEquals("20261016",clock.dayKey)
        assertEquals("2026-10-16",clock.date)
        assertEquals(9,clock.hour)
        assertEquals(30,clock.minute)
        assertEquals("09:30:00",clock.clock)
        assertFalse(clock.weekend)
        assertTrue(marketClock(quoteMillis("20261017093000")!!).weekend)
        for(raw in listOf("20260230093000","20261016253000","2026101609300","20261016093000x"))assertNull(quoteMillis(raw))
    }
}
