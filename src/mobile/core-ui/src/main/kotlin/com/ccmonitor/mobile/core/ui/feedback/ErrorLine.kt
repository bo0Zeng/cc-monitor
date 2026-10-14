package com.ccmonitor.mobile.core.ui.feedback

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.width
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens

/**
 * 整句出错（规范 V3）：字用正文色，红只给左边一道 3 dp 的 `--error` 条 —— 红字在 `--card` 上只有 3.4:1 读不清。
 * 短的红字（危险按钮 · stderr · 出错 chip）另用 `LocalAppTokens.current.errorText`，不走这里。
 */
@Composable
fun ErrorLine(
    text: String,
    modifier: Modifier = Modifier,
    style: TextStyle = MaterialTheme.typography.bodyMedium,
) {
    val tokens = LocalAppTokens.current
    Row(modifier.height(IntrinsicSize.Min)) {
        Spacer(Modifier.width(3.dp).fillMaxHeight().background(MaterialTheme.colorScheme.error))
        Spacer(Modifier.width(tokens.spacing.sm))
        Text(text, style = style, color = MaterialTheme.colorScheme.onSurface)
    }
}
