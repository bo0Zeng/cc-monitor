package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.bridge.SendOutcome
import com.ccmonitor.mobile.core.claude.bridge.SendRequest
import com.ccmonitor.mobile.core.claude.bridge.UplinkLimits
import com.ccmonitor.mobile.core.claude.bridge.UplinkSink
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.ssh.TmuxCommands.MODAL_SENTINEL
import com.ccmonitor.mobile.ssh.TmuxCommands.SENT_MARKER
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.flow.toList

/**
 * 上行：用 `tmux send-keys` 把文字投进 Claude TUI 所在的 pane，走 SSH exec。
 *
 * [echoesBack] 为 true：TUI 会回显投进去的文字，那句话会以下行帧回来，上层不做乐观回显。
 *
 * 注意：[RemoteCommandChannel.exec] 只拿得到 stdout，拿不到 stderr 和 exit code，
 * 所以命令用 `2>&1` 加哨兵字符串，判定全看 stdout 文本（见 [classify]）。
 *
 * 命令串由 [TmuxCommands.buildSendCommand] 造；本类只管产品行为：长度闸门、取消不算失败、
 * [interrupt] 明确拒绝、[classify] 把 stdout 翻成给人看的一句话和能不能重试。
 * 判送达用的哨兵读网关的常量，命令和判定同源。
 */
class TmuxSendKeysSink(
    private val channel: RemoteCommandChannel,
    private val target: String,
) : UplinkSink {
    /** TUI 会回显投进去的文字 ⇒ 上层别再乐观上屏一份（否则同一句显示两遍）。 */
    override val echoesBack: Boolean = true

    override suspend fun send(request: SendRequest): SendOutcome {
        // 上行单条超过 56KB 一律拒绝，改走 SFTP。整条命令是 sshd 的单个 argv 元素，内核 `MAX_ARG_STRLEN`
        // 是 131070B，128KB 就会「参数列表过长」；56KB 之上的余量留给转义膨胀。纯文本也会超（粘一段堆栈）。
        // 没有这道门时，远端 shell 把错误打进 stdout、落进 catch-all，屏上对着一条注定失败的消息无限重试。
        // 量的是拼好之后的命令串，不是正文：正文转义后会明显变长。
        val command =
            TmuxCommands.buildSendCommand(target, request.text)
                ?: return SendOutcome.Rejected("没有可投递的 tmux 会话", retryable = false)
        UplinkLimits.rejectIfTooLong(command, request.text)?.let { return it }
        val stdout =
            runCatching {
                channel
                    .exec(command)
                    .toList()
                    .fold(ByteArray(0)) { acc, b -> acc + b }
                    .decodeToString()
            }.getOrElse { e ->
                // 取消不是失败。吞掉的话，离开对话这类协程取消会被报成可重试的拒绝，屏上出现重试键，
                // 点一下就发第二遍；也破坏取消语义，父作用域以为取消完了，这层还在往下走。
                if (e is CancellationException) throw e
                // 传输层出问题（连接断了、命令没跑起来）→ 可重试，与远端明确答复区分开
                return SendOutcome.Rejected(e.message ?: e.javaClass.simpleName, retryable = true)
            }
        return classify(stdout)
    }

    /**
     * 这条路停不了，明确拒绝。
     *
     * 不投 `C-c` / `Escape`：这条路连的是 tmux 里的官方 TUI，没有常驻管道那种 stdin 上的 `control_request`。
     * 投键在这条路上不安全，中断键一样会落进正在等回答的弹窗里（盲发最坏会替人点「是，我信任」）；
     * TUI 的中断键语义也随版本变。
     *
     * 不悄悄 no-op：那会让界面一直停在「正在让远端停下…」。给一句人话和 `retryable = false`，
     * 上层退回「停止显示」并如实说远端可能还在跑。
     */
    override suspend fun interrupt(): SendOutcome =
        SendOutcome.Rejected(INTERRUPT_UNSUPPORTED, retryable = false)

    companion object {
        /**
         * 这条路停不了时说的那句话。上屏文案不许出现「连接」「SSH」「tmux」、技术义的「会话」「exec」「daemon」等词。
         */
        const val INTERRUPT_UNSUPPORTED = "这条对话是接到电脑上那个界面里的，手机这头停不了它——去电脑上按停止"

        /** 上行长度上限，见 [UplinkLimits]。 */
        const val MAX_UPLINK_BYTES = UplinkLimits.MAX_UPLINK_BYTES

        /**
         * stdout 文本 → [SendOutcome]。几种哨兵都是 `retryable = false`：远端明确答复了，重试同一目标不会变。
         * 只有传输层异常和认不出的输出才可重试。
         *
         * `ATERM_SENT` / `ATERM_MODAL_WAIT` 读 [TmuxCommands] 的常量；`NO_TMUX` / `CCM_NO_SESSION` /
         * `CCM_GUARD_REJECTED` 在网关里是内联在 `printf` 里的，这里只能手写字面，两边要一起改。
         */
        fun classify(stdout: String): SendOutcome {
            val trimmed = stdout.trim()
            return when {
                // 先认肯定的成功证据：没有 [TmuxCommands.SENT_MARKER] 就不算送到。
                trimmed.endsWith(SENT_MARKER) -> SendOutcome.Accepted
                trimmed == "NO_TMUX" -> SendOutcome.Rejected("远端没装 tmux", retryable = false)
                trimmed == "CCM_NO_SESSION" -> SendOutcome.Rejected("那个会话已经不在了", retryable = false)
                trimmed.startsWith("CCM_GUARD_REJECTED") ->
                    SendOutcome.Rejected("这个 tmux 会话不归我们管（$trimmed）", retryable = false)
                trimmed.startsWith(MODAL_SENTINEL) ->
                    SendOutcome.Rejected(
                        // 屏幕原文原样带回给上层显示（这一句随送字换到核心那一路时删：需手动时不送由核心判）。
                        "电脑上有个问题待答 · 现在发消息会被当成对它的回答：\n" +
                            trimmed.removePrefix(MODAL_SENTINEL).trim(),
                        retryable = false,
                    )
                // catch-all 必须是拒绝，不是接受：`can't find session: …`、`no server running on …` 都落在这里。
                // target 是 `cc-<sid8>` 时身份门不生成，`CCM_NO_SESSION` 根本不会出现，这里是唯一的失败探测口。
                else ->
                    SendOutcome.Rejected(
                        trimmed.ifBlank { "tmux 没给任何回应，这条可能没发出去" },
                        // 不知道是不是暂时的，给重试键
                        retryable = true,
                    )
            }
        }
    }
}
