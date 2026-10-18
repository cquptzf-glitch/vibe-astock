package com.vibeastock.reminders

import android.app.*
import android.content.*
import android.os.*
import org.json.JSONArray
import org.json.JSONObject
import java.net.HttpURLConnection
import java.net.URL
import java.nio.charset.Charset
import java.util.concurrent.Executors
import java.util.concurrent.TimeUnit
import kotlin.math.abs

internal const val MONITOR_PREFS = "vibe_market_monitor"
internal const val MONITOR_CHANNEL = "vibe_market_monitor"
private const val MONITOR_ID = 74100

internal data class MonitorPoint(val time: Long, val price: Double)
internal fun surgePercent(points: List<MonitorPoint>, point: MonitorPoint): Double? {
    val base = points.firstOrNull { point.time - it.time in 60_000L..180_000L } ?: return null
    return if (base.price > 0 && point.price > 0) (point.price / base.price - 1) * 100 else null
}

/** Explicitly enabled foreground service; never starts itself after a reboot/kill. */
class MarketMonitorService : Service() {
    private val executor = Executors.newSingleThreadScheduledExecutor()
    private val frames = mutableMapOf<String, MutableList<MonitorPoint>>()
    private val cooldown = mutableMapOf<String, Long>()
    private var config = JSONObject()
    private var bytesUsed = 0L
    @Volatile private var stopped = false
    private var started = false
    override fun onBind(intent: Intent?) = null

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (intent?.action == "stop") { stopWithReason("已手动停止"); return START_NOT_STICKY }
        if (started) return START_NOT_STICKY
        config = JSONObject(getSharedPreferences(MONITOR_PREFS, MODE_PRIVATE).getString("config", "{}") ?: "{}")
        if (config.optJSONArray("symbols")?.length() == 0) { stopSelf(); return START_NOT_STICKY }
        started = true
        val manager = getSystemService(NOTIFICATION_SERVICE) as NotificationManager
        if (Build.VERSION.SDK_INT >= 26) manager.createNotificationChannel(NotificationChannel(MONITOR_CHANNEL, "持续后台盯盘", NotificationManager.IMPORTANCE_LOW))
        startForeground(MONITOR_ID, notification("后台盯盘已开启", "仅交易时段采样，可随时停止。"))
        val prefs = getSharedPreferences(MONITOR_PREFS, MODE_PRIVATE)
        prefs.edit().putBoolean("running", true).putString("message", "已开启").putLong("bytes", 0).apply()
        val interval = config.optLong("interval_seconds", 60).coerceIn(30, 300)
        executor.scheduleWithFixedDelay({ if (!stopped) sample() }, 0, interval, TimeUnit.SECONDS)
        return START_NOT_STICKY
    }

    private fun sample() {
        val prefs = getSharedPreferences(MONITOR_PREFS, MODE_PRIVATE)
        try {
            val now = marketClock()
            val hhmm = now.hour * 100 + now.minute
            prefs.edit().putLong("heartbeat", System.currentTimeMillis()).apply()
            val battery = registerReceiver(null, IntentFilter(Intent.ACTION_BATTERY_CHANGED))
            val level = battery?.getIntExtra(BatteryManager.EXTRA_LEVEL, -1) ?: -1
            val scale = battery?.getIntExtra(BatteryManager.EXTRA_SCALE, 100) ?: 100
            val charging = battery?.getIntExtra(BatteryManager.EXTRA_PLUGGED, 0) != 0
            if (!charging && level >= 0 && scale > 0 && level * 100 / scale <= config.optInt("stop_battery_pct", 15)) { stopWithReason("电量达到停止线"); return }
            if (now.weekend || !(hhmm in 930..1130 || hhmm in 1300..1500)) {
                frames.clear()
                prefs.edit().putString("message", "非连续竞价时段，等待下一场").apply()
                return
            }
            val symbols = config.optJSONArray("symbols") ?: return
            val codes = (0 until symbols.length()).mapNotNull { symbols.optString(it).takeIf { s -> Regex("^[0-9]{6}$").matches(s) } }
            val newAlerts = mutableListOf<JSONObject>()
            for (batch in codes.chunked(60)) {
                if (stopped) return
                if (bytesUsed >= config.optLong("max_bytes_mb", 50) * 1024 * 1024) { stopWithReason("已达到本次流量上限"); return }
                val quoteKeys = batch.mapNotNull { code ->
                    when {
                        listOf("600", "601", "603", "605", "688", "689").any { code.startsWith(it) } -> "sh$code"
                        listOf("000", "001", "002", "003", "300", "301", "302").any { code.startsWith(it) } -> "sz$code"
                        listOf("920", "43", "83", "87").any { code.startsWith(it) } -> "bj$code"
                        else -> null
                    }
                }
                val connection = URL("https://qt.gtimg.cn/q=${quoteKeys.joinToString(",")}").openConnection() as HttpURLConnection
                try {
                    connection.connectTimeout = 8_000; connection.readTimeout = 8_000
                    connection.setRequestProperty("Referer", "https://gu.qq.com/")
                    val bytes = connection.inputStream.use { input ->
                        val output = java.io.ByteArrayOutputStream()
                        val chunk = ByteArray(8192)
                        while (output.size() < 512 * 1024) { val count = input.read(chunk); if (count < 0) break; output.write(chunk, 0, count) }
                        output.toByteArray()
                    }
                    bytesUsed += bytes.size
                    val raw = String(bytes, Charset.forName("GBK"))
                    for (line in raw.split(';')) {
                        val fields = line.substringAfter('"', "").substringBeforeLast('"', "").split('~')
                        val code = fields.getOrNull(2).orEmpty()
                        if (code !in batch) continue
                        val price = fields.getOrNull(3)?.toDoubleOrNull()?.takeIf { it.isFinite() && it > 0 } ?: continue
                        val stamp = fields.getOrNull(30).orEmpty()
                        val time = quoteMillis(stamp) ?: continue
                        if (stamp.take(8) != now.dayKey || now.millis-time !in 0L..180_000L) continue
                        val point = MonitorPoint(time, price)
                        val history = frames.getOrPut(code) { mutableListOf() }
                        if (history.lastOrNull()?.time?.let { it >= point.time } == true) continue
                        history.removeAll { point.time - it.time > 180_000 }
                        val delta = surgePercent(history, point)
                        history.add(point)
                        if (delta == null || abs(delta) < config.optDouble("threshold_pct", 1.5)) continue
                        val key = "$code:${delta > 0}"
                        val millis = System.currentTimeMillis()
                        if (millis - (cooldown[key] ?: 0) < 300_000) continue
                        cooldown[key] = millis
                        newAlerts.add(JSONObject().put("code", code).put("name", fields.getOrNull(1).orEmpty()).put("price", price).put("change_pct", delta).put("time", stamp))
                    }
                } finally { connection.disconnect() }
            }
            if (newAlerts.isNotEmpty()) {
                val old = JSONArray(prefs.getString("alerts", "[]") ?: "[]")
                val merged = JSONArray()
                newAlerts.reversed().take(200).forEach { merged.put(it) }
                for (i in 0 until minOf(old.length(), 200 - merged.length())) merged.put(old.get(i))
                prefs.edit().putString("alerts", merged.toString()).apply()
                val text = newAlerts.take(3).joinToString("；") { "${it.optString("name")} ${String.format(java.util.Locale.ROOT, "%+.2f%%", it.optDouble("change_pct"))}" }
                (getSystemService(NOTIFICATION_SERVICE) as NotificationManager).notify(MONITOR_ID + 1, notification("${newAlerts.size} 只股票发生分钟价格变化", text, false))
            }
            prefs.edit().putLong("bytes", bytesUsed).putLong("last_sample", System.currentTimeMillis()).putString("message", "已采样 ${codes.size} 只；累计 ${bytesUsed / 1024} KiB").apply()
            (getSystemService(NOTIFICATION_SERVICE) as NotificationManager).notify(MONITOR_ID, notification("后台盯盘运行中", "${codes.size} 只 · ${bytesUsed / 1024} KiB · ${now.clock}"))
        } catch (e: Exception) { prefs.edit().putString("message", "取数失败：${e.javaClass.simpleName}，稍后重试").apply() }
    }

    private fun notification(title: String, text: String, ongoing: Boolean = true): Notification {
        val stop = Intent(this, MarketMonitorService::class.java).setAction("stop")
        val pendingStop = PendingIntent.getService(this, 0, stop, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT)
        val launch = packageManager.getLaunchIntentForPackage(packageName)
        val pendingLaunch = launch?.let { PendingIntent.getActivity(this, 1, it, PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT) }
        val builder = if (Build.VERSION.SDK_INT >= 26) Notification.Builder(this, MONITOR_CHANNEL) else @Suppress("DEPRECATION") Notification.Builder(this)
        builder.setSmallIcon(applicationInfo.icon.takeIf { it != 0 } ?: android.R.drawable.ic_dialog_info).setContentTitle(title).setContentText(text).setOngoing(ongoing)
        if (pendingLaunch != null) builder.setContentIntent(pendingLaunch)
        if (ongoing) builder.addAction(Notification.Action.Builder(null, "停止盯盘", pendingStop).build())
        return builder.build()
    }
    private fun stopWithReason(reason: String) {
        stopped = true
        getSharedPreferences(MONITOR_PREFS, MODE_PRIVATE).edit().putBoolean("running", false).putString("message", reason).putLong("bytes", bytesUsed).apply()
        stopForeground(STOP_FOREGROUND_REMOVE); stopSelf()
    }
    override fun onTimeout(startId: Int, fgsType: Int) { stopWithReason("系统限制了本次前台数据服务时长") }
    override fun onDestroy() { stopped = true; executor.shutdownNow(); getSharedPreferences(MONITOR_PREFS, MODE_PRIVATE).edit().putBoolean("running", false).apply(); super.onDestroy() }
}
