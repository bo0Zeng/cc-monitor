package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.ssh.TmuxCommands.PANE_PROBE_ABSENT
import com.ccmonitor.mobile.ssh.TmuxCommands.PANE_PROBE_HAS_CHILD
import com.ccmonitor.mobile.ssh.TmuxCommands.PANE_PROBE_NO_CHILD
import com.ccmonitor.mobile.ssh.TmuxCommands.PANE_PROBE_NO_PGREP
import com.ccmonitor.mobile.ssh.TmuxCommands.RESUME_FOREGROUND_SHELLS
import com.ccmonitor.mobile.ssh.TmuxCommands.isResumeForegroundShell
import com.ccmonitor.mobile.ssh.TmuxCommands.paneConfirmedNoChild
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxAttachCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxCapturePaneCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxForegroundProbeCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxKillCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxKillSucceeded
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxListCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxMobileSetup
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxNewDetachedCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxNewDetachedRunning
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxNewOrAttach
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxPaneChildProbeCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxResumeCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxSendModelCommand
import com.ccmonitor.mobile.ssh.TmuxCommands.tmuxSessionCwdCommand
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 远端命令生成纯函数（命令字符串是终端工效与启动器的地基，须逐字稳定）。
 *
 * tmux 那族住 [TmuxCommands] 网关；`shQuote` / `appendAuthorizedKeyCommand` 住 `RemoteCommands.kt`。
 */
class RemoteCommandsTest {
    // 移动端 setup 段（status off + 唯一的滚动模式 mouse on），进会话前无条件注入。逐字锁死；
    // 下方拼接断言仍是全串 assertEquals。只对进的那个会话（`set -t '=<名>:'`），不用 `set -g`（整台机器）；
    // 不设 `terminal-overrides`（服务器级、无法收窄），理由见 `TmuxCommands.tmuxMobileSetup` 的 KDoc。
    private val defaultClaude = AgentProfile.of(AgentKind.ClaudeCode)
    private val codex = AgentProfile.of(AgentKind.Codex)

    private fun setup(session: String) = "tmux set -t '=$session:' status off 2>/dev/null; tmux set -t '=$session:' mouse on 2>/dev/null"

    // 移动端 setup 契约：滚动模式（mouse on）是唯一模式，无条件随 tmux 进入序列施加。
    @Test fun tmuxMobileSetupBakesScrollModeOnTheNamedSessionOnly() {
        assertEquals(setup("work"), tmuxMobileSetup("work"))
        // 空名字：`=:` 会被 tmux 解析成「当前会话」——那不是我们点名的会话 ⇒ 不施加。
        assertNull(tmuxMobileSetup(""))
    }

    @Test fun shQuoteWrapsPlain() = assertEquals("'main'", shQuote("main"))

    @Test fun shQuoteEscapesSingleQuote() = assertEquals("""'it'\''s'""", shQuote("it's"))

    @Test fun shQuoteHandlesSpaces() = assertEquals("'a b'", shQuote("a b"))

    @Test fun tmuxNewOrAttachNoWd() = assertEquals("tmux new-session -d -s 'main' 2>/dev/null; ${setup("main")}; tmux attach -t 'main'", tmuxNewOrAttach("main"))

    @Test fun tmuxNewOrAttachWithWd() =
        assertEquals("tmux new-session -d -s 'proj' -c '/srv/app' 2>/dev/null; ${setup("proj")}; tmux attach -t 'proj'", tmuxNewOrAttach("proj", "/srv/app"))

    @Test fun tmuxAttachWhenNotInTmux() = assertEquals("${setup("work")}; tmux attach -t 'work'", tmuxAttachCommand("work", alreadyInTmux = false))

    @Test fun tmuxAttachWhenAlreadyInTmuxUsesSwitchClient() =
        assertEquals("${setup("work")}; tmux switch-client -t 'work'", tmuxAttachCommand("work", alreadyInTmux = true))

    // 后台新建不施加显示设置（此刻没有客户端挂着它；之后从 app 里进它时由进入那条只对它施加）。
    @Test fun tmuxNewDetached() = assertEquals("tmux new-session -d -s 'bg'", tmuxNewDetachedCommand("bg"))

    /**
     * kill 命令带成功标记。
     *
     * `execStream` 只泵 stdout、不读退出码与 stderr：裸 `tmux kill-session -t 'bg'` 失败时 stdout 为空、
     * `runCatching` 走 onSuccess，就会把失败报成「已结束」。
     * `2>&1` 让诊断信息不丢，`&& echo <marker>` 只在真成功时输出。
     */
    @Test fun tmuxKillCarriesSuccessMarker() {
        val cmd = tmuxKillCommand("bg")
        assertEquals("tmux kill-session -t 'bg' 2>&1 && echo __aterm_kill_ok__", cmd)
        assertTrue("会话名必须被 quote", cmd.contains("'bg'"))
    }

    /** 判定必须要求肯定的证据——命令没跑起来时 stdout 同样是空的。 */
    @Test fun tmuxKillSuccessNeedsPositiveEvidence() {
        assertTrue(tmuxKillSucceeded("__aterm_kill_ok__\n"))
        assertFalse("空输出不算成功（命令可能根本没跑起来）", tmuxKillSucceeded(""))
        assertFalse("只有 stderr 不算成功", tmuxKillSucceeded("can't find session: bg\n"))
    }

    @Test fun tmuxSessionCwd() =
        assertEquals("tmux display-message -p -t 'work' '#{pane_current_path}'", tmuxSessionCwdCommand("work"))

    @Test fun tmuxListFormatStable() =
        assertEquals(
            "tmux ls -F '#{session_name}\t#{session_windows}\t#{?session_attached,1,0}\t#{pane_current_command}' 2>/dev/null",
            tmuxListCommand(),
        )

    @Test fun tmuxNewDetachedRunningCommand() =
        assertEquals(
            "tmux new-session -d -s 'bg'; tmux send-keys -t 'bg' 'claude' Enter",
            tmuxNewDetachedRunning("bg", "claude"),
        )

    @Test fun tmuxNewDetachedRunningQuotesNameWithSpace() =
        assertEquals(
            "tmux new-session -d -s 'my bg'; tmux send-keys -t 'my bg' 'claude' Enter",
            tmuxNewDetachedRunning("my bg", "claude"),
        )

    @Test fun sessionNameWithSpaceIsQuoted() = assertEquals("${setup("my proj")}; tmux attach -t 'my proj'", tmuxAttachCommand("my proj", alreadyInTmux = false))

    @Test fun tmuxSendModelCommandPlain() =
        assertEquals("tmux send-keys -t 'cc' '/model opus' Enter", tmuxSendModelCommand("cc", "opus"))

    @Test fun tmuxSendModelCommandQuotesSessionWithSpace() =
        assertEquals("tmux send-keys -t 'my cc' '/model sonnet' Enter", tmuxSendModelCommand("my cc", "sonnet"))

    @Test fun tmuxCapturePanePlain() = assertEquals("tmux capture-pane -t 'cc' -p", tmuxCapturePaneCommand("cc"))

    @Test fun tmuxCapturePaneQuotesSessionWithSpace() =
        assertEquals("tmux capture-pane -t 'my cc' -p", tmuxCapturePaneCommand("my cc"))

    // 公钥推送命令 —— 幂等 + 净化 + printf 防注入。
    @Test fun appendAuthorizedKeyIsIdempotentAndQuoted() {
        val cmd = appendAuthorizedKeyCommand("ssh-ed25519 AAAAKEY comment")
        assertTrue(cmd.contains("grep -qxF 'ssh-ed25519 AAAAKEY comment'")) // 整行精确匹配去重
        assertTrue(cmd.contains(">> ~/.ssh/authorized_keys")) // 追加
        assertTrue(cmd.contains("mkdir -p ~/.ssh")) // 首次建目录 + chmod
        assertTrue(cmd.contains("ATERM_ADDED") && cmd.contains("ATERM_ALREADY")) // 反馈标记
        assertTrue("写入应用 printf 而非 echo", cmd.contains("printf '%s\\n' 'ssh-ed25519 AAAAKEY comment'"))
    }

    @Test fun appendAuthorizedKeyStripsRealNewlinesToPreventSecondLineInjection() {
        val cmd = appendAuthorizedKeyCommand("ssh-ed25519 AAAAKEY evil\nrm -rf ~")
        assertFalse("净化后不应残留换行注入的第二行", cmd.contains("\nrm -rf ~"))
        assertTrue("真实换行应被折叠成单行并引用", cmd.contains("'ssh-ed25519 AAAAKEY evil rm -rf ~'"))
    }

    @Test fun appendAuthorizedKeyUsesPrintfSoLiteralBackslashNStaysOneLine() {
        // 字面 \n（反斜杠+字母 n，非真换行）——净化(\s)挡不住，靠 printf '%s' 不解释反斜杠闭合（dash/ash 的 echo 会展开）。
        val cmd = appendAuthorizedKeyCommand("""ssh-ed25519 KEY a\nEVIL""")
        assertFalse("不应用 echo 写公钥（dash 会把 \\n 展开成换行）", cmd.contains("echo 'ssh-ed25519"))
        assertTrue("字面反斜杠n 原样保留在引用公钥里，printf %s 不解释", cmd.contains("""printf '%s\n' 'ssh-ed25519 KEY a\nEVIL'"""))
    }

    // === resume：远端 find-by-sid 读权威 cwd → cwd 守卫 → create → 前台探测 send → attach ===

    /** resume 命令的 find/extract/fallback 段（期望串组件；claudeDir=内置默认 → 双引号 shell 表达式）。 */
    private val defaultClaudeDir = "\${CLAUDE_CONFIG_DIR:-\$HOME/.claude}"

    private fun locateSeg(sid: String) = "j=\$(find \"$defaultClaudeDir/projects\" -maxdepth 2 -name '$sid.jsonl' -print -quit 2>/dev/null)"

    private val extractSeg =
        "cwd=\$([ -n \"\$j\" ] && grep -a -m1 -o '\"cwd\":\"[^\"]*\"' -- \"\$j\" 2>/dev/null | head -n 1 | sed 's/.*\"cwd\":\"//;s/\"\$//')"

    private fun resume(
        name: String,
        sid: String,
        fallbackCwd: String?,
        alreadyInTmux: Boolean = false,
        launchCommand: String? = null,
    ) = tmuxResumeCommand(defaultClaude, name, sid, defaultClaudeDir, fallbackCwd, alreadyInTmux, launchCommand)

    @Test fun tmuxCodexResumeUsesSubcommandAndDateGlobFindNoUnset() {
        // Codex resume：`codex resume <uuid>` 子命令 + find 走 sessions 日期分区（*uuid.jsonl*）；无 --resume/无 unset。
        val uuid = "019f78e8-84dd-7ac0-b479-e9c1b9caec67"
        val cmd = tmuxResumeCommand(codex, "cx-019f78e8", uuid, "/h/.codex", "/home/pi/proj", alreadyInTmux = false)
        assertTrue("find 走 sessions 目录 + maxdepth 4 + *uuid.jsonl* 名：$cmd", cmd.contains("find '/h/.codex/sessions' -maxdepth 4 -name '*$uuid.jsonl*'"))
        assertTrue("payload=codex resume <uuid>", cmd.contains("'codex resume $uuid'"))
        assertFalse("非 --resume flag", cmd.contains("--resume"))
        assertFalse("无 unset 嵌套环境（Codex 无此病）", cmd.contains("unset "))
        assertTrue("cwd 抽取沿用 grep（Codex session_meta 首个 cwd）", cmd.contains("grep -a -m1 -o '\"cwd\":\"[^\"]*\"'"))
        assertTrue("tmux 编排一致（new-session + attach）", cmd.contains("tmux new-session -d -s 'cx-019f78e8'") && cmd.contains("tmux attach -t 'cx-019f78e8'"))
    }

    @Test fun tmuxResumeNotInTmuxAttaches() {
        val cmd = resume("cc-abcd", "abcd1234-uuid", "/home/pi/proj")
        val payloadQ =
            "'unset CLAUDECODE CLAUDE_CODE_ENTRYPOINT CLAUDE_CODE_SESSION_ID CLAUDE_CODE_CHILD_SESSION; claude --resume abcd1234-uuid'"
        val sendSeg = "tmux send-keys -t 'cc-abcd' $payloadQ Enter"
        assertEquals(
            "${locateSeg("abcd1234-uuid")}; $extractSeg; [ -z \"\$cwd\" ] && cwd='/home/pi/proj'; " +
                "if [ -n \"\$cwd\" ] && [ -d \"\$cwd\" ]; then " +
                // create 成功（fresh）→ 无条件 send；失败（stale）→ 才 guard 探前台补送
                // create 成功分支先设 @ccm_sid（后端 attach/kill 靠它定位），再 send
                "if tmux new-session -d -s 'cc-abcd' -c \"\$cwd\" 2>/dev/null; then tmux set-option -t 'cc-abcd' @ccm_sid 'abcd1234-uuid' 2>/dev/null; $sendSeg; " +
                "else case \"\$(tmux display-message -p -t 'cc-abcd' '#{pane_current_command}' 2>/dev/null)\" in " +
                "(bash|-bash|sh|-sh|ash|-ash|dash|-dash|zsh|-zsh|ksh|-ksh|csh|-csh|tcsh|-tcsh|fish|-fish|login) " +
                "$sendSeg;; esac; fi; " +
                "${setup("cc-abcd")}; tmux attach -t 'cc-abcd'; " +
                "else echo 'aterm: resume 失败——无法确定工作目录（会话 abcd1234）'; fi",
            cmd,
        )
    }

    // resume 首建 server 时移动端 setup（含滚动模式）在 create 之后、attach 之前无条件施加（经尾段 attach 命令）。
    @Test fun tmuxResumeMobileSetupAppliedBeforeAttach() {
        val cmd = resume("cc-abcd", "sid", "/p")
        assertTrue("定位段在前：$cmd", cmd.startsWith("${locateSeg("sid")}; "))
        assertTrue("setup 段在 attach 前逐字出现：$cmd", cmd.contains("; ${setup("cc-abcd")}; tmux attach -t 'cc-abcd'; else echo"))
    }

    // 按 create 退出码分支：fresh 会话（create 成功）无条件 send。`new-session -d` 之后 pane_current_command
    // 会瞬时为 `tmux`，走白名单 guard 的话会漏送、watchdog 误 kill。stale 会话（create 失败）才 guard 探前台，
    // 仅闲置 shell 补送（残留 Claude 已退），Claude 正跑则不打脏命令。
    @Test fun tmuxResumeBranchesSendOnCreateSuccessVsForegroundGuard() {
        val cmd = resume("cc-abcd", "sid", "/p")
        assertTrue(
            "fresh 分支：create 成功即无条件 send（不经 pane 探测），先 set @ccm_sid 再 send",
            cmd.contains("if tmux new-session -d -s 'cc-abcd' -c \"\$cwd\" 2>/dev/null; then tmux set-option -t 'cc-abcd' @ccm_sid 'sid' 2>/dev/null; tmux send-keys -t 'cc-abcd' "),
        )
        assertTrue(
            "stale 分支：create 失败 → else 探前台，仅白名单 shell 补送",
            cmd.contains(
                "else case \"\$(tmux display-message -p -t 'cc-abcd' '#{pane_current_command}' 2>/dev/null)\" in " +
                    "($RESUME_FOREGROUND_SHELLS) tmux send-keys",
            ),
        )
        assertTrue("guard case 收尾并闭合内层 if", cmd.contains(";; esac; fi;"))
        // 「create 后无条件 guard 探测」的竞态形状（`2>/dev/null; case …`）不得出现。
        assertFalse("不得再对 fresh 会话也走 pane 探测（竞态源）", cmd.contains("2>/dev/null; case \"\$(tmux display-message"))
        assertFalse("不用 create && send（残留会话会永不 resume）", cmd.contains("2>/dev/null && tmux send-keys"))
    }

    // 白名单含 BusyBox/Alpine `ash`、`csh`/`tcsh` 及各 login-shell `-` 变体；缺项时这些 shell 上
    // resume 的 send-keys 被静默跳过（attach 进空 shell、Claude 永不启动）。每个 shell 裸名 + `-` 变体对称成对。
    @Test fun tmuxResumeForegroundShellAllowlistCoversAshCshTcshAndLoginVariants() {
        val shells = RESUME_FOREGROUND_SHELLS.split("|")
        listOf("bash", "sh", "ash", "dash", "zsh", "ksh", "csh", "tcsh", "fish").forEach { sh ->
            assertTrue("白名单应含 $sh", sh in shells)
            assertTrue("白名单应含 login 变体 -$sh", "-$sh" in shells)
        }
        assertTrue("login(1) 中间态", "login" in shells)
        // 白名单逐字进入生成的 case 模式（ash pane 场景：前台=ash 时命中 → send 发出）。
        val cmd = resume("n", "sid", "/p")
        assertTrue("case 模式携带完整白名单", cmd.contains("in ($RESUME_FOREGROUND_SHELLS) tmux send-keys"))
    }

    @Test fun tmuxResumeAlreadyInTmuxUsesSwitchClient() {
        val cmd = resume("cc-x", "sid", "/srv/a", alreadyInTmux = true)
        assertTrue("已在 tmux 内用 switch-client 防嵌套：$cmd", cmd.contains("tmux switch-client -t 'cc-x'; else echo"))
        assertFalse("不 attach（防嵌套）", cmd.contains("tmux attach"))
        assertTrue("fallback cwd 透传", cmd.contains("&& cwd='/srv/a';"))
        assertTrue("create 用双引号变量展开的权威 cwd", cmd.contains("-c \"\$cwd\""))
        assertTrue("send-keys 跑 claude --resume（默认命令）", cmd.contains("; claude --resume sid' Enter"))
    }

    // fallback cwd 含空格/单引号 → shQuote 安全；权威 cwd 走双引号变量展开天然安全。
    @Test fun tmuxResumeQuotesFallbackCwdWithSpaces() {
        val cmd = resume("n", "id", "/a b/c")
        assertTrue("含空格 fallback cwd 单引号包裹：$cmd", cmd.contains("&& cwd='/a b/c';"))
    }

    @Test fun tmuxResumeQuotesFallbackCwdWithSingleQuote() {
        val cmd = resume("n", "id", "/a'b/c")
        assertTrue("单引号经 POSIX 转义：$cmd", cmd.contains("&& cwd='/a'\\''b/c';"))
    }

    // === 远端 find-by-sid 权威 cwd + 防泄漏守卫 ===

    @Test fun tmuxResumeLocatesJsonlBySidRemotely() {
        val cmd = resume("cc-ab", "abcd1234-5678", "/p")
        assertTrue("find 按 sid 定位 jsonl（默认 claudeDir 双引号展开）：$cmd", cmd.startsWith("${locateSeg("abcd1234-5678")}; "))
        assertTrue("grep -o 抽权威 cwd + sed 剥壳", cmd.contains(extractSeg))
        assertTrue("找不到时回退批量探针 cwd", cmd.contains("[ -z \"\$cwd\" ] && cwd='/p';"))
    }

    // grep 后 head -n 1：首个含 cwd 的行若含嵌套裸片段会输出多行 → 只取首行，防 $cwd 变多行令 [ -d ] 误拒。
    @Test fun tmuxResumeHeadClampsCwdExtractToSingleLine() {
        val cmd = resume("cc-ab", "sid", "/p")
        assertTrue("grep -o 与 sed 之间夹 head -n 1：$cmd", cmd.contains("2>/dev/null | head -n 1 | sed 's/.*\"cwd\":\"//"))
    }

    // === pane 子进程探测（kill 前防误杀正在跑的 Claude） ===

    @Test fun paneChildProbeCommandShapeAndSafety() {
        val cmd = tmuxPaneChildProbeCommand("cc-x")
        assertTrue("取 pane_pid", cmd.contains("p=\$(tmux display-message -p -t 'cc-x' '#{pane_pid}' 2>/dev/null)"))
        assertTrue("pane 缺 → ABSENT", cmd.contains("[ -z \"\$p\" ]; then echo $PANE_PROBE_ABSENT"))
        assertTrue("无 pgrep → NOPGREP（保守）", cmd.contains("! command -v pgrep >/dev/null 2>&1; then echo $PANE_PROBE_NO_PGREP"))
        assertTrue("有子进程 → HASCHILD", cmd.contains("pgrep -P \"\$p\" >/dev/null 2>&1; then echo $PANE_PROBE_HAS_CHILD"))
        assertTrue("否则 → NOCHILD", cmd.contains("else echo $PANE_PROBE_NO_CHILD"))
        assertTrue("pane_pid 进 pgrep 双引号展开（注入安全）", cmd.contains("pgrep -P \"\$p\""))
        assertTrue("会话名 shQuote", tmuxPaneChildProbeCommand("my cc").contains("-t 'my cc'"))
    }

    @Test fun paneConfirmedNoChildOnlyOnExplicitNoChild() {
        assertTrue("明确无子进程 → 可 kill", paneConfirmedNoChild("$PANE_PROBE_NO_CHILD\n"))
        assertFalse("有子进程 → 不 kill", paneConfirmedNoChild(PANE_PROBE_HAS_CHILD))
        assertFalse("无 pgrep → 保守不 kill", paneConfirmedNoChild(PANE_PROBE_NO_PGREP))
        assertFalse("pane 缺 → 不 kill", paneConfirmedNoChild(PANE_PROBE_ABSENT))
        assertFalse("空输出 → 不 kill", paneConfirmedNoChild(""))
        assertFalse("未知输出 → 不 kill", paneConfirmedNoChild("garbage"))
    }

    @Test fun tmuxResumeCwdGuardBlocksCreateOnFailure() {
        val cmd = resume("cc-ab", "sid", null)
        // 防泄漏第一道闸：create 必须在 if 守卫内：定位不到 jsonl/cwd 或目录不存在 → 不建会话 + 可读错误。
        val guardIdx = cmd.indexOf("if [ -n \"\$cwd\" ] && [ -d \"\$cwd\" ]; then")
        val createIdx = cmd.indexOf("tmux new-session")
        assertTrue("守卫在 create 之前（$guardIdx < $createIdx）", guardIdx in 0 until createIdx)
        assertTrue("失败分支打印可读错误（含 sid8）", cmd.endsWith("else echo 'aterm: resume 失败——无法确定工作目录（会话 sid）'; fi"))
        assertTrue("fallbackCwd 为空 → cwd=''（守卫 [ -n ] 拦下）", cmd.contains("[ -z \"\$cwd\" ] && cwd='';"))
    }

    @Test fun tmuxResumeQuotesCustomClaudeDirLiterally() {
        val cmd = tmuxResumeCommand(defaultClaude, "n", "sid", "/srv/claude data", "/p", alreadyInTmux = false)
        assertTrue("用户覆写 claudeDir 作字面路径 shQuote：$cmd", cmd.contains("find '/srv/claude data/projects' -maxdepth 2"))
    }

    @Test fun tmuxResumeRejectsIllegalSessionIdBeforeEmbedding() {
        // sid 进 find -name 与 send 载荷：非法 sid（含引号/元字符）必须在拼串前被拒（require），防注入。
        assertThrows(IllegalArgumentException::class.java) { resume("n", "sid; rm -rf /", "/p") }
        assertThrows(IllegalArgumentException::class.java) { resume("n", "a'b", "/p") }
        assertThrows(IllegalArgumentException::class.java) { resume("n", "", "/p") }
    }

    // 前台探测/判定纯函数：watchdog 与 tmuxResumeCommand 共用同一命令形状（等价性钉死）。
    @Test fun foregroundProbeCommandShapeAndEquivalence() {
        assertEquals("tmux display-message -p -t 'cc-x' '#{pane_current_command}' 2>/dev/null", tmuxForegroundProbeCommand("cc-x"))
        val cmd = resume("cc-x", "sid", "/p")
        assertTrue("resume 内嵌的探测与抽出的纯函数逐字一致", cmd.contains("case \"\$(${tmuxForegroundProbeCommand("cc-x")})\" in"))
    }

    @Test fun isResumeForegroundShellJudgement() {
        assertTrue(isResumeForegroundShell("bash"))
        assertTrue("login 变体", isResumeForegroundShell("-bash"))
        assertTrue("BusyBox ash", isResumeForegroundShell("ash"))
        assertTrue("输出带换行/空白 → trim 后判定", isResumeForegroundShell(" -zsh\n"))
        assertTrue("login(1) 中间态", isResumeForegroundShell("login"))
        assertFalse("node（Claude 已起）非 shell", isResumeForegroundShell("node"))
        assertFalse("claude 包装名非 shell", isResumeForegroundShell("claude"))
        assertFalse("空输出（会话不存在）非 shell", isResumeForegroundShell(""))
    }

    // === resume 参数化主机自定义命令 + 嵌套环境清洗 + sessionId 校验 ===

    @Test fun tmuxResumeUsesCustomLaunchCommand() {
        val cmd = resume("cc-ab", "sid", "/p", launchCommand = "cct")
        assertTrue("跑主机自定义命令 cct 而非硬编码 claude：$cmd", cmd.contains("; cct --resume sid' Enter"))
        assertFalse("不残留 claude 调用", cmd.contains("claude --resume"))
    }

    // 注入元字符的 launcher 命令穿过 tmuxResumeCommand → 最终命令被中和为 claude，串联部分不出现。
    @Test fun tmuxResumeNeutralizesMetacharLaunchCommand() {
        val cmd = resume("n", "sid", "/p", launchCommand = "cc; rm -rf /")
        assertFalse("注入串联段不得进入最终命令", cmd.contains("rm -rf"))
        assertTrue("中和为默认 claude", cmd.contains("; claude --resume sid' Enter"))
    }

    // resume 会话在 create 成功分支打 tmux user-option @ccm_sid=<完整 sid>：后端的 findClaudeTmux
    // 按 @ccm_sid 全等 sid 定位到本会话、给出 attach/kill。session-scoped（-t 指名，detached 会话必须）、
    // value=完整 sid（非 cc-<sid8> 前 8）；else 分支=会话已存在=幂等重连，建时已带、不重设。
    @Test fun tmuxResumeSetsCcmSidOnCreateForCcMonitor() {
        val cmd = resume("cc-ab", "sid", "/p")
        assertTrue(
            "create 成功分支设 @ccm_sid=完整 sid（session-scoped -t 指名）、在 send 之前：$cmd",
            cmd.contains("then tmux set-option -t 'cc-ab' @ccm_sid 'sid' 2>/dev/null; tmux send-keys -t 'cc-ab'"),
        )
        // else（会话已存在=幂等重连）分支不得重设 @ccm_sid（建时已带）。
        val elseBranch = cmd.substringAfter("else case", "")
        assertFalse("else 分支不重设 @ccm_sid", elseBranch.contains("set-option -t 'cc-ab' @ccm_sid"))
    }

    // Codex resume 不设 @ccm_sid：后端自己从不给 Codex 打这个事实标记（`has_identity: false`），我们也不替它伪造。
    // 由档案 `hasCcmIdentity` 那一格答（`ResumeScaffoldTest` 钉着那一格）。
    @Test fun codexResumeDoesNotSetCcmSid() {
        val cmd = tmuxResumeCommand(codex, "cx-ab", "019f75dd-1111-2222-3333-444455556666", "/home/u/.codex", "/p", alreadyInTmux = false)
        assertFalse("Codex resume 不设 @ccm_sid：$cmd", cmd.contains("@ccm_sid"))
    }
}
