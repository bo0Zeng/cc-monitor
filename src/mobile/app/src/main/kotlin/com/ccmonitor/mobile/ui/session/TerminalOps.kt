package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.claude.link.AttachLine
import com.ccmonitor.mobile.core.claude.link.BackendFeed
import com.ccmonitor.mobile.core.claude.link.Kill
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionNew
import com.ccmonitor.mobile.core.claude.link.TerminalInput
import com.ccmonitor.mobile.core.claude.link.TerminalPreview
import com.ccmonitor.mobile.core.claude.link.TerminalsList
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.feedback.Notice
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import java.util.UUID

/**
 * 终端屏里要那台核心做的几件（列 · 结束 · 后台起 · 送字 · 抓屏 · 进终端那一行），都经这台的常驻流 [feed]。
 * 找哪个窗格、怎么附着、带不带 `-u` 全在那台；这里只把回话翻成屏上那一句。失败的那一句是那台写好的（[problem]）。
 * [machine] 是这台在手机上的名字（走不通时那一句里用）。
 */
class TerminalOps(
    private val feed: BackendFeed,
    private val machine: suspend () -> String,
) {
    /** 这台的终端名单。 */
    suspend fun list(): Result<List<TerminalsList.Row>> = ask(TerminalsList.COMMAND, TerminalsList.args(), TerminalsList::of)

    /** 名叫 [name] 的那个终端的句柄；名单里没有 ⇒ 失败。 */
    private suspend fun handleOf(name: String): Result<String> =
        list().mapCatching { rows -> TerminalsList.handleOf(rows, name) ?: throw Problem(copyText("sessionHead.attach.none", "machine" to machine())) }

    /** 结束 [name]，回屏上那一句（成败不同；被拒 ⇒ 那台写好的那一句原样）。 */
    suspend fun kill(name: String): Notice =
        ask(Kill.COMMAND, Kill.args(name), Kill::of)
            .fold(
                onSuccess = { killed ->
                    Notice(
                        if (killed) copyText("mobTmux.kill.done", "name" to name) else copyText("tmuxControl.kill.notConfirmed", "machine" to machine(), "target" to name),
                    )
                },
                onFailure = { Notice(it.message.orEmpty()) },
            )

    /** 在名叫 [name] 的会话里后台起 Claude（不进入）。失败 ⇒ 那一句。 */
    suspend fun startClaude(name: String): String? =
        ask(SessionNew.COMMAND, SessionNew.inTmux(agent = "claude", name = name, ticket = UUID.randomUUID().toString()), SessionNew::of, blocking = true)
            .exceptionOrNull()
            ?.message

    /** 往名叫 [name] 的那个终端里送一句（末尾补回车）。送到了 ⇒ `null`；否则那一句。 */
    suspend fun typeInto(
        name: String,
        text: String,
    ): String? =
        handleOf(name)
            .mapCatching { h -> ask(TerminalInput.COMMAND, TerminalInput.textInto(h, text), TerminalInput::of).getOrThrow() }
            .fold(
                onSuccess = { r ->
                    when (r) {
                        TerminalInput.Result.Delivered -> null
                        TerminalInput.Result.Unsure -> copyText("terminal.input.unsure", "machine" to machine())
                        is TerminalInput.Result.Refused -> r.said
                    }
                },
                onFailure = { it.message },
            )

    /** 抓名叫 [name] 的那个终端的一屏（纯文字）。 */
    suspend fun capture(name: String): Result<String> =
        handleOf(name).mapCatching { h -> ask(TerminalPreview.COMMAND, TerminalPreview.args(h), TerminalPreview::of).getOrThrow() }

    /** 进 [name] 那个会话要在终端里敲的那一行（那台渲的，与桌面「命令已复制」同一条）。 */
    suspend fun attachLine(
        name: String,
        agent: String,
        defaultLauncher: String,
    ): Result<String> = ask(AttachLine.COMMAND, AttachLine.args(agent, defaultLauncher, name), AttachLine::of)

    private suspend fun <T> ask(
        command: String,
        args: Map<String, Any?>,
        decode: (Any?) -> T?,
        blocking: Boolean = false,
    ): Result<T> {
        val m = machine()
        return when (val r = feed.call(command, args, blocking = blocking)) {
            is Reply.Ok -> decode(r.data)?.let { Result.success(it) } ?: Result.failure(Problem(unreadableReply(m, command, r.data.toString()).text))
            else -> Result.failure(Problem(r.problem(m, command)?.text.orEmpty()))
        }
    }

    /** 屏上那一句（[message]）。 */
    private class Problem(
        said: String,
    ) : Exception(said)
}
