package com.vibeastock.reminders

import java.net.URI

/** Same allowlist on cold and warm starts. External text is never executed. */
internal fun externalRoute(raw: String?, httpsHost: String? = null): String? {
    if (raw == null || raw.length > 160) return null
    val uri = try { URI(raw) } catch (_: Exception) { return null }
    if (uri.rawQuery != null || uri.rawFragment != null || uri.userInfo != null || uri.port != -1) return null
    if (uri.scheme == "https") {
        if (httpsHost.isNullOrBlank() || uri.host?.lowercase() != httpsHost.lowercase()) return null
        return when {
            uri.path == "/calendar" -> "/mobile/discovery/calendar"
            uri.path == "/research" -> "/mobile/ai"
            uri.path?.startsWith("/stock/") == true -> uri.path.removePrefix("/stock/").uppercase().takeIf { Regex("^(SH|SZ|BJ)?[0-9]{6}$").matches(it) }?.let { "/mobile/stock/$it" }
            else -> null
        }
    }
    if (uri.scheme != "vibeastock") return null
    return when (uri.host) {
        "stock" -> uri.path?.removePrefix("/")?.uppercase()?.takeIf { Regex("^(SH|SZ|BJ)?[0-9]{6}$").matches(it) }?.let { "/mobile/stock/$it" }
        "calendar" -> if (uri.path.isNullOrBlank() || uri.path == "/") "/mobile/discovery/calendar" else null
        "research" -> if (uri.path.isNullOrBlank() || uri.path == "/") "/mobile/ai" else null
        else -> null
    }
}
