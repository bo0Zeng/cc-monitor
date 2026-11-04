package com.ccmonitor.mobile.ui.agents

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.HistoryFacts
import com.ccmonitor.mobile.core.claude.link.HistoryRun
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Turns
import com.ccmonitor.mobile.core.claude.model.NoticeCard
import com.ccmonitor.mobile.core.claude.model.NoticeLook
import com.ccmonitor.mobile.core.claude.model.RecordUnits
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.model.StreamItem
import com.ccmonitor.mobile.core.claude.model.TurnFolds
import com.ccmonitor.mobile.core.wire.gen.NoticeRun
import com.ccmonitor.mobile.core.wire.gen.TurnRow
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import com.ccmonitor.mobile.core.wire.gen.HistoryRun as RunPage

/** agent 屏正文此刻的样子。 */
data class AgentRunUi(
    val stream: List<StreamItem> = emptyList(),
    /** 前面还有（手上最上面那一页的 `from` 不是 0）。 */
    val canOlder: Boolean = false,
    /** 后面还有（那台说这一页没读到头）。 */
    val canNewer: Boolean = false,
    val loading: Boolean = true,
    /** 读不成那一句（核心写好的）；读成了 ⇒ `null`。 */
    val failed: String? = null,
    /** 手上那几张卡（通知组按 id 找成员）。 */
    val units: List<RenderUnit> = emptyList(),
    /** 这份记录会话事实里的通知组（`history-facts` 的 `notices[]`）；还没问到 · 问不到 ⇒ 空（一条一张）。 */
    val notices: List<NoticeRun> = emptyList(),
)

/** 一张收场通知卡此刻画什么（同聊天屏那一处，[NoticeLook.card]）；并进别的组 · 整组不露 ⇒ `null`。 */
fun AgentRunUi.noticeOf(u: RenderUnit.Notice): NoticeCard? = NoticeLook.card(u, units, notices)

/**
 * 一个子运行的正文（只读）：按运行读（`history-run`），收场的从头读、在跑的从尾巴读（⇐ 后端 B3）；往上翻拿最上面那一页的 `from` 当 `before`，
 * 往下读拿最下面那一页的 `end` 当 `from`。记录排卡同聊天屏（[RecordUnits]），过程照这份记录的 `history-turns` 分段折（[TurnFolds]，开合按 `after` 记在内存）。
 */
class AgentRun(
    private val link: CoreLink,
    /** 这台在手机上的名字（「{machine} 无应答」那几句）。 */
    private val machine: String,
    /** 父会话的记录路径。 */
    private val parent: String,
    private val run: String,
    /** 进屏那一刻还在跑。 */
    private val running: Boolean,
    private val scope: CoroutineScope,
) {
    private val _state = MutableStateFlow(AgentRunUi())
    val state: StateFlow<AgentRunUi> = _state.asStateFlow()

    private val lock = Mutex()
    private val body = RecordUnits()
    private var turns: List<TurnRow> = emptyList()
    private val opened = HashSet<String>()

    /** 手上最上面那一页从哪起 · 最下面那一页读到哪；还没读到 ⇒ `null`。 */
    private var top: Long? = null
    private var end: Long? = null

    /** 首屏：收场的从头读，在跑的从尾巴读。 */
    fun open() = read(first(parent, run, running), Place.RESET)

    /** 往上翻：前面还有才问。 */
    fun older() {
        val from = top?.takeIf { it > 0 } ?: return
        read(HistoryRun.before(parent, run, from), Place.FRONT)
    }

    /** 往下读（在跑的那一个记录长了 · 这一页没读到头）。 */
    fun newer() {
        val at = end ?: return
        read(HistoryRun.after(parent, run, at), Place.BACK)
    }

    /** 过程行开 ⇄ 合。 */
    fun toggle(after: String) {
        scope.launch {
            lock.withLock {
                if (!opened.remove(after)) opened += after
                publish()
            }
        }
    }

    private enum class Place { RESET, FRONT, BACK }

    private fun read(
        args: Map<String, Any?>,
        place: Place,
    ) {
        scope.launch {
            val page = ask(args) ?: return@launch
            val records = page.rows.map { it.record }
            lock.withLock {
                when (place) {
                    Place.RESET -> body.reset(records)
                    Place.FRONT -> body.prepend(records)
                    Place.BACK -> body.append(records)
                }
                if (place != Place.BACK) top = page.from
                if (place != Place.FRONT) {
                    end = page.end
                    _state.update { it.copy(canNewer = page.more) }
                }
                publish(loading = false, failed = null)
            }
            fold(page.path)
            notices(page.path)
        }
    }

    /** 问一页；没成 ⇒ 那一句上屏、`null`。 */
    private suspend fun ask(args: Map<String, Any?>): RunPage? {
        val r = link.call(HistoryRun.COMMAND, args, READ_MS)
        if (r !is Reply.Ok) return null.also { fail(r.problem(machine, HistoryRun.COMMAND, link.offline.value)?.text) }
        return HistoryRun.of(r.data) ?: null.also { fail(unreadableReply(machine, HistoryRun.COMMAND, r.data.toString()).text) }
    }

    private fun fail(said: String?) = _state.update { it.copy(loading = false, failed = said) }

    /** 过程分段：照这份记录、从手上最上面那一页起问（问不到不算读不出：照摊开画）。 */
    private suspend fun fold(p: String) {
        val cover = lock.withLock { top } ?: return
        val got = mutableListOf<TurnRow>()
        var args = Turns.cover(p, cover)
        while (true) {
            val page = (link.call(Turns.COMMAND, args, READ_MS) as? Reply.Ok)?.let { Turns.of(it.data) } ?: return
            got += page.turns
            args = Turns.from(p, page.next ?: break)
        }
        lock.withLock {
            turns = got
            publish()
        }
    }

    /** 通知组：照这份记录问会话事实（问不到不算读不出：一条一张）。 */
    private suspend fun notices(p: String) {
        val facts = (link.call(HistoryFacts.COMMAND, HistoryFacts.args(p), READ_MS) as? Reply.Ok)?.let { HistoryFacts.of(it.data) } ?: return
        _state.update { it.copy(notices = facts.notices) }
    }

    /** 锁里调。 */
    private fun publish(
        loading: Boolean = _state.value.loading,
        failed: String? = _state.value.failed,
    ) {
        val units = body.units()
        val stream = TurnFolds.fold(units, turns, opened)
        _state.update { it.copy(stream = stream, units = units, canOlder = (top ?: 0L) > 0L, loading = loading, failed = failed) }
    }

    companion object {
        /** 按运行读一页的期限（同聊天屏读正文那一问）。 */
        const val READ_MS = 15_000L

        /** 首屏那一问：收场的从头（`from=0`），在跑的从尾巴（`tail`）。 */
        fun first(
            parent: String,
            run: String,
            running: Boolean,
        ): Map<String, Any?> = if (running) HistoryRun.tail(parent, run) else HistoryRun.after(parent, run, 0L)
    }
}

/** agent 屏的栈：同一个运行只出现一次。 */
object AgentStack {
    /**
     * 进 [run] 那一屏：就在顶上（[top]）⇒ 不动；栈里已有 ⇒ 退回到它（[popTo] 回 `true`）；没有 ⇒ 压一层（[push]）。
     * [popTo] 照导航层的返回栈按那一屏的路由（连参数）找，找不到回 `false`、不动栈。
     */
    fun open(
        top: String?,
        run: String,
        popTo: (String) -> Boolean,
        push: (String) -> Unit,
    ) {
        if (top == run) return
        if (!popTo(run)) push(run)
    }
}
