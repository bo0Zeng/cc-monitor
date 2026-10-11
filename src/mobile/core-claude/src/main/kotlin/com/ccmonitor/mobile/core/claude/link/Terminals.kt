package com.ccmonitor.mobile.core.claude.link

/*
 * 那台的终端：列 · 抓屏 · 结束 · 进终端那一行。都是那台核心的帧命令（`IPC-COMMANDS.md` 各节），
 * 找哪个窗格、怎么附着全在那台；手机只拼入参、解成品，句柄（`terminal`）不拼不解析，原样交回。
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
    )

    fun of(data: Any?): List<Row>? =
        decoding {
            data.asObj().objs("terminals").need().map { t ->
                Row(
                    terminal = t.str("terminal").need(),
                    name = t.str("tmux_name").need(),
                    title = t.str("title").orEmpty(),
                    attached = t.objs("clients").need().isNotEmpty(),
                    agent = t.obj("session")?.str("agent"),
                )
            }
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
    fun of(data: Any?): String? =
        decoding {
            data
                .asObj()
                .objs("lines")
                .need()
                .joinToString("\n") { it.str("text").need() }
        }
}

/** `kill`：结束一个终端（那台过身份门；被拒 ⇒ 失败信封里是那台写好的那一句）。 */
object Kill {
    const val COMMAND: String = "kill"

    fun args(name: String): Map<String, Any?> = mapOf("name" to name, "client" to CLIENT)

    /** 杀成了没有。 */
    fun of(data: Any?): Boolean? = decoding { data.asObj().bool("killed").need() }
}

/**
 * 进终端那一行：问 `launch-render-cli` 要那台要跑的 `ccm …`（与桌面「命令已复制」复制的是同一条），
 * 手机在自己的终端里原样敲进去。意图与桌面 `planAttach` 一样：接回 [name] 那个会话、不起 agent。
 */
object AttachLine {
    const val COMMAND: String = "launch-render-cli"

    fun args(
        agent: String,
        defaultLauncher: String,
        name: String,
    ): Map<String, Any?> =
        mapOf(
            "agent" to agent,
            "action" to mapOf("kind" to "attach", "name" to name),
            "container" to mapOf("kind" to "tmux", "name" to name, "send_into" to false),
            "cwd" to null,
            "account" to mapOf("kind" to "base"),
            "ccmSid" to null,
            "model" to null,
            "models" to emptyMap<String, String>(),
            "launcher" to defaultLauncher,
            "defaultLauncher" to defaultLauncher,
        )

    fun of(data: Any?): String? = decoding { data.asObj().str("cmd").need() }
}
