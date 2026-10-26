package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.gen.LiveCard
import com.ccmonitor.mobile.core.claude.link.wire.gen.LivePhase

/** 活卡里的一块：[kind] 是那台的块种（`text` · `thinking` · `tool` · `other`）；[text] 只有文字块有；[tool] 只有工具块有。 */
data class LiveBlock(
    val kind: String,
    val text: String,
    val tool: String?,
)

/** 一张活卡此刻的样子（给屏读）。 */
data class LiveCardState(
    val card: Long,
    val rid: String,
    val awaiting: Boolean,
    val clipped: Boolean,
    val blocks: List<LiveBlock>,
)

/**
 * 纯状态：照那台的帧换卡、长卡、撤卡（同桌面 `live-card.ts` 的 `LiveBoard`）。唯一一条规矩：一张卡的 `n` 不接上一帧 ⇒ 先不画它，
 * 等那台下一张整卡（`open`）或撤卡（`close`）—— 那台在发件箱满过时自己补整卡，这边从不自己补缺口。
 */
class LiveBoard {
    private class Card(
        val sid: String,
        val run: String?,
        var next: Long,
        var state: LiveCardState,
        val born: Long,
    )

    private val cards = LinkedHashMap<Long, Card>()
    private var born = 0L

    /**
     * 流上的一帧 `live_card`（[LiveCard]：后端 `stream/live.rs` 出的成品，某个会话某个运行里正在说的那一次上游应答，一张一张地开、长、撤）。
     * 对账 · 缺口 · 截头 · 挤谁全在后端，这里只认几种 `op`。卡变了 ⇒ `true`。
     */
    fun apply(f: LiveCard): Boolean {
        if (f is LiveCard.Open) {
            val was = cards[f.card]
            val blocks = f.blocks.map { LiveBlock(it.kind.wire, it.text.orEmpty(), it.tool) }
            val state = LiveCardState(f.card, f.rid, f.phase == LivePhase.AWAITING, f.clipped, blocks)
            cards[f.card] = Card(f.sid, f.run, f.n + 1, state, was?.born ?: born++)
            return true
        }
        val c = cards[f.card] ?: return false
        if (f.n != c.next || f is LiveCard.Close) {
            cards.remove(f.card)
            return true
        }
        c.next += 1
        val s = c.state
        c.state =
            when (f) {
                is LiveCard.Block -> {
                    val i = f.i.toInt()
                    val blocks = s.blocks.toMutableList()
                    while (blocks.size <= i) blocks += LiveBlock(OTHER, "", null)
                    blocks[i] = LiveBlock(f.block.wire, "", f.tool)
                    s.copy(blocks = blocks)
                }
                is LiveCard.Text -> {
                    val i = f.i.toInt()
                    s.copy(blocks = s.blocks.mapIndexed { j, b -> if (j == i) b.copy(text = b.text + f.s) else b })
                }
                is LiveCard.Awaiting -> s.copy(awaiting = true)
                is LiveCard.Open, is LiveCard.Close -> s
            }
        return true
    }

    /** 一个会话里某个运行此刻的活卡（先来的在前；[run] 缺 ＝ 主运行）。 */
    fun cardsOf(
        sid: String,
        run: String? = null,
    ): List<LiveCardState> =
        cards.values
            .filter { it.sid == sid && it.run == run }
            .sortedBy { it.born }
            .map { it.state }

    /** 这个会话的卡不留了。变了 ⇒ `true`。 */
    fun drop(sid: String): Boolean = cards.values.removeAll { it.sid == sid }

    /** 换了一条流（这条连接上的号作废，那台会给名单里的会话补整卡）⇒ 全撤。变了 ⇒ `true`。 */
    fun clear(): Boolean = cards.isNotEmpty().also { cards.clear() }

    val size: Int get() = cards.size

    private companion object {
        const val OTHER = "other"
    }
}
