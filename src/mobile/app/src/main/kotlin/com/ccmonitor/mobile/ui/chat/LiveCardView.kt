package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.wire.gen.LiveCardNow
import com.ccmonitor.mobile.ui.claude.MdBlocksView

/**
 * 正文尾巴上的活卡：那台 `live_card` 帧经端核心的活卡板折好、头一行与正文都写好的那一张照画（桌面同一份）。
 * 不进时间线；正文照那台解析好的树逐块排（前面没变的块不重画），代码不上色 —— 那一轮的记录到了，那台撤卡，定稿那一张走正文渲染。
 */
internal fun LazyListScope.liveCards(cards: List<LiveCardNow>) {
    items(cards, key = { "live:" + it.card }) { LiveCardItem(it) }
}

/** 一张活卡：头一行（生成中 · 已生成等写入记录）照核心写好的摆，正文照树。 */
@Composable
private fun LiveCardItem(card: LiveCardNow) {
    val tokens = LocalAppTokens.current
    Column(Modifier.fillMaxWidth().testTag(ChatTestTags.LIVE).padding(vertical = tokens.spacing.xs)) {
        Text(card.head, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (card.body.isNotEmpty()) MdBlocksView(card.body, Modifier.padding(top = tokens.spacing.xs))
    }
}
