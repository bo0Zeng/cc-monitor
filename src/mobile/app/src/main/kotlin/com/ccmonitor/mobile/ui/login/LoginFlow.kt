package com.ccmonitor.mobile.ui.login

import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Login
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.LoginAgain
import com.ccmonitor.mobile.core.wire.gen.LoginAgainHow
import com.ccmonitor.mobile.core.wire.gen.LoginStartReq
import com.ccmonitor.mobile.core.wire.gen.LoginStarted
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
    /** 登录什么（`login-start` 的入参原样：号 · 号里的一个 MCP 服务器）。 */
    val target: LoginStartReq,
    /** `login-start` 交回来的那一份；还在起 · 没起成 ⇒ `null`。 */
    val started: LoginStarted? = null,
    /** 没起成 · 作业没了那一句（那台写的）；有它 ⇒ 粘贴区收起。 */
    val failed: String? = null,
    /** 那之后还能不能重来（那台给的 `again`）：能 ⇒［重新开始］；不能 ⇒ 只剩［关闭］。 */
    val canRestart: Boolean = false,
    /** 没起成 · 交了没成、那台说在终端里登录这条路在（失败回包的 `terminal`）⇒ 多一颗［在终端里登录］；回包里没有那一份 ⇒ 不给。 */
    val terminal: Boolean = false,
    /** 粘贴那一格下面那一行（那台写的）：作业还在，改了再提交。 */
    val pasteError: String? = null,
    val submitting: Boolean = false,
    /** 登成了 ⇒ 抽屉关。 */
    val done: Boolean = false,
)

/**
 * 在 app 里登录一个号 · 号里的一个 MCP 服务器（三处入口共用：新建会话 · 设置账号 · 总览）：目标是 `login-start` 的入参原样
 * （总览那一项的 `fix.args`，或新建会话 / 设置账号那边照同一形拼的 `Login.account`），起那一家的登录、交回链接 ⇒ 系统浏览器开；粘回来的那一段经 `login-finish` 交给它；
 * 取消 · 收起 ⇒ `login-cancel`。令牌由那一家自己换、自己写，手机只转链接与那一段。
 * 失败之后怎么办照那台的 `again`（[LoginAgain]）：接着粘 ⇒ 那一格下面一行、改了再交；重新开始 ⇒ 红字 ＋［重新开始］；没法再来 ⇒ 只红字。
 * 没起成就没有作业，接着粘无从谈起 ⇒ 当重新开始；回包里没有 `again`（超时 · 断线）⇒ 也当重新开始（同桌面）。
 * 那台说终端那条路在（`terminal`）⇒ 还能在那台新开一个终端 tab 敲登录那一行（[terminalLine]）。
 */
class LoginFlow(
    private val link: CoreLink,
    /** 哪一台（hostId）：［在终端里登录］在这台开终端 tab。 */
    val host: String,
    private val machine: String,
    target: LoginStartReq,
    private val scope: CoroutineScope,
) {
    private val _state = MutableStateFlow(LoginUi(target))
    val state: StateFlow<LoginUi> = _state.asStateFlow()

    fun start() {
        _state.update { LoginUi(it.target) }
        scope.launch {
            val r = link.call(Login.START, Login.start(_state.value.target), CALL_MS)
            val s = (r as? Reply.Ok)?.let { Login.started(it.data) }
            _state.update { if (s != null) it.copy(started = s) else it.copy(failed = said(r, Login.START), canRestart = again(r) != LoginAgainHow.NONE, terminal = terminal(r)) }
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
                    else -> it.copy(started = null, submitting = false, failed = said(r, Login.FINISH), canRestart = again(r) != LoginAgainHow.NONE, terminal = terminal(r))
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

    /**
     * ［在终端里登录］：问那台在终端里登录这个号的那一行（`accounts-login-cmd`，桌面同一条）。
     * 没成 ⇒ 红字换成那台那一句，那一颗留着（再点再问）。那台只对号那一种目标说这条路在，号那一种 `account` 恒有。
     */
    suspend fun terminalLine(): Result<String> {
        val account = _state.value.target.account ?: return Result.failure(IllegalStateException(Login.CMD))
        val r = link.call(Login.CMD, Login.cmd(account), CALL_MS)
        val line = (r as? Reply.Ok)?.let { Login.cmdOf(it.data) }
        if (line != null) return Result.success(line)
        val said = (r as? Reply.Ok)?.let { unreadableReply(machine, Login.CMD, it.data.toString()).text } ?: said(r, Login.CMD)
        _state.update { it.copy(failed = said) }
        return Result.failure(IllegalStateException(said))
    }

    /** 失败回包里那台说的「还能怎么办」；没说 ⇒ 重新开始。 */
    private fun again(r: Reply): LoginAgainHow = how(r)?.again ?: LoginAgainHow.RESTART

    /** 失败回包里那台说的终端那条路在不在；没说 ⇒ 不在。 */
    private fun terminal(r: Reply): Boolean = how(r)?.terminal ?: false

    private fun how(r: Reply): LoginAgain? = (r as? Reply.Failed)?.failure?.data?.let { Wire.bind<LoginAgain>(it) }

    private fun said(
        r: Reply,
        command: String,
    ): String = (r.problem(machine, command, link.offline.value) ?: unreadableReply(machine, command, r.toString())).text

    private companion object {
        const val CALL_MS = 20_000L

        /** 交了那一段、等那一家换完（`login-finish` 阻塞到换完）。 */
        const val FINISH_MS = 60_000L
    }
}
