package com.vibeastock.share

import android.app.Activity
import android.content.Intent
import app.tauri.annotation.Command
import app.tauri.annotation.InvokeArg
import app.tauri.annotation.TauriPlugin
import app.tauri.plugin.Invoke
import app.tauri.plugin.JSObject
import app.tauri.plugin.Plugin

private const val MAX_SHARE_TEXT_CHARS = 100_000
private const val MAX_TITLE_CHARS = 300

@InvokeArg
class ShareTextArgs {
    var title: String? = null
    lateinit var text: String
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
}
