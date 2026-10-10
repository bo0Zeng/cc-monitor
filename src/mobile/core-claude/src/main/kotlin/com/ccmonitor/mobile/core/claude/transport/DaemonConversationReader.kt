package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.transport.DaemonConversationCatalog.Conversation
import com.ccmonitor.mobile.core.claude.transport.DaemonConversationCatalog.Project
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.toList

/**
 * 把 [DaemonConversationCatalog] 的命令真的发出去。core 只出事实，exec 在这一层，解析是纯函数。
 *
 * 失败一律回 `null`，不回空列表。传输层异常（连不上 / 流中断）与「命令不认识」同档，都是问不出来。
 */
class DaemonConversationReader(
    private val channel: RemoteCommandChannel,
    private val daemonPath: String,
) {
    /**
     * 最近一次失败的原因，供诊断，不上屏：这里存的是异常类名、命令片段这类原文，
     * 而 `daemon` / `exec` / `SSH` 这类词不许出现在界面上。上屏的是上层分档后的人话。
     */
    var lastFailure: String? = null
        private set

    /** 这台机器上有哪些项目。`null` = 问不出来（原因见 [lastFailure]）。 */
    suspend fun projects(): List<Project>? {
        val out = capture(DaemonConversationCatalog.projectsCommand(daemonPath)) ?: return null
        return DaemonConversationCatalog
            .parseProjects(out)
            .also { if (it == null) lastFailure = NO_SUCCESS_MARKER }
    }

    /** 某个项目下有哪些对话。`null` = 问不出来（原因见 [lastFailure]）。 */
    suspend fun conversations(dirName: String): List<Conversation>? {
        val out = capture(DaemonConversationCatalog.sessionsCommand(daemonPath, dirName)) ?: return null
        return DaemonConversationCatalog
            .parseSessions(out)
            .also { if (it == null) lastFailure = NO_SUCCESS_MARKER }
    }

    private suspend fun capture(command: String): String? =
        try {
            channel
                .exec(command)
                .toList()
                .fold(ByteArray(0)) { acc, b -> acc + b }
                .decodeToString()
                .also { lastFailure = null } // 成功了就清掉上一次的原因
        } catch (e: kotlin.coroutines.cancellation.CancellationException) {
            throw e // 取消不是故障，必须向上传播
        } catch (e: java.io.IOException) {
            // 连不上 / 流中断 ⇒ 与「命令不认识」同档：问不出来，但原因要留住。
            lastFailure = "${e::class.simpleName}: ${e.message}"
            null
        }

    private companion object {
        /**
         * 命令跑了、但没吐出正向标记：对端不认识这个子命令（会 exit 2、stdout 空）。
         * 与「连不上」分开：前者换个后端版本就好，后者是网络。
         */
        const val NO_SUCCESS_MARKER = "查询没有返回成功标记（对端可能不认识这个子命令）"
    }
}
