package com.vibeastock.reminders

import java.net.URI

/** Same allowlist on cold and warm starts. External text is never executed. */
internal fun externalRoute(raw: String?): String? {
    if (raw == null || raw.length > 160) return null
    val uri = try { URI(raw) } catch (_: Exception) { return null }
    if (uri.scheme != "vibeastock" || uri.rawQuery != null || uri.rawFragment != null || uri.userInfo != null || uri.port != -1) return null
    return when (uri.host) {
        "stock" -> uri.path?.removePrefix("/")?.uppercase()?.takeIf { Regex("^(SH|SZ|BJ)?[0-9]{6}$").matches(it) }?.let { "/mobile/stock/$it" }
        "calendar" -> if (uri.path.isNullOrBlank() || uri.path == "/") "/mobile/discovery/calendar" else null
        "research" -> if (uri.path.isNullOrBlank() || uri.path == "/") "/mobile/ai" else null
        else -> null
    }
}
