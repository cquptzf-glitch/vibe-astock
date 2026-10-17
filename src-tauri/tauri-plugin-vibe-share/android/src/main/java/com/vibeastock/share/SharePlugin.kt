package com.vibeastock.share

import android.app.Activity
import android.content.ClipData
import android.content.Intent
import android.util.Base64
import androidx.core.content.FileProvider
import java.io.File
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

private const val MAX_SHARE_TEXT_CHARS = 100_000
private const val MAX_TITLE_CHARS = 300
private const val MAX_FILE_BYTES = 8 * 1024 * 1024
private const val SHARE_DIR = "vibe-share"
private const val STALE_MS = 24L * 60 * 60 * 1000
private val ALLOWED_MIME = setOf("image/png", "image/jpeg", "text/markdown", "text/plain", "application/json")

@InvokeArg
class ShareTextArgs {
    var title: String? = null
    lateinit var text: String
    var chooserTitle: String? = null
}

@InvokeArg
class ShareFileArgs {
    var title: String? = null
    lateinit var name: String
    lateinit var mime: String
    lateinit var dataBase64: String
    var chooserTitle: String? = null
}

@TauriPlugin
class SharePlugin(private val activity: Activity) : Plugin(activity) {
    @Command
    fun shareText(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(ShareTextArgs::class.java)
            require(args.text.isNotBlank()) { "share text is empty" }
            require(args.text.length <= MAX_SHARE_TEXT_CHARS) { "share text is too large" }
            require((args.title?.length ?: 0) <= MAX_TITLE_CHARS) { "share title is too large" }

            activity.runOnUiThread {
                try {
                    val sendIntent = Intent(Intent.ACTION_SEND).apply {
                        type = "text/plain"
                        putExtra(Intent.EXTRA_TEXT, args.text)
                        args.title?.trim()?.takeIf { it.isNotEmpty() }?.let { title ->
                            putExtra(Intent.EXTRA_TITLE, title)
                            putExtra(Intent.EXTRA_SUBJECT, title)
                        }
                    }
                    val chooser = Intent.createChooser(
                        sendIntent,
                        args.chooserTitle?.trim()?.takeIf { it.isNotEmpty() } ?: "分享到"
                    )
                    activity.startActivity(chooser)
                    invoke.resolve(JSObject().apply { put("ok", true) })
                } catch (error: Exception) {
                    invoke.reject(error.message ?: "failed to open Android Sharesheet")
                }
            }
        } catch (error: Exception) {
            invoke.reject(error.message ?: "invalid share request")
        }
    }

    /** 分享一个文件(图片分享卡、报告 .md):写到应用缓存目录,用 FileProvider 给系统分享面板一个只读 Uri。旧的缓存文件超过一天会顺手清掉。 */
    @Command
    fun shareFile(invoke: Invoke) {
        try {
            val args = invoke.parseArgs(ShareFileArgs::class.java)
            require(args.mime in ALLOWED_MIME) { "unsupported share mime type" }
            require(args.dataBase64.isNotBlank()) { "share file is empty" }
            require((args.title?.length ?: 0) <= MAX_TITLE_CHARS) { "share title is too large" }
            val safeName = args.name.replace(Regex("[^A-Za-z0-9._-]"), "_").take(80).trim('.', '_').ifEmpty { "share" }
            val bytes = Base64.decode(args.dataBase64, Base64.DEFAULT)
            require(bytes.isNotEmpty() && bytes.size <= MAX_FILE_BYTES) { "share file size is out of range" }

            val dir = File(activity.cacheDir, SHARE_DIR)
            dir.mkdirs()
            val now = System.currentTimeMillis()
            dir.listFiles()?.filter { now - it.lastModified() > STALE_MS }?.forEach { it.delete() }
            val file = File(dir, safeName)
            file.writeBytes(bytes)
            val uri = FileProvider.getUriForFile(activity, "${activity.packageName}.vibe.fileprovider", file)

            activity.runOnUiThread {
                try {
                    val sendIntent = Intent(Intent.ACTION_SEND).apply {
                        type = args.mime
                        putExtra(Intent.EXTRA_STREAM, uri)
                        clipData = ClipData.newRawUri(safeName, uri)
                        addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION)
                        args.title?.trim()?.takeIf { it.isNotEmpty() }?.let { title ->
                            putExtra(Intent.EXTRA_TITLE, title)
                            putExtra(Intent.EXTRA_SUBJECT, title)
                        }
                    }
                    val chooser = Intent.createChooser(
                        sendIntent,
                        args.chooserTitle?.trim()?.takeIf { it.isNotEmpty() } ?: "分享到"
                    )
                    activity.startActivity(chooser)
                    invoke.resolve(JSObject().apply { put("ok", true) })
                } catch (error: Exception) {
                    invoke.reject(error.message ?: "failed to open Android Sharesheet")
                }
            }
        } catch (error: Exception) {
            invoke.reject(error.message ?: "invalid share file request")
        }
    }
}
