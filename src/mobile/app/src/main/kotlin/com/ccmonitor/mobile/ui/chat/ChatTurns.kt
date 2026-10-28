package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Turns
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.model.StreamItem
import com.ccmonitor.mobile.core.claude.model.TurnFolds
import com.ccmonitor.mobile.core.wire.gen.FoldSegment
import com.ccmonitor.mobile.core.wire.gen.TurnRow
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.launch

/**
 * 一条会话的过程分段（那台 `history-turns` 的成品）与开合：手上那份分段 · 它是拿哪一处问的 · 点开着的那几段（按 `after` 记，只在内存里）。
 * 读写都在 [ChatSession] 的锁里（[read] 的那一问在锁外，见 [ChatSession]）。产品不替人开合：只有落点收在折着的一段里才替它展开一次。
 */
internal class ChatTurns(
    private val link: CoreLink,
) {
    private var turns: List<TurnRow> = emptyList()

    /** 手上那份是拿哪一处问的（`cover`：当时已拉到的最上面那一行）；还没问到 ⇒ `null`。 */
    private var cover: Long? = null
    private val opened = HashSet<String>()

    /** 已经替它展开过所在那一段的落点。 */
    private var openedFor: String? = null

    /** 记录有变 ⇒ 再问一次（攒着的并成一问）。 */
    private val wanted = Channel<Unit>(Channel.CONFLATED)

    fun want() {
        wanted.trySend(Unit)
    }

    /** 换了一份记录 · 从头重读了：手上那份作废，下一问从已拉到的最上面那一行重问。[all] ⇒ 开合也清（换了一条会话）。 */
    fun forget(all: Boolean = false) {
        turns = emptyList()
        cover = null
        if (all) opened.clear()
    }

    fun toggle(after: String) {
        if (!opened.remove(after)) opened += after
    }

    /**
     * 跟着记录问：有人 [want]（记录有变）⇒ 问一次；还没收尾那一轮行上的「现在 · 在等」跟着那台此刻的会话事实（那台在 `history-turns` 里拼）⇒
     * 这条会话的脸（[faces]）变了也问一次。手上那份只在 [locked] 里读写（它在会话的锁里跑那一块，块回 `true` ⇒ 上屏）；
     * 问之前在锁里取已拉到的最上面那一行（[top]），那一问在锁外。
     */
    fun follow(
        scope: CoroutineScope,
        p: String,
        faces: Flow<Any?>,
        top: () -> Long,
        locked: suspend (() -> Boolean) -> Unit,
    ) {
        scope.launch {
            while (true) {
                wanted.receive()
                var at = 0L
                var from: Long? = null
                locked {
                    at = top()
                    from = Turns.resumeAt(turns).takeIf { cover?.let { c -> c <= at } == true }
                    false
                }
                val got = read(p, at, from) ?: continue
                locked {
                    take(at, from, got)
                    true
                }
            }
        }
        scope.launch { faces.distinctUntilChanged().drop(1).collect { want() } }
    }

    /** 问一次（锁外）：一页不够照 `next` 接着问。问不到 ⇒ `null`（分段问不到不算读不出：照摊开画）。 */
    private suspend fun read(
        p: String,
        top: Long,
        from: Long?,
    ): List<TurnRow>? {
        var args = if (from == null) Turns.cover(p, top) else Turns.from(p, from)
        val got = mutableListOf<TurnRow>()
        while (true) {
            val page = (link.call(Turns.COMMAND, args, ChatReads.READ_MS) as? Reply.Ok)?.let { Turns.of(it.data) } ?: return null
            got += page.turns
            args = Turns.from(p, page.next ?: break)
        }
        return got
    }

    /** 问到的那一份接上（锁里）。 */
    private fun take(
        top: Long,
        from: Long?,
        got: List<TurnRow>,
    ) {
        if (from == null) {
            turns = got
            cover = top
        } else {
            turns = Turns.merge(turns, from, got)
        }
    }

    /** 「跳到第一处失败」：那一段展开（记成手动开），回那台指的那一步那张卡的 key；那一段没有失败 · 那张卡还没读到 ⇒ `null`。 */
    fun failOf(
        units: List<RenderUnit>,
        after: String,
    ): String? {
        val fail = turns.flatMap { it.folds }.firstOrNull { it.after == after }?.firstFail ?: return null
        val key =
            units
                .firstOrNull { u -> u.sourceUuid == fail.uuid && (fail.tool == null || (u as? RenderUnit.ToolCall)?.toolUseId == fail.tool) }
                ?.key ?: return null
        opened += after
        return key
    }

    /** 上一次折的：拿哪份单元 · 哪份分段 · 什么开合折出来的流（开合以后变过 ⇒ 不能接着折）。 */
    private var last: Folded? = null

    private class Folded(
        val units: List<RenderUnit>,
        val folds: List<FoldSegment>,
        val open: Set<String>,
        val stream: List<StreamItem>,
    )

    /**
     * 流里画的那几项；落点收在折着的一段里 ⇒ 先展开那一段（记成手动开；之后人收起不再替他开）。
     * 开合没变 ⇒ 从上一份的最后一段过程起往下重折（[TurnFolds.foldTail]：那之前的单元与分段有变它自己整份折）。
     */
    fun fold(
        units: List<RenderUnit>,
        landing: String?,
    ): List<StreamItem> {
        if (landing != null && landing != openedFor) {
            TurnFolds.owner(units, turns, landing)?.let {
                opened += it
                openedFor = landing
            }
        }
        val prev = last?.takeIf { it.open == opened }
        val stream = prev?.let { TurnFolds.foldTail(it.stream, it.units, it.folds, units, turns, opened) } ?: TurnFolds.fold(units, turns, opened)
        last = Folded(units, turns.flatMap { it.folds }, opened.toSet(), stream)
        return stream
    }
}
