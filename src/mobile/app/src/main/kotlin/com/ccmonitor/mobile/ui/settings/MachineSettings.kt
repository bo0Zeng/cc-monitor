package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.core.claude.link.AccountsList
import com.ccmonitor.mobile.core.claude.link.AccountsSetDefault
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.MachinePrefs
import com.ccmonitor.mobile.core.claude.link.QuotaRead
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionOpenFacts
import com.ccmonitor.mobile.core.claude.link.TabKind
import com.ccmonitor.mobile.core.claude.link.seen
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.gen.AccountRow
import com.ccmonitor.mobile.core.wire.gen.AgentFacts
import com.ccmonitor.mobile.core.wire.gen.Changed
import com.ccmonitor.mobile.core.wire.gen.PrefsReply
import com.ccmonitor.mobile.core.wire.gen.Tab
import com.ccmonitor.mobile.core.wire.gen.Usage
import com.ccmonitor.mobile.link.Fetched
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
import com.ccmonitor.mobile.core.wire.gen.QuotaRead as QuotaReadReply

/** 设置屏 · 账号一览此刻画的那几样（全是那台的回包；读到之前是空的，不画旧值）。 */
data class MachineSettingsUi(
    val accounts: List<AccountRow> = emptyList(),
    /** 这台的新会话默认号（那台判的）。 */
    val defaultAccount: String? = null,
    val accountsFailed: Problem? = null,
    /** 号列表答过一次了（[accounts] 是那台答的，空 ＝ 真没有号）；没答过 ⇒ `false`（读中）。 */
    val accountsRead: Boolean = false,
    /** 那台的额度账（用量屏整份画；账号卡 · 账号一览按号名取用量那一格）。 */
    val quota: Fetched<QuotaReadReply> = Fetched.Loading,
    /** 手上有一份、再读没成的那一句（那份照画，顶上说这一句）；没有 ⇒ `null`。 */
    val quotaRereadFailed: Problem? = null,
    /** 读回的那一份偏好；还没读到 ⇒ `null`。 */
    val prefs: PrefsReply? = null,
    val prefsFailed: Problem? = null,
    /** 拨了还没读回的那一格（画拨到的值）。 */
    val pending: Map<TabKind, Tab> = emptyMap(),
    /** 没存上的那一行下面那一句。 */
    val notSaved: Map<TabKind, String> = emptyMap(),
    /** 默认那一家（模型清单 · 「跟随」那一句的主语）；还没问 ⇒ `null`。 */
    val agent: AgentFacts? = null,
    /** 设为默认 · 改模型没成那一句（账号一览顶上）。 */
    val note: String? = null,
) {
    /** 那一行此刻画哪一段：拨了没读回 ⇒ 拨到的；读回了 ⇒ 那台答的；没读到 ⇒ `null`（不画选中）。 */
    fun shown(kind: TabKind): Tab? =
        pending[kind] ?: prefs?.prefs?.let {
            when (kind) {
                TabKind.CHILD -> it.childTab
                TabKind.BG -> it.bgTab
            }
        }

    /** 这个号用量那一格（出过数的与没出过数的同一种）；还没读到 · 读不成 · 账上没有这个号 ⇒ `null`。 */
    fun usage(account: String): Usage? =
        (quota as? Fetched.Got)?.value?.let { q ->
            q.accounts.firstOrNull { it.account == account }?.usage
                ?: q.unseen
                    .firstOrNull { it.account == account }
                    ?.usage
                    ?.let { Usage(it.slot, it.window, it.value, null, it.text, it.tone) }
        }

    /** 这个号的默认模型（那台存的原值）；没设 ⇒ `null`（跟随那一家的默认）。 */
    fun model(account: String): String? = prefs?.prefs?.models?.get(account)
}

/**
 * 设置屏与账号一览读写那台的那几份：接上的每一回（核心那一格 [seen]；进来时已通着 ⇒ 当场一回）各读一次 `accounts-list` · `quota-read` · `prefs-read`；
 * 流上 `quota` 变了换上它带的那份（没带 ⇒ 重读用量）、`accounts` 变了重读账号。偏好拨了马上 `prefs-set`、照回包画；没存上退回读到的值、那一行下面说一句。
 * 判（默认号 · 能不能设为默认 · 用量的字与语气 · 偏好的值）全在那台，这里只按号名对上两份回包。
 */
class MachineSettings(
    private val link: CoreLink,
    private val machine: String,
    changes: Flow<Changed>,
    private val scope: CoroutineScope,
) {
    private val _state = MutableStateFlow(MachineSettingsUi())
    val state: StateFlow<MachineSettingsUi> = _state.asStateFlow()

    init {
        scope.launch { link.seen.collect { load() } }
        scope.launch {
            changes.collect { c ->
                when (c) {
                    is Changed.Accounts -> loadAccounts()
                    is Changed.Quota -> c.body?.let(::gotQuota) ?: loadQuota()
                    else -> Unit
                }
            }
        }
    }

    private fun load() {
        scope.launch { loadAccounts() }
        scope.launch { loadQuota() }
        scope.launch { loadPrefs() }
    }

    /** 号列表再问一次（账号屏读不出那一条的［重试］）。 */
    fun reloadAccounts() {
        scope.launch { loadAccounts() }
    }

    private suspend fun loadAccounts() {
        val r = link.call(AccountsList.COMMAND, AccountsList.args(""), READ_MS)
        val list = (r as? Reply.Ok)?.let { AccountsList.of(it.data) }
        _state.update {
            if (list == null) {
                it.copy(accountsFailed = failed(r, AccountsList.COMMAND))
            } else {
                it.copy(accounts = list.accounts, defaultAccount = list.meta.effectiveDefault, accountsFailed = null, accountsRead = true)
            }
        }
    }

    /** 读成了 ⇒ 换成新的那份；读不成 ⇒ 手上有一份就照留（顶上说一句），没有才记成读不成。 */
    private suspend fun loadQuota() {
        val r = link.call(QuotaRead.COMMAND, null, READ_MS)
        val q = (r as? Reply.Ok)?.let { QuotaRead.of(it.data) } ?: return failQuota(r)
        gotQuota(q)
    }

    /** 那台的一份额度账（读回的，或推来的 `changed {quota}` 带的体）：整份换上。 */
    private fun gotQuota(q: QuotaReadReply) = _state.update { it.copy(quota = Fetched.Got(q), quotaRereadFailed = null) }

    private fun failQuota(r: Reply) =
        _state.update {
            if (it.quota is Fetched.Got) {
                it.copy(quotaRereadFailed = failed(r, QuotaRead.COMMAND))
            } else {
                it.copy(quota = Fetched.Failed(failed(r, QuotaRead.COMMAND)))
            }
        }

    /** 用量屏［重试］。 */
    fun reloadQuota() {
        scope.launch { loadQuota() }
    }

    private suspend fun loadPrefs() {
        val r = link.call(MachinePrefs.READ, null, READ_MS)
        val p = (r as? Reply.Ok)?.let { MachinePrefs.of(it.data) }
        _state.update { if (p == null) it.copy(prefsFailed = failed(r, MachinePrefs.READ)) else it.copy(prefs = p, prefsFailed = null) }
    }

    /** 拨一行：先画拨到的，交那台；读回 ⇒ 照回包；没存上 ⇒ 退回读到的值、那一行下面一句。 */
    fun setTab(
        kind: TabKind,
        value: Tab,
    ) {
        if (_state.value.shown(kind) == value) return
        _state.update { it.copy(pending = it.pending + (kind to value), notSaved = it.notSaved - kind) }
        scope.launch {
            val r = link.call(MachinePrefs.SET, MachinePrefs.tab(kind, value), WRITE_MS)
            val p = (r as? Reply.Ok)?.let { MachinePrefs.of(it.data) }
            _state.update { s ->
                if (p != null) {
                    s.copy(prefs = p, pending = s.pending - kind)
                } else {
                    val said = failed(r, MachinePrefs.SET).text
                    s.copy(pending = s.pending - kind, notSaved = s.notSaved + (kind to copyText("settingsPanel.save.notSaved", "what" to tabLabel(kind), "said" to said)))
                }
            }
        }
    }

    /** 设为这台的新会话默认号；之后再问一次账号。 */
    fun setDefault(name: String) {
        scope.launch {
            val r = link.call(AccountsSetDefault.COMMAND, AccountsSetDefault.args(name), WRITE_MS)
            if (r !is Reply.Ok) {
                note(copyText("acctPage.default.failed", "name" to name), r, AccountsSetDefault.COMMAND)
                return@launch
            }
            _state.update { it.copy(note = null) }
            loadAccounts()
        }
    }

    /** 默认模型那一层抽屉要的清单：问一次那台能起的几家，取默认那一家。 */
    fun loadModels() {
        if (_state.value.agent != null) return
        scope.launch {
            val facts = (link.call(SessionOpenFacts.COMMAND, emptyMap(), READ_MS) as? Reply.Ok)?.let { SessionOpenFacts.of(it.data) } ?: return@launch
            _state.update { it.copy(agent = SessionOpenFacts.pick(facts.agents, null)) }
        }
    }

    /** 一个号的默认模型；[model] 为 `null` ⇒ 跟随那一家的默认。写完照回包画。 */
    fun setModel(
        account: String,
        model: String?,
    ) {
        scope.launch {
            val r = link.call(MachinePrefs.SET, MachinePrefs.model(account, model), WRITE_MS)
            val p = (r as? Reply.Ok)?.let { MachinePrefs.of(it.data) }
            if (p == null) {
                note(copyText("settingsPanel.save.notSaved", "what" to copyText("acctPage.detail.model"), "said" to failed(r, MachinePrefs.SET).text), null, null)
                return@launch
            }
            _state.update { it.copy(prefs = p, note = null) }
        }
    }

    private fun note(
        head: String,
        r: Reply?,
        command: String?,
    ) {
        val why = if (r != null && command != null) r.problem(machine, command, link.offline.value)?.text else null
        _state.update { it.copy(note = listOfNotNull(head, why).joinToString(copyText("kit.text.sep"))) }
    }

    private fun failed(
        r: Reply,
        command: String,
    ): Problem = r.problem(machine, command, link.offline.value) ?: unreadableReply(machine, command, r.toString())

    companion object {
        private const val READ_MS = 15_000L
        private const val WRITE_MS = 20_000L

        /** 两行的名字（桌面「这台上的 cc-monitor」那两格同一份字）。 */
        fun tabLabel(kind: TabKind): String =
            when (kind) {
                TabKind.CHILD -> copyText("settings.machine.childTab")
                TabKind.BG -> copyText("settings.machine.bgTab")
            }

        /** 两行名字下面那一句（桌面悬停那一句；手机没有悬停）。 */
        fun tabTip(kind: TabKind): String =
            when (kind) {
                TabKind.CHILD -> copyText("settings.machine.childTabTip")
                TabKind.BG -> copyText("settings.machine.bgTabTip")
            }
    }
}
