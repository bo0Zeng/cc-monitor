package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.LiveCardFrame
import com.ccmonitor.mobile.core.claude.link.wire.LiveOpWord
import com.ccmonitor.mobile.core.claude.link.wire.LivePhaseWord
import com.ccmonitor.mobile.core.claude.link.wire.Wire

/**
 * 流上的 `live_card` 帧（`IPC-COMMANDS.md`「`live_card`」，后端 `stream/live.rs` 出的成品）：某个会话某个运行里正在说的那一次上游应答，
 * 一张一张地开、长、撤。只发 `stream-watch` 名单里的会话。对账 · 缺口 · 截头 · 挤谁全在后端，这里只认几种 `op`。
 */
data class LiveFrame(
    val sid: String,
    /** 归哪个子运行；`null` ＝ 主运行。 */
    val run: String?,
    /** 卡号（那台整台唯一）。 */
    val card: Long,
    /** 这条连接上这张卡的第几帧：`open` 从它起，之后连续。 */
    val n: Long,
    val op: Op,
) {
    sealed interface Op {
        /** 整张卡：照它整张换。 */
        data class Open(
            /** 对账键（与那一轮记录的 `rid` 同值）。 */
            val rid: String,
            /** 上游说完了、那一轮的记录还没到。 */
            val awaiting: Boolean,
            /** 正文截过头（只留了尾巴）。 */
            val clipped: Boolean,
            val blocks: List<LiveBlock>,
        ) : Op

        /** 第 [i] 块开始了。 */
        data class Block(
            val i: Int,
            val kind: String,
            val tool: String?,
        ) : Op

        /** 第 [i] 块（文字块）接着一段字。 */
        data class Text(
            val i: Int,
            val s: String,
        ) : Op

        /** 上游说完了，等写入记录。 */
        data object Awaiting : Op

        /** 撤卡（为什么都一样：撤）。 */
        data object Close : Op
    }

    companion object {
        /** 是活卡帧 ⇒ 装进声明的类（核心按 `cmd-wire` 那一份重写过）；不是 ⇒ `null`；装不进 · 那一种 `op` 该有的格缺 ⇒ `null`（调用方报一次契约对不上）。 */
        fun of(frame: Map<String, Any?>): LiveFrame? {
            if (frame.str("kind") != KIND) return null
            val f = Wire.bind<LiveCardFrame>(frame) ?: return null
            return decoding {
                val op =
                    when (f.op) {
                        LiveOpWord.OPEN ->
                            Op.Open(
                                rid = f.rid.need(),
                                awaiting = f.phase.need() == LivePhaseWord.AWAITING,
                                clipped = f.clipped.need(),
                                blocks = f.blocks.need().map { LiveBlock(it.kind, it.text.orEmpty(), it.tool) },
                            )
                        LiveOpWord.BLOCK -> Op.Block(f.i.need().toInt(), f.block.need(), f.tool)
                        LiveOpWord.TEXT -> Op.Text(f.i.need().toInt(), f.s.need())
                        LiveOpWord.AWAITING -> Op.Awaiting
                        LiveOpWord.CLOSE -> Op.Close
                    }
                LiveFrame(f.sid, f.run, f.card, f.n, op)
            }
        }

        const val KIND: String = "live_card"
    }
}

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

    /** 一帧。卡变了 ⇒ `true`。 */
    fun apply(f: LiveFrame): Boolean {
        val op = f.op
        if (op is LiveFrame.Op.Open) {
            val was = cards[f.card]
            cards[f.card] = Card(f.sid, f.run, f.n + 1, LiveCardState(f.card, op.rid, op.awaiting, op.clipped, op.blocks), was?.born ?: born++)
            return true
        }
        val c = cards[f.card] ?: return false
        if (f.n != c.next || op == LiveFrame.Op.Close) {
            cards.remove(f.card)
            return true
        }
        c.next += 1
        val s = c.state
        c.state =
            when (op) {
                is LiveFrame.Op.Block -> {
                    val blocks = s.blocks.toMutableList()
                    while (blocks.size <= op.i) blocks += LiveBlock(OTHER, "", null)
                    blocks[op.i] = LiveBlock(op.kind, "", op.tool)
                    s.copy(blocks = blocks)
                }
                is LiveFrame.Op.Text ->
                    s.copy(blocks = s.blocks.mapIndexed { i, b -> if (i == op.i) b.copy(text = b.text + op.s) else b })
                LiveFrame.Op.Awaiting -> s.copy(awaiting = true)
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
