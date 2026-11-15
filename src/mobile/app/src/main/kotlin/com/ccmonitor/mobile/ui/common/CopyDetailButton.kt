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
    CopyBodyButton(copyDetailBody(text, detail))
}

/** ［复制详情］放进剪贴板的那一份：句子 [text] ＋ 换行 ＋ 原话 [detail]；原话缺、空或只有空白 ⇒ `null`，那颗不画。判空全仓只这一处。 */
fun copyDetailBody(
    text: String,
    detail: String?,
): String? = detail?.takeUnless { it.isBlank() }?.let { text + "\n" + it }

/** 同一颗「复制详情」，剪贴板里放的是核心写好的整份正文 [body]（原样，不再拼句子）。`null` ⇒ 不画（空不空由交的那一方定）。 */
@Composable
fun CopyBodyButton(body: String?) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    if (body == null) return
    SecondaryButton(onClick = { scope.launch { clipboard.copyPlainText(body) } }) { Text(copyText("detail.act.copy")) }
}
