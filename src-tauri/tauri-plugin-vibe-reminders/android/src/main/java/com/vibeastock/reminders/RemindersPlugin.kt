package com.vibeastock.reminders
import android.net.Uri

import android.Manifest
import android.app.Activity
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Intent
import android.webkit.WebView
import android.content.Context
import android.content.pm.PackageManager
import android.os.Build
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin
import androidx.work.ExistingPeriodicWorkPolicy
import androidx.work.PeriodicWorkRequestBuilder
import androidx.work.Worker
import androidx.work.WorkerParameters
import androidx.work.WorkManager
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.nio.charset.Charset
import java.util.concurrent.TimeUnit
import kotlin.math.abs

private const val EXTRA_ROUTE = "com.vibeastock.reminders.ROUTE"

internal fun validReminderRoute(route: String?): String? {
    if (route == "/mobile/journal/daily" || route == "/mobile/reminders" || route == "/mobile/discovery/calendar") return route
    return route?.takeIf { Regex("^/mobile/stock/(SH|SZ|BJ)[0-9]{6}$").matches(it) }
}

private const val PREFS_NAME = "vibe_astock_reminders"
private const val SNAPSHOT_KEY = "snapshot_json"
private const val UNIQUE_WORK = "vibe_astock_periodic_reminders"
private const val CHANNEL_ID = "vibe_astock_market_reminders"
private const val INTERVAL_MINUTES = 15L

@InvokeArg
class SyncRulesArgs {
    lateinit var snapshotJson: String
}

private data class QuoteSnapshot(val price: Double, val quoteDate: String)

@TauriPlugin
class RemindersPlugin(private val activity: Activity) : Plugin(activity) {
    private var webView: WebView? = null
    private var pendingRoute: String? = null

    override fun load(webView: WebView) {
        this.webView = webView
        captureNavigation(activity.intent)
    }

    override fun onNewIntent(intent: Intent) {
        captureNavigation(intent)
    }

    private fun captureNavigation(intent: Intent?) {
        val host = try { activity.packageManager.getApplicationInfo(activity.packageName, PackageManager.GET_META_DATA).metaData?.getString("com.vibeastock.APP_LINK_HOST") } catch (_: Exception) { null }
        val route = validReminderRoute(intent?.getStringExtra(EXTRA_ROUTE)) ?: externalRoute(intent?.data?.toString(), host) ?: return
        intent?.removeExtra(EXTRA_ROUTE)
        synchronized(this) { pendingRoute = route }
        // The command retains the route until React is mounted. This event only
        // wakes an already running UI and carries no executable/user content.
        webView?.post {
            webView?.evaluateJavascript(
                "window.dispatchEvent(new Event('vibe-reminder-open'))",
                null,
            )
        }
    }

    @Command
    fun takeNavigation(invoke: Invoke) {
        val route = synchronized(this) {
            val next = pendingRoute
            pendingRoute = null
            next
        }
        invoke.resolve(JSObject().apply { put("route", route ?: JSONObject.NULL) })
    }

    @Command
    fun monitor(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SyncRulesArgs::class.java)
            val config = JSONObject(args.snapshotJson)
            val prefs = activity.getSharedPreferences(MONITOR_PREFS, Context.MODE_PRIVATE)
            when (config.optString("action", "status")) {
                "start" -> {
                    if (Build.VERSION.SDK_INT >= 33 && activity.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) != PackageManager.PERMISSION_GRANTED) { invoke.reject("请先在设置里允许通知，再开启后台盯盘"); return }
                    if ((config.optJSONArray("symbols")?.length() ?: 0) !in 1..10000) { invoke.reject("订阅列表为空或过大"); return }
                    activity.stopService(Intent(activity, MarketMonitorService::class.java))
                    prefs.edit().putString("config", config.toString()).apply()
                    val intent = Intent(activity, MarketMonitorService::class.java)
                    if (Build.VERSION.SDK_INT >= 26) activity.startForegroundService(intent) else activity.startService(intent)
                }
                "stop" -> { activity.stopService(Intent(activity, MarketMonitorService::class.java)); prefs.edit().putBoolean("running", false).putString("message", "已手动停止").apply() }
                "status" -> {}
                else -> { invoke.reject("后台盯盘操作无效"); return }
            }
            invoke.resolve(JSObject().apply {
                put("supported", true); put("running", prefs.getBoolean("running", false)); put("bytes", prefs.getLong("bytes", 0)); put("last_sample", prefs.getLong("last_sample", 0)); put("message", prefs.getString("message", "尚未开启")); put("alerts", org.json.JSONArray(prefs.getString("alerts", "[]")))
            })
        } catch (e: Exception) { invoke.reject("后台盯盘操作失败：${e.javaClass.simpleName}") }
    }

    @Command
    fun syncRules(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(SyncRulesArgs::class.java)
            val root = JSONObject(args.snapshotJson)
            val count = root.optJSONArray("rules")?.length() ?: 0
            activity.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .edit()
                .putString(SNAPSHOT_KEY, args.snapshotJson)
                .apply()

            val manager = WorkManager.getInstance(activity.applicationContext)
            if (count == 0) {
                manager.cancelUniqueWork(UNIQUE_WORK)
            } else {
                val request = PeriodicWorkRequestBuilder<ReminderWorker>(INTERVAL_MINUTES, TimeUnit.MINUTES)
                    .build()
                manager.enqueueUniquePeriodicWork(
                    UNIQUE_WORK,
                    ExistingPeriodicWorkPolicy.UPDATE,
                    request,
                )
            }
            invoke.resolve(statusPayload(count > 0, count))
        } catch (error: Exception) {
            invoke.reject("同步后台提醒失败: ${error.message ?: error.javaClass.simpleName}")
        }
    }

    @Command
    fun status(invoke: Invoke) {
        try {
            val snapshot = activity.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
                .getString(SNAPSHOT_KEY, null)
            val count = snapshot
                ?.let { JSONObject(it).optJSONArray("rules")?.length() }
                ?: 0
            invoke.resolve(statusPayload(count > 0, count))
        } catch (error: Exception) {
            invoke.reject("读取后台提醒状态失败: ${error.message ?: error.javaClass.simpleName}")
        }
    }

    private fun statusPayload(scheduled: Boolean, ruleCount: Int): JSObject {
        return JSObject().apply {
            put("scheduled", scheduled)
            put("rule_count", ruleCount)
            put("interval_minutes", INTERVAL_MINUTES)
        }
    }
}

class ReminderWorker(
    appContext: Context,
    workerParams: WorkerParameters,
) : Worker(appContext, workerParams) {
    override fun doWork(): Result {
        val prefs = applicationContext.getSharedPreferences(PREFS_NAME, Context.MODE_PRIVATE)
        val snapshot = prefs.getString(SNAPSHOT_KEY, null) ?: return Result.success()
        val rules = try {
            JSONObject(snapshot).optJSONArray("rules") ?: return Result.success()
        } catch (_: Exception) {
            return Result.success()
        }

        if (!canPostNotifications()) {
            return Result.success()
        }
        ensureNotificationChannel()

        val quoteCache = mutableMapOf<String, QuoteSnapshot?>()
        val now = System.currentTimeMillis()
        val marketNow = marketClock(now)
        val marketDate = marketNow.dayKey
        val marketDateIso = marketNow.date

        for (index in 0 until rules.length()) {
            val rule = rules.optJSONObject(index) ?: continue
            val id = rule.optLong("id", -1L)
            if (id <= 0L || !rule.optBoolean("enabled", true)) continue

            val kind = rule.optString("kind")
            if (kind == "calendar_event") {
                val eventDate = rule.optString("date")
                val hour = rule.optInt("review_hour", 9).coerceIn(0, 23)
                val key = "calendar_event_date_$id"
                if (eventDate == marketDateIso && marketNow.hour >= hour && prefs.getString(key, "") != marketDateIso) {
                    postNotification(id, rule.optString("name", "日历事件"), rule.optString("detail").ifBlank { "今天有待关注的日历事件。" }, rule.optString("route", "/mobile/discovery/calendar"))
                    prefs.edit().putString(key, marketDateIso).apply()
                }
                continue
            }
            if (kind == "daily_review") {
                val reviewHour = rule.optInt("review_hour", 16).coerceIn(0, 23)
                val lastDateKey = "review_date_$id"
                val lastDate = prefs.getString(lastDateKey, "")
                if (marketNow.hour >= reviewHour && lastDate != marketDateIso) {
                    val title = rule.optString("name").ifBlank { "收盘复盘提醒" }
                    postNotification(
                        id,
                        title,
                        "A股收盘复盘时间到了，记得填写今日观察和明日验证。",
                        rule.optString("route", "/mobile/reminders"),
                    )
                    prefs.edit().putString(lastDateKey, marketDateIso).apply()
                }
                continue
            }

            val quoteKey = rule.optString("quote_key")
            val threshold = rule.optDouble("threshold", Double.NaN)
            if (quoteKey.isBlank() || !threshold.isFinite() || threshold <= 0.0) continue

            val quote = if (quoteCache.containsKey(quoteKey)) {
                quoteCache[quoteKey]
            } else {
                fetchTencentQuote(quoteKey).also { quoteCache[quoteKey] = it }
            } ?: continue

            // Avoid triggering from previous-trading-day snapshots on weekends,
            // holidays, pre-market startup, or long suspensions.
            if (quote.quoteDate != marketDate) continue

            val tolerance = rule.optDouble("tolerance_pct", 1.0).coerceIn(0.05, 20.0)
            val condition = when (kind) {
                "price_above" -> quote.price >= threshold
                "price_below" -> quote.price <= threshold
                "stop_near", "target_near" -> abs(quote.price - threshold) / threshold * 100.0 <= tolerance
                else -> false
            }
            val conditionKey = "condition_$id"
            val previousCondition = prefs.getBoolean(conditionKey, false)
            prefs.edit().putBoolean(conditionKey, condition).apply()
            if (!condition || previousCondition) continue

            val cooldownMinutes = rule.optLong("cooldown_minutes", 60L).coerceIn(15L, 10_080L)
            val lastTriggerKey = "last_trigger_$id"
            val lastTrigger = prefs.getLong(lastTriggerKey, 0L)
            if (lastTrigger > 0L && now - lastTrigger < cooldownMinutes * 60_000L) continue

            val symbol = rule.optString("symbol")
            val name = rule.optString("name").ifBlank { symbol.ifBlank { "股票提醒" } }
            val message = when (kind) {
                "price_above" -> "$symbol 当前 ${formatPrice(quote.price)}，已达到上穿 ${formatPrice(threshold)}。"
                "price_below" -> "$symbol 当前 ${formatPrice(quote.price)}，已跌到 ${formatPrice(threshold)} 以下。"
                "stop_near" -> "$symbol 当前 ${formatPrice(quote.price)}，距离止损价 ${formatPrice(threshold)} 已进入 ${formatPercent(tolerance)}% 范围。"
                "target_near" -> "$symbol 当前 ${formatPrice(quote.price)}，距离目标价 ${formatPrice(threshold)} 已进入 ${formatPercent(tolerance)}% 范围。"
                else -> continue
            }
            postNotification(id, name, message, rule.optString("route", "/mobile/reminders"))
            prefs.edit().putLong(lastTriggerKey, now).apply()
        }

        return Result.success()
    }

    private fun fetchTencentQuote(quoteKey: String): QuoteSnapshot? {
        var connection: HttpURLConnection? = null
        return try {
            connection = URL("https://qt.gtimg.cn/q=$quoteKey").openConnection() as HttpURLConnection
            connection.connectTimeout = 8_000
            connection.readTimeout = 8_000
            connection.requestMethod = "GET"
            connection.setRequestProperty("Referer", "https://gu.qq.com/")
            connection.setRequestProperty("User-Agent", "Mozilla/5.0 VibeAStock-Android")
            val bytes = connection.inputStream.use { it.readBytes() }
            val text = String(bytes, Charset.forName("GBK"))
            val payload = text.substringAfter('"', "").substringBeforeLast('"', "")
            val fields = payload.split('~')
            val price = fields.getOrNull(3)?.toDoubleOrNull() ?: return null
            val timestamp = fields.getOrNull(30).orEmpty().filter(Char::isDigit)
            val quoteDate = timestamp.take(8)
            if (!price.isFinite() || price <= 0.0 || quoteDate.length != 8) return null
            QuoteSnapshot(price, quoteDate)
        } catch (_: Exception) {
            null
        } finally {
            connection?.disconnect()
        }
    }

    private fun canPostNotifications(): Boolean {
        return Build.VERSION.SDK_INT < Build.VERSION_CODES.TIRAMISU ||
            applicationContext.checkSelfPermission(Manifest.permission.POST_NOTIFICATIONS) == PackageManager.PERMISSION_GRANTED
    }

    private fun ensureNotificationChannel() {
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.O) return
        val manager = applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val channel = NotificationChannel(
            CHANNEL_ID,
            "行情与复盘提醒",
            NotificationManager.IMPORTANCE_DEFAULT,
        ).apply {
            description = "价格、止损、目标价和收盘复盘提醒"
        }
        manager.createNotificationChannel(channel)
    }

    private fun postNotification(ruleId: Long, title: String, message: String, route: String) {
        val manager = applicationContext.getSystemService(Context.NOTIFICATION_SERVICE) as NotificationManager
        val launchIntent = applicationContext.packageManager
            .getLaunchIntentForPackage(applicationContext.packageName)
        val pendingIntent = launchIntent?.let {
            it.data = Uri.parse("vibeastock://reminder/$ruleId")
            it.putExtra(EXTRA_ROUTE, validReminderRoute(route) ?: "/mobile/reminders")
            it.addFlags(Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP)
            PendingIntent.getActivity(
                applicationContext,
                ruleId.hashCode(),
                it,
                PendingIntent.FLAG_UPDATE_CURRENT or PendingIntent.FLAG_IMMUTABLE,
            )
        }
        val icon = applicationContext.applicationInfo.icon.takeIf { it != 0 }
            ?: android.R.drawable.ic_dialog_info
        val builder = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            Notification.Builder(applicationContext, CHANNEL_ID)
        } else {
            @Suppress("DEPRECATION")
            Notification.Builder(applicationContext)
        }
        builder
            .setSmallIcon(icon)
            .setContentTitle(title)
            .setContentText(message)
            .setStyle(Notification.BigTextStyle().bigText(message))
            .setAutoCancel(true)
        if (pendingIntent != null) builder.setContentIntent(pendingIntent)
        manager.notify(notificationId(ruleId), builder.build())
    }

    private fun notificationId(ruleId: Long): Int {
        return (ruleId xor (ruleId ushr 32)).toInt().let { if (it == 0) 1 else it }
    }

    private fun formatPrice(value: Double): String = String.format("%.2f", value)
    private fun formatPercent(value: Double): String = String.format("%.2f", value)
}
