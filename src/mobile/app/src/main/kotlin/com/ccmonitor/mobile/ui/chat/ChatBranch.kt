package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.BranchFrame
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.HistoryBranch
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.model.BranchFolds
import com.ccmonitor.mobile.core.claude.model.BranchList
import com.ccmonitor.mobile.core.claude.model.StreamItem
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.flow.onSubscription
import kotlinx.coroutines.launch

/**
 * 一条会话的主线外清单（Esc 回退掉的那几条）与回退框的开合：清单 ＝ `history-branch` 一页页读完的那一份，之后流上这份记录的 `session_branch` 一帧换一截
 * （[BranchList.put]）；开合按段首条 id 记在内存。读写都在 [ChatSession] 的锁里（那几问在锁外）。分段 · 框头那一句都是那台给的。
 */
internal class ChatBranch(
    private val link: CoreLink,
) {
    private var list = BranchList.EMPTY
    private val opened = HashSet<String>()

    /** 换了一条会话：清单与开合都清。 */
    fun forget() {
        list = BranchList.EMPTY
        opened.clear()
    }

    fun toggle(run: String) {
        if (!opened.remove(run)) opened += run
    }

    /** 流（过程分段折好的那一份）照清单收框。 */
    fun fold(stream: List<StreamItem>): List<StreamItem> = BranchFolds.fold(stream, list, opened)

    /**
     * 跟着 [sid] 那份记录 [path]：先订流上的帧（攒着），再冷读到头，读完照到达先后接上攒着的，之后来一帧接一帧。
     * 每一下在 [locked] 里改账、上屏。冷读没成 ⇒ 不画框（流上的帧照接）。
     */
    fun follow(
        scope: CoroutineScope,
        sid: String,
        path: String,
        locked: suspend (() -> Unit) -> Unit,
    ) {
        scope.launch {
            val frames = Channel<BranchFrame>(Channel.UNLIMITED)
            val subscribed = CompletableDeferred<Unit>()
            launch { link.branches.onSubscription { subscribed.complete(Unit) }.collect { if (it.sid == sid && it.path == path) frames.send(it) } }
            subscribed.await()
            read(path)?.let { cold -> locked { list = cold } }
            for (f in frames) locked { list = list.put(f.at, f.off, f.runs) }
        }
    }

    /** 冷读：照 `next` 一页页问到头；有一页没问成 ⇒ `null`。 */
    private suspend fun read(path: String): BranchList? {
        var got = BranchList.EMPTY
        var from = 0L
        while (true) {
            val page = (link.call(HistoryBranch.COMMAND, HistoryBranch.args(path, from), ChatReads.READ_MS) as? Reply.Ok)?.let { HistoryBranch.of(it.data) } ?: return null
            got = got.put(from, page.off, page.runs)
            from = page.next ?: return got
        }
    }
}
