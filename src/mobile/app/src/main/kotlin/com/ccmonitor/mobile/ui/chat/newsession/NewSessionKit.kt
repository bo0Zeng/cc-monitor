package com.ccmonitor.mobile.ui.chat.newsession

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoBody

/*
 * 新建会话那几屏共用的小件：chip · 抽屉头 · 抽屉里的一行 · 号牌 · 状态点。chip 画出来 36 dp 高、命中区 48；抽屉一行 52。
 * 展开 · 下一层用 Phosphor 的 caret；勾选那一颗仍是字（不在图标表里）。
 */

internal const val CHECK = "✓"

/** 顶栏下那一排里的一颗：点开底部抽屉。画出来 36 dp 高，命中区照 M3 的 48（`Surface(onClick)` 自带）。 */
@Composable
internal fun Chip(
    onClick: () -> Unit,
    tag: String,
    modifier: Modifier = Modifier,
    content: @Composable RowScope.() -> Unit,
) {
    Surface(
        onClick = onClick,
        shape = RoundedCornerShape(6.dp),
        color = MaterialTheme.colorScheme.surfaceContainer,
        border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline),
        modifier = modifier.heightIn(min = 36.dp).testTag(tag),
    ) {
        Row(
            Modifier.padding(horizontal = 10.dp, vertical = 6.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(6.dp),
        ) {
            content()
            PhIcon(Ph.CaretDown, contentDescription = null, size = Touch.iconInline, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/** 抽屉头：标题（16 · 600）＋ 右边机器名（灰）。 */
@Composable
internal fun SheetHead(
    title: String,
    machine: String?,
) {
    Row(Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
        Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold, modifier = Modifier.weight(1f))
        machine?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

/** 抽屉里一节的小标题（「最近的」「轮换」）。 */
@Composable
internal fun SheetSection(title: String) {
    Text(
        title,
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 12.dp, bottom = 4.dp),
    )
}

/**
 * 抽屉里的一行：行首 · 一二行字 · 行尾。选中 ⇒ 淡底 ＋ 勾；[enabled] 为假 ⇒ 灰（照样能长按、行尾的按钮照样能点）。
 * 52 dp 起（两行 64）。
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
internal fun PickRow(
    title: String,
    tag: String,
    modifier: Modifier = Modifier,
    second: String? = null,
    selected: Boolean = false,
    enabled: Boolean = true,
    mono: Boolean = false,
    onClick: () -> Unit = {},
    onLongClick: (() -> Unit)? = null,
    lead: (@Composable () -> Unit)? = null,
    trail: (@Composable RowScope.() -> Unit)? = null,
) {
    val tokens = LocalAppTokens.current
    val bg = if (selected) tokens.selectedBg else Color.Transparent
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = if (second == null) 52.dp else 64.dp)
            .background(bg)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(horizontal = 20.dp, vertical = 8.dp)
            .testTag(tag),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        val dim = Modifier.alpha(if (enabled) 1f else DIM)
        lead?.let { Box(dim) { it() } }
        Column(dim.weight(1f)) {
            Text(
                title,
                style = if (mono) monoBody else MaterialTheme.typography.bodyLarge,
                maxLines = 1,
                overflow = TextOverflow.StartEllipsis,
            )
            second?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1) }
        }
        trail?.invoke(this)
        // 勾用强调色（今天的主题槽里强调色在 `primary`）。
        if (selected) Text(CHECK, color = MaterialTheme.colorScheme.primary, style = MaterialTheme.typography.titleMedium)
    }
}

/** 号牌：号色那一格做底、号牌字照那台写好的（[text]）。那台没给这个号 ⇒ 号色第一格、字写「—」。 */
@Composable
internal fun AccountBadge(
    text: String?,
    color: Int?,
    size: Int = 22,
) {
    val acct = LocalAppTokens.current.acct
    val (bg, ink) = acct[(color ?: 0).mod(acct.size)]
    Box(Modifier.size(size.dp).background(bg, RoundedCornerShape(4.dp)), contentAlignment = Alignment.Center) {
        Text(text ?: copyText("accounts.sessionBadge.none"), color = ink, fontSize = 10.sp, fontWeight = FontWeight.SemiBold, textAlign = TextAlign.Center, maxLines = 1)
    }
}

/** 机器那一颗状态点：通着 ⇒ 实心绿；断着 ⇒ 空心；还没连过 ⇒ 不画（[up] 为 `null`）。 */
@Composable
internal fun MachineDot(up: Boolean?) {
    val tokens = LocalAppTokens.current
    val m = Modifier.size(8.dp)
    when (up) {
        true -> Box(m.background(tokens.success, CircleShape))
        false -> Box(m.border(1.dp, MaterialTheme.colorScheme.onSurfaceVariant, CircleShape))
        null -> Box(m)
    }
}

private const val DIM = 0.45f

/** 路径里那台的家目录缩成 `~`（同桌面 `kit/path.ts::homeShort`）；家目录不知道 ⇒ 原样。 */
internal fun homeShort(
    path: String,
    home: String?,
): String {
    val h = home?.trimEnd('/')?.takeIf { it.isNotEmpty() } ?: return path
    return when {
        path == h -> "~"
        path.startsWith("$h/") -> "~" + path.removePrefix(h)
        else -> path
    }
}
