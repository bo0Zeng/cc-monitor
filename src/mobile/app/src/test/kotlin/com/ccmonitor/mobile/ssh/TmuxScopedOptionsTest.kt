package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 我们拼出的任何一条 tmux 命令都不改 tmux server 的全局状态。
 *
 * 进 tmux、附着、续接、管理器新建、起一条聊天管道时若跑 `tmux set -g status off; tmux set -g mouse on;
 * tmux set -gu terminal-overrides`：`-g` 改的是共享 tmux server 的全局默认，那台机器上所有没有自己覆盖值的会话
 * （后端的 `cc-*`、电脑上开的）状态栏一起没了、鼠标一起开了；`-gu terminal-overrides` 把整个服务器的 override
 * 重置成默认。主机上的 `cc-*` 会话不许碰，改全局就是间接碰了它们。
 *
 * | # | 问什么 | bug 在时 |
 * |---|---|---|
 * | ① | 枚举我们会发出去的每一条 tmux 命令（真实返回值），其中每一次写选项都带 `-t`、不带 `-g`/`-s`、不碰服务器级选项；不出现 `bind`/`source-file`/`kill-server` 这类整台机器的动作 | 全部 `set -g` ⇒ 红 |
 * | ② | 显示设置只落在要进的那个会话上（目标逐字等于 `'=<名>:'`）| 落到别的目标上 ⇒ 红 |
 * | ③ | 聊天托管（`atermpipe-*`）与后台新建一个选项都不写 | 起管道时又顺手设显示选项 ⇒ 红 |
 * | ④ | 全部生产源码的字面里没有 `set -g`/`set-option -g`/`setw -g`… | 有人在没被①枚举到的新路径里写 `-g` ⇒ 红 |
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 新长出一支 tmux 命令、又没被①枚举到 | 只剩④兜：字面里写 `-g` 会红；拼接出来的（`"set " + "-g"`）抓不到（防手滑不防拼接） |
 * | 用户在管理器里主动附着别人的会话（如电脑上的 `cc-*`） | 本文件不拦：两条显示设置照样落在那一个会话上（会话级选项各客户端共享）。这是「手机上要能滚」与「不碰别人的会话」的真冲突，见 `TmuxCommands.tmuxMobileSetup` 的 KDoc |
 * | 用户自己在终端里敲的 tmux 命令、按钮里的命令 | 不在射程：那是用户的命令，不是我们拼的 |
 * | 命令在远端真按预期生效（`-t '=名:'` 在各版本 tmux 上都认） | 单测答不了，没在真机上验（后端的 `set-option -t <exact_target>` 是同形） |
 */
class TmuxScopedOptionsTest {
    private val sid = "0f9d3c2a-1111-4222-8333-444455556666"

    /** ①：每一条真实命令里，写选项都只对一个点名的会话。 */
    @Test
    fun noTmuxCommandWeBuildTouchesTheServerWideState() {
        val all = everyTmuxCommandWeBuild()
        assertTrue("前提：枚举得到命令，实得 ${all.size} 条", all.size >= 15)
        assertTrue(
            "前提：枚举里得真有写选项的命令（否则①守了个寂寞）",
            all.values.any { optionWrites(it).isNotEmpty() },
        )
        for ((who, cmd) in all) {
            val bad = optionWrites(cmd).mapNotNull { w -> whyGlobal(w)?.let { "$it：$w" } } + serverWideVerbs(cmd)
            assertEquals("`$who` 改了 tmux server 的全局状态（那台机器上所有会话都被波及，包括 cc-monitor 的 `cc-*`）：\n$cmd", emptyList<String>(), bad)
        }
    }

    /** ②：显示设置只落在要进的那个会话上。 */
    @Test
    fun theDisplaySetupLandsOnlyOnTheSessionBeingEntered() {
        for (name in listOf("main", "my proj")) {
            val writes = optionWrites(TmuxBackend.enter(name, null))
            assertEquals("进 `$name` 时写了哪些选项", setOf("status", "mouse"), writes.map { it.option }.toSet())
            assertEquals("进 `$name` 时，选项只许落在 `'=$name:'` 上", setOf(TmuxCommands.exactTarget(name)), writes.map { it.target }.toSet())
        }
        for (p in AgentProfile.ALL) {
            val plan =
                TmuxBackend.resume(ResumeSpec(sid, listOf(null), "/d", fallbackCwd = "/w", agentKind = p.kind))
                    ?: error("前提：${p.kind} 对合法 sid 必须给出计划")
            val display = optionWrites(plan.command).filter { it.option in DISPLAY_OPTIONS }
            assertEquals("${p.kind} resume：显示设置两条都得在", DISPLAY_OPTIONS, display.map { it.option }.toSet())
            assertEquals(
                "${p.kind} resume：显示设置只许落在它自己那个会话 `'=${plan.sessionName}:'` 上",
                setOf(TmuxCommands.exactTarget(plan.sessionName)),
                display.map { it.target }.toSet(),
            )
        }
    }

    /** ③：聊天托管与后台新建一个选项都不写。 */
    @Test
    fun theChatPipeAndDetachedCreationsWriteNoOptionsAtAll() {
        val pipe = PipeLauncher.startCommand(TmuxBackend, sid, null) ?: error("前提：合法 sid 必须给出管道命令")
        assertTrue("前提：管道那条真的会建会话：\n$pipe", pipe.contains("new-session -d -s "))
        for ((who, cmd) in mapOf(
            "聊天管道（atermpipe-*）" to pipe,
            "管理器·后台新建" to TmuxBackend.newDetachedCommand("work"),
            "管理器·后台 cc" to TmuxBackend.newDetachedRunningCommand("work", "claude"),
        )) {
            assertEquals("$who 不该写任何 tmux 选项（此刻没有客户端挂着它，显示设置毫无意义）：\n$cmd", emptyList<OptionWrite>(), optionWrites(cmd))
        }
    }

    /** ④：生产源码字面里不许写回全局选项（①没枚举到的新路径由它兜）。 */
    @Test
    fun noProductionSourceSpellsAGlobalOptionWrite() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，实得 ${sources.size} 个", sources.size > 150)
        val hits = sortedMapOf<String, Int>()
        for (f in sources) {
            val n = GLOBAL_WRITE_IN_SOURCE.findAll(KotlinSourceScanner.codeOnlyKeepingLiterals(f.readText())).count()
            if (n > 0) hits[f.relativeTo(root).invariantSeparatorsPath] = n
        }
        assertEquals("生产源码的字面里出现了改 tmux 全局状态的写法（`-g` / `-s`）：", emptyMap<String, Int>(), hits.toMap())
    }

    /** 解析器与正则的自检：上面四条的判别力全押在它们身上。 */
    @Test
    fun theDetectorsSeeGlobalWritesAndOnlyThose() {
        fun globals(cmd: String) = optionWrites(cmd).mapNotNull { whyGlobal(it) } + serverWideVerbs(cmd)
        // 真全局：各种拼法都要认出来
        for (cmd in listOf(
            "tmux set -g status off 2>/dev/null",
            "tmux set -gu terminal-overrides 2>/dev/null",
            "tmux set-option -g mouse on",
            "tmux setw -g mode-keys vi",
            "tmux set -s escape-time 0",
            "tmux set -as terminal-overrides ',xterm*:Tc'",
            "tmux set mouse on",
            "tmux set -t '=a:' terminal-overrides 'x'",
            "x=\"\$(tmux set -qg status off)\"",
            "tmux bind-key r source-file ~/.tmux.conf",
            "tmux kill-server",
        )) {
            assertTrue("该认出全局：$cmd", globals(cmd).isNotEmpty())
        }
        // 只对一个会话：不该误报
        for (cmd in listOf(
            "tmux set -t '=main:' status off 2>/dev/null; tmux set -t '=main:' mouse on 2>/dev/null",
            "tmux set-option -t 'cc-ab' @ccm_sid 'sid' 2>/dev/null",
            "tmux new-session -d -s 'work'; tmux send-keys -t 'work' 'set -g x' Enter",
            "tmux send-keys -t 'w' 'it'\\''s; tmux set -g status off' Enter",
        )) {
            assertEquals("不该误报：$cmd", emptyList<String>(), globals(cmd))
        }
        assertEquals(
            "目标与选项名都得抠准",
            listOf(OptionWrite("set", "t", "'=my proj:'", "status")),
            optionWrites("tmux set -t '=my proj:' status off"),
        )
        // ④的正则：字面里认、注释里不认
        assertEquals(1, GLOBAL_WRITE_IN_SOURCE.findAll(KotlinSourceScanner.codeOnlyKeepingLiterals("val a = \"tmux set -g status off\"\n")).count())
        assertEquals(0, GLOBAL_WRITE_IN_SOURCE.findAll(KotlinSourceScanner.codeOnlyKeepingLiterals("// tmux set -g status off\n")).count())
        assertEquals(0, GLOBAL_WRITE_IN_SOURCE.findAll(KotlinSourceScanner.codeOnlyKeepingLiterals("val a = \"tmux new-session -d -s 'x'\"\n")).count())
    }

    // ---- 枚举 ---------------------------------------------------------------

    /** 我们会发往远端的每一条 tmux 命令（经 [SessionBackend] 与上行网关、管道启动器的真实返回值）。 */
    private fun everyTmuxCommandWeBuild(): Map<String, String> =
        buildMap {
            val b: SessionBackend = TmuxBackend
            put("enter", b.enter("main", null))
            put("enter+wd", b.enter("proj", "/home/u/my proj"))
            for (p in AgentProfile.ALL) {
                for (inTmux in listOf(false, true)) {
                    val spec = ResumeSpec(sid, listOf(null), "/d", fallbackCwd = "/w", alreadyInTmux = inTmux, agentKind = p.kind)
                    put("resume ${p.kind} inTmux=$inTmux", b.resume(spec)?.command ?: error("前提：${p.kind} 必须给出计划"))
                }
            }
            put("list", b.listCommand())
            put("kill", b.killCommand("work"))
            put("newDetached", b.newDetachedCommand("work"))
            put("newDetachedRunning", b.newDetachedRunningCommand("work", "claude"))
            put("startOnce", b.startOnceCommand("atermpipe-x", "echo hi"))
            put("sessionCwd", b.sessionCwdCommand("work"))
            put("sendModel", b.sendModelCommand("work", "opus"))
            put("capture", b.captureCommand("work"))
            put("foregroundProbe", b.foregroundProbeCommand("work"))
            put("paneChildProbe", b.paneChildProbeCommand("work"))
            put("uplink ours", TmuxCommands.buildSendCommand("cc-abc12345", "hi") ?: error("前提"))
            put("uplink foreign", TmuxCommands.buildSendCommand("someone-elses", "hi") ?: error("前提"))
            put("pipe start", PipeLauncher.startCommand(b, sid, null) ?: error("前提"))
        }

    // ---- 解析 ---------------------------------------------------------------

    /** 一次 `tmux set…`：动词 · 合起来的 flag 字母 · `-t` 的目标（原样，含引号）· 选项名。 */
    private data class OptionWrite(
        val verb: String,
        val flags: String,
        val target: String?,
        val option: String?,
    )

    /**
     * 抠出命令串里每一次写选项（含 `$(…)` 里面的）。被单引号包住、当作 send-keys 载荷的那一段不算（它是要敲进去的字）。
     * 只用正则切词（`SharedScannerGateTest` ⑥：测试里不许再手搓逐字符扫描器）。
     */
    private fun optionWrites(cmd: String): List<OptionWrite> {
        val quoted = singleQuotedSpans(cmd)
        return OPTION_VERB
            .findAll(cmd)
            .filterNot { m -> quoted.any { m.range.first in it } }
            .map { m -> parseArgs(m.groupValues[1], argWordsAfter(cmd, m.range.last + 1)) }
            .toList()
    }

    /** 动词之后、到第一个命令终止符（`;` `)` 换行 `|` `&`，引号里的不算）为止的那些词。 */
    private fun argWordsAfter(
        cmd: String,
        from: Int,
    ): List<String> {
        val span = ARGS_SPAN.find(cmd.substring(from))?.value.orEmpty()
        return ARG_WORD.findAll(span).map { it.value }.toList()
    }

    /** `-x` 合进 flags，`-t` 吃下一个词当目标，第一个非 flag 词是选项名。 */
    private fun parseArgs(
        verb: String,
        words: List<String>,
    ): OptionWrite {
        var flags = ""
        var target: String? = null
        var expectTarget = false
        for (w in words) {
            when {
                expectTarget -> {
                    target = w
                    expectTarget = false
                }
                w.startsWith("-") && w.length > 1 -> {
                    flags += w.drop(1)
                    expectTarget = w.endsWith("t")
                }
                else -> return OptionWrite(verb, flags, target, w)
            }
        }
        return OptionWrite(verb, flags, target, null)
    }

    /** 这一次写为什么算「改了整台机器」；只对一个点名的会话 ⇒ `null`。 */
    private fun whyGlobal(w: OptionWrite): String? =
        when {
            'g' in w.flags -> "带 -g（全局默认）"
            's' in w.flags -> "带 -s（服务器级）"
            w.target == null -> "没有 -t（落到哪个会话由 tmux 猜）"
            w.option in SERVER_OPTIONS -> "服务器级选项（没有会话作用域，-t 也拦不住）"
            else -> null
        }

    private fun serverWideVerbs(cmd: String): List<String> =
        SERVER_WIDE_VERB
            .findAll(cmd)
            .filterNot { m -> singleQuotedSpans(cmd).any { m.range.first in it } }
            .map { "整台机器的动作：${it.value}" }
            .toList()

    /**
     * shell 单引号段的位置（按 shell 的读法：单引号外反斜杠转义下一个字符；双引号里的单引号是普通字符）。
     * `'\''` 那种转义引号会被切成「引号段 · 转义的引号 · 引号段」，与 shell 一致。
     */
    private fun singleQuotedSpans(s: String): List<IntRange> =
        SHELL_PIECE
            .findAll(s)
            .filter { it.value.startsWith("'") }
            .map { it.range }
            .toList()

    // ---- 扫描面 -------------------------------------------------------------

    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

    /** 扫描面：`app/src/main/kotlin` + 每个 `core-…` 模块的 `src/main/kotlin`。 */
    private fun productionSources(root: File): List<File> {
        val roots =
            (listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList()))
                .map { File(it, "src/main/kotlin") }
                .filter { it.isDirectory }
        assertTrue("前提：得找到扫描面（app + core-*），实得 ${roots.size} 个源根", roots.size >= 2)
        return roots.flatMap { r -> r.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
    }

    private companion object {
        const val MAX_WALK_UP = 6

        /** shell 切片：转义字符 · 双引号段 · 单引号段 · 其余。 */
        val SHELL_PIECE = Regex("""\\.|"(?:\\.|[^"\\])*"|'[^']*'|[^\\"']+""", RegexOption.DOT_MATCHES_ALL)

        /** 一个参数词：单引号段 / 双引号段（引号保留）/ 到空白或终止符为止。 */
        private const val WORD_SRC = """'[^']*'|"[^"]*"|[^\s;)\n|&'"]+"""
        val ARG_WORD = Regex(WORD_SRC)

        /** 从开头起连续的参数词（遇到终止符即止）。 */
        val ARGS_SPAN = Regex("""^(?:[ \t]*(?:$WORD_SRC))*""")

        /** 显示设置那两条（会话级选项）。 */
        val DISPLAY_OPTIONS = setOf("status", "mouse")

        /** 写选项的动词（`set` 是 `set-option` 的别名，`setw` 是 `set-window-option` 的）。 */
        val OPTION_VERB = Regex("""\btmux\s+(set-option|set-window-option|setw|set-hook|set-environment|setenv|set)(?=\s)""")

        /** 没有会话作用域的动作：一出现就是整台机器。 */
        val SERVER_WIDE_VERB = Regex("""\btmux\s+(bind-key|bind|unbind-key|unbind|source-file|source|kill-server)\b""")

        /**
         * tmux 3.x 手册「Server options」里的那几个（节选：我们可能碰到的）。它们没有会话作用域，`-t` 也拦不住。
         * `terminal-overrides` 就是 `set -gu terminal-overrides` 改的东西。
         */
        val SERVER_OPTIONS =
            setOf(
                "terminal-overrides",
                "terminal-features",
                "default-terminal",
                "escape-time",
                "focus-events",
                "set-clipboard",
                "exit-empty",
                "exit-unattached",
                "extended-keys",
                "buffer-limit",
                "history-file",
                "message-limit",
                "command-alias",
                "user-keys",
            )

        /** ④：源码字面里写全局的样子（动词后紧跟一组含 `g` 或 `s` 的 flag）。 */
        val GLOBAL_WRITE_IN_SOURCE =
            Regex("""(?<![\w-])(?:set|set-option|setw|set-window-option|set-hook|set-environment|setenv)\s+-[A-Za-z]*[gs]""")
    }
}
