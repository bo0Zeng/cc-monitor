package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.ssh.SessionBackend
import com.ccmonitor.mobile.ssh.TmuxBackend
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.delay

/**
 * resume 后 watchdog 的三态判定，调用方据此处理 `_tmuxSession` 与 Toast。
 * - [SUCCESS]：Claude 接管了前台，或前台是 shell 但 pane 有子进程 / 无从判定（疑似包装脚本下的 Claude），保留会话；
 * - [FAILED]：两次探测都是闲置 shell 且明确没有子进程，空会话已被 `kill-session`；调用方 Toast「resume 失败」并清 `_tmuxSession`；
 * - [ABSENT]：前台为空（会话不存在：被 cwd 守卫拦下，或慢机上还没建出）。清掉乐观置的 `_tmuxSession`，不 kill，不 Toast（pty 里已有守卫的 echo）。
 */
internal enum class ResumeOutcome {
    SUCCESS,
    FAILED,
    ABSENT,
}

/**
 * resume 后的失败探测与清理：失败要可见，空会话不能泄漏。
 *
 * resume 命令发出后，Claude 应在几秒内接管 pane 前台（`pane_current_command` 变成 node/claude/bun 等）。
 * 自定义命令不存在时（`cct: command not found`），shell 打印错误后前台仍是闲置 shell，并留下一个空的 `cc-<sid8>` 会话。
 * 探两次：
 * - T+[firstDelayMs]（默认 4s）快路径：前台非空且不是 shell，判 [ResumeOutcome.SUCCESS] 提前停。
 *   前台为空时不提前结束：resume 命令打进新的 login shell，慢机上此刻 `cc-<sid8>` 可能还没建出来，与 shell 一样等复核。
 * - T+[firstDelayMs]+[recheckDelayMs]（默认再 6s）终判：
 *   - 前台不是 shell → [ResumeOutcome.SUCCESS]；
 *   - 前台为空 → [ResumeOutcome.ABSENT]；
 *   - 前台是闲置 shell → 经 `pane_pid` 与 `pgrep -P` 查子进程，明确没有子进程才 `kill-session` 并判
 *     [ResumeOutcome.FAILED]；有子进程、没有 pgrep 或 pane 不在，都保守判 [ResumeOutcome.SUCCESS]，不 kill。
 * 探测走 exec 通道，不打扰 pty；探测与 kill 的传输失败一律静默降级。
 */
internal class ResumeWatchdog(
    private val channel: RemoteCommandChannel,
    private val firstDelayMs: Long = FIRST_PROBE_DELAY_MS,
    private val recheckDelayMs: Long = RECHECK_DELAY_MS,
    private val backend: SessionBackend = TmuxBackend, // 命令族由会话后端给出；resume 只在 POSIX 上，即 TmuxBackend
) {
    /** 监视 tmux 会话 [name] 的 resume 结果，返回三态 [ResumeOutcome]（各态语义见其 KDoc）。 */
    suspend fun watch(name: String): ResumeOutcome {
        delay(firstDelayMs)
        val first = probe(name)
        // 快路径：非空且不是 shell 即 Claude 已接管，成功停；空或 shell 都未定论，等复核。
        if (first != null && !backend.isForegroundShell(first)) return ResumeOutcome.SUCCESS
        delay(recheckDelayMs)
        val second = probe(name)
        return when {
            second == null -> ResumeOutcome.ABSENT // 会话确实不存在（cwd 守卫拦下/未建）
            !backend.isForegroundShell(second) -> ResumeOutcome.SUCCESS // Claude 接管前台
            confirmedNoChild(name) -> {
                killSession(name) // 明确没有子进程才是真失败，kill 掉空会话免得泄漏
                ResumeOutcome.FAILED
            }
            else -> ResumeOutcome.SUCCESS // 有子进程、没有 pgrep 或 pane 不在：疑似 Claude 在跑，保守不 kill
        }
    }

    /** pane 前台是 shell 时，是否明确没有子进程（可安全 kill）。探测失败或无从判定返回 false。 */
    private suspend fun confirmedNoChild(name: String): Boolean =
        try {
            backend.confirmedNoChild(exec(backend.paneChildProbeCommand(name)))
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            false
        }

    private suspend fun killSession(name: String) {
        // kill 传输失败也静默——失败判定已成立（交调用方 Toast + 清 tmuxSession）。
        try {
            exec(backend.killCommand(name))
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            // 连接可能已断；无碍失败判定。
        }
    }

    /** 探 pane 前台命令；空输出/传输失败 → null（CancellationException 重抛）。 */
    private suspend fun probe(name: String): String? =
        try {
            exec(backend.foregroundProbeCommand(name)).trim().ifEmpty { null }
        } catch (e: CancellationException) {
            throw e
        } catch (_: Exception) {
            null
        }

    private suspend fun exec(cmd: String): String {
        val buf = java.io.ByteArrayOutputStream()
        channel.exec(cmd).collect { buf.write(it) }
        return buf.toString("UTF-8")
    }

    companion object {
        /** 发出后首探的延时，给 launcher/claude 正常启动留时间。 */
        const val FIRST_PROBE_DELAY_MS = 4_000L

        /** 首探未定论后的复核延时，防慢机上误杀。 */
        const val RECHECK_DELAY_MS = 6_000L
    }
}
