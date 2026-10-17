package com.vibeastock.share

import androidx.core.content.FileProvider

/**
 * 分享文件用的 FileProvider。必须是自己的子类:Tauri 生成的应用清单里已经有一个 androidx.core.content.FileProvider,
 * 同名的两个 <provider> 会让清单合并报重复元素、整个 APK 构建失败。
 */
class VibeFileProvider : FileProvider()
