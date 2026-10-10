package com.ccmonitor.mobile.core.claude.transport
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.shellQuote
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.flow

/**
 * 自读原文的传输：`tail -c +<startOffset+1> -F <jsonlPath>` 读单个会话文件，经 [LineFramer] 切行 → [JsonlFrame.Line]。
 * Flow 被取消时，[RemoteCommandChannel] 负责杀远端 tail。
 *
 * 字节偏移续传：[startOffsetBytes] = 0 → `-c +1`（全量历史）；> 0 → `-c +<N+1>`（只取第 N 字节之后），
 * 切 tab / 重连不必重下整份。用 `-c` 而不是 `-n` 是要按精确字节位续；`-F` = 跟随，文件重建后重开。
 * [currentOffset] 逐行推进，调用方据此更新 resume 点。
 */
class TailTransport(
    private val channel: RemoteCommandChannel,
    private val jsonlPath: String,
    private val startOffsetBytes: Long = 0L,
    private val sessionId: String? = null,
) : JsonlTransport {
    private val framer = LineFramer()

    /**
     * 已 emit 的最后一行结束处的绝对文件字节 offset（逐行推进，不按整 chunk）。调用方每收到一行就读它作 resume 起点，
     * offset 与已收到的行逐行一致：chunk 中途取消也不会让 offset 领先记录、续传跳行。
     */
    var currentOffset: Long = startOffsetBytes
        private set

    override fun frames(): Flow<JsonlFrame> =
        flow {
            emit(JsonlFrame.Hello)
            var seq = 0L
            val command = tailCommand(jsonlPath, startOffsetBytes)
            channel.exec(command).collect { chunk ->
                for (framed in framer.feedFramed(chunk)) {
                    currentOffset = startOffsetBytes + framed.endOffset // 先推进到本行行尾，再 emit
                    // byteOffset 随行携带（= 本行行尾累计字节 = resume 锚点）。
                    emit(JsonlFrame.Line(sessionId, jsonlPath, seq++, framed.raw, currentOffset))
                }
            }
            // 流正常结束（tail 退出，少见）：冲掉残行，不推进 offset（残行无 \n，resume 要重读以待补全）。
            framer.flush()?.let { emit(JsonlFrame.Line(sessionId, jsonlPath, seq, it, currentOffset)) }
        }

    companion object {
        /**
         * 硬化过的 tail 命令，三件套：
         *
         * 1. `timeout` 外套：手机断网常是「socket 开着但不转发字节」，远端 tail 不会被回收；
         *    sshd 没有应用层保活，内核 TCP keepalive 默认两小时才开始探测。每掉线一次就漏一个 `tail -F`
         *    和一个 sshd session。`timeout` 是绝对上限。
         * 2. `2>/dev/null`：sshj 的 stderr 与 stdout 共用同一个通道窗口，`tail -F` 在文件不存在时持续往 stderr
         *    刷重试信息，不读就会耗尽窗口、把 stdout 一起拖停；流式 `exec` 读不了 stderr，只能丢弃。
         * 3. 重连前先 [killStaleCommand] 掉自己上次那条：`timeout` 只保证最终回收，这条是立刻回收。
         *
         * 注意：管道里那条 `tail -f in.ndjson` 不能套 `timeout`。它跑在 tmux 里、就是要常驻，
         * 套上等于定时杀掉正在进行的对话。区别在跑在 SSH exec 上还是跑在 tmux 里。
         */
        fun tailCommand(
            jsonlPath: String,
            startOffsetBytes: Long,
        ): String =
            "timeout $TAIL_MAX_SECONDS tail -c +${startOffsetBytes + 1} -F ${shellQuote(jsonlPath)} 2>/dev/null"

        /**
         * 杀掉这个文件此前遗留的 tail（重连时先跑一次，幂等）。
         *
         * 匹配用完整路径而不是 `tail`：`pkill -f tail` 会连别人开的、别的会话的 tail 一起杀。
         * 匹配串不带具体 offset（每次重连都不同），否则杀不掉上一次那条。
         */
        fun killStaleCommand(jsonlPath: String): String =
            // `|| true`：没有可杀的进程时 pkill 退出码非 0，那是正常情况。
            "pkill -f ${shellQuote("tail -c +[0-9]* -F $jsonlPath")} || true"

        /**
         * 远端 tail 的绝对存活上限（秒）。6 小时比任何一次合理的前台阅读都长。到点后 tail 自己退出，
         * 手机侧表现为流正常结束，上层照常重挂（`tailFlowResilient`）。
         */
        const val TAIL_MAX_SECONDS = 6 * 60 * 60
    }
}
