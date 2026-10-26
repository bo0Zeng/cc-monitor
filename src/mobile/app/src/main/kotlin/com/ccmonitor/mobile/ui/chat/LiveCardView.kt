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
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.wire.gen.BlockKind
import com.ccmonitor.mobile.core.wire.gen.LiveCardNow
import com.ccmonitor.mobile.core.wire.gen.LivePhase

/**
 * 正文尾巴上的活卡：那台 `live_card` 帧拼好的那一张照画（同桌面 `live-card.ts` 的 `renderCardText`）。
 * 不进时间线、不走 Markdown：那一轮的记录到了，那台撤卡，定稿那一张走正文渲染。
 */
internal fun LazyListScope.liveCards(cards: List<LiveCardNow>) {
    items(cards, key = { "live:" + it.card }) { LiveCardItem(it) }
}

/** 一张活卡画成两段纯文字：头一行（生成中 · 已生成等写入记录）与正文（截过头先说一句；思考 · 工具块各一句；文字块原样）。 */
internal fun liveCardText(card: LiveCardNow): Pair<String, String> {
    val head = if (card.phase == LivePhase.AWAITING) copyText("liveCard.state.awaitingRecord") else copyText("liveCard.state.streaming")
    val parts = mutableListOf<String>()
    if (card.clipped) parts += copyText("liveCard.body.clipped")
    for (b in card.blocks) {
        when (b.kind) {
            BlockKind.TEXT -> parts += b.text.orEmpty()
            BlockKind.THINKING -> parts += copyText("liveCard.block.thinking")
            BlockKind.TOOL -> parts += copyText("liveCard.block.toolUse", "tool" to (b.tool ?: UNKNOWN_TOOL))
            BlockKind.OTHER -> {}
        }
    }
    return head to parts.filter { it.isNotEmpty() }.joinToString("\n")
}

@Composable
private fun LiveCardItem(card: LiveCardNow) {
    val (head, body) = liveCardText(card)
    val tokens = LocalAppTokens.current
    Column(Modifier.fillMaxWidth().testTag(ChatTestTags.LIVE).padding(vertical = tokens.spacing.xs)) {
        Text(head, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (body.isNotEmpty()) Text(body, modifier = Modifier.padding(top = tokens.spacing.xs), style = MaterialTheme.typography.bodyLarge)
    }
}

/** 工具块没带名字时的那一格。 */
private const val UNKNOWN_TOOL = "?"
