package com.ccmonitor.mobile.core.claude.bridge

import com.ccmonitor.mobile.core.claude.command.PipeCommands
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import java.io.ByteArrayOutputStream

/**
 * 把管道进程写进 `bridge.log` 的错误输出读回来，给出一句能上屏的话。
 *
 * 起成功但界面空白时（远端没登录、会话编号已被占用之类），唯一的线索在这份日志里。
 * 读法是有界的：远端进程可能往 stderr 吐很多，只读开头 [LOG_BYTE_CAP] 字节。
 * 命令由 [PipeCommands.diagnosticsReadCommand] 出，本类不拼路径也不拼命令。
 */
class PipeDiagnostics(
    private val channel: RemoteCommandChannel,
    private val sessionId: String,
) {
    /**
     * @return 日志里第一句有内容的话；null 表示没有线索（文件不存在或为空），
     *   这时说什么由调用方决定。
     */
    suspend fun explainFailure(): String? {
        val raw =
            runCatching { collect(PipeCommands.diagnosticsReadCommand(sessionId, LOG_BYTE_CAP)) }
                .getOrElse { return null }
        return firstMeaningfulLine(raw)
    }

    private suspend fun collect(command: String): String {
        val buf = ByteArrayOutputStream()
        channel.exec(command).collect { buf.write(it) }
        return buf.toString("UTF-8")
    }

    companion object {
        /** 日志读多少字节封顶：错误总在最前面。 */
        const val LOG_BYTE_CAP = 4096L

        /** 上屏那句话的长度上限。 */
        const val MAX_REASON_CHARS = 200

        /**
         * 取第一行非空并截断。stderr 来自任意程序、没有固定结构，所以不做更细的解析。
         */
        fun firstMeaningfulLine(raw: String): String? =
            raw
                .lineSequence()
                .map { it.trim() }
                .firstOrNull { it.isNotEmpty() }
                ?.take(MAX_REASON_CHARS)
    }
}
