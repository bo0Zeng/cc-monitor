package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.command.CodexInvocation
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * `tmux new-session` 的名字只来自三个点名的来源。
 *
 * tmux 会话名分三类：
 * ① 管道会话只许 `PipeLauncher.PIPE_SESSION_PREFIX`（`atermpipe-`）；
 * ② 终端 resume 那条路按与后端的约定起 `cc-<sid8>`（Codex `cx-`），那是后端认的前缀，不是越界；
 *    唯一定义处是 `AgentInvocation.resumeSessionName` 的两个实现；
 * ③ 用户显式给的名字：`HostConnect.resolveTmuxSession`（`LaunchSpec.tmuxSession` > `DEFAULT_TMUX_SESSION="main"`）
 *    与 tmux 管理器新建行里手敲的那个名。那是终端的自由 tmux 会话，既不在我们的名字空间也不在后端的；
 *    透传那几支不许自己加前缀。
 *
 * | # | 问什么 | 量什么 |
 * |---|---|---|
 * | ③a | `new-session` 在生产代码里恰好 4 处、全在 tmux 网关 `TmuxCommands.kt` | 源码扫描（闭集） |
 * | ③b | 四处的真实命令串里 `-s` 后那个名字，逐字等于其上游来源的产物 | 真实返回值 |
 * | ③c | `"cc-`（写会话名那个，不是 `cc-monitor-…`）只住两个文件 | 源码扫描（定值钉） |
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 这几条判据 |
 * |---|---|
 * | 某支 `new-session` 偷偷给名字加前缀 / 硬写 `"cc-" + sid` | 红（③b 点名是哪一支） |
 * | 别处再长出一处 `new-session` | 红（③a 报文件与计数） |
 * | 有人在消费方复制一份 `"cc-"` 前缀去构名 | 红（③c 点名文件） |
 * | `"cc" + "-"` · `String(charArrayOf('c','c','-'))` · 反射 | 抓不到：防手滑不防拼接 |
 * | 调用方传进来一个 `cc-` 开头的名字（透传那三支） | 抓不到：那是调用方的事，本判据只管这四支自己不加料 |
 * | `androidTest` / `src/debug` 源集 | 不在扫描面内 |
 */
class TmuxSessionNameSourceTest {
    private val sid = "0f9d3c2a-1111-4222-8333-444455556666"

    // ---- ③a：new-session 的闭集 -------------------------------------------

    /**
     * ③a：剥注释后 `new-session` 在全部生产源集里恰好 4 处，全在 tmux 网关 `TmuxCommands.kt`。
     *
     * 这一条是③b 的分母：③b 只覆盖这四支；第 5 支一出现，本条当场红，
     * 于是「新长出来一处没人管」不可能悄悄发生。
     */
    @Test
    fun everyNewSessionInProductionLivesInTheTmuxGateway() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，否则这条是空跑（实得 ${sources.size} 个）", sources.size > 150)

        val counts = sortedMapOf<String, Int>()
        for (f in sources) {
            val n = countOccurrences(codeOnly(f.readText()), NEW_SESSION)
            if (n > 0) counts[f.relativeTo(root).invariantSeparatorsPath] = n
        }
        assertTrue("前提：`$NEW_SESSION` 整份语料零命中 ⇒ 扫描器瞎了、本条恒绿", counts.values.sum() > 0)
        assertEquals(
            "`$NEW_SESSION` 只许住 tmux 网关 `TmuxCommands.kt`（每个协议面一个网关类，" +
                "别处只拿结构化结果），且恰好 4 支。" +
                "多出来的那一支请说明它的会话名来自哪个来源，再一起改这张表与 ③b。",
            PINNED_NEW_SESSION_SITES,
            counts.toMap(),
        )
    }

    // ---- ③b：四支各自的名字来源 -------------------------------------------

    /**
     * ③b-①：管道那支的名字逐字等于 `PIPE_SESSION_PREFIX + sid`。
     *
     * 期望值一半用常量、一半用手写定值 [PIPE_PREFIX_LITERAL] —— 只用常量的话，
     * 「有人把常量改成 `cc-`」这一刀会恒绿。
     */
    @Test
    fun thePipeSessionNameComesFromThePipePrefixAndNothingElse() {
        assertEquals("管道前缀", PIPE_PREFIX_LITERAL, PipeLauncher.PIPE_SESSION_PREFIX)

        val cmd = PipeLauncher.startCommand(TmuxBackend, sid, null)
        assertNotNull("前提：命令没造出来（sid 被判非法？）⇒ 本条量不到东西", cmd)
        val names = newSessionNames(cmd!!)
        assertEquals("前提：这条串里得恰好有一支 `$NEW_SESSION`：\n$cmd", 1, names.size)
        assertEquals("管道会话名只许是 `$PIPE_PREFIX_LITERAL` + 完整 sid（不许截、不许换前缀）", "$PIPE_PREFIX_LITERAL$sid", names[0])
        assertEquals("而且要与 `tmuxNameFor` 同源（别处不许再拼一份）", PipeLauncher.tmuxNameFor(sid), names[0])
    }

    /**
     * ③b-②：resume 那两支的名字逐字等于 `resumeSessionName(sid)`。
     *
     * `cc-<sid8>` / Codex `cx-<sid8>` 是与后端约定的前缀（后端的 `findClaudeTmux` 认它）。
     * 本条钉的是「名字必须来自那个函数」。
     */
    @Test
    fun theResumeSessionNamesComeFromResumeSessionName() {
        val expectations =
            listOf(
                Triple(AgentKind.ClaudeCode, ClaudeInvocation.resumeSessionName(sid), "cc-"),
                Triple(AgentKind.Codex, CodexInvocation.resumeSessionName(sid), "cx-"),
            )
        for ((kind, expected, literalPrefix) in expectations) {
            val plan =
                TmuxBackend.resume(
                    ResumeSpec(sessionId = sid, launchCandidates = emptyList(), claudeDir = "/home/u/.claude", fallbackCwd = "/home/u", agentKind = kind),
                )
            assertNotNull("前提：$kind 的 resume 计划没造出来 ⇒ 本条量不到东西", plan)
            assertEquals("$kind 的乐观会话名必须来自 `resumeSessionName`", expected, plan!!.sessionName)
            assertTrue(
                "那个名字得真是约定的 `$literalPrefix<sid8>` 形状（定值钉，防有人改了 `resumeSessionName` 还全绿）",
                expected == "$literalPrefix${sid.take(8)}",
            )
            val names = newSessionNames(plan.command)
            assertEquals("前提：$kind 的 resume 串里得恰好有一支 `$NEW_SESSION`：\n${plan.command}", 1, names.size)
            assertEquals("$kind 的 `$NEW_SESSION` 用的名字必须就是那个（别处不许再拼一份）", expected, names[0])
        }
    }

    /**
     * ③b-③：其余三支原样透传调用方给的名字，不许自己加前缀、不许截。
     * （`TmuxBackend.enter` 在表里量了两遍：带/不带工作目录。）
     *
     * 那三支收的名字是用户显式值（`LaunchSpec.tmuxSession` > `DEFAULT_TMUX_SESSION`，
     * 或 tmux 管理器新建行里手敲的那个）。防的是有人在这三支里「顺手」给会话名加个 `cc-` / `atermpipe-`
     * 前缀：那会让终端的自由会话撞进后端或我们自己的名字空间，而调用方一无所知。
     *
     * 探针名 [PROBE_NAME] 刻意选一个既不像我们也不像后端的串：
     * 名字里要是本来就带 `cc-`，透传与「加前缀」就分不开了。
     */
    @Test
    fun theOtherNewSessionSitesPassTheCallerNameThrough() {
        val passthrough: Map<String, String> =
            linkedMapOf(
                "TmuxBackend.enter(无工作目录)" to TmuxBackend.enter(PROBE_NAME, null),
                "TmuxBackend.enter(带工作目录)" to TmuxBackend.enter(PROBE_NAME, "/home/u/proj"),
                "TmuxBackend.newDetachedCommand" to TmuxBackend.newDetachedCommand(PROBE_NAME),
                "TmuxBackend.newDetachedRunningCommand" to TmuxBackend.newDetachedRunningCommand(PROBE_NAME, "claude"),
                "TmuxBackend.startOnceCommand" to TmuxBackend.startOnceCommand(PROBE_NAME, "claude"),
            )
        for ((who, cmd) in passthrough) {
            val names = newSessionNames(cmd)
            assertTrue("前提：$who 里一支 `$NEW_SESSION` 都没抠出来 ⇒ 抠取器瞎了、本条对它恒绿：\n$cmd", names.isNotEmpty())
            for (n in names) {
                assertEquals("$who 擅自改了调用方给的会话名 —— 透传那几支不许加前缀/截断。\n整条命令：\n$cmd", PROBE_NAME, n)
            }
        }
        // 用户显式值那条链的默认端也钉一下：auto-tmux 起的是 `main`，不带任何名字空间前缀。
        assertEquals("auto-tmux 的默认会话名", "main", DEFAULT_TMUX_SESSION)
    }

    // ---- ③c：写侧 `cc-` 的住址 --------------------------------------------

    /**
     * ③c：`"cc-`（后面不是 `monitor` 的那种）在生产代码里只住两个文件。
     *
     * | 文件 | 侧 | 为什么它可以有 |
     * |---|---|---|
     * | `ClaudeInvocation.kt` | 写 | `resumeSessionName = "cc-<sid8>"`，唯一定义处 |
     * | `TmuxCommands.kt` | 读 | `isCcmTmuxName` 认老前缀（没有 `@ccm_sid` 的老会话只能靠名字认），纯识别，不构名 |
     *
     * 二进制名 `"cc-monitor-…"` 不是会话名，由 [countSessionCcPrefix] 按「后面是不是 `monitor`」排除掉。
     */
    @Test
    fun theCcPrefixIsWrittenOnlyByResumeSessionName() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，否则这条是空跑（实得 ${sources.size} 个）", sources.size > 150)

        val found = sortedSetOf<String>()
        var hits = 0
        for (f in sources) {
            val n = countSessionCcPrefix(codeOnly(f.readText()))
            hits += n
            if (n > 0) found += f.relativeTo(root).invariantSeparatorsPath
        }
        assertTrue("前提：会话名前缀 `\"cc-` 整份语料零命中 ⇒ 扫描器瞎了、本条恒绿", hits > 0)
        assertEquals(
            "构造 `cc-` 会话名只许 `ClaudeInvocation.resumeSessionName` 一处；" +
                "读侧识别老前缀只许 `TmuxCommands`。别处要用请调那个函数。",
            PINNED_CC_PREFIX_FILES,
            found.toSet(),
        )
    }

    // ---- app 侧写命令串的落点 -----------------------------------

    /**
     * app 侧会写远端的命令串，其真实输出里的落点逐格钉死。
     *
     * 只有两条：
     * - `PipeLauncher.startCommand`：tmux 那一层只碰 `/dev/null`（`has-session` 的噪声），
     *   真正的写在被单引号包住的载荷里，那一层归 `NamespacePrefixTest` 管（`pipeInvocation`）；
     * - `RemoteCommands.appendAuthorizedKeyCommand`：落 `~/.ssh/`，第三个名字空间，
     *   既不是我们的 `.aterm` 也不是后端的 `.cc-monitor`，而是 OpenSSH 的标准位。多一格红。
     *
     * 抠取器 [writeTargets] 与 `core-claude` 那份是两份拷贝（跨模块共享测试夹具没配），
     * 两份都各自带扫描器自检；改一份记得核另一份。
     */
    @Test
    fun theAppSideWriteCommandsLandInPinnedNamespaces() {
        val pipe = PipeLauncher.startCommand(TmuxBackend, sid, null)!!
        assertFalse("管道那条命令串里出现了后端的名字空间：\n$pipe", pipe.contains(CC_MONITOR))
        assertFalse("管道那条命令串里出现了共享的 `/tmp`：\n$pipe", pipe.contains("/tmp"))
        for (t in writeTargets(pipe)) {
            assertTrue("管道那条 tmux 层往 `$t` 写东西 —— 只该碰 `/dev/null`：\n$pipe", t in ALLOWED_SINKS)
        }

        val keys = appendAuthorizedKeyCommand("ssh-ed25519 AAAAC3NzaC1lZDI1NTE5 probe")
        assertFalse("推公钥那条命令串里出现了后端的名字空间：\n$keys", keys.contains(CC_MONITOR))
        val keyTargets = writeTargets(keys)
        assertTrue("前提：推公钥那条串里一个写动词都没抠出来 ⇒ 抠取器瞎了：\n$keys", keyTargets.isNotEmpty())
        assertEquals(
            "推公钥只许落 OpenSSH 的标准位。这张表是定值钉：多一个落点红（又往别处写了），" +
                "少一个也红（写的路数变了，要重新过眼）。",
            PINNED_AUTHORIZED_KEY_TARGETS,
            keyTargets.toSortedSet().toSet(),
        )
    }

    // ---- 扫描器 / 抠取器自检 ---------------------------------------------------

    /** 扫描器自检：认得准注释与 `cc-monitor`，否则上面几条要么恒绿要么误红。 */
    @Test
    fun theScannersReadCodeAndNothingElse() {
        val sample =
            "// 注释里的 tmux new-session 与 \"cc-\" 不算\n" +
                "/* 块注释里的 new-session 也不算 */\n" +
                "fun f() = \"tmux new-session -d -s x\"\n" +
                "val a = \"cc-\${sid.take(8)}\"\n" +
                "val b = \"cc-monitor-backend\"\n" // 二进制名，不是会话名
        val code = codeOnly(sample)
        assertEquals("`$NEW_SESSION` 只该认出代码里那一处：\n$code", 1, countOccurrences(code, NEW_SESSION))
        assertEquals("会话名前缀只该认出 `a` 那一处（`b` 是 cc-monitor 的二进制名）：\n$code", 1, countSessionCcPrefix(code))
        assertEquals("注释剥干净了没有", 0, countOccurrences(codeOnly("// tmux new-session\n"), NEW_SESSION))
        // 反过来也要证：真命中一定认得出 —— 否则「剥得干净」可以靠「什么都不认」作弊。
        assertEquals("裸一行真命中必须认出", 1, countOccurrences(codeOnly("\"tmux new-session -d\""), NEW_SESSION))
    }

    /** 抠取器自检：`-s` 后那个名字要抠得准，六支的形状各不相同。 */
    @Test
    fun theSessionNameExtractorReadsTheDashSArgument() {
        assertEquals(listOf("main"), newSessionNames("tmux new-session -d -s 'main' 2>/dev/null; tmux attach -t 'main'"))
        assertEquals(listOf("a b"), newSessionNames("tmux new-session -d -s 'a b' -c 'wd'"))
        assertEquals(listOf("n"), newSessionNames("if tmux new-session -d -s 'n' -c \"\$cwd\" 2>/dev/null; then x; fi"))
        assertEquals("两支都要抠出来", listOf("p", "q"), newSessionNames("tmux new-session -d -s 'p'; tmux new-session -d -s 'q'"))
        assertEquals("没有 new-session 就该是空的（`attach -t` 不算）", emptyList<String>(), newSessionNames("tmux attach -t 'main'"))
    }

    /** 写落点抠取器自检：与 `NamespacePrefixTest` 那份同款（两份拷贝）。 */
    @Test
    fun theWriteTargetExtractorFindsWritesAndOnlyWrites() {
        assertEquals(
            listOf("a/1", "b/2", "c/3", "d/4", "e/5", "f/6"),
            writeTargets("mkdir -p 'a/1' && touch 'b/2'; mv x 'c/3'; cp y 'd/4'; echo hi > 'e/5' 2>> 'f/6'"),
        )
        assertEquals("读一个都不许算成写", emptyList<String>(), writeTargets("tail -n 0 -f 'r/1' | grep -qF x 'r/2'"))
        assertEquals("贴着写的重定向也要认出来", listOf("/dev/null"), writeTargets("tmux has-session -t '=x' 2>/dev/null"))
        assertEquals("`2>&1` 是复制 fd、不写文件", emptyList<String>(), writeTargets("tmux kill-session -t 'x' 2>&1"))
        assertEquals("被引用的整段载荷是一个参数，不是本层的写", emptyList<String>(), writeTargets("tmux send-keys -t x 'mkdir -p q' Enter"))
    }

    // ---- 实现 -----------------------------------------------------------------

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 必须留字面：`new-session` 与会话名前缀都住在命令串字面里。
     * [theScannersReadCodeAndNothingElse] 里那句「裸一行真命中必须认出」喂的就是一条字面。
     * 它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    /** 数 [mark] 出现几次（不做标识符护栏 —— `new-session` 里有连字符，不可能被别的名字包住）。 */
    private fun countOccurrences(
        code: String,
        mark: String,
    ): Int {
        var n = 0
        var i = 0
        while (true) {
            val at = code.indexOf(mark, i)
            if (at < 0) break
            i = at + mark.length
            n++
        }
        return n
    }

    /** 数会话名用的 `"cc-` 前缀：后面紧跟 `monitor` 的是 cc-monitor 的二进制名，不算。 */
    private fun countSessionCcPrefix(code: String): Int {
        var n = 0
        var i = 0
        while (true) {
            val at = code.indexOf(CC_NAME_PREFIX, i)
            if (at < 0) break
            i = at + CC_NAME_PREFIX.length
            if (code.startsWith("monitor", i)) continue
            n++
        }
        return n
    }

    /** 从一条命令串里抠出每支 `tmux new-session` 的 `-s` 参数（`shQuote` 出来的单引号形态）。 */
    private fun newSessionNames(cmd: String): List<String> = NEW_SESSION_NAME_RE.findAll(cmd).map { it.groupValues[1] }.toList()

    /**
     * 把一条 shell 命令串切成词；单引号内的空白不切，`;` 与 `|` 在引号外也是分词符。
     * 与 `core-claude` 的 `NamespacePrefixTest.tokenize` 是同一份逻辑的两份拷贝。
     */
    private fun tokenize(cmd: String): List<String> {
        val out = mutableListOf<String>()
        val sb = StringBuilder()
        var inQuote = false
        var started = false

        fun flush() {
            if (started) out += sb.toString()
            sb.clear()
            started = false
        }
        for (c in cmd) {
            when {
                c == '\'' -> {
                    inQuote = !inQuote
                    started = true
                }
                !inQuote && (c.isWhitespace() || c == ';' || c == '|') -> flush()
                else -> {
                    sb.append(c)
                    started = true
                }
            }
        }
        flush()
        return out
    }

    /**
     * 从一条命令串里抠出被写的路径：写命令后面第一个非 flag 的词（`mv`/`cp` 取第二个），
     * 加上重定向（`>` `>>`，可带 fd 前缀）的目标。`2>&1` 是复制 fd、不算。
     *
     * 整段被单引号包住时（`send-keys '<载荷>'`）不下钻 —— 那一层的落点由造载荷的函数自己那条判据管。
     */
    private fun writeTargets(cmd: String): List<String> {
        val t = tokenize(cmd)
        val out = mutableListOf<String>()
        var i = 0
        while (i < t.size) {
            val tok = t[i]
            val redirect = REDIRECT_RE.matchEntire(tok)
            when {
                tok in WRITE_VERBS -> {
                    var j = i + 1
                    while (j < t.size && t[j].startsWith("-")) j++
                    val last = if (tok in TWO_OPERAND_VERBS) j + 1 else j
                    if (last < t.size) out += t[last]
                }
                redirect != null -> {
                    val glued = redirect.groupValues[2]
                    when {
                        glued.startsWith("&") -> Unit
                        glued.isNotEmpty() -> out += glued
                        i + 1 < t.size -> out += t[i + 1]
                    }
                }
            }
            i++
        }
        return out
    }

    /** 仓根 = 往上走到含 `settings.gradle.kts` 的那一层（Gradle 跑测试时 cwd = 模块目录）。 */
    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

    /** 扫描面：`app/src/main/kotlin` + 每个 `core-…` 模块的 `src/main/kotlin`（= 本仓全部 Kotlin 生产代码）。 */
    private fun productionSources(root: File): List<File> {
        val roots =
            (listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList()))
                .map { File(it, "src/main/kotlin") }
                .filter { it.isDirectory }
                .sortedBy { it.path }
        assertTrue("前提：得找到扫描面（app + core-*），实得 ${roots.size} 个源根", roots.size >= 2)
        return roots.flatMap { r -> r.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
    }

    companion object {
        private const val MAX_WALK_UP = 6

        private const val NEW_SESSION = "new-session"
        private const val CC_NAME_PREFIX = "\"cc-"
        private const val CC_MONITOR = ".cc-monitor"

        /** 管道前缀：手写定值，刻意不引 `PipeLauncher.PIPE_SESSION_PREFIX`。 */
        private const val PIPE_PREFIX_LITERAL = "atermpipe-"

        /** 透传探针名：刻意既不像我们（`atermpipe-`）也不像后端（`cc-`），否则「透传」与「加前缀」分不开。 */
        private const val PROBE_NAME = "probe_session_1"

        /** 允许出现的非路径写落点（丢弃噪声用，不留下任何东西）。 */
        private val ALLOWED_SINKS = setOf("/dev/null")

        /**
         * 定值钉：`new-session` 在生产代码里的 file -> 次数。
         *
         * 四支：`tmuxNewOrAttach` · `tmuxNewDetachedCommand` · `tmuxNewDetachedRunning` · `tmuxResumeCommand`。
         */
        private val PINNED_NEW_SESSION_SITES =
            mapOf("app/src/main/kotlin/com/ccmonitor/mobile/ssh/TmuxCommands.kt" to 4)

        /**
         * 定值钉：会话名前缀 `"cc-` 的两个合法住处（写侧 `ClaudeInvocation.resumeSessionName` 一处，
         * 读侧 `TmuxCommands.isCcmTmuxName` 一处）。
         */
        private val PINNED_CC_PREFIX_FILES =
            setOf(
                "app/src/main/kotlin/com/ccmonitor/mobile/ssh/TmuxCommands.kt",
                "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/command/ClaudeInvocation.kt",
            )

        /** 定值钉：推公钥那条命令串写到哪几个地方（OpenSSH 标准位，第三个名字空间）。 */
        private val PINNED_AUTHORIZED_KEY_TARGETS = setOf("~/.ssh", "~/.ssh/authorized_keys")

        /** `tmux new-session … -s '<名字>'`。`[^']*` 过不了引号 ⇒ 不会跨到下一支去。 */
        private val NEW_SESSION_NAME_RE = Regex("""new-session\b[^']*-s\s+'([^']*)'""")

        private val WRITE_VERBS = setOf("mkdir", "touch", "mv", "cp", "install")
        private val TWO_OPERAND_VERBS = setOf("mv", "cp", "install")
        private val REDIRECT_RE = Regex("""^([0-9]*>>?)(.*)$""")
    }
}
