package com.ccmonitor.mobile.ui.common

import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.platform.LocalClipboard
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.ui.copyPlainText
import kotlinx.coroutines.launch

/**
 * 失败处的「复制详情」（同桌面失败应答的那一格）：屏上只有表里那一句 [text]，下层的原话 [detail] 不上屏，
 * 点这颗才把「句子 ＋ 原话」放进剪贴板。没有原话 ⇒ 不画。
 */
@Composable
fun CopyDetailButton(
    text: String,
    detail: String?,
) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    if (detail.isNullOrBlank()) return
    SecondaryButton(onClick = { scope.launch { clipboard.copyPlainText(text + "\n" + detail) } }) { Text(copyText("detail.act.copy")) }
}
