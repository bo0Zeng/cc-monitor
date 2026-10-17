package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.HistoryFacts
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionModel
import com.ccmonitor.mobile.core.claude.link.SessionOpenFacts
import com.ccmonitor.mobile.core.claude.link.SessionPermission
import com.ccmonitor.mobile.core.claude.link.blocked
import com.ccmonitor.mobile.core.claude.link.wire.AgentPick
import com.ccmonitor.mobile.core.claude.link.wire.ModeSwitchResult
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.launch

/**
 * 一条会话输入框里那两颗 chip（模型 · 权限档）：字（读回的 · 那台从记录里读的）· 能不能换（那台判的、离线照连接那一层）· 换 · 抽屉里列什么。
 * 状态写进 [ChatSession] 的那一份 [state]；[sid] 给此刻对着的那条会话。
 */
internal class ChatChips(
    private val link: CoreLink,
    private val machine: String,
    private val scope: CoroutineScope,
    private val state: MutableStateFlow<ChatUiState>,
    private val sid: () -> String?,
) {
    /**
     * 换模型：那台送命令、从记录里读回换成了什么（`session-model`）。读回 `done` 才换字；没读回（`unsure` · 超时）· 被拒 ⇒ 回原字 ＋ chip 上方那一句。
     * 已经在换（模型或权限档）⇒ 不再起一趟。
     */
    fun switchModel(pick: AgentPick) =
        switching(pick, { it.model }, { st, c -> st.copy(model = c) }) { s ->
            val r = link.call(SessionModel.COMMAND, SessionModel.args(s, pick.value), SWITCH_MS)
            when {
                r is Reply.Ok ->
                    SessionModel.of(r.data)?.let { Switched(it.result, it.model, it.said) }
                        ?: Switched.failed(unreadableReply(machine, SessionModel.COMMAND, r.data.toString()).text)
                r == Reply.TimedOut -> Switched.failed(copyText("session.model.unsure"))
                else -> Switched.failed(r.problem(machine, SessionModel.COMMAND)?.text)
            }.orUnsure(copyText("session.model.unsure"))
        }

    /** 切权限档（`session-permission`）：同 [switchModel]，读回的字是那一档写给人看的字。 */
    fun switchPermission(pick: AgentPick) =
        switching(pick, { it.permission }, { st, c -> st.copy(permission = c) }) { s ->
            val r = link.call(SessionPermission.COMMAND, SessionPermission.args(s, pick.value), SWITCH_MS)
            when {
                r is Reply.Ok ->
                    SessionPermission.of(r.data)?.let { Switched(it.result, it.modeText, it.said) }
                        ?: Switched.failed(unreadableReply(machine, SessionPermission.COMMAND, r.data.toString()).text)
                r == Reply.TimedOut -> Switched.failed(copyText("session.permission.unsure"))
                else -> Switched.failed(r.problem(machine, SessionPermission.COMMAND)?.text)
            }.orUnsure(copyText("session.permission.unsure"))
        }

    /** 一趟换的结局：[text] 读回的字，[said] 那台那一句。 */
    private data class Switched(
        val result: ModeSwitchResult,
        val text: String?,
        val said: String?,
    ) {
        /** 没成却没那一句 ⇒ 补上「结果未知」那一句（unsure 时那台多半带着自己的那一句）。 */
        fun orUnsure(unsure: String): Switched = if (result != ModeSwitchResult.DONE && said == null) copy(said = unsure) else this

        companion object {
            fun failed(said: String?) = Switched(ModeSwitchResult.UNSURE, null, said)
        }
    }

    private fun switching(
        pick: AgentPick,
        chipOf: (ChatUiState) -> SwitchChip,
        put: (ChatUiState, SwitchChip) -> ChatUiState,
        ask: suspend (String) -> Switched,
    ) {
        val s = sid() ?: return
        val st = state.value
        if (st.model.pending != null || st.permission.pending != null) return
        state.value = put(st, chipOf(st).copy(pending = pick)).copy(note = null)
        scope.launch {
            val r = ask(s)
            val now = state.value
            val was = chipOf(now).copy(pending = null)
            val chip = if (r.result == ModeSwitchResult.DONE && r.text != null) was.copy(text = r.text, value = pick.value) else was
            state.value = put(now, chip).copy(note = if (r.result == ModeSwitchResult.DONE) r.said.takeIf { r.text == null } else r.said)
        }
    }

    /** 抽屉里列什么：那台给这条会话那一家的模型与权限档（会话表里认那一家；认不出 ⇒ 那台列的第一家）。问过一次就不再问。 */
    fun loadPicks(agent: String?) {
        if (state.value.picks?.error == null && state.value.picks != null) return
        scope.launch {
            val r = link.call(SessionOpenFacts.COMMAND, emptyMap(), READ_MS)
            val picks =
                when (r) {
                    is Reply.Ok ->
                        SessionOpenFacts.of(r.data)?.let { all -> SessionOpenFacts.pick(all.agents, agent).let { Picks(it?.models.orEmpty(), it?.permissionModes.orEmpty(), it?.modelDefault, it?.modelNote) } }
                            ?: Picks(error = unreadableReply(machine, SessionOpenFacts.COMMAND, r.data.toString()).text)
                    else -> Picks(error = r.problem(machine, SessionOpenFacts.COMMAND)?.text.orEmpty())
                }
            state.value = state.value.copy(picks = picks)
        }
    }

    /**
     * 两颗 chip 此刻能不能换：照那台给的这条会话的脸（`can.model` · `can.permission`）；那台离线 ⇒ 两颗都灰、写离线那一句。
     * 流上还没有它的脸 ⇒ 不灰（由两条命令自己照实回）。
     */
    fun CoroutineScope.watch(s: String) {
        launch {
            combine(link.state, link.table) { st, t -> st to t }.collect { (st, t) ->
                val (model, permission) =
                    if (st is LinkState.Down) {
                        copyText("newSession.machine.offline", "machine" to machine).let { it to it }
                    } else {
                        val can = t.faces[s]?.can
                        can?.model?.blocked() to can?.permission?.blocked()
                    }
                val now = state.value
                if (now.model.blocked != model || now.permission.blocked != permission) {
                    state.value = now.copy(model = now.model.copy(blocked = model), permission = now.permission.copy(blocked = permission))
                }
            }
        }
    }

    /** chip 上的两个字（那台从记录里读的此刻的模型 · 权限档）与模型抽屉勾哪一项（`modelPick`）：还没换过才用它；问不到就算了，chip 写这颗是什么。 */
    suspend fun readFacts(p: String) {
        val f = (link.call(HistoryFacts.COMMAND, HistoryFacts.args(p), READ_MS) as? Reply.Ok)?.let { HistoryFacts.of(it.data) } ?: return
        val now = state.value
        state.value =
            now.copy(
                model = now.model.copy(text = now.model.text ?: f.modelText, value = now.model.value ?: f.modelPick),
                permission = if (now.permission.text == null) now.permission.copy(text = f.permissionModeText) else now.permission,
            )
    }

    private companion object {
        /** 换模型 · 切档等多久读回（那台送了命令要等那一家重画 / 写记录）；到点没读回 ⇒ 回原字、说结果未知。 */
        const val SWITCH_MS = 20_000L
        const val READ_MS = 15_000L
    }
}
