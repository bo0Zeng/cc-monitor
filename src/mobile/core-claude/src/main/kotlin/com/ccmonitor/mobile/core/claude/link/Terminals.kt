package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.InputNow
import com.ccmonitor.mobile.core.wire.gen.Preview
import com.ccmonitor.mobile.core.wire.gen.TermInput
import com.ccmonitor.mobile.core.wire.gen.TerminalEnded
import com.ccmonitor.mobile.core.wire.gen.TerminalsList as TerminalsListReply

/*
 * 那台的终端：列 · 抓屏 · 结束。都是那台核心的帧命令（`IPC-COMMANDS.md` 各节），
 * 找哪个窗格、怎么附着全在那台（进终端那一行是 [SessionOpen] 的回包）；手机只拼入参，回包由嵌的 Rust 核心按 `cmd-wire` 那一份解过、装进 `link/wire/` 的声明类；
 * 句柄（`terminal`）不拼不解析，原样交回。
 */

/** 请求里自报的前端（「哪个前端的会话」那一维）。 */
internal const val CLIENT: String = "mobile"

/** `terminals-list`：这台的终端名单。 */
object TerminalsList {
    const val COMMAND: String = "terminals-list"

    fun args(): Map<String, Any?> = mapOf("client" to CLIENT)

    data class Row(
        /** 名单里那一行的句柄（送字 · 抓屏时交回）。 */
        val terminal: String,
        /** 会话名（那台 tmux 里的名字）。 */
        val name: String,
        /** 那台写好的标题（跑着会话 ⇒ 窗格标题；否则前台程序名）。 */
        val title: String,
        /** 此刻有终端客户端连着。 */
        val attached: Boolean,
        /** 里面跑着会话时是哪一家（线上名）；没有 ⇒ `null`。 */
        val agent: String?,
        /** 里面跑着的会话（`session-model` 认它）；没有 ⇒ `null`。 */
        val sid: String?,
        /** 托管那一形：此刻谁在输入（那台写好的那一句 ＋ 是不是手机）；tmux 那一形 ⇒ `null`。 */
        val holder: Holder? = null,
    )

    /** 托管终端的输入持有者：那台写好的那一句 · 是不是手机这一端。 */
    data class Holder(
        val said: String,
        val mine: Boolean,
    )

    /** 输入那一格 ⇒ 持有者（tmux 那一形 ⇒ `null`）。名单那一行与接管 · 交还的回包同一个读法。 */
    fun holderOf(input: TermInput?): Holder? = (input as? TermInput.Held)?.holder?.let { Holder(it.said, it.mine) }

    fun of(data: Any?): List<Row>? =
        Wire.bind<TerminalsListReply>(data)?.terminals?.map { t ->
            Row(t.terminal, t.name, t.title, attached = t.clients.isNotEmpty(), agent = t.session?.agent, sid = t.session?.sid, holder = holderOf(t.input))
        }

    /** 里面跑着 [sid] 那条会话的那一行（名单上的 `session.sid`，那台认的）；没有 ⇒ `null`。 */
    fun holding(
        rows: List<Row>,
        sid: String,
    ): Row? = rows.firstOrNull { it.sid == sid }

    /** 名字一模一样的那一行的句柄；没有 ⇒ `null`。 */
    fun handleOf(
        rows: List<Row>,
        name: String,
    ): String? = rows.firstOrNull { it.name == name }?.terminal
}

/** `terminal-preview`：抓一个终端的一屏（纯文字）。 */
object TerminalPreview {
    const val COMMAND: String = "terminal-preview"

    fun args(terminal: String): Map<String, Any?> = mapOf("terminal" to terminal, "color" to false)

    /** 自上而下的行拼成一段。 */
    fun of(data: Any?): String? = Wire.bind<Preview>(data)?.lines?.joinToString("\n") { it.text }
}

/** `terminal-end`：按名单里的句柄结束一个终端（两种宿主同一条；那台过身份门；被拒 ⇒ 失败信封里是那台写好的那一句）。 */
object TerminalEnd {
    const val COMMAND: String = "terminal-end"

    fun args(terminal: String): Map<String, Any?> = mapOf("terminal" to terminal, "client" to CLIENT)

    /** 结束了的那个终端（回的句柄）。 */
    fun of(data: Any?): String? = Wire.bind<TerminalEnded>(data)?.terminal
}

/** `terminal-take-input` · `terminal-release-input`：替手机接管 / 交还托管终端的输入（只认托管那一形）；回那一行此刻的持有者。 */
object TerminalHold {
    const val TAKE: String = "terminal-take-input"
    const val RELEASE: String = "terminal-release-input"

    fun args(terminal: String): Map<String, Any?> = mapOf("terminal" to terminal, "client" to CLIENT)

    /** 那一行此刻的持有者（回的不是托管那一形 ⇒ `null`）。 */
    fun of(data: Any?): TerminalsList.Holder? = TerminalsList.holderOf(Wire.bind<InputNow>(data)?.input)
}
