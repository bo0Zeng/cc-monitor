package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.SidsReq
import com.ccmonitor.mobile.core.wire.gen.TermInput
import com.ccmonitor.mobile.core.wire.gen.TerminalCanEnd
import com.ccmonitor.mobile.core.wire.gen.TerminalEndReq
import com.ccmonitor.mobile.core.wire.gen.TerminalHost
import com.ccmonitor.mobile.core.wire.gen.TerminalPreviewReq
import com.ccmonitor.mobile.core.wire.gen.TerminalsListReq
import com.ccmonitor.mobile.core.wire.gen.Whereabout

/*
 * 那台的终端：列 · 抓屏 · 结束。都是那台核心的帧命令（`IPC-COMMANDS.md` 各节），
 * 找哪个窗格、怎么附着全在那台（进终端那一行是 [SessionOpen] 的回包）；手机只拼入参，回包由嵌的 Rust 核心按 `cmd-wire` 那一份解过、装进 `link/wire/` 的声明类；
 * 句柄（`terminal`）不拼不解析，原样交回。
 */

/** 请求里自报的前端（「哪个前端的会话」那一维）。 */
internal const val CLIENT: String = "mobile"

/** `terminals-list`：这台的终端名单。 */
object TerminalsList {
    const val COMMAND: String = Commands.TerminalsList.NAME

    fun args(): Map<String, Any?> = Commands.TerminalsList.args(TerminalsListReq(client = CLIENT))

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
        /** 住在 tmux 里（副标题带「tmux」）；托管那一形 ⇒ `false`。 */
        val tmux: Boolean = true,
        /** 托管那一形：此刻谁在输入，那台写好的那一句（同桌面 · 你的终端那一句）；tmux 那一形 ⇒ `null`。
         * 谁在输入照接入协议（`host/managed/attach.rs`）：终端 tab 接上即持有，只看时回车接管，别的端接上即交出；
         * 手机不另调 `terminal-take-input` · `terminal-release-input`。
         */
        val holder: String? = null,
        /** 这一行结束不了时那台写好的那一句（`can.end` 说不能）；能结束 ⇒ `null`。 */
        val endRefused: String? = null,
    )

    fun of(data: Any?): List<Row>? =
        Commands.TerminalsList.reply(data)?.terminals?.map { t ->
            Row(
                t.terminal,
                t.name,
                t.title,
                attached = t.clients.isNotEmpty(),
                agent = t.session?.agent,
                sid = t.session?.sid,
                tmux = t.host == TerminalHost.TMUX,
                holder = (t.input as? TermInput.Held)?.holder?.said,
                endRefused =
                    when (val e = t.can.end) {
                        is TerminalCanEnd.Flag -> if (e.value) null else ""
                        is TerminalCanEnd.Obj -> e.value.said
                    },
            )
        }

    /** 名字一模一样的那一行的句柄；没有 ⇒ `null`。 */
    fun handleOf(
        rows: List<Row>,
        name: String,
    ): String? = rows.firstOrNull { it.name == name }?.terminal
}

/**
 * `sessions-where`：一条会话此刻在那台哪个终端里（在跑 · 空着 · 不止一个 · 没有 · 那台没 tmux）。判定只在那台，手机不拿名单自己找。
 */
object SessionsWhere {
    const val COMMAND: String = Commands.SessionsWhere.NAME

    fun args(sid: String): Map<String, Any?> = Commands.SessionsWhere.args(SidsReq(sids = listOf(sid), client = CLIENT))

    /** 问的那一条（回包与入参同序，只问了一条 ⇒ 第一个）；回包里没有 ⇒ `null`。 */
    fun of(data: Any?): Whereabout? = Commands.SessionsWhere.reply(data)?.results?.firstOrNull()
}

/** `terminal-preview`：抓一个终端的一屏（纯文字）。 */
object TerminalPreview {
    const val COMMAND: String = Commands.TerminalPreview.NAME

    fun args(terminal: String): Map<String, Any?> = Commands.TerminalPreview.args(TerminalPreviewReq(terminal = terminal, color = false))

    /** 自上而下的行拼成一段。 */
    fun of(data: Any?): String? =
        Commands.TerminalPreview
            .reply(data)
            ?.lines
            ?.joinToString("\n") { it.text }
}

/** `terminal-end`：按名单里的句柄结束一个终端（两种宿主同一条；那台过身份门；被拒 ⇒ 失败信封里是那台写好的那一句）。 */
object TerminalEnd {
    const val COMMAND: String = Commands.TerminalEnd.NAME

    fun args(terminal: String): Map<String, Any?> = Commands.TerminalEnd.args(TerminalEndReq(terminal = terminal, client = CLIENT))

    /** 结束了的那个终端（回的句柄）。 */
    fun of(data: Any?): String? = Commands.TerminalEnd.reply(data)?.terminal
}
