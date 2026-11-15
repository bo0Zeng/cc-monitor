package com.ccmonitor.mobile.ui.chat.newsession

import com.ccmonitor.mobile.core.claude.link.AccountsList
import com.ccmonitor.mobile.core.claude.link.AccountsSetDefault
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.FilesHome
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.RotationStrategies
import com.ccmonitor.mobile.core.claude.link.SessionOpen
import com.ccmonitor.mobile.core.claude.link.SessionOpenDir
import com.ccmonitor.mobile.core.claude.link.SessionOpenFacts
import com.ccmonitor.mobile.core.claude.link.seen
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.AccountRow
import com.ccmonitor.mobile.core.wire.gen.AgentFacts
import com.ccmonitor.mobile.core.wire.gen.AgentPick
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.RotStrategyItem
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.filter
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.flow.merge
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** 最近用过的一个目录（那台给的；[shown] 是它按自己的家目录写好的 `~/…`）：[said] ＝ 点过、那台说不在时它写好的那一句（照抄）。 */
data class RecentRow(
    val cwd: String,
    val shown: String,
    val lastText: String,
    val said: String? = null,
)

/** 新建会话那一屏此刻的选项（全是那台的事实 ＋ 这张草稿上点过的）。 */
data class NewSessionUi(
    /** 这一张起在哪个目录（打开就是填好的：从哪进来用哪的 → 那台最近的第一个 → `~`）。 */
    val cwd: String,
    /** [cwd] 屏上那一串（那台写好的；还没问到 ⇒ 原样）。 */
    val cwdShown: String = cwd,
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
    /** 这一张用哪个模型（那一家 `models` 里一项的原值）；`null` ＝ 跟随（不交）。 */
    val model: String? = null,
    /** 这一张用哪一档推理（选的模型 `efforts` 里一项的原值）；那个模型不分档 ⇒ `null`。 */
    val effort: String? = null,
) {
    /** 那一家列的模型（那台写的字与先后）。 */
    val models: List<AgentPick> get() = agents.firstOrNull { it.kind == agent }?.models.orEmpty()

    /** 那一家没列模型 ⇒ 不画那一颗。 */
    val showModelChip: Boolean get() = models.isNotEmpty()

    /** 那颗 chip 上的字：选的那个模型那台写的字；跟随 ⇒「跟随」。 */
    val modelText: String get() = models.firstOrNull { it.value == model }?.text ?: copyText("mobAccounts.model.follow")

    /** 选的模型能选的推理档（那台排的先后）；跟随 · 不分档 ⇒ 空。 */
    val efforts: List<AgentPick> get() = models.firstOrNull { it.value == model }?.efforts.orEmpty()

    /** 选的模型不分档 ⇒ 不画推理档那一颗。 */
    val showEffortChip: Boolean get() = efforts.isNotEmpty()

    val effortText: String? get() = efforts.firstOrNull { it.value == effort }?.text

    /** 只有一家 agent 时不画那一颗。 */
    val showAgentChip: Boolean get() = agents.size > 1

    /** 只有一个号时不画那一颗。 */
    val showAccountChip: Boolean get() = accounts.size > 1

    val agentText: String? get() = agents.firstOrNull { it.kind == agent }?.text
}

/** 发第一条时交给 `session-open` 的那几格：账号 · 轮换只在改过时有（没改 ⇒ `null`，那台按它的默认）。 */
data class Chosen(
    val cwd: String,
    val agent: String,
    val account: String?,
    val rotation: SessionOpen.Rotation?,
    /** 选了模型才有（跟随 ⇒ `null`、不交）。 */
    val model: String? = null,
    /** 选的模型分档才有。 */
    val effort: String? = null,
)

/**
 * 一张新建会话草稿的选项：接上那一回问那台一次（`session-open-facts` · `accounts-list`，照 [seen]；没问到 ⇒ 下一回接上再问），四样默认值都是那台的；
 * 点 chip 只为改，改了才多交那一格。账号抽屉打开那一刻再问一次账号 · 额度 · 轮换（关了不再问）。手机不另记默认。
 */
class NewSessionPicker(
    private val link: CoreLink,
    private val machine: String,
    /** 从哪进来的那个目录（在一条会话里点「新建会话」⇒ 那条的目录）；没有 ⇒ `null`。 */
    private val from: String?,
    private val scope: CoroutineScope,
) {
    private val _state = MutableStateFlow(NewSessionUi(cwd = from ?: FilesHome.TILDE))
    val state: StateFlow<NewSessionUi> = _state.asStateFlow()

    private val loaded = CompletableDeferred<Unit>()

    init {
        // 接上的每一回（核心那一格：打开时已通着 ⇒ 当场一回）问一次；问到了 ⇒ 再换流不重问（不把填好的冲掉），没问到 ⇒ 下一回接上再问。
        scope.launch {
            link.seen.collect {
                if (!loaded.isCompleted || _state.value.factsFailed != null) load()
                loaded.complete(Unit)
            }
        }
    }

    private suspend fun load() {
        loadFacts()
        loadFromShown()
        loadAccounts()
    }

    /**
     * 起会话前等选项：头一回接上那一问问完了（成没成都算）；核心那一行是 [LinkLine.Down]（不论会不会自己接回）⇒ 不等，
     * 起会话那一侧照 Down 不起（字回输入框）。在连 ⇒ 等它接上（那一问本来也要等）。
     */
    suspend fun settled() {
        merge(flow { emit(loaded.await()) }, link.state.filter { it is LinkLine.Down }.map { }).first()
    }

    private suspend fun loadFacts() {
        val r = link.call(SessionOpenFacts.COMMAND, emptyMap(), READ_MS)
        val facts = (r as? Reply.Ok)?.let { SessionOpenFacts.of(it.data) }
        if (facts == null) {
            _state.update { it.copy(factsFailed = copyText("newSession.facts.failed", "machine" to machine, "why" to failed(r, SessionOpenFacts.COMMAND))) }
            return
        }
        val agent = SessionOpenFacts.pick(facts.agents, null)?.kind
        val first = facts.recent.firstOrNull()
        _state.update { s ->
            s.copy(
                agents = facts.agents,
                agent = agent,
                recent = facts.recent.map { RecentRow(it.cwd, it.shown, it.lastText) },
                // 那台一条记录都没有 ⇒ 起在家目录（那台按它的家目录读）。
                cwd = from ?: first?.cwd ?: FilesHome.TILDE,
                cwdShown = if (from != null) s.cwdShown else first?.shown ?: FilesHome.TILDE,
                factsFailed = null,
            )
        }
    }

    /** 从一条会话里进来的那个目录：屏上那一串问那台（`session-open-dir` 写好的）；问不到 ⇒ 原样。 */
    private suspend fun loadFromShown() {
        val cwd = from ?: return
        val got = (link.call(SessionOpenDir.COMMAND, SessionOpenDir.args(cwd), READ_MS) as? Reply.Ok)?.let { SessionOpenDir.of(it.data) } ?: return
        _state.update { s -> if (s.cwd == cwd) s.copy(cwdShown = got.shown) else s }
    }

    private suspend fun loadAccounts(keep: Boolean = false) {
        val agent = _state.value.agent.orEmpty()
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
        _state.update { it.copy(agent = kind, account = null, accounts = emptyList(), model = null, effort = null) }
        scope.launch { loadAccounts() }
    }

    /** 选模型（`null` ＝ 跟随）：那个模型分档 ⇒ 推理档预选它的 `defaultEffort`（那台没说 ⇒ 第一档）；不分档 ⇒ 不选。 */
    fun pickModel(value: String?) =
        _state.update { s ->
            val m = s.models.firstOrNull { it.value == value }
            val efforts = m?.efforts.orEmpty()
            val def = efforts.firstOrNull { it.value == m?.defaultEffort } ?: efforts.firstOrNull()
            s.copy(model = value, effort = def?.value)
        }

    /** 选推理档：只认选的模型列着的那几档。 */
    fun pickEffort(value: String) = _state.update { s -> if (s.efforts.any { it.value == value }) s.copy(effort = value) else s }

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
            val got = (link.call(SessionOpenDir.COMMAND, SessionOpenDir.args(cwd), READ_MS) as? Reply.Ok)?.let { SessionOpenDir.of(it.data) }
            _state.update { s ->
                val shown = s.recent.firstOrNull { it.cwd == cwd }?.shown ?: got?.shown ?: cwd
                when {
                    // 问不到 ⇒ 照选：起的时候那台再核（`no_dir` 那一句）。
                    got == null -> s.copy(cwd = cwd, cwdShown = shown)
                    got.said == null -> s.copy(cwd = cwd, cwdShown = shown, recent = s.recent.map { if (it.cwd == cwd) it.copy(said = null) else it })
                    else -> s.copy(recent = s.recent.map { if (it.cwd == cwd) it.copy(said = got.said) else it })
                }
            }
            done(got?.said == null)
        }
    }

    /** 浏览屏里选的（那台列出来的，在；屏上那一串照那一行）。 */
    fun pickDir(picked: Picked) = _state.update { it.copy(cwd = picked.path, cwdShown = picked.shown) }

    /** 设为这台的新会话默认号（账号库里那一格，桌面同一份）；之后再问一次账号。 */
    fun setDefault(name: String) {
        scope.launch {
            val r = link.call(AccountsSetDefault.COMMAND, AccountsSetDefault.args(name), WRITE_MS)
            if (r !is Reply.Ok) {
                val why = r.problem(machine, AccountsSetDefault.COMMAND, link.offline.value)?.text
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
        val model = s.models.firstOrNull { it.value == s.model }?.value
        val effort = s.effort?.takeIf { e -> s.efforts.any { it.value == e } }
        return Chosen(s.cwd, s.agent.orEmpty(), account, rotation, model, effort)
    }

    private fun a(
        list: List<AccountRow>,
        name: String,
    ) = list.firstOrNull { it.name == name }

    private fun failed(
        r: Reply,
        command: String,
    ): String = (r.problem(machine, command, link.offline.value) ?: unreadableReply(machine, command, r.toString())).text

    companion object {
        private const val READ_MS = 15_000L
        private const val WRITE_MS = 20_000L
    }
}
