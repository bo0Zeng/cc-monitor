package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * tmux 面每一条命令串的逐字节定值。
 *
 * 串里有守卫逻辑，错一个字节就是行为变了（`=name:` 的精确匹配包装 · resume 的 cwd 守卫与 `@ccm_sid` 标记 · kill 的成功标记）。
 * 往会话里打字不在这里：走那台核心的 `terminal-input`。
 *
 * 期望值是手写的规格，刻意不引任何生产常量：引了的话，「有人把 `TmuxCommands.TMUX_KILL_OK_MARKER` 改成别的串」
 * 这一刀会恒绿（期望值跟着一起变）。改规格时先写期望、再让实现去对，红了看 diff。
 *
 * 移动端显示设置（`tmuxMobileSetup`）只对要进的那个会话施加（`set -t '=<名>:'`），不用 `set -g`：
 * `-g` 改的是共享 tmux server 的全局默认，后端的 `cc-*` 会话会一起被改；`terminal-overrides` 是服务器级、
 * 收不窄，所以不设。后台新建（`GOLDEN_NEW_DETACHED` / `_RUNNING`）不带 setup：
 * 那一刻没有客户端挂着，之后进它时由进入那条只对它施加。Codex 的 resume 不打 `@ccm_sid`
 * （档案 `hasCcmIdentity=false`，后端那侧 `has_identity: false`）。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 网关里任何一格串变了（`-d` 丢了、`=`/尾冒号丢了、哨兵改名、flag 顺序换了） | 红，且 diff 直接指出是哪一段 |
 * | 新增一支没进这张表的命令产出函数 | 抓不到：由 `TmuxSurfaceLiteralScanTest` 从字面那头、`TmuxGatewayCallSiteTest` 从函数集那头接住 |
 * | 入参空间里这些条没覆盖到的组合（如 `-c` 带工作目录的 resume、别的 `permissionMode`） | 不在射程：由 `RemoteCommandsTest` / `AgentResumeGoldenTest` 的逐项判据管 |
 * | 命令串对远端 tmux 是不是真能跑 | 单测答不了（本仓没有 tmux e2e） |
 * | 谁在调这些函数、有没有人绕开网关 | 本文件不答，`TmuxGatewayCallSiteTest` 答 |
 */
class TmuxGatewayGoldenTest {
    /** 进入 / 新建那一族（含移动端 setup 段）。 */
    @Test
    fun theSessionLifecycleCommandsAreByteIdenticalToTheGolden() {
        assertEquals("移动端 setup 段变了", GOLDEN_MOBILE_SETUP, TmuxCommands.tmuxMobileSetup("work"))
        assertEquals("`new -A` 等价式（无工作目录）变了", GOLDEN_NEW_OR_ATTACH_MIN, TmuxCommands.tmuxNewOrAttach("main"))
        assertEquals(
            "`new -A` 等价式（带工作目录，名字里有空格要被引住）变了",
            GOLDEN_NEW_OR_ATTACH_WD,
            TmuxCommands.tmuxNewOrAttach("proj", "/home/u/my proj"),
        )
        assertEquals("attach（不在 tmux 内）变了", GOLDEN_ATTACH_OUTSIDE, TmuxCommands.tmuxAttachCommand("work", alreadyInTmux = false))
        assertEquals("switch-client（已在 tmux 内，防嵌套）变了", GOLDEN_ATTACH_INSIDE, TmuxCommands.tmuxAttachCommand("work", alreadyInTmux = true))
        assertEquals("后台新建变了", GOLDEN_NEW_DETACHED, TmuxCommands.tmuxNewDetachedCommand("work"))
        assertEquals(
            "后台新建并跑一条命令变了",
            GOLDEN_NEW_DETACHED_RUNNING,
            TmuxCommands.tmuxNewDetachedRunning("work", "claude --resume x"),
        )
    }

    /** 探测 / 查询那一族。 */
    @Test
    fun theProbeAndQueryCommandsAreByteIdenticalToTheGolden() {
        assertEquals("前台探测变了", GOLDEN_FOREGROUND_PROBE, TmuxCommands.tmuxForegroundProbeCommand(CC_NAME))
        assertEquals(
            "pane 子进程探测变了：它是 kill 前的防误杀闸，四态输出少一态就会误杀正在干活的 Claude。",
            GOLDEN_PANE_CHILD_PROBE,
            TmuxCommands.tmuxPaneChildProbeCommand(CC_NAME),
        )
        assertEquals("取会话 cwd 变了", GOLDEN_SESSION_CWD, TmuxCommands.tmuxSessionCwdCommand("work"))
        assertEquals("`tmux ls` 的格式串变了（读它的 `parseTmuxList` 会跟着错位）", GOLDEN_LIST, TmuxCommands.tmuxListCommand())
        assertEquals("抓屏变了", GOLDEN_CAPTURE_PANE, TmuxCommands.tmuxCapturePaneCommand(CC_NAME))
    }

    /** resume 那两支（Claude / Codex）。 */
    @Test
    fun theResumeCommandsAreByteIdenticalToTheGolden() {
        assertEquals(
            "Claude resume 变了：里面有 locate / cwd 守卫 / `@ccm_sid` 标记 / create 分支 send 四段承重逻辑。",
            GOLDEN_RESUME_CLAUDE,
            TmuxCommands.tmuxResumeCommand(
                profile = AgentProfile.of(AgentKind.ClaudeCode),
                name = CC_NAME,
                sessionId = SID,
                agentDir = "~/.claude",
                fallbackCwd = "/home/u/proj",
                alreadyInTmux = false,
                launchCommand = "cct",
            ),
        )
        assertEquals(
            "Claude resume（已在 tmux 内 + 非默认配置目录 + 无 fallback cwd）变了。",
            GOLDEN_RESUME_CLAUDE_INSIDE,
            TmuxCommands.tmuxResumeCommand(
                profile = AgentProfile.of(AgentKind.ClaudeCode),
                name = CC_NAME,
                sessionId = SID,
                agentDir = "/opt/cfg/.claude",
                fallbackCwd = null,
                alreadyInTmux = true,
                launchCommand = null,
            ),
        )
        assertEquals(
            "Codex resume 变了（locate 与 payload 是它与 Claude 版仅有的两处不同，别的必须逐字一致；不打 `@ccm_sid`）。",
            GOLDEN_RESUME_CODEX,
            TmuxCommands.tmuxResumeCommand(
                profile = AgentProfile.of(AgentKind.Codex),
                name = CX_NAME,
                sessionId = CX_SID,
                agentDir = "~/.codex",
                fallbackCwd = "/home/u/proj",
                alreadyInTmux = false,
                launchCommand = null,
            ),
        )
    }

    /** kill / 切模型那两支。 */
    @Test
    fun theKillAndModelCommandsAreByteIdenticalToTheGolden() {
        assertEquals("kill 那条变了：`2>&1 && echo <marker>` 是「必须要求肯定的证据」的那一段。", GOLDEN_KILL, TmuxCommands.tmuxKillCommand("work"))
        assertEquals("切模型那条变了", GOLDEN_SEND_MODEL, TmuxCommands.tmuxSendModelCommand(CC_NAME, "opus"))
    }

    /**
     * 本文件自己不许是空真：证明这套比法真的会红。
     *
     * 上面全是 `assertEquals(定值, 真实输出)`。它唯一可能沦为装饰的走法，是定值里
     * 恰好没装那几个承重片段（例如有人「顺手整理」时把 golden 缩成了半条）。
     * 这里就地问一遍：那几个片段确实在定值里，而且改一格就不等。
     */
    @Test
    fun theGoldensActuallyCarryTheLoadBearingFragments() {
        for (frag in LOAD_BEARING_RESUME) {
            assertTrue("前提：Claude resume 那条 golden 里必须真的有 `$frag`", GOLDEN_RESUME_CLAUDE.contains(frag))
        }
        // 改一格就不等：拿 golden 自己造一刀，断言它不再等于原串。
        assertNotEquals(
            "`new-session -d` 丢掉 `-d` 之后必须不等于原 golden",
            GOLDEN_NEW_DETACHED,
            GOLDEN_NEW_DETACHED.replace("new-session -d ", "new-session "),
        )
    }

    private companion object {
        /** 入参定值。 */
        private const val SID = "0f9d3c2a-1111-4222-8333-444455556666"
        private const val CX_SID = "aaaabbbb-1111-4222-8333-cccc55556666"
        private const val CC_NAME = "cc-0f9d3c2a"
        private const val CX_NAME = "cx-aaaabbbb"

        /** resume 那条串里承重的片段：cwd 守卫、`@ccm_sid` 互操作标记、stale 分支的 shell 白名单。 */
        private val LOAD_BEARING_RESUME =
            listOf(
                "-maxdepth 2 -name ",
                "@ccm_sid ",
                "bash|-bash|sh|-sh|ash|-ash|dash|-dash|zsh|-zsh|ksh|-ksh|csh|-csh|tcsh|-tcsh|fish|-fish|login",
                "aterm: resume 失败",
            )

        private val GOLDEN_MOBILE_SETUP: String =
            """tmux set -t '=work:' status off 2>/dev/null; tmux set -t '=work:' mouse on 2>/dev/null"""

        private val GOLDEN_NEW_OR_ATTACH_MIN: String =
            """tmux new-session -d -s 'main' 2>/dev/null; tmux set -t '=main:' status off 2>/dev/null; tmux set -t '=main:' mouse on 2>/dev/null; tmux attach -t 'main'"""

        private val GOLDEN_NEW_OR_ATTACH_WD: String =
            """tmux new-session -d -s 'proj' -c '/home/u/my proj' 2>/dev/null; tmux set -t '=proj:' status off 2>/dev/null; tmux set -t '=proj:' mouse on 2>/dev/null; tmux attach -t 'proj'"""

        private val GOLDEN_ATTACH_OUTSIDE: String =
            """tmux set -t '=work:' status off 2>/dev/null; tmux set -t '=work:' mouse on 2>/dev/null; tmux attach -t 'work'"""

        private val GOLDEN_ATTACH_INSIDE: String =
            """tmux set -t '=work:' status off 2>/dev/null; tmux set -t '=work:' mouse on 2>/dev/null; tmux switch-client -t 'work'"""

        private val GOLDEN_NEW_DETACHED: String =
            """tmux new-session -d -s 'work'"""

        private val GOLDEN_NEW_DETACHED_RUNNING: String =
            """tmux new-session -d -s 'work'; tmux send-keys -t 'work' 'claude --resume x' Enter"""

        private val GOLDEN_FOREGROUND_PROBE: String =
            """tmux display-message -p -t 'cc-0f9d3c2a' '#{pane_current_command}' 2>/dev/null"""

        private val GOLDEN_PANE_CHILD_PROBE: String =
            """p=${'$'}(tmux display-message -p -t 'cc-0f9d3c2a' '#{pane_pid}' 2>/dev/null); if [ -z "${'$'}p" ]; then echo ATERM_PANE_ABSENT; elif ! command -v pgrep >/dev/null 2>&1; then echo ATERM_PANE_NOPGREP; elif pgrep -P "${'$'}p" >/dev/null 2>&1; then echo ATERM_PANE_HASCHILD; else echo ATERM_PANE_NOCHILD; fi"""

        private val GOLDEN_RESUME_CLAUDE: String =
            """j=${'$'}(find '~/.claude/projects' -maxdepth 2 -name '0f9d3c2a-1111-4222-8333-444455556666.jsonl' -print -quit 2>/dev/null); cwd=${'$'}([ -n "${'$'}j" ] && grep -a -m1 -o '"cwd":"[^"]*"' -- "${'$'}j" 2>/dev/null | head -n 1 | sed 's/.*"cwd":"//;s/"${'$'}//'); [ -z "${'$'}cwd" ] && cwd='/home/u/proj'; if [ -n "${'$'}cwd" ] && [ -d "${'$'}cwd" ]; then if tmux new-session -d -s 'cc-0f9d3c2a' -c "${'$'}cwd" 2>/dev/null; then tmux set-option -t 'cc-0f9d3c2a' @ccm_sid '0f9d3c2a-1111-4222-8333-444455556666' 2>/dev/null; tmux send-keys -t 'cc-0f9d3c2a' 'unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; cct --resume 0f9d3c2a-1111-4222-8333-444455556666' Enter; else case "${'$'}(tmux display-message -p -t 'cc-0f9d3c2a' '#{pane_current_command}' 2>/dev/null)" in (bash|-bash|sh|-sh|ash|-ash|dash|-dash|zsh|-zsh|ksh|-ksh|csh|-csh|tcsh|-tcsh|fish|-fish|login) tmux send-keys -t 'cc-0f9d3c2a' 'unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; cct --resume 0f9d3c2a-1111-4222-8333-444455556666' Enter;; esac; fi; tmux set -t '=cc-0f9d3c2a:' status off 2>/dev/null; tmux set -t '=cc-0f9d3c2a:' mouse on 2>/dev/null; tmux attach -t 'cc-0f9d3c2a'; else echo 'aterm: resume 失败——无法确定工作目录（会话 0f9d3c2a）'; fi"""

        private val GOLDEN_RESUME_CLAUDE_INSIDE: String =
            """j=${'$'}(find '/opt/cfg/.claude/projects' -maxdepth 2 -name '0f9d3c2a-1111-4222-8333-444455556666.jsonl' -print -quit 2>/dev/null); cwd=${'$'}([ -n "${'$'}j" ] && grep -a -m1 -o '"cwd":"[^"]*"' -- "${'$'}j" 2>/dev/null | head -n 1 | sed 's/.*"cwd":"//;s/"${'$'}//'); [ -z "${'$'}cwd" ] && cwd=''; if [ -n "${'$'}cwd" ] && [ -d "${'$'}cwd" ]; then if tmux new-session -d -s 'cc-0f9d3c2a' -c "${'$'}cwd" 2>/dev/null; then tmux set-option -t 'cc-0f9d3c2a' @ccm_sid '0f9d3c2a-1111-4222-8333-444455556666' 2>/dev/null; tmux send-keys -t 'cc-0f9d3c2a' 'unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; claude --resume 0f9d3c2a-1111-4222-8333-444455556666' Enter; else case "${'$'}(tmux display-message -p -t 'cc-0f9d3c2a' '#{pane_current_command}' 2>/dev/null)" in (bash|-bash|sh|-sh|ash|-ash|dash|-dash|zsh|-zsh|ksh|-ksh|csh|-csh|tcsh|-tcsh|fish|-fish|login) tmux send-keys -t 'cc-0f9d3c2a' 'unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; claude --resume 0f9d3c2a-1111-4222-8333-444455556666' Enter;; esac; fi; tmux set -t '=cc-0f9d3c2a:' status off 2>/dev/null; tmux set -t '=cc-0f9d3c2a:' mouse on 2>/dev/null; tmux switch-client -t 'cc-0f9d3c2a'; else echo 'aterm: resume 失败——无法确定工作目录（会话 0f9d3c2a）'; fi"""

        private val GOLDEN_RESUME_CODEX: String =
            """j=${'$'}(find '~/.codex/sessions' -maxdepth 4 -name '*aaaabbbb-1111-4222-8333-cccc55556666.jsonl*' -print -quit 2>/dev/null); cwd=${'$'}([ -n "${'$'}j" ] && grep -a -m1 -o '"cwd":"[^"]*"' -- "${'$'}j" 2>/dev/null | head -n 1 | sed 's/.*"cwd":"//;s/"${'$'}//'); [ -z "${'$'}cwd" ] && cwd='/home/u/proj'; if [ -n "${'$'}cwd" ] && [ -d "${'$'}cwd" ]; then if tmux new-session -d -s 'cx-aaaabbbb' -c "${'$'}cwd" 2>/dev/null; then tmux send-keys -t 'cx-aaaabbbb' 'codex resume aaaabbbb-1111-4222-8333-cccc55556666' Enter; else case "${'$'}(tmux display-message -p -t 'cx-aaaabbbb' '#{pane_current_command}' 2>/dev/null)" in (bash|-bash|sh|-sh|ash|-ash|dash|-dash|zsh|-zsh|ksh|-ksh|csh|-csh|tcsh|-tcsh|fish|-fish|login) tmux send-keys -t 'cx-aaaabbbb' 'codex resume aaaabbbb-1111-4222-8333-cccc55556666' Enter;; esac; fi; tmux set -t '=cx-aaaabbbb:' status off 2>/dev/null; tmux set -t '=cx-aaaabbbb:' mouse on 2>/dev/null; tmux attach -t 'cx-aaaabbbb'; else echo 'aterm: resume 失败——无法确定工作目录（会话 aaaabbbb）'; fi"""

        private val GOLDEN_KILL: String =
            """tmux kill-session -t 'work' 2>&1 && echo __aterm_kill_ok__"""

        private val GOLDEN_SEND_MODEL: String =
            """tmux send-keys -t 'cc-0f9d3c2a' '/model opus' Enter"""

        private val GOLDEN_CAPTURE_PANE: String =
            """tmux capture-pane -t 'cc-0f9d3c2a' -p"""

        private val GOLDEN_SESSION_CWD: String =
            """tmux display-message -p -t 'work' '#{pane_current_path}'"""

        private val GOLDEN_LIST: String =
            """tmux ls -F '#{session_name}${'\t'}#{session_windows}${'\t'}#{?session_attached,1,0}${'\t'}#{pane_current_command}' 2>/dev/null"""
    }
}
