package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.AccountsList
import com.ccmonitor.mobile.core.claude.link.Changed
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.RotWhere
import com.ccmonitor.mobile.core.claude.link.Rotation
import com.ccmonitor.mobile.core.claude.link.RotationStrategies
import com.ccmonitor.mobile.core.claude.link.wire.RotSeg
import com.ccmonitor.mobile.core.claude.link.wire.RotationStrategiesReply
import com.ccmonitor.mobile.core.claude.link.wire.RotationViewReply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock

/** 提示的颜色（轮换屏）：出错红 · 需手动与停发琥珀 · 别处被改了蓝 · 只告知灰（同桌面那张表）。 */
enum class NoteTone {
    ERROR,
    WARN,
    INFO,
    PLAIN,
}

/** 一格改动没收下：哪一格（[RotCellId]）· 那一句（`rot.fail.settings` ＋ 那台的话）。 */
data class CellFailure(
    val cell: String,
    val text: String,
)

/** 等人点头的那一件（确认抽屉）。 */
sealed interface RotAsk {
    /** 有没存的改动时换策略：丢掉没存的改动并换。 */
    data class Discard(
        val strategy: String?,
    ) : RotAsk

    /** 删一份策略（撤不回）。 */
    data class Delete(
        val id: String,
    ) : RotAsk

    /** 问一个名字：复制 · 改名 · 从文件添加（[initial] 是框里先填的）。 */
    data class Name(
        val title: String,
        val initial: String,
        val go: (String) -> Unit,
    ) : RotAsk
}

/** 轮换屏此刻要画的（全是那台的回包 ＋ 这一屏上点过的几样）。 */
data class RotationUi(
    val view: RotationViewReply? = null,
    val list: RotationStrategiesReply? = null,
    /** `rotation-view` 读不成那一句（红条 ·［重试］）；读成了 ⇒ `null`。 */
    val readFailed: Problem? = null,
    /** 头一回读完了没有（没读完 ⇒ 骨架）。 */
    val loaded: Boolean = false,
    val hours: Int = HOURS_12,
    /** 点开了哪个号。 */
    val open: String? = null,
    /** ［立即用］被拦下那一句（那个号 · 那一句）。 */
    val refused: Pair<String, String>? = null,
    val cellFailed: CellFailure? = null,
    /** 存着（［存成这份策略］转圈、［还原］灰）。 */
    val saving: Boolean = false,
    /** 旧格式那几份的抽屉开着。 */
    val oldOpen: Boolean = false,
    val ask: RotAsk? = null,
    /** 号牌上的号色与名字（那台账号库的事实，`accounts-list`）。 */
    val accounts: Map<String, AccountsList.Account> = emptyMap(),
    /** 一件写没成那一句（底部提示一次）。 */
    val failure: Problem? = null,
) {
    companion object {
        const val HOURS_12 = 12
        const val HOURS_24 = 24
    }
}

/**
 * 轮换屏的接线（桌面 `rot-mount.ts` 同一套）：读 `rotation-strategies` · `rotation-view` → 画 → 手势翻成命令 → 照回包再读。
 * 会话（[RotWhere.Session]）改动先成它的草稿（在那台）；设置页（[RotWhere.Strategy] 之外由 `forNew` 定）草稿在手上，随读交给那台、存时交。
 * 判 · 排 · 算时刻 · 写句子全是那台的：这里只拼入参、照码分流（`conflict` 重读 · `dirty` 问一句 · `unusable` 写那一句）。
 */
class RotationBoard(
    private val link: CoreLink,
    /** 会话的 sid；设置页（新会话用的那份）⇒ `null`。 */
    private val sid: String?,
    private val machine: String,
    private val scope: CoroutineScope,
    /** 流上的 `changed`：这个会话的轮换 · 这台的策略目录变了 ⇒ 再读。 */
    changes: Flow<Changed>? = null,
) {
    private val _state = MutableStateFlow(RotationUi())
    val state: StateFlow<RotationUi> = _state.asStateFlow()

    /** 设置页手上的草稿与它照哪个版本改出来的。 */
    private var draft: Any? = null
    private var draftRev: String? = null
    private val reading = Mutex()

    val inSession: Boolean get() = sid != null

    init {
        scope.launch {
            loadAccounts()
            refresh()
        }
        changes?.let { flow ->
            scope.launch {
                flow.collect { c ->
                    val mine = (c.topic == Rotation.TOPIC && (sid == null || c.key == sid)) || c.topic == Rotation.TOPIC_STRATEGIES
                    if (mine) refresh()
                }
            }
        }
    }

    /** 读一遍：策略目录 ＋ 这一处的 `rotation-view`。读不成 ⇒ 红条，旧画面留着（读完整块换，不跳位）。 */
    suspend fun refresh() =
        reading.withLock {
            val l = link.call(Rotation.STRATEGIES, emptyMap(), READ_MS)
            val list = (l as? Reply.Ok)?.let { RotationStrategies.of(it.data) }
            if (list == null) return@withLock failRead(l, Rotation.STRATEGIES)
            val where = where(list)
            if (where == null) {
                _state.update { it.copy(list = list, view = null, readFailed = null, loaded = true) }
                return@withLock
            }
            val v = link.call(Rotation.VIEW, Rotation.view(where, _state.value.hours, draft, draftRev), READ_MS)
            val view = (v as? Reply.Ok)?.let { Rotation.ofView(it.data) } ?: return@withLock failRead(v, Rotation.VIEW)
            _state.update { it.copy(list = list, view = view, readFailed = null, loaded = true) }
        }

    private fun failRead(
        r: Reply,
        command: String,
    ) = _state.update { it.copy(readFailed = problemOf(r, command), loaded = true) }

    /** 看哪一处：会话 ⇒ 它；设置页 ⇒ 新会话用的那份（没有 ⇒ `null`，只画第一行）。 */
    private fun where(list: RotationStrategiesReply): RotWhere? = sid?.let { RotWhere.Session(it) } ?: list.forNew.takeIf { it.isNotEmpty() }?.let { RotWhere.Strategy(it) }

    private suspend fun loadAccounts() {
        val r = link.call(AccountsList.COMMAND, AccountsList.args(""), READ_MS)
        val got = (r as? Reply.Ok)?.let { AccountsList.of(it.data) } ?: return
        _state.update { it.copy(accounts = got.accounts.associateBy { a -> a.name }) }
    }

    /** 显示的这份设置照哪个版本（那台给的）：写的时候当 `ifRev`。 */
    private fun rev(): String =
        _state.value.view
            ?.baseRev
            .orEmpty()

    /**
     * 发一条写：成了 ⇒ 再读；`conflict`（别处改过）⇒ 不报、再读（提示由那台出）；别的 ⇒ [onFail]（缺省：底部一句「做什么失败 · 那台的话」）。
     * 回「成没成」。
     */
    private suspend fun write(
        what: String,
        command: String,
        args: Map<String, Any?>,
        onFail: (Problem) -> Unit = { p -> fail(what, p) },
    ): Boolean {
        val r = link.call(command, args, WRITE_MS)
        if (r is Reply.Ok) {
            refresh()
            return true
        }
        if ((r as? Reply.Failed)?.failure?.code == CONFLICT) {
            refresh()
            return false
        }
        onFail(problemOf(r, command))
        return false
    }

    private fun fail(
        what: String,
        p: Problem,
    ) = _state.update { it.copy(failure = Problem(listOf(what, p.text).filter(String::isNotEmpty).joinToString(copyText("kit.text.sep")), p.detail)) }

    fun failureShown() = _state.update { it.copy(failure = null) }

    // ── 第一行

    /** 下拉选了一份（`null` ＝ 不轮换）。会话 ⇒ `rotation-use`（有没存的改动 ⇒ 先问）；设置页 ⇒ `rotation-for-new`。 */
    fun pick(id: String?) {
        scope.launch {
            if (sid == null) {
                draft = null
                draftRev = null
                write(copyText("rot.fail.forNew"), Rotation.FOR_NEW, Rotation.forNew(id))
                return@launch
            }
            val r = link.call(Rotation.USE, Rotation.use(sid, id, discard = false), WRITE_MS)
            when {
                r is Reply.Ok -> refresh()
                (r as? Reply.Failed)?.failure?.code == DIRTY -> _state.update { it.copy(ask = RotAsk.Discard(id)) }
                else -> fail(copyText("rot.fail.use"), problemOf(r, Rotation.USE))
            }
        }
    }

    fun answer(yes: Boolean) {
        val ask = _state.value.ask ?: return
        _state.update { it.copy(ask = null) }
        if (!yes) return
        scope.launch {
            when (ask) {
                is RotAsk.Discard -> sid?.let { write(copyText("rot.fail.use"), Rotation.USE, Rotation.use(it, ask.strategy, discard = true)) }
                is RotAsk.Delete -> write(copyText("rot.fail.delete"), Rotation.DELETE, Rotation.delete(ask.id))
                is RotAsk.Name -> Unit
            }
        }
    }

    /** 名字框交回来的名字（空 ⇒ 不做）。 */
    fun named(name: String) {
        val ask = _state.value.ask as? RotAsk.Name ?: return
        _state.update { it.copy(ask = null) }
        if (name.isNotBlank()) ask.go(name.trim())
    }

    fun copy(id: String) =
        _state.update {
            it.copy(ask = RotAsk.Name(copyText("rot.ask.copyTitle", "name" to id), "") { name -> launchWrite(copyText("rot.fail.copy"), Rotation.COPY, Rotation.copy(id, name)) })
        }

    fun rename(
        id: String,
        ifRev: String,
    ) = _state.update {
        it.copy(
            ask =
                RotAsk.Name(copyText("rot.ask.renameTitle", "name" to id), id) { name ->
                    if (name != id) launchWrite(copyText("rot.fail.rename"), Rotation.RENAME, Rotation.rename(id, name, ifRev))
                },
        )
    }

    fun forNew(id: String) = launchWrite(copyText("rot.fail.forNew"), Rotation.FOR_NEW, Rotation.forNew(id))

    fun remove(id: String) = _state.update { it.copy(ask = RotAsk.Delete(id)) }

    /** 从文件添加：手机上选好的那一份（[fileName] 去掉 `.rhai` 作框里先填的名字）。 */
    fun add(
        fileName: String,
        source: String,
    ) = _state.update {
        it.copy(ask = RotAsk.Name(copyText("rot.ask.addTitle"), fileName.removeSuffix(".rhai")) { name -> launchWrite(copyText("rot.fail.add"), Rotation.ADD, Rotation.add(source, name)) })
    }

    private fun launchWrite(
        what: String,
        command: String,
        args: Map<String, Any?>,
    ) {
        scope.launch { write(what, command, args) }
    }

    // ── 设置

    /**
     * 改了一格（[cell] ＝ 哪一格，没收下时那一行底下写那一句）· 拖了号牌 · 移出 · 加号：整份新设置。
     * 会话 ⇒ `rotation-settings`（`ifRev` ＝ 那台给的 `baseRev`）；设置页 ⇒ 草稿留在手上、随读交给那台。
     */
    fun settings(
        next: Map<String, Any?>,
        cell: String = "",
    ) {
        if (sid == null) {
            if (draft == null) {
                draftRev =
                    _state.value.view
                        ?.strategy
                        ?.rev
            }
            draft = next
            _state.update { it.copy(cellFailed = null) }
            scope.launch { refresh() }
            return
        }
        scope.launch {
            val ok =
                write(copyText("rot.fail.settings"), Rotation.SETTINGS, Rotation.settings(RotWhere.Session(sid), next, rev())) { p ->
                    _state.update { it.copy(cellFailed = CellFailure(cell, listOf(copyText("rot.fail.settings"), p.text).filter(String::isNotEmpty).joinToString(copyText("kit.text.sep")))) }
                }
            if (ok) _state.update { it.copy(cellFailed = null) }
        }
    }

    /** 24 小时条刷一段 ⇒ 那台刷完的那一组（`null` ＝ 没成，那一句已报）。 */
    suspend fun dayPaint(
        segs: List<RotSeg>,
        from: Long,
        to: Long,
        v: Double?,
    ): List<RotSeg>? {
        val r = link.call(Rotation.DAY_PAINT, Rotation.dayPaint(segs, from, to, v), WRITE_MS)
        return (r as? Reply.Ok)?.let { Rotation.ofDayPaint(it.data) } ?: null.also { fail(copyText("rot.fail.settings"), problemOf(r, Rotation.DAY_PAINT)) }
    }

    fun save() {
        val view = _state.value.view ?: return
        scope.launch {
            _state.update { it.copy(saving = true) }
            if (sid != null) {
                write(copyText("rot.fail.save"), Rotation.SAVE, Rotation.save(RotWhere.Session(sid), rev()))
            } else {
                val id = view.strategy?.id
                if (id != null && write(copyText("rot.fail.save"), Rotation.SAVE, Rotation.save(RotWhere.Strategy(id), rev(), draft))) {
                    draft = null
                    draftRev = null
                    refresh()
                }
            }
            _state.update { it.copy(saving = false) }
        }
    }

    fun revert() {
        if (sid != null) {
            launchWrite(copyText("rot.fail.revert"), Rotation.REVERT, Rotation.revert(sid))
        } else {
            draft = null
            draftRev = null
            scope.launch { refresh() }
        }
    }

    // ── 号

    fun open(account: String?) = _state.update { it.copy(open = account, refused = null) }

    /** ［立即用］：那台说用不了（`unusable`）⇒ 按钮下照 `rot.acct.refused` 写那一句（那台事实上的那一句 `blocked.text`，没有就照回的话）。 */
    fun useNow(account: String) {
        val s = sid ?: return
        scope.launch {
            val r = link.call(Rotation.USE_NOW, Rotation.useNow(s, account), WRITE_MS)
            val f = (r as? Reply.Failed)?.failure
            when {
                r is Reply.Ok -> _state.update { it.copy(refused = null) }
                f?.code == UNUSABLE -> {
                    val why =
                        _state.value.view
                            ?.accounts
                            ?.firstOrNull { it.account == account }
                            ?.blocked
                            ?.text ?: f.message
                    _state.update { it.copy(refused = account to copyText("rot.acct.refused", "account" to label(account), "why" to why)) }
                }
                else -> fail(copyText("rot.fail.useNow"), problemOf(r, Rotation.USE_NOW))
            }
            refresh()
        }
    }

    // ── 模拟

    fun hours(h: Int) {
        _state.update { it.copy(hours = h) }
        scope.launch { refresh() }
    }

    /** 拖到最上那条：排到第 [at] 段（几何换出来的位置；能不能用 · 跳不跳由那台推演答）。 */
    fun place(
        account: String,
        at: Int,
    ) {
        val s = sid ?: return
        launchWrite(copyText("rot.fail.temp"), Rotation.TEMP, Rotation.place(s, account, at))
    }

    fun clearTemp() {
        val s = sid ?: return
        launchWrite(copyText("rot.fail.temp"), Rotation.TEMP, Rotation.clearTemp(s))
    }

    // ── 提示

    fun setTz(zone: String) {
        val st = _state.value.view?.strategy ?: return
        launchWrite(copyText("rot.fail.tz"), Rotation.TZ, Rotation.tz(st.id, zone, st.rev))
    }

    /** 别处改了 · 载入新的：丢掉这边的草稿，照文件此刻的。 */
    fun loadNew() = revert()

    /** 别处改了 · 留着这边的：草稿照旧，基于的版本跟上文件此刻的（提示随之消失）。 */
    fun keepMine() {
        val now =
            _state.value.view
                ?.strategy
                ?.rev ?: return
        if (sid != null) {
            launchWrite(copyText("rot.fail.settings"), Rotation.SETTINGS, Rotation.settings(RotWhere.Session(sid), _state.value.view?.settings, now))
        } else {
            draftRev = now
            scope.launch { refresh() }
        }
    }

    fun toggleOld() = _state.update { it.copy(oldOpen = !it.oldOpen) }

    /** 号在人眼里叫什么（那台账号库写的 `label`；没有 ⇒ 号名）。 */
    fun label(account: String): String = _state.value.accounts[account]?.label ?: account

    private fun problemOf(
        r: Reply,
        command: String,
    ): Problem = r.problem(machine, command) ?: unreadableReply(machine, command, r.toString())

    private companion object {
        const val READ_MS = 15_000L
        const val WRITE_MS = 15_000L
        const val CONFLICT = "conflict"
        const val DIRTY = "dirty"
        const val UNUSABLE = "unusable"
    }
}
