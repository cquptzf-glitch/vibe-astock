package com.vibeastock.reminders

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ExternalNavigationTest {
    @Test fun httpsLinksRequireTheConfiguredOwnerHost() {
        assertEquals("/mobile/stock/600519", externalRoute("https://links.example.com/stock/600519", "links.example.com"))
        assertEquals("/mobile/discovery/calendar", externalRoute("https://links.example.com/calendar", "links.example.com"))
        assertEquals("/mobile/ai", externalRoute("https://links.example.com/research", "links.example.com"))
        assertEquals("/mobile/ai", externalRoute("https://LINKS.EXAMPLE.COM/research", "links.example.com"))
        for (raw in listOf("https://other.example.com/research", "https://links.example.com/stock/600519?x=1", "https://links.example.com/stock/600519/extra", "http://links.example.com/research", "https://user@links.example.com/research")) assertNull(externalRoute(raw,"links.example.com"))
        assertNull(externalRoute("https://links.example.com/research"))
    }
    @Test fun onlyKnownApplicationLinksAreAccepted() {
        assertEquals("/mobile/stock/SH600519", externalRoute("vibeastock://stock/SH600519"))
        assertEquals("/mobile/stock/000001", externalRoute("vibeastock://stock/000001"))
        assertEquals("/mobile/discovery/calendar", externalRoute("vibeastock://calendar"))
        assertEquals("/mobile/ai", externalRoute("vibeastock://research/"))
    }
    @Test fun rejectsUnknownHostsCredentialsAndInjectedPaths() {
        for (raw in listOf(null, "", "https://stock/600519", "vibeastock://stock/600519?key=x", "vibeastock://stock/600519#x", "vibeastock://user@stock/600519", "vibeastock://stock:80/600519", "vibeastock://stock/600519/extra", "vibeastock://settings", "vibeastock://calendar/extra", "vibeastock://stock/../../settings")) assertNull(raw, externalRoute(raw))
    }
}
