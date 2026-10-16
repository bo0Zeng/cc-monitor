package com.ccmonitor.mobile.ui.chat.newsession

import com.ccmonitor.mobile.core.claude.link.AccountsList
import com.ccmonitor.mobile.core.claude.link.AccountsSetDefault
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.FilesHome
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.RotationStrategies
import com.ccmonitor.mobile.core.claude.link.SessionOpen
import com.ccmonitor.mobile.core.claude.link.SessionNewDir
import com.ccmonitor.mobile.core.claude.link.SessionNewFacts
import com.ccmonitor.mobile.core.claude.link.wire.AccountRow
import com.ccmonitor.mobile.core.claude.link.wire.AgentFacts
import com.ccmonitor.mobile.core.claude.link.wire.RotStrategyItem
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** 最近用过的一个目录（那台给的）：[missing] ＝ 点过、那台说不在。 */
data class RecentRow(
    val cwd: String,
    val lastText: String,
    val missing: Boolean = false,
)

/** 新建会话那一屏此刻的选项（全是那台的事实 ＋ 这张草稿上点过的）。 */
data class NewSessionUi(
    /** 这一张起在哪个目录（打开就是填好的：从哪进来用哪的 → 那台最近的第一个 → `~`）。 */
    val cwd: String,
    val agents: List<AgentFacts> = emptyList(),
    /** 起哪一家（线上 kind）。 */
    val agent: String? = null,
    val recent: List<RecentRow> = emptyList(),
    /** 读不到那台的新建会话信息那一句；`null` ＝ 读到了 / 还在读。 */
    val factsFailed: String? = null,
    val accounts: List<AccountRow> = emptyList(),
    /** 这一张用哪个号；那台没开多账号 ⇒ `null`。 */
    val account: String? = null,
    /** 那台的新会话默认号。 */
    val defaultAccount: String? = null,
    /** 那台的家目录（路径缩成 `~` 用）。 */
    val home: String? = null,
    val accountsFailed: String? = null,
    /** 轮换策略目录；还没问（账号抽屉没开过）⇒ `null`。 */
    val strategies: List<RotStrategyItem>? = null,
    /** 这一张用哪份（`null` ＝ 不轮换）。 */
    val rotation: String? = null,
    /** 那台「新会话用」的那份（`null` ＝ 不轮换）。 */
    val forNew: String? = null,
    val rotationFailed: String? = null,
    /** 设为默认没成那一句。 */
    val note: String? = null,
) {
    /** 只有一家 agent 时不画那一颗。 */
    val showAgentChip: Boolean get() = agents.size > 1

    /** 只有一个号时不画那一颗。 */
    val showAccountChip: Boolean get() = accounts.size > 1

    val agentText: String? get() = agents.firstOrNull { it.kind == agent }?.text
}

/** 发第一条时交给 `session-new` 的那几格：账号 · 轮换只在改过时有（没改 ⇒ `null`，那台按它的默认）。 */
data class Chosen(
    val cwd: String,
    val agent: String,
    val account: String?,
    val rotation: SessionOpen.Rotation?,
)

/**
 * 一张新建会话草稿的选项：打开就问那台一次（`session-new-facts` · `accounts-list`；没问到 ⇒ 流接上了再问），四样默认值都是那台的；
 * 点 chip 只为改，改了才多交那一格。账号抽屉打开那一刻再问一次账号 · 额度 · 轮换（关了不再问）。手机不另记默认。
 */
class NewSessionPicker(
    private val link: CoreLink,
    private val machine: String,
    /** 从哪进来的那个目录（在一条会话里点「新建会话」⇒ 那条的目录）；没有 ⇒ `null`。 */
    private val from: String?,
    /** 这台存的那一家（主机设置里选的）。 */
    private val storedAgent: String?,
    private val scope: CoroutineScope,
) {
    private val _state = MutableStateFlow(NewSessionUi(cwd = from ?: HOME))
    val state: StateFlow<NewSessionUi> = _state.asStateFlow()

    private val loaded = CompletableDeferred<Unit>()

    init {
        scope.launch {
            val seen = link.epoch.value
            load()
            loaded.complete(Unit)
            // 打开时那台没问到（离线 · 还没接上）⇒ 常驻流接上了再问一遍，不等人重开这一张。
            link.epoch.filter { it != seen }.collect { if (_state.value.factsFailed != null) load() }
        }
    }

    private suspend fun load() {
        loadFacts()
        loadHome()
        loadAccounts()
    }

    /** 打开时那一问问完了（成没成都算）。 */
    suspend fun settled() = loaded.await()

    private suspend fun loadFacts() {
        val r = link.call(SessionNewFacts.COMMAND, emptyMap(), READ_MS)
        val facts = (r as? Reply.Ok)?.let { SessionNewFacts.of(it.data) }
        if (facts == null) {
            _state.update { it.copy(factsFailed = copyText("newSession.facts.failed", "machine" to machine, "why" to failed(r, SessionNewFacts.COMMAND))) }
            return
        }
        val agent = SessionNewFacts.pick(facts.agents, storedAgent)?.kind
        _state.update { s ->
            s.copy(
                agents = facts.agents,
                agent = agent,
                recent = facts.recent.map { RecentRow(it.cwd, it.lastText) },
                cwd = from ?: facts.recent.firstOrNull()?.cwd ?: HOME,
                factsFailed = null,
            )
        }
    }

    /** 那台的家目录（路径里缩成 `~`）；问不到 ⇒ 路径原样。 */
    private suspend fun loadHome() {
        val home = (link.call(FilesHome.COMMAND, emptyMap(), READ_MS) as? Reply.Ok)?.let { FilesHome.text(it.data) } ?: return
        _state.update { it.copy(home = home) }
    }

    private suspend fun loadAccounts(keep: Boolean = false) {
        val agent = _state.value.agent ?: storedAgent.orEmpty()
        val r = link.call(AccountsList.COMMAND, AccountsList.args(agent), READ_MS)
        val list = (r as? Reply.Ok)?.let { AccountsList.of(it.data) }
        if (list == null) {
            _state.update { it.copy(accountsFailed = failed(r, AccountsList.COMMAND)) }
            return
        }
        _state.update { s ->
            val kept = s.account?.takeIf { keep && a(list.accounts, it)?.selectable == true }
            s.copy(
                accounts = list.accounts,
                defaultAccount = list.meta.effectiveDefault,
                account = kept ?: list.meta.effectiveDefault?.takeIf { d -> list.accounts.any { it.name == d } },
                home = s.home ?: list.meta.home,
                accountsFailed = null,
            )
        }
    }

    /** 账号抽屉打开：问一次账号（每号带 5h 那一格）· 轮换。 */
    fun openAccounts() {
        scope.launch { loadAccounts(keep = true) }
        scope.launch {
            val r = link.call(RotationStrategies.COMMAND, emptyMap(), READ_MS)
            val list = (r as? Reply.Ok)?.let { RotationStrategies.of(it.data) }
            if (list == null) {
                _state.update { it.copy(rotationFailed = failed(r, RotationStrategies.COMMAND)) }
                return@launch
            }
            val forNew = list.forNew.ifEmpty { null }
            _state.update { s ->
                // 头一回读到 ⇒ 这一张用新会话用的那份；之后再开抽屉 ⇒ 点过的那份还在就留着。
                val keep = s.strategies != null && (s.rotation == null || list.strategies.any { it.id == s.rotation })
                s.copy(strategies = list.strategies, forNew = forNew, rotation = if (keep) s.rotation else forNew, rotationFailed = null)
            }
        }
    }

    fun pickAgent(kind: String) {
        if (kind == _state.value.agent) return
        _state.update { it.copy(agent = kind, account = null, accounts = emptyList()) }
        scope.launch { loadAccounts() }
    }

    /** 选不了的号（那台说 `selectable: false`）点了不选上。 */
    fun pickAccount(name: String) {
        if (a(_state.value.accounts, name)?.selectable != true) return
        _state.update { it.copy(account = name) }
    }

    fun pickRotation(id: String?) = _state.update { it.copy(rotation = id) }

    /** 最近里点一行：先问那台在不在；不在 ⇒ 那一行灰掉、不选。[done] 带选上了没有（抽屉照它收不收）。 */
    fun pickRecent(
        cwd: String,
        done: (Boolean) -> Unit = {},
    ) {
        scope.launch {
            val exists = (link.call(SessionNewDir.COMMAND, SessionNewDir.args(cwd), READ_MS) as? Reply.Ok)?.let { SessionNewDir.of(it.data) }
            _state.update { s ->
                when (exists) {
                    true -> s.copy(cwd = cwd, recent = s.recent.map { if (it.cwd == cwd) it.copy(missing = false) else it })
                    false -> s.copy(recent = s.recent.map { if (it.cwd == cwd) it.copy(missing = true) else it })
                    // 问不到 ⇒ 照选：起的时候那台再核（`no_dir` 那一句）。
                    null -> s.copy(cwd = cwd)
                }
            }
            done(exists != false)
        }
    }

    /** 浏览屏里选的（那台列出来的，在）。 */
    fun pickDir(path: String) = _state.update { it.copy(cwd = path) }

    /** 设为这台的新会话默认号（账号库里那一格，桌面同一份）；之后再问一次账号。 */
    fun setDefault(name: String) {
        scope.launch {
            val r = link.call(AccountsSetDefault.COMMAND, AccountsSetDefault.args(name), WRITE_MS)
            if (r !is Reply.Ok) {
                val why = r.problem(machine, AccountsSetDefault.COMMAND)?.text
                _state.update { it.copy(note = listOfNotNull(copyText("acctPage.default.failed", "name" to name), why).joinToString(copyText("kit.text.sep"))) }
                return@launch
            }
            _state.update { it.copy(note = null) }
            loadAccounts(keep = true)
        }
    }

    fun chosen(): Chosen {
        val s = _state.value
        val account = s.account?.takeIf { it != s.defaultAccount }
        val rotation = s.strategies?.let { if (s.rotation != s.forNew) SessionOpen.Rotation(s.rotation) else null }
        return Chosen(s.cwd, s.agent ?: storedAgent.orEmpty(), account, rotation)
    }

    private fun a(
        list: List<AccountRow>,
        name: String,
    ) = list.firstOrNull { it.name == name }

    private fun failed(
        r: Reply,
        command: String,
    ): String = (r.problem(machine, command) ?: unreadableReply(machine, command, r.toString())).text

    companion object {
        /** 那台一条记录都没有时起在家目录（那台按它的家目录读）。 */
        const val HOME = "~"
        private const val READ_MS = 15_000L
        private const val WRITE_MS = 20_000L
    }
}
