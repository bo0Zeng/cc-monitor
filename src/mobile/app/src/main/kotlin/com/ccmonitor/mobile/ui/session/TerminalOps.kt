package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.claude.link.AttachLine
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Kill
import com.ccmonitor.mobile.core.claude.link.Profiles
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionModel
import com.ccmonitor.mobile.core.claude.link.SessionNew
import com.ccmonitor.mobile.core.claude.link.SessionNewFacts
import com.ccmonitor.mobile.core.claude.link.TerminalInput
import com.ccmonitor.mobile.core.claude.link.TerminalPreview
import com.ccmonitor.mobile.core.claude.link.TerminalsList
import com.ccmonitor.mobile.core.claude.link.wire.AgentFacts
import com.ccmonitor.mobile.core.claude.link.wire.ModeSwitchResult
import com.ccmonitor.mobile.core.claude.link.wire.NewSessionPick
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.MovedLaunch
import com.ccmonitor.mobile.core.ui.feedback.Notice
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import java.util.UUID

/**
 * 终端屏里要那台核心做的几件（列 · 结束 · 后台起 · 送字 · 抓屏 · 进终端那一行 · 别名），都经这台的常驻流 [feed]。
 * 找哪个窗格、怎么附着、带不带 `-u` 全在那台；这里只把回话翻成屏上那一句。失败的那一句是那台写好的（[problem]）。
 * [machine] 是这台在手机上的名字（走不通时那一句里用）；[stored] 是这台存的那一家（线上 kind，主机设置里选的；没选 ⇒ `null`）。
 * 哪几家、叫什么、能换哪些模型都问那台（`session-new-facts`），手机一家都不认识。
 */
class TerminalOps(
    private val feed: CoreLink,
    private val machine: suspend () -> String,
    private val stored: suspend () -> String? = { null },
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

    /** 这台能起的几家（那台的 `session-new-facts`：名字 · 可选的模型都是那台给的）。 */
    suspend fun agents(): Result<List<AgentFacts>> = ask(SessionNewFacts.COMMAND, emptyMap(), SessionNewFacts::of)

    /** 这台起会话用哪一家：存的那一家 · 那台列的第一家（线上 kind）。问不到 ⇒ 失败（那一句）。 */
    private suspend fun agent(): Result<String> =
        agents().mapCatching { SessionNewFacts.pick(it, stored())?.kind ?: throw Problem(copyText("peerVersion.said.unreadable", "machine" to machine())) }

    /** 在名叫 [name] 的会话里后台起这台那一家（不进入）。失败 ⇒ 那一句。 */
    suspend fun startAgent(name: String): String? =
        agent()
            .mapCatching { a -> ask(SessionNew.COMMAND, SessionNew.inTmux(agent = a, name = name, ticket = UUID.randomUUID().toString()), SessionNew::of).getOrThrow() }
            .exceptionOrNull()
            ?.message

    /** 名叫 [name] 的那个终端里跑着的那一家能换的模型（那台给的）；里面没跑会话 ⇒ 空。 */
    suspend fun modelsIn(name: String): List<NewSessionPick> {
        val row = list().getOrNull()?.firstOrNull { it.name == name } ?: return emptyList()
        if (row.sid == null) return emptyList()
        val agents = agents().getOrNull() ?: return emptyList()
        return SessionNewFacts.pick(agents, row.agent)?.models.orEmpty()
    }

    /** 给名叫 [name] 的那个终端里跑着的会话换模型（`session-model`）。换成了 ⇒ `Ok(那一句)`；没成 ⇒ 失败（那一句）。 */
    suspend fun switchModel(
        name: String,
        model: String,
    ): Result<String> =
        list().mapCatching { rows ->
            val sid = rows.firstOrNull { it.name == name }?.sid ?: throw Problem(copyText("sessionHead.attach.none", "machine" to machine()))
            val r = ask(SessionModel.COMMAND, SessionModel.args(sid, model), SessionModel::of).getOrThrow()
            if (r.result != ModeSwitchResult.DONE) throw Problem(r.said.orEmpty())
            r.said.orEmpty()
        }

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

    /**
     * 进 [name] 那个会话要在终端里敲的那一行（那台渲的，与桌面「命令已复制」同一条）。手机只交意图：哪一家取那个终端里跑着的那一家，
     * 没跑会话 ⇒ 这台起会话用的那一家。
     */
    suspend fun attachLine(name: String): Result<String> {
        val running = list().getOrNull()?.firstOrNull { it.name == name }?.agent
        return (running?.let { Result.success(it) } ?: agent()).mapCatching { a -> ask(AttachLine.COMMAND, AttachLine.args(a, name), AttachLine::of).getOrThrow() }
    }

    /** 这台能用的别名（那台的 `profiles-read`，名字与顺序照那台）：终端屏「用哪个启动」那一排。 */
    suspend fun aliases(): Result<List<Profiles.Alias>> = ask(Profiles.READ, emptyMap(), Profiles::of).map { it.usable }

    /**
     * 在这台建一段别名（别名页同一条写命令 `profiles-write`，指纹取刚读回的那一份）。建好 ⇒ 那台要说的「已开的终端要重读」那一句
     * （没有 ⇒ `null`）；被拒 ⇒ 失败（那台写好的那一句）。
     */
    suspend fun makeAlias(m: MovedLaunch): Result<String?> =
        ask(Profiles.FROM_LINE, Profiles.fromLineArgs(m.label.trim(), m.command.orEmpty(), m.workingDir, m.tmuxSession), Profiles::formOf).mapCatching { form ->
            val book = ask(Profiles.READ, emptyMap(), Profiles::of).getOrThrow()
            ask(Profiles.WRITE, Profiles.createArgs(book.fingerprint, form), Profiles::written).getOrThrow().reload
        }

    private suspend fun <T> ask(
        command: String,
        args: Map<String, Any?>,
        decode: (Any?) -> T?,
    ): Result<T> {
        val m = machine()
        return when (val r = feed.call(command, args)) {
            is Reply.Ok -> decode(r.data)?.let { Result.success(it) } ?: Result.failure(Problem(unreadableReply(m, command, r.data.toString()).text))
            else -> Result.failure(Problem(r.problem(m, command)?.text.orEmpty()))
        }
    }

    /** 屏上那一句（[message]）。 */
    private class Problem(
        said: String,
    ) : Exception(said)
}
