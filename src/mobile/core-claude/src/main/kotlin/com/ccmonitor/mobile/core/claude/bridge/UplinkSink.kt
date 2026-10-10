package com.ccmonitor.mobile.core.claude.bridge

/**
 * 上行的唯一出口：把用户写的话送到远端 Claude。
 *
 * 两条实现路径的回声行为不同：常驻管道（往 `in.ndjson` 追加、`tail -f` 喂进 CLI 的 stdin）
 * 不回声；`tmux send-keys` 会由 TUI 回显，随后作为记录出现。见 [echoesBack]。
 *
 * ### 静默分叉：幂等治不了它
 *
 * 手机与电脑同时对着一条对话打字：两条上行各自拿到成功、两侧判据各自全绿、
 * 两条都进了同一条线性上下文；无报错、无日志，用户侧唯一症状是「AI 突然变笨了」。
 * 幂等治的是「同一条消息发两遍」，治不了这个 —— 那是两条真的不同的消息进同一条上下文，
 * 我们把它们串行化也串不成一条。
 * 要解它得靠后端的排队语义；在后端做出队列之前，这段不许删。
 */
interface UplinkSink {
    // 管道路的上行形状：往 `in.ndjson` 追加一行
    //
    //   {"type":"user","message":{"role":"user","content":"帮我看看这个报错"}}
    //
    // 停止生成也走同一个 stdin：追加一行
    //   {"type":"control_request","request_id":…,"request":{"subtype":"interrupt"}}
    // 远端就中止这一轮。

    /**
     * 对端会不会把这条消息回显回来，决定要不要乐观回显。
     *
     * - `false`（常驻管道）：用户消息不会作为帧回来，本地乐观回显的那条就是最终形态。
     * - `true`（`tmux send-keys`）：TUI 回显输入、随后作为记录到达，所以不做乐观回显，等回声再上屏。
     *
     * `true` 时不做「回显后认领」：回声帧（`ut`）没有 id，TUI 重发的回声也带不回 [SendRequest.localId]；
     * 按内容加时间窗去猜会把「说两遍同样的话」吞掉。代价是这条路上屏慢一个往返，但不会重复显示。
     */
    val echoesBack: Boolean

    /** 送一条出去。不抛异常：失败是正常路径，要能在界面上显示并重试。 */
    suspend fun send(request: SendRequest): SendOutcome

    /**
     * 让远端这一轮停下来。
     *
     * 停下不是杀掉：中断之后 `claude` 进程仍活着，同一条对话随后照常进行。
     * 走 `C-c` 会打掉整条管道（pane 的前台进程组是 `tail -f | claude | while read` 一整条）。
     *
     * 返回 `Accepted` 只表示中断投递出去了，不表示远端已停。停没停看下行 `res` 的
     * `terminal_reason == "aborted_streaming"`；在那之前界面只能说「正在让远端停下」。
     *
     * 没有默认实现，免得某个实现悄悄退化成 no-op。不抛异常，同 [send]。
     */
    suspend fun interrupt(): SendOutcome
}

/**
 * 一条待发送的消息。
 *
 * @param localId 本地铸的稳定 id，不从内容派生：内容哈希会把「说两遍同样的话」合并，
 *   重试时也会随文本编辑而变。
 */
data class SendRequest(
    val localId: String,
    val text: String,
)

/**
 * 上行的幂等标识：同一条消息重发不变，换一个实例一定不同。
 *
 * 不能直接用 [SendRequest.localId]：它是 `local#<seq>`，序号随会话实例重来，
 * 重启后发的第一条会被远端当成上次那条的重复而吞掉——吞消息比重复更坏，因为是静默的。
 * 所以拌上每个实例各自铸的随机 [origin]。
 *
 * [origin] 铸在实例上而不是每次 [idFor] 现铸：现铸会让重试拿到新标识，幂等失效。
 *
 * 注意：一个实例只对应一条对话。sink 若活得比它所属的会话长，新会话的 `local#0` 会撞上旧 origin，第一条被吞。
 *
 * 这串是机器码，不上屏，不进 [SendOutcome.Rejected.reason] 或任何渲染单元。
 */
class UplinkIdentity(
    /** 本实例的随机前缀。跨进程唯一靠它，不靠 `localId`。 */
    val origin: String = newOrigin(),
) {
    /**
     * 这条消息的幂等标识。纯函数：同一实例、同一 [request] 恒得同一串。
     * 只吃 [SendRequest.localId]，不吃正文。
     */
    fun idFor(request: SendRequest): String = "$PREFIX$origin-${request.localId}"

    companion object {
        /** 标识前缀。远端用它 grep 查重。 */
        const val PREFIX: String = "aterm-"

        /**
         * 铸一个新 origin：128 位随机，32 个小写十六进制字符，不带连字符。
         * 不走 `String.format`：这串进协议帧，不能随系统语言变。
         */
        fun newOrigin(): String =
            java.util.UUID
                .randomUUID()
                .toString()
                .replace("-", "")
    }
}

/**
 * 远端追加命令的三档结局：已追加、本来就在、失败。
 *
 * 界面只有两档 [SendOutcome]，由 [asOutcome] 折回去；「这条已经发过了」在界面上暂时说不出来。
 */
sealed interface UplinkAppendOutcome {
    /** 这一次把那一行写进了 `in.ndjson`。 */
    data object Appended : UplinkAppendOutcome

    /** 那一行本来就在里面，这一次没写。断线重连后重发同一条消息落在这一档。 */
    data object AlreadyThere : UplinkAppendOutcome

    /** 没写成。语义与 [SendOutcome.Rejected] 一致。 */
    data class Failed(
        val reason: String,
        val retryable: Boolean = true,
    ) : UplinkAppendOutcome

    /**
     * 折回两档 [SendOutcome]。[AlreadyThere] → [SendOutcome.Accepted] 是有意的：
     * 那一行确实在远端，不该给重试按钮。
     */
    fun asOutcome(): SendOutcome =
        when (this) {
            Appended, AlreadyThere -> SendOutcome.Accepted
            is Failed -> SendOutcome.Rejected(reason, retryable)
        }
}

/** 发送结果。没有「抛异常」这一支：失败要能显示、能重试。 */
sealed interface SendOutcome {
    /** 已交给对端。不等于 Claude 已经看到，那要等 `res` 帧。 */
    data object Accepted : SendOutcome

    /**
     * 没送出去。
     *
     * @param retryable 是否值得重试。`false` 用于重试也没用的情形（权限模式被拒、会话已结束），
     *   这时界面不给重试按钮。
     */
    data class Rejected(
        val reason: String,
        val retryable: Boolean = true,
    ) : SendOutcome
}

/** 上行长度闸门，全仓唯一一处。 */
object UplinkLimits {
    /**
     * 上行单行上限 56KB。整条命令是 sshd 的单个 argv 元素，内核 `MAX_ARG_STRLEN` = 131070B；
     * 56KB 之上的余量留给转义膨胀。
     */
    const val MAX_UPLINK_BYTES = 56 * 1024

    /**
     * 量最终命令串的字节数，超了就拒。
     *
     * 量命令而不是正文：上行先 JSON 序列化再整体 shell 引号，两轮膨胀叠加，
     * 一条全是引号的 55KB 正文照样能顶穿内核上限。那时远端把「参数列表过长」打进 stdout，
     * 用户会对着一条注定失败的消息无限重试。
     *
     * @param command 拼好、马上要发的完整命令串。
     * @param text 用户写的正文，只用于文案。
     * @return `null` = 没超，可以发。
     */
    fun rejectIfTooLong(
        command: String,
        text: String,
    ): SendOutcome.Rejected? {
        if (command.toByteArray().size <= MAX_UPLINK_BYTES) return null
        val textKb = text.toByteArray().size / 1024
        return SendOutcome.Rejected(
            "这条消息太长了（约 ${textKb}KB，转义后超过 ${MAX_UPLINK_BYTES / 1024}KB 上限）——大段文本要走文件传输",
            retryable = false, // 重试同一条只会再失败一次
        )
    }
}
