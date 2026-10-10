package com.ccmonitor.mobile.core.claude.command

import com.ccmonitor.mobile.core.claude.bridge.PipeSession
import com.ccmonitor.mobile.core.claude.transport.ByteRange
import com.ccmonitor.mobile.core.claude.transport.SkeletonScan
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.emptyFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 管道网关的每一条命令串逐字节钉成 golden。
 *
 * 这些串的任何一个字节变了，失败形态都是「管道起成功了、界面永远空白、说不出为什么」，
 * 所以期望值是真跑生产函数取回的字节，且刻意不引任何生产常量：引了的话，
 * 改常量那一刀会恒绿（期望值跟着一起变）。
 *
 * 抓得到：网关里任何一格串变了（`>>` 写成 `>`、`-n 0` 丢了、路径变了、flag 顺序换了）。
 * 抓不到：新增一支不在这里的命令产出函数（由 app 侧的源码扫描接住）；入参组合没覆盖到的格
 * （由 `ClaudeInvocationPipeTest` 逐项管）；命令在远端是不是真能跑（单测答不了）。
 */
class PipeGatewayGoldenTest {
    /**
     * 起管道那一串（四支入参组合）。
     *
     * 四支覆盖的分叉：无 workdir / 带 workdir+权限+resume+配置目录 / 新对话带 workdir（远端当场决定 flag）/
     * 新对话无 workdir（问不了 ⇒ 退回原样传 `--session-id`）。
     */
    @Test
    fun theStartCommandIsByteIdenticalToTheGolden() {
        assertEquals(
            "起管道（最小入参）的命令串变了。",
            GOLDEN_START_MIN,
            ClaudeInvocation.pipeInvocation(null, SID),
        )
        assertEquals(
            "起管道（workdir + 权限模式 + resume + 配置目录）的命令串变了。",
            GOLDEN_START_FULL,
            ClaudeInvocation.pipeInvocation("cct", SID, "/home/u/proj", "plan", SID, null, "/home/u/.claude"),
        )
        assertEquals(
            "起管道（新对话 + workdir ⇒ 远端当场决定用哪个 flag）的命令串变了。",
            GOLDEN_START_NEW,
            ClaudeInvocation.pipeInvocation("cct", SID, "/home/u/proj", null, null, NEW_SID, null),
        )
        assertEquals(
            "起管道（新对话但没有 workdir ⇒ 问不了，退回原样传 `--session-id`）的命令串变了。",
            GOLDEN_START_NEW_NO_WORKDIR,
            ClaudeInvocation.pipeInvocation(null, SID, null, null, null, NEW_SID, null),
        )
    }

    /** 写那两串（幂等追加 · 盲追加/中断）。 */
    @Test
    fun theUplinkCommandsAreByteIdenticalToTheGolden() {
        val session = PipeSession(RemoteCommandChannel { emptyFlow() }, SID)
        assertEquals(
            "幂等追加的命令串变了：针、路径、两个哨兵、`2>/dev/null` 都在这一串里。",
            GOLDEN_UP_IDEMPOTENT,
            session.idempotentAppendCommand("你好 'q' \\n", "aterm-abc-local#1"),
        )
        assertEquals(
            "中断（盲追加）的命令串变了。",
            GOLDEN_UP_INTERRUPT,
            session.interruptCommand("req_1_deadbeef"),
        )
    }

    /** 读那一串与三个落点。 */
    @Test
    fun theReadCommandAndLayoutPathsAreByteIdenticalToTheGolden() {
        assertEquals("会话目录变了", GOLDEN_SESSION_DIR, PipeCommands.sessionDir(SID))
        assertEquals("上行文件落点变了", GOLDEN_PATH_IN, PipeCommands.inPath(SID))
        assertEquals("下行契约文件落点变了", GOLDEN_PATH_EVENTS, PipeCommands.eventsPath(SID))
        assertEquals("stderr 落点变了", GOLDEN_PATH_LOG, PipeCommands.logPath(SID))
        assertEquals(
            "读诊断日志那条有界命令变了。",
            GOLDEN_READ_DIAGNOSTICS,
            PipeCommands.diagnosticsReadCommand(SID, DIAG_CAP),
        )
    }

    /**
     * 读诊断日志那条命令等于
     * `SkeletonScan.rangeContentCommand("<会话目录>/<日志名>", ByteRange(0, LOG_BYTE_CAP))`：
     * 网关没有另写一套有界读，用的就是这条原语。
     */
    @Test
    fun theDiagnosticsReadCommandIsTheBoundedRangeRead() {
        val before = SkeletonScan.rangeContentCommand("${PipeCommands.sessionDir(SID)}/bridge.log", ByteRange(0, DIAG_CAP))
        assertEquals(
            "网关那条读命令与有界读原语不再等价。",
            before,
            PipeCommands.diagnosticsReadCommand(SID, DIAG_CAP),
        )
    }

    /** 附件那两串（建目录命令 · 上传落点）。 */
    @Test
    fun theAttachmentCommandsAreByteIdenticalToTheGolden() {
        assertEquals("建附件目录的命令变了", GOLDEN_ATT_MKDIR, ChatAttachment.mkdirCommand(SID))
        assertEquals("附件上传落点变了", GOLDEN_ATT_PATH, ChatAttachment.remotePath(SID, "a b.png"))
    }

    /**
     * 这套比法自己不许是空真。
     *
     * 上面全是 `assertEquals(定值, 真实输出)`；唯一可能沦为装饰的走法是定值里恰好没装那几个承重片段。
     * 这里就地问一遍：那几个片段确实在定值里，而且改一格就不等。
     */
    @Test
    fun theGoldensActuallyCarryTheLoadBearingFragments() {
        for (frag in LOAD_BEARING) {
            assertTrue("前提：起管道那条 golden 里必须真的有 `$frag`，否则这条判据守了个寂寞", GOLDEN_START_MIN.contains(frag))
        }
        // 改一格就不等：拿 golden 自己造两刀（`>>` 变 `>`、`-n 0` 丢掉），断言两刀都不再等于原串。
        assertNotEquals("把 `} >> ` 改成 `} > ` 之后必须不等于原 golden", GOLDEN_START_MIN, GOLDEN_START_MIN.replace("} >> ", "} > "))
        assertNotEquals("把 `tail -n 0 -f` 改成 `tail -f` 之后必须不等于原 golden", GOLDEN_START_MIN, GOLDEN_START_MIN.replace("tail -n 0 -f", "tail -f"))
    }

    private companion object {
        /** 入参定值，与抓 golden 时用的那一组逐字相同。 */
        private const val SID = "0f9d3c2a-1111-4222-8333-444455556666"
        private const val NEW_SID = "11111111-2222-4333-8444-555566667777"

        /** 读诊断日志的字节上限；与 `PipeDiagnostics.LOG_BYTE_CAP` 同值，刻意手写（引常量会让改常量那一刀恒绿）。 */
        private const val DIAG_CAP = 4096L

        /**
         * 起管道那条串里承重的片段，少一个都不是风格差异：
         * `-n 0` 丢了 = 重建时重复花额度；`2>>` 丢了 = 诊断静默；
         * `--include-partial-messages` 丢了 = 没有逐字流式；`unset` 丢了 = 会话不写 JSONL。
         */
        private val LOAD_BEARING =
            listOf(
                "unset CLAUDECODE",
                "mkdir -p ",
                " && touch ",
                "tail -n 0 -f ",
                "--include-partial-messages",
                "} >> ",
                " 2>> ",
            )

        private val GOLDEN_SESSION_DIR: String =
            """.aterm/s/0f9d3c2a-1111-4222-8333-444455556666"""

        private val GOLDEN_START_MIN: String =
            """unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; mkdir -p '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666' && touch '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson' && { printf '%s\n' '{"__meta__":{"source":"pipeline","proto":"anthropic-sse-v1","by":"aterm"}}'; tail -n 0 -f '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson' | env claude --input-format stream-json --output-format stream-json --include-partial-messages --verbose | { ATERM_T0=${'$'}(date +%s); ATERM_N=0; while IFS= read -r l; do ATERM_N=${'$'}((ATERM_N+1)); printf '{"t_ns":%s%09d,"event":%s}\n' "${'$'}ATERM_T0" "${'$'}ATERM_N" "${'$'}l"; done; }; } >> '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/events.ndjson' 2>> '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/bridge.log'"""

        private val GOLDEN_START_FULL: String =
            """unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; mkdir -p '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666' && touch '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson' && { printf '%s\n' '{"__meta__":{"source":"pipeline","proto":"anthropic-sse-v1","by":"aterm"}}'; tail -n 0 -f '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson' | ( cd '/home/u/proj' && exec env CLAUDE_CONFIG_DIR='/home/u/.claude' cct --permission-mode plan --resume 0f9d3c2a-1111-4222-8333-444455556666 --input-format stream-json --output-format stream-json --include-partial-messages --verbose ) | { ATERM_T0=${'$'}(date +%s); ATERM_N=0; while IFS= read -r l; do ATERM_N=${'$'}((ATERM_N+1)); printf '{"t_ns":%s%09d,"event":%s}\n' "${'$'}ATERM_T0" "${'$'}ATERM_N" "${'$'}l"; done; }; } >> '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/events.ndjson' 2>> '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/bridge.log'"""

        private val GOLDEN_START_NEW: String =
            """unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; if [ -e "${'$'}{CLAUDE_CONFIG_DIR:-${'$'}HOME/.claude}"/projects/-home-u-proj/11111111-2222-4333-8444-555566667777.jsonl ]; then ATERM_SID_FLAG='--resume 11111111-2222-4333-8444-555566667777'; else ATERM_SID_FLAG='--session-id 11111111-2222-4333-8444-555566667777'; fi; mkdir -p '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666' && touch '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson' && { printf '%s\n' '{"__meta__":{"source":"pipeline","proto":"anthropic-sse-v1","by":"aterm"}}'; tail -n 0 -f '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson' | ( cd '/home/u/proj' && exec env cct ${'$'}ATERM_SID_FLAG --input-format stream-json --output-format stream-json --include-partial-messages --verbose ) | { ATERM_T0=${'$'}(date +%s); ATERM_N=0; while IFS= read -r l; do ATERM_N=${'$'}((ATERM_N+1)); printf '{"t_ns":%s%09d,"event":%s}\n' "${'$'}ATERM_T0" "${'$'}ATERM_N" "${'$'}l"; done; }; } >> '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/events.ndjson' 2>> '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/bridge.log'"""

        private val GOLDEN_START_NEW_NO_WORKDIR: String =
            """unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; mkdir -p '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666' && touch '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson' && { printf '%s\n' '{"__meta__":{"source":"pipeline","proto":"anthropic-sse-v1","by":"aterm"}}'; tail -n 0 -f '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson' | env claude --session-id 11111111-2222-4333-8444-555566667777 --input-format stream-json --output-format stream-json --include-partial-messages --verbose | { ATERM_T0=${'$'}(date +%s); ATERM_N=0; while IFS= read -r l; do ATERM_N=${'$'}((ATERM_N+1)); printf '{"t_ns":%s%09d,"event":%s}\n' "${'$'}ATERM_T0" "${'$'}ATERM_N" "${'$'}l"; done; }; } >> '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/events.ndjson' 2>> '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/bridge.log'"""

        private val GOLDEN_UP_IDEMPOTENT: String =
            """if grep -qF '"aterm_id":"aterm-abc-local#1"' '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson' 2>/dev/null; then printf 'ATERM_UP_DUP\n'; else printf '%s\n' '{"type":"user","aterm_id":"aterm-abc-local#1","message":{"role":"user","content":"你好 '\''q'\'' \\n"}}' >> '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson' && printf 'ATERM_UP_ADD\n'; fi"""

        private val GOLDEN_UP_INTERRUPT: String =
            """printf '%s\n' '{"type":"control_request","request_id":"req_1_deadbeef","request":{"subtype":"interrupt"}}' >> '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson'"""

        private val GOLDEN_ATT_MKDIR: String =
            """mkdir -p '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/att'"""

        private val GOLDEN_ATT_PATH: String =
            """.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/att/up-a_b.png"""

        private val GOLDEN_PATH_IN: String =
            """.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/in.ndjson"""

        private val GOLDEN_PATH_EVENTS: String =
            """.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/events.ndjson"""

        private val GOLDEN_PATH_LOG: String =
            """.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/bridge.log"""

        private val GOLDEN_READ_DIAGNOSTICS: String =
            """LC_ALL=C tail -c +1 '.aterm/s/0f9d3c2a-1111-4222-8333-444455556666/bridge.log' | LC_ALL=C head -c 4096"""
    }
}
