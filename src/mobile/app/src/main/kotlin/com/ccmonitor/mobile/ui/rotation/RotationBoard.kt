package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.RotWhere
import com.ccmonitor.mobile.core.claude.link.Rotation
import com.ccmonitor.mobile.core.claude.link.RotationStrategies
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.RotPlate
import com.ccmonitor.mobile.core.wire.gen.RotationStrategiesReply
import com.ccmonitor.mobile.core.wire.gen.RotationViewReply
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

/** 提示的颜色（轮换屏）：出错红 · 需手动与停发琥珀 · 只告知蓝。 */
enum class NoteTone {
    ERROR,
    WARN,
    INFO,
}

/** 放下之后那一次写的结局：成 · 拒（滑回）· 说不清（没回包 · 断线：留在放下处标「没确认」）。 */
enum class MoveEnd {
    DONE,
    BACK,
    UNSURE,
}

/** 一格改动没收下：哪一格（[RotCellId]）· 那一句（`rot.fail.settings` ＋ 那台的话）。 */
data class CellFailure(
    val cell: String,
    val text: String,
)

/** 等人点头的那一件（确认抽屉）。 */
sealed interface RotAsk {
    /** 删一份策略（撤不回）。 */
    data class Delete(
        val id: String,
    ) : RotAsk

    /** 问一个名字：复制 · 改名 · 从文件添加 · 新建（[initial] 是框里先填的；[note] ＝ 框下面那一句）。 */
    data class Name(
        val title: String,
        val initial: String,
        val label: String = copyText("rot.ask.name"),
        val action: String = copyText("rot.ask.go"),
        val note: String? = null,
        val go: (String) -> Unit,
    ) : RotAsk
}

/** 设置页先看哪一份、开哪个号的编辑小窗（会话里［编辑策略］跳过来带的）。 */
data class RotFocus(
    val strategy: String,
    val account: String?,
)

/** 轮换屏此刻要画的（全是那台的回包 ＋ 这一屏上点过的几样）。 */
data class RotationUi(
    val view: RotationViewReply? = null,
    val list: RotationStrategiesReply? = null,
    /** `rotation-view` 读不成那一句（红条 ·［重试］）；读成了 ⇒ `null`。 */
    val readFailed: Problem? = null,
    /** 头一回读完了没有（没读完 ⇒ 骨架）。 */
    val loaded: Boolean = false,
    val hours: Int = HOURS_12,
    /** 设置页在看哪一份（`null` ⇒ 新会话用的那份）。 */
    val shown: String? = null,
    /** 点开了哪个号（设置页 ⇒ 编辑小窗；会话 ⇒ 只看的那一张），都从底下拉出。 */
    val open: String? = null,
    /** ［立即用］被拦下那一句（那个号 · 那一句）。 */
    val refused: Pair<String, String>? = null,
    val cellFailed: CellFailure? = null,
    /** 旧格式那几份的抽屉开着。 */
    val oldOpen: Boolean = false,
    val ask: RotAsk? = null,
    /** 一件写没成那一句（底部提示一次）。 */
    val failure: Problem? = null,
) {
    /** 视图里这个号身上那一枚号牌（号色 · 号名 · 号牌字，那台写好的）；视图里没有 ⇒ `null`。 */
    fun plate(account: String): RotPlate? = view?.accounts?.firstOrNull { it.account == account }?.plate

    companion object {
        const val HOURS_12 = 12
        const val HOURS_24 = 24
    }
}

/**
 * 轮换屏的接线（桌面 `rot-mount.ts` 同一套）：读 `rotation-strategies` · `rotation-view` → 画 → 手势翻成命令 → 照回包再读。
 * 会话（[RotWhere.Session]）照它载入的副本，只做这个会话的临时动作；设置页（新会话用的那份）改一格当场存成那份的新版。
 * 判 · 排 · 算时刻 · 写句子全是那台的：这里只拼入参、照码分流（`stale` 重读 · `unusable` 写那一句）。
 */
class RotationBoard(
    private val link: CoreLink,
    /** 会话的 sid；设置页（新会话用的那份）⇒ `null`。 */
    private val sid: String?,
    private val machine: String,
    private val scope: CoroutineScope,
    /** 流上的 `changed`：这个会话的轮换 · 这台的策略目录变了 ⇒ 再读。 */
    changes: Flow<Changed>? = null,
    /** 设置页：先看这一份、开这个号（会话里［编辑策略］跳过来）。 */
    focus: RotFocus? = null,
) {
    private val _state = MutableStateFlow(RotationUi(shown = focus?.strategy, open = focus?.account))
    val state: StateFlow<RotationUi> = _state.asStateFlow()

    private val reading = Mutex()

    init {
        scope.launch { refresh() }
        changes?.let { flow ->
            scope.launch {
                flow.collect { c ->
                    val mine =
                        when (c) {
                            is Changed.Rotation -> sid == null || c.key == sid
                            is Changed.RotationStrategies -> true
                            else -> false
                        }
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
            val v = link.call(Rotation.VIEW, Rotation.view(where, _state.value.hours), READ_MS)
            val view = (v as? Reply.Ok)?.let { Rotation.ofView(it.data) } ?: return@withLock failRead(v, Rotation.VIEW)
            _state.update { it.copy(list = list, view = view, readFailed = null, loaded = true) }
        }

    private fun failRead(
        r: Reply,
        command: String,
    ) = _state.update { it.copy(readFailed = problemOf(r, command), loaded = true) }

    /** 看哪一处：会话 ⇒ 它；设置页 ⇒ 在看的那份（还在目录里），否则新会话用的那份（没有 ⇒ `null`，只画第一行）。 */
    private fun where(list: RotationStrategiesReply): RotWhere? {
        if (sid != null) return RotWhere.Session(sid)
        val shown = _state.value.shown?.takeIf { id -> list.strategies.any { it.id == id } }
        return (shown ?: list.forNew).takeIf { it.isNotEmpty() }?.let { RotWhere.Strategy(it) }
    }

    /**
     * 发一条写：成了 ⇒ 再读；`stale`（别处改过）⇒ 不报、再读（提示由那台出）；别的 ⇒ [onFail]（缺省：底部一句「做什么失败 · 那台的话」）。
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
        if ((r as? Reply.Failed)?.failure?.code == STALE) {
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

    /** 下拉选了一份（`null` ＝ 不轮换）。会话 ⇒ `rotation-use`（载入它此刻的版本）；设置页 ⇒ 看它（「不轮换」⇒ 新会话不轮换）。 */
    fun pick(id: String?) {
        when {
            sid != null -> launchWrite(copyText("rot.fail.use"), Rotation.USE, Rotation.use(sid, id))
            id != null -> show(id)
            else -> {
                _state.update { it.copy(shown = null, open = null) }
                launchWrite(copyText("rot.fail.forNew"), Rotation.FOR_NEW, Rotation.forNew(null))
            }
        }
    }

    /** 设置页换去看 [id] 那一份。 */
    private fun show(id: String) {
        _state.update { it.copy(shown = id, open = null) }
        scope.launch { refresh() }
    }

    fun answer(yes: Boolean) {
        val ask = _state.value.ask ?: return
        _state.update { it.copy(ask = null) }
        if (!yes) return
        scope.launch {
            when (ask) {
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

    /** 新建策略：只问名字，那台照编辑器那一份建；设置页接着看新建的那份。 */
    fun create() =
        _state.update {
            it.copy(
                ask =
                    RotAsk.Name(
                        copyText("rot.ask.createTitle"),
                        "",
                        label = copyText("rot.ask.createName"),
                        action = copyText("rot.ask.createGo"),
                        note = copyText("rot.ask.createNote"),
                    ) { name ->
                        scope.launch {
                            val r = link.call(Rotation.ADD, Rotation.add(null, name), WRITE_MS)
                            if (r !is Reply.Ok) return@launch fail(copyText("rot.fail.create"), problemOf(r, Rotation.ADD))
                            if (sid == null) Rotation.ofId(r.data)?.let { id -> _state.update { it.copy(shown = id, open = null) } }
                            refresh()
                        }
                    },
            )
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
     * 设置页改了一格（[cell] ＝ 哪一格，没收下时那一行底下写那一句）· 改了一段 · 拖了号牌 · 移出 · 加号：一个意图（[RotEdit]），
     * 那台照声明改完、当场存成那份的新版（`ifRev` ＝ 读到的那一版；对不上 ⇒ 重读）。
     */
    fun edit(
        edit: Any,
        cell: String = "",
    ) {
        val st = _state.value.view?.strategy ?: return
        val failed = { p: Problem ->
            _state.update { it.copy(cellFailed = CellFailure(cell, listOf(copyText("rot.fail.settings"), p.text).filter(String::isNotEmpty).joinToString(copyText("kit.text.sep")))) }
        }
        scope.launch {
            if (write(copyText("rot.fail.settings"), Rotation.SETTINGS, Rotation.settings(st.id, edit, st.rev), failed)) _state.update { it.copy(cellFailed = null) }
        }
    }

    // ── 号

    fun open(account: String?) = _state.update { it.copy(open = account, refused = null) }

    /** ［立即用］：那台说用不了（`unusable`）⇒ 按钮下照 `rot.acct.refused` 写那一句（那台事实上的那一句 `blocked.text`，没有就照回的话）。 */
    fun useNow(account: String) {
        val s = sid ?: return
        switchTo(account, copyText("rot.fail.useNow"), Rotation.USE_NOW, Rotation.useNow(s, account))
    }

    /** 换到 [account]（［立即用］· 顺序里拖到第一格）：那台说它事实上发不出去（`unusable`）⇒ 不换，照 `rot.acct.refused` 写那一句。 */
    private fun switchTo(
        account: String,
        what: String,
        command: String,
        args: Map<String, Any?>,
    ) {
        scope.launch {
            settled(account, what, command, link.call(command, args, WRITE_MS))
            refresh()
        }
    }

    /** 一次换号 / 挪动的回包落到哪：成 · 拒（用不了 ⇒ 顺序上方那一句；别的 ⇒ 底部一次）· 说不清（没回包 · 断线）。 */
    private fun settled(
        account: String,
        what: String,
        command: String,
        r: Reply,
    ): MoveEnd {
        val f = (r as? Reply.Failed)?.failure
        val blocked =
            _state.value.view
                ?.accounts
                ?.firstOrNull { it.account == account }
                ?.blocked
                ?.text
        val line = account to copyText("rot.acct.refused", "account" to label(account), "why" to (blocked ?: f?.message.orEmpty()))
        return when {
            r is Reply.Ok -> {
                _state.update { it.copy(refused = null) }
                MoveEnd.DONE
            }
            f?.code == UNUSABLE -> {
                _state.update { it.copy(refused = line) }
                MoveEnd.BACK
            }
            f != null -> {
                fail(what, problemOf(r, command))
                MoveEnd.BACK
            }
            else -> MoveEnd.UNSURE
        }
    }

    // ── 模拟

    fun hours(h: Int) {
        _state.update { it.copy(hours = h) }
        scope.launch { refresh() }
    }

    /**
     * 顺序里拿起的那一行放到第 [to] 格：交拖的号与第几格（第一格当现在用 · 拖完那一串都由那台认），回包才定 ⇒ [done]；
     * 成 · 拒 ⇒ 重读照那台画；说不清 ⇒ 留在放下处，下一次重读（连上 · 推送）照那台画。
     */
    fun drop(
        account: String,
        to: Int,
        done: (MoveEnd) -> Unit,
    ) {
        val s = sid ?: return done(MoveEnd.BACK)
        scope.launch {
            val command = Rotation.TEMP
            val end = settled(account, copyText("rot.fail.temp"), command, link.call(command, Rotation.move(s, account, to), WRITE_MS))
            done(end)
            if (end != MoveEnd.UNSURE) refresh()
        }
    }

    fun clearTemp() {
        val s = sid ?: return
        launchWrite(copyText("rot.fail.temp"), Rotation.TEMP, Rotation.clearTemp(s))
    }

    /** 顺序里一行的勾：去掉 ＝ 这个会话先不用 [account]（不改策略）。 */
    fun off(
        account: String,
        off: Boolean,
    ) {
        val s = sid ?: return
        launchWrite(copyText("rot.fail.temp"), Rotation.TEMP, Rotation.off(s, account, off))
    }

    // ── 提示

    fun setTz(zone: String) {
        val st = _state.value.view?.strategy ?: return
        launchWrite(copyText("rot.fail.tz"), Rotation.TZ, Rotation.tz(st.id, zone, st.rev))
    }

    /** 策略改过了：会话 ⇒ 载入它那份此刻的版本；设置页 ⇒ 在用旧版的会话都载入新版。 */
    fun loadNew() {
        val st = _state.value.view?.strategy ?: return
        val id = st.id
        if (sid != null) {
            launchWrite(copyText("rot.fail.reload"), Rotation.USE, Rotation.use(sid, id))
        } else {
            launchWrite(copyText("rot.fail.reload"), Rotation.RELOAD, Rotation.reload(id))
        }
    }

    fun toggleOld() = _state.update { it.copy(oldOpen = !it.oldOpen) }

    /** 号在人眼里叫什么（那台账号库写的 `label`；没有 ⇒ 号名）。 */
    fun label(account: String): String = _state.value.plate(account)?.label ?: account

    private fun problemOf(
        r: Reply,
        command: String,
    ): Problem = r.problem(machine, command, link.offline.value) ?: unreadableReply(machine, command, r.toString())

    private companion object {
        const val READ_MS = 15_000L
        const val WRITE_MS = 15_000L
        const val STALE = "stale"
        const val UNUSABLE = "unusable"
    }
}
