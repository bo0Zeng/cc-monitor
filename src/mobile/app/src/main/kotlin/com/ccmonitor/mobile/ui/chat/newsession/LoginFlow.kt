package com.ccmonitor.mobile.ui.chat.newsession

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Login
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.wire.LoginAgain
import com.ccmonitor.mobile.core.claude.link.wire.LoginAgainHow
import com.ccmonitor.mobile.core.claude.link.wire.LoginStarted
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update
import kotlinx.coroutines.launch

/** 登录抽屉此刻。 */
data class LoginUi(
    /** 登录的是哪个号（目标行）。 */
    val account: String,
    /** `login-start` 交回来的那一份；还在起 · 没起成 ⇒ `null`。 */
    val started: LoginStarted? = null,
    /** 没起成 · 作业没了那一句（那台写的）；有它 ⇒ 粘贴区收起。 */
    val failed: String? = null,
    /** 那之后还能不能重来（那台给的 `again`）：能 ⇒［重新开始］；不能 ⇒ 只剩［关闭］。 */
    val canRestart: Boolean = false,
    /** 粘贴那一格下面那一行（那台写的）：作业还在，改了再提交。 */
    val pasteError: String? = null,
    val submitting: Boolean = false,
    /** 登成了 ⇒ 抽屉关。 */
    val done: Boolean = false,
)

/**
 * 在 app 里登录一个号：`login-start` 起那一家的登录、交回链接 ⇒ 系统浏览器开；粘回来的那一段经 `login-finish` 交给它；
 * 取消 · 收起 ⇒ `login-cancel`。令牌由那一家自己换、自己写，手机只转链接与那一段。
 * 失败之后怎么办照那台的 `again`（[LoginAgain]）：接着粘 ⇒ 那一格下面一行、改了再交；重新开始 ⇒ 红字 ＋［重新开始］；没法再来 ⇒ 只红字。
 * 没起成就没有作业，接着粘无从谈起 ⇒ 当重新开始；回包里没有 `again`（超时 · 断线）⇒ 也当重新开始（同桌面）。
 */
class LoginFlow(
    private val link: CoreLink,
    private val machine: String,
    private val agent: String,
    account: String,
    private val scope: CoroutineScope,
) {
    private val _state = MutableStateFlow(LoginUi(account))
    val state: StateFlow<LoginUi> = _state.asStateFlow()

    fun start() {
        _state.update { LoginUi(it.account) }
        scope.launch {
            val r = link.call(Login.START, Login.startAccount(agent, _state.value.account), CALL_MS)
            val s = (r as? Reply.Ok)?.let { Login.started(it.data) }
            _state.update { if (s != null) it.copy(started = s) else it.copy(failed = said(r, Login.START), canRestart = again(r) != LoginAgainHow.NONE) }
        }
    }

    fun submit(pasted: String) {
        val job = _state.value.started?.job ?: return
        if (pasted.isBlank() || _state.value.submitting) return
        _state.update { it.copy(submitting = true, pasteError = null) }
        scope.launch {
            val r = link.call(Login.FINISH, Login.finish(job, pasted.trim()), FINISH_MS)
            _state.update {
                when {
                    r is Reply.Ok -> it.copy(submitting = false, done = true)
                    again(r) == LoginAgainHow.PASTE -> it.copy(submitting = false, pasteError = said(r, Login.FINISH))
                    else -> it.copy(started = null, submitting = false, failed = said(r, Login.FINISH), canRestart = again(r) != LoginAgainHow.NONE)
                }
            }
        }
    }

    /** 收起（取消 · ✕ · 下滑 · 点遮罩）：作业还在 ⇒ 交 `login-cancel`；不确认（再点［登录…］就是）。 */
    fun cancel() {
        val job = _state.value.started?.job ?: return
        if (_state.value.done) return
        scope.launch { link.call(Login.CANCEL, Login.cancel(job), CALL_MS) }
    }

    /** 失败回包里那台说的「还能怎么办」；没说 ⇒ 重新开始。 */
    private fun again(r: Reply): LoginAgainHow = (r as? Reply.Failed)?.failure?.data?.let { Wire.bind<LoginAgain>(it) }?.again ?: LoginAgainHow.RESTART

    private fun said(
        r: Reply,
        command: String,
    ): String = (r.problem(machine, command) ?: unreadableReply(machine, command, r.toString())).text

    private companion object {
        const val CALL_MS = 20_000L

        /** 交了那一段、等那一家换完（`login-finish` 阻塞到换完）。 */
        const val FINISH_MS = 60_000L
    }
}
