package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.Killed
import com.ccmonitor.mobile.core.claude.link.wire.Preview
import com.ccmonitor.mobile.core.claude.link.wire.Rendered
import com.ccmonitor.mobile.core.claude.link.wire.TerminalsListReply
import com.ccmonitor.mobile.core.claude.link.wire.Wire

/*
 * 那台的终端：列 · 抓屏 · 结束 · 进终端那一行。都是那台核心的帧命令（`IPC-COMMANDS.md` 各节），
 * 找哪个窗格、怎么附着全在那台；手机只拼入参，回包由嵌的 Rust 核心按 `cmd-wire` 那一份解过、装进 `link/wire/` 的声明类；
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
    )

    fun of(data: Any?): List<Row>? =
        Wire.bind<TerminalsListReply>(data)?.terminals?.map { t ->
            Row(t.terminal, t.name, t.title, attached = t.clients.isNotEmpty(), agent = t.session?.agent, sid = t.session?.sid)
        }

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

/** `kill`：结束一个终端（那台过身份门；被拒 ⇒ 失败信封里是那台写好的那一句）。 */
object Kill {
    const val COMMAND: String = "kill"

    fun args(name: String): Map<String, Any?> = mapOf("name" to name, "client" to CLIENT)

    /** 杀成了没有。 */
    fun of(data: Any?): Boolean? = Wire.bind<Killed>(data)?.killed
}

/**
 * 进终端那一行：问 `launch-render-cli` 要那台要跑的 `ccm …`（与桌面「命令已复制」复制的是同一条），
 * 手机在自己的终端里原样敲进去。只交意图（接回 [name] 那个会话、不起 agent），启动命令一截都不拼：那台自己定。
 */
object AttachLine {
    const val COMMAND: String = "launch-render-cli"

    /** [agent]：那个终端里那一家的线上名（照 `terminals-list` · `session-new-facts` 给的原值）。 */
    fun args(
        agent: String,
        name: String,
    ): Map<String, Any?> =
        mapOf(
            "agent" to agent,
            "action" to mapOf("kind" to "attach", "name" to name),
            "container" to mapOf("kind" to "tmux", "name" to name, "send_into" to false),
            "account" to mapOf("kind" to "base"),
        )

    fun of(data: Any?): String? = Wire.bind<Rendered>(data)?.cmd
}
