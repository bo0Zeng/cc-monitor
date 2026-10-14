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
import com.ccmonitor.mobile.core.claude.link.LiveCardState
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens

/**
 * 正文尾巴上的活卡：那台 `live_card` 帧拼好的那一张照画（同桌面 `live-card.ts` 的 `renderCardText`）。
 * 不进时间线、不走 Markdown：那一轮的记录到了，那台撤卡，定稿那一张走正文渲染。
 */
internal fun LazyListScope.liveCards(cards: List<LiveCardState>) {
    items(cards, key = { "live:" + it.card }) { LiveCardItem(it) }
}

/** 一张活卡画成两段纯文字：头一行（生成中 · 已生成等写入记录）与正文（截过头先说一句；思考 · 工具块各一句；文字块原样）。 */
internal fun liveCardText(card: LiveCardState): Pair<String, String> {
    val head = if (card.awaiting) copyText("liveCard.state.awaitingRecord") else copyText("liveCard.state.streaming")
    val parts = mutableListOf<String>()
    if (card.clipped) parts += copyText("liveCard.body.clipped")
    for (b in card.blocks) {
        when (b.kind) {
            TEXT -> parts += b.text
            THINKING -> parts += copyText("liveCard.block.thinking")
            TOOL -> parts += copyText("liveCard.block.toolUse", "tool" to (b.tool ?: UNKNOWN_TOOL))
        }
    }
    return head to parts.filter { it.isNotEmpty() }.joinToString("\n")
}

@Composable
private fun LiveCardItem(card: LiveCardState) {
    val (head, body) = liveCardText(card)
    val tokens = LocalAppTokens.current
    Column(Modifier.fillMaxWidth().testTag(ChatTestTags.LIVE).padding(vertical = tokens.spacing.xs)) {
        Text(head, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        if (body.isNotEmpty()) Text(body, modifier = Modifier.padding(top = tokens.spacing.xs), style = MaterialTheme.typography.bodyLarge)
    }
}

/** 那台的块种（`BlockKind`）：画法只分这三种，其余不画。 */
private const val TEXT = "text"
private const val THINKING = "thinking"
private const val TOOL = "tool"
private const val UNKNOWN_TOOL = "?"
