package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.remote.RemoteExecutor

/**
 * 核心写好的失败：`code` 给程序认，`message` 原样上屏，`detail` 进复制详情（时刻 · 机器 · 命令 · 码 · 原话都在里面）。
 * 帧面 `reply` 与一次性 CLI 的 stderr 信封是同一份（`IPC-PROTOCOL.md` §4 · §8）。
 */
data class CoreFailure(
    val code: String,
    val message: String,
    val detail: String,
    val data: Any? = null,
) {
    companion object {
        /** 一行信封 `{code, message, detail, data?}`；缺 `code` 或 `message` ⇒ `null`（不是信封）。 */
        fun of(m: Map<String, Any?>): CoreFailure? {
            val code = m.str("code") ?: return null
            val message = m.str("message") ?: return null
            return CoreFailure(code, message, m.str("detail").orEmpty(), m["data"])
        }
    }
}

/**
 * 那台的后端住在哪：部署落点 `~/.cc-monitor/bin/ccm`。不读设置里的路径、不找 PATH（`后端.md` §1）。
 * 拼后端命令只许在这里：`"$HOME"/.cc-monitor/bin/ccm -- --<子命令>`（`--` 是 argv 分派门，挪走就整行交给 claude）。
 */
object BackendBin {
    /** 远端 shell 里展开 `$HOME`、带空格的家目录也不拆开。 */
    const val PATH_WORD: String = "\"\$HOME\"/.cc-monitor/bin/ccm"

    private val WORD = Regex("--[a-z][a-z-]*")

    /** 只收我们自己的常量词（`--resident-attach` 这一形），不收任何外来串 ⇒ 不用 quote、也拼不出注入。 */
    fun command(vararg words: String): String {
        require(words.isNotEmpty() && words.all { WORD.matches(it) }) { "后端子命令只许是常量词：${words.toList()}" }
        return "$PATH_WORD -- ${words.joinToString(" ")}"
    }
}

/** 一次性 CLI 的结局（C 通道：只在接常驻流之前用）。 */
sealed interface OneShotOutcome {
    /** 退出 0、stdout 一行 JSON 对象（stderr 恒 0 字节）。 */
    data class Ok(
        val data: Map<String, Any?>,
    ) : OneShotOutcome

    /** 退出非 0，stderr 正好一行核心写好的信封。 */
    data class Failed(
        val failure: CoreFailure,
    ) : OneShotOutcome

    /** 那台没有 `~/.cc-monitor/bin/ccm`（shell 回 127：找不到 / 不能执行）。 */
    data object Absent : OneShotOutcome

    /** 跑了但对不上协议（成功却不是 JSON 对象 · 失败却没有信封 · 退出码没回）：原样带回，进复制详情。 */
    data class Unreadable(
        val exitStatus: Int?,
        val stdout: String,
        val stderr: String,
    ) : OneShotOutcome
}

/**
 * C 通道：`ccm -- --<子命令>` 跑一次。成功 ⇒ stdout 一行 JSON；失败 ⇒ stderr 正好一行 `{code, message, detail}`、退出 2。
 * 连接本身不可用 ⇒ [RemoteExecutor] 抛 `ConnectionDeadException`，这里不吞。
 */
class OneShot(
    private val executor: RemoteExecutor,
) {
    suspend fun run(vararg words: String): OneShotOutcome = read(executor.execCapture(BackendBin.command(*words)).let { Triple(it.exitStatus, it.stdout, it.stderrTail) })

    companion object {
        /** shell 找不到命令 / 不能执行：127 · 126。 */
        private val ABSENT_CODES = setOf(126, 127)

        /** 判法只这一处（纯函数，单测直接喂）。 */
        fun read(result: Triple<Int?, String, String>): OneShotOutcome {
            val (exit, stdout, stderr) = result
            val outLine = stdout.trim()
            val errLine = stderr.trim()
            return when {
                exit == 0 && errLine.isEmpty() ->
                    Json.obj(outLine)?.let { OneShotOutcome.Ok(it) } ?: OneShotOutcome.Unreadable(exit, stdout, stderr)
                exit != null && exit in ABSENT_CODES -> OneShotOutcome.Absent
                exit != null && exit != 0 ->
                    errLine
                        .lines()
                        .singleOrNull()
                        ?.let(Json::obj)
                        ?.let(CoreFailure::of)
                        ?.let { OneShotOutcome.Failed(it) }
                        ?: OneShotOutcome.Unreadable(exit, stdout, stderr)
                else -> OneShotOutcome.Unreadable(exit, stdout, stderr)
            }
        }
    }
}
