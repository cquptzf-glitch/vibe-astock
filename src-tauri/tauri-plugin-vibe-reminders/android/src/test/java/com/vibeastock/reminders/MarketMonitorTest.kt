package com.vibeastock.reminders
import org.junit.Assert.*
import org.junit.Test
class MarketMonitorTest {
    @Test fun minimumSpanAndWindowAreEnforced() {
        assertNull(surgePercent(listOf(MonitorPoint(0, 10.0)), MonitorPoint(59000, 11.0)))
        assertNull(surgePercent(listOf(MonitorPoint(0, 10.0)), MonitorPoint(180001, 11.0)))
        assertEquals(10.0, surgePercent(listOf(MonitorPoint(0, 10.0)), MonitorPoint(60000, 11.0))!!, 1e-9)
        assertNull(surgePercent(listOf(MonitorPoint(0, 0.0)), MonitorPoint(60000, 11.0)))
    }
}
