package com.ccmonitor.mobile.core.claude.command

import com.ccmonitor.mobile.core.claude.transport.ByteRange
import com.ccmonitor.mobile.core.claude.transport.SkeletonScan
import com.ccmonitor.mobile.core.remote.shellQuote

/**
 * app 自己那条文件管道的远端命令网关：路径、起、写、读诊断全在这里拼，纯函数，只造串不执行。
 *
 * 只管 app 自己的会话通道（`~/.aterm/s/<sid>/`）。读 agent 自己的会话记录（`~/.claude/projects` 下的 jsonl）
 * 是另一面，在 `TailTransport` / `SkeletonScan` / `ResumeOffset`。下行与诊断日志的读借那边既有的有界读，
 * 这里只给落点（[eventsPath] / [logPath]）和一条成品命令（[diagnosticsReadCommand]）。
 *
 * 管子（建目录、建上行文件、`tail -n 0 -f` 喂 stdin、信封、重定向）是协议，归这里；管子中间跑的程序
 * 是 agent 知识，由调用方经 [startCommand] 的 `agentCommand` 传进来。接第二个 agent 时只改后者。
 *
 * 注意：[idempotentAppendCommand] 把查重与追加放进同一条远端命令，没有网络往返，但远端本机仍有窗口：
 * 追加写到一半被打断会留下残行。而且 dash 的 `printf` 超过 8 KiB 会分多次 `write(2)`，`O_APPEND`
 * 只保证单次原子，并发写方（中断不受在飞门约束、两台设备同时打字）可能把长行撕开。
 * 根治要先写临时文件再 `mv`。
 *
 * 不知 tmux，不执行，不定义 `ATERM_HOME`（在 [ClaudeInvocation]）。
 */
object PipeCommands {
    // ---- 布局 -------------------------------------------------------------------

    /**
     * 一个会话的文件通道目录，相对登录 cwd。
     *
     * 要守的不变量是起管道的一端与读写的一端指同一个目录。写成 `$HOME/...` 时，一端放双引号（展开）、
     * 另一端经 `shellQuote` 单引号（不展开），就会一端建真目录、一端读字面 `$HOME` 目录，下行完全静默。
     * 相对路径由构造保证一致；前提是两次 exec 的 cwd 相同（同主机同用户的非交互 exec 恒起于家目录）。
     *
     * 相对路径不意味着 agent 在这个目录里干活：`cd <项目>` 收在子 shell 里，只有 agent 进程换 cwd。
     *
     * 校验借 [ClaudeInvocation.isValidSessionId]：这条管道目前只承载 Claude 会话；接第二个 agent 时应改从 `AgentProfile` 问。
     */
    fun sessionDir(sessionId: String): String {
        require(ClaudeInvocation.isValidSessionId(sessionId)) { "非法 Claude sessionId: $sessionId" }
        return "${ClaudeInvocation.ATERM_HOME}/s/$sessionId"
    }

    /** 上行文件的远端路径。写方与 `tail -f` 必须是同一处，所以只有这一个产地。 */
    fun inPath(sessionId: String): String = "${sessionDir(sessionId)}/$IN"

    /** 下行契约文件的远端路径（`PipeSession.frames` 交给 `TailTransport` 的那个）。 */
    fun eventsPath(sessionId: String): String = "${sessionDir(sessionId)}/$EVENTS"

    /** 诊断日志的远端路径（`PipeDiagnostics` 读的那个）。 */
    fun logPath(sessionId: String): String = "${sessionDir(sessionId)}/$LOG"

    // ---- 起 ---------------------------------------------------------------------

    /**
     * 起常驻管道（管子这一半）：
     *
     * ```sh
     * <prelude>mkdir -p <dir> && touch <dir>/in && { printf 契约首行; tail -n 0 -f <dir>/in | <agentCommand> | 信封; } >> <dir>/events 2>> <dir>/log
     * ```
     *
     * `tail -f` 让 stdin 永不 EOF，进程常驻；上行就是往上行文件追加一行，中断控制帧也走同一条 stdin。
     *
     * 约束：
     * 1. 目录要先建：`>>` 不建父目录。
     * 2. `-n 0` 不能省：`tail -f` 默认先重放最后 10 行，重建管道时那正是用户最近的话与控制帧，等于重复喂给 agent。
     * 3. stderr 必须有落点：否则 agent 一句 "Not logged in" 就退出，手机上只看到界面一直空白。
     *    不能 `2>&1`：混进下行契约文件的非 JSON 行会被逐行丢弃，诊断和数据一起毁掉。
     *
     * @param agentCommand 管子中间那个程序的完整调用串（含 flag 与可能的 `( cd … && exec … )` 子 shell），原样嵌，不加引号。
     * @param prelude 整条命令最前面那一段（unset 与新对话的 flag 判定），同样原样嵌。
     */
    fun startCommand(
        sessionId: String,
        agentCommand: String,
        prelude: String = "",
    ): String {
        val dir = sessionDir(sessionId)
        return prelude + "mkdir -p ${shellQuote(dir)} && touch ${shellQuote("$dir/$IN")} && " +
            // 落盘文件名、首行 `__meta__.source` 与信封是对外契约：relay / native / pipeline 三种产出方式
            // 对下游同构，换产出方式时下游一行不改。
            "{ printf '%s\\n' ${shellQuote(META_LINE)}; " +
            "tail -n 0 -f ${shellQuote("$dir/$IN")} | $agentCommand | " +
            // 每行套上信封 `{"t_ns":…,"event":…}`，见 [ENVELOPE_LOOP]。
            "$ENVELOPE_LOOP; " +
            "} >> ${shellQuote("$dir/$EVENTS")} 2>> ${shellQuote("$dir/$LOG")}"
    }

    // ---- 写 ---------------------------------------------------------------------

    /**
     * 往上行文件盲追加一行（不查重），控制帧走这条。
     *
     * 用 `printf '%s\n'` 不用 `echo`：正文是任意用户文本，`echo` 对反斜杠和 `-n` 各家 shell 不一致。
     * [payload] 应已是 JSON，本函数整体 shell-quote；路径不含 `$HOME`，见 [sessionDir]。
     */
    fun appendLineCommand(
        sessionId: String,
        payload: String,
    ): String = "printf '%s\\n' ${shellQuote(payload)} >> ${shellQuote(inPath(sessionId))}"

    /**
     * 同一条远端命令里先查标识在不在，不在才追加：
     * ```sh
     * if grep -qF '<needle>' '<dir>/in' 2>/dev/null; then printf 'ATERM_UP_DUP\n';
     * else printf '%s\n' '<payload>' >> '<dir>/in' && printf 'ATERM_UP_ADD\n'; fi
     * ```
     *
     * 必须是一条 exec：拆成先查再追加两条，读写之间就有了网络往返的竞态窗口。
     * `2>/dev/null` 给「文件还不存在」：grep 退 2 正好走 else（追加会创建文件），丢掉 stderr 免得污染 stdout 判读。
     *
     * @param needle `grep -qF` 的定长针，必须是 [payload] 的字面子串，由调用方保证；本网关不认识 JSON。
     */
    fun idempotentAppendCommand(
        sessionId: String,
        payload: String,
        needle: String,
    ): String {
        val path = shellQuote(inPath(sessionId))
        return "if grep -qF ${shellQuote(needle)} $path 2>/dev/null; then printf '$DUPLICATE_MARKER\\n'; " +
            "else printf '%s\\n' ${shellQuote(payload)} >> $path && printf '$APPENDED_MARKER\\n'; fi"
    }

    /**
     * 在会话目录下建一个子目录（附件落点用）。`mkdir` 是会写远端的动词，所以也收在网关里。
     */
    fun mkdirCommand(remotePath: String): String = "mkdir -p ${shellQuote(remotePath)}"

    // ---- 读 ---------------------------------------------------------------------

    /**
     * 读诊断日志的有界命令（`tail -c +N | head -c LEN`）。必须有界：远端进程可能往 stderr 吐几百 MB。
     */
    fun diagnosticsReadCommand(
        sessionId: String,
        capBytes: Long,
    ): String = SkeletonScan.rangeContentCommand(logPath(sessionId), ByteRange(0, capBytes))

    // ---- 契约常量 ---------------------------------------------------------------

    /** 下行契约文件名。下游只认它，不认产出方式。 */
    const val EVENTS = "events.ndjson"

    /** 上行文件名。 */
    const val IN = "in.ndjson"

    /** 管道的 stderr 落点，不进 [EVENTS]（那是逐行 JSON 的契约文件）。 */
    const val LOG = "bridge.log"

    /**
     * 契约首行 `{"__meta__":{"source":…}}`。本管道是 `"pipeline"`；daemon 中转是 `"relay"`，
     * 官方 `--event-file` 是 `"native"`，三者对下游同构。
     */
    const val META_LINE: String =
        """{"__meta__":{"source":"pipeline","proto":"anthropic-sse-v1","by":"aterm"}}"""

    /** 远端答「已经在里面了」的哨兵。必须是肯定证据：`exec` 拿不到退出码。 */
    const val DUPLICATE_MARKER: String = "ATERM_UP_DUP"

    /** 远端答「这一次真写进去了」的哨兵，理由同 [DUPLICATE_MARKER]。 */
    const val APPENDED_MARKER: String = "ATERM_UP_ADD"

    /**
     * 给下行每一行套上信封 `{"t_ns":…,"event":…}`。
     *
     * shell 循环 + 只取一次基准时刻：`printf` 在 dash/bash/BusyBox ash 里都是内建，整条循环零 fork。
     * 每行 `date +%s%N` 不行：每行一次 fork 很慢，而且 `%N` 是 GNU 扩展，BusyBox 上原样吐出，每行都不是 JSON。
     * awk 也不行：mawk 对 stdin 块缓冲，攒到 EOF 才吐，逐字流式就没了。
     *
     * `t_ns` 是排序键，不是时钟：管道启动那一刻的秒数 × 10^9 + 行序。排序正确，但不是墙钟也不单调。
     * 用 `%s%09d` 字符串拼接而不是乘法，因为 1.7×10^18 超过 double 的 2^53。
     */
    const val ENVELOPE_LOOP: String =
        "{ ATERM_T0=\$(date +%s); ATERM_N=0; while IFS= read -r l; do ATERM_N=\$((ATERM_N+1)); " +
            "printf '{\"t_ns\":%s%09d,\"event\":%s}\\n' \"\$ATERM_T0\" \"\$ATERM_N\" \"\$l\"; done; }"
}
