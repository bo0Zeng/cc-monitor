package com.ccmonitor.mobile.ui

import android.content.ClipData
import androidx.compose.ui.platform.ClipEntry
import androidx.compose.ui.platform.Clipboard

/*
 * Compose Clipboard（suspend API）的纯文本封装，各处复制/粘贴共用。
 */

/** 复制纯文本到系统剪贴板。 */
suspend fun Clipboard.copyPlainText(text: String) {
    setClipEntry(ClipEntry(ClipData.newPlainText("aterm", text)))
}

/** 读系统剪贴板首条纯文本（无则 null）。 */
suspend fun Clipboard.pastePlainText(): String? =
    getClipEntry()
        ?.clipData
        ?.takeIf { it.itemCount > 0 }
        ?.getItemAt(0)
        ?.text
        ?.toString()
