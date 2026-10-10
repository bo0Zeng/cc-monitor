package com.ccmonitor.mobile.core.claude

import com.ccmonitor.mobile.core.claude.bridge.PipeSession
import com.ccmonitor.mobile.core.claude.command.ChatAttachment
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.command.PipeCommands
import com.ccmonitor.mobile.core.claude.transport.DaemonCommands
import com.ccmonitor.mobile.core.claude.transport.DaemonLocator
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.emptyFlow
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 会写远端的命令串，其真实输出里的路径只落我们的名字空间。
 *
 * 远端分两层：通用层是 cc-monitor 的（`~/.cc-monitor/` 下的一切、tmux `cc-*`），只读；
 * 专属层是我们的（`~/.aterm/` 下的一切、tmux `atermpipe-*`），才写。越界的失败形态是
 * 两边都以为自己成功了（共用落点、谁后放谁生效），不会有任何一处报错。
 *
 * 注意：这段注释里别写 `.aterm/` 紧跟两个星号，Kotlin 块注释可嵌套，会报 `Unclosed comment`。
 *
 * 这里量的是真实输出，不是源码：把所有会写远端的命令产出函数各打一次，从返回的那串字节里
 * 把写动词后面那个路径抠出来。源码扫描住 app 侧的 `NamespaceLiteralScanTest`：读别的模块源文件的
 * 判据必须住在那个 task 的输入涵盖得到的地方，`:core-claude:test` 的输入不含 `app/`，
 * 改了那边它照样 `UP-TO-DATE`；`:app:testDebugUnitTest` 的 classpath 含全部 core 模块，任何改动都会让它重跑。
 *
 * 抓得到：把 `pipeInvocation` 的 `dir` 换成 cc-monitor 的目录；把 `ATERM_HOME` 常量本身改掉
 * （[theConstantsStillSayWhatTheCharterSays] 拿手写定值比）。
 * 抓不到：新增一支不在下面那张表里的写命令（由 `NamespaceLiteralScanTest` 的源码扫描当第二道）；
 * 载荷被单引号整段包住那一层（`send-keys '<载荷>'`），抠取器不下钻。
 *
 * 两种合法写法都不当红：`shellQuote` 无条件加单引号，被引用的值里 `~` / `$HOME` 不会展开。
 * 于是 `ATERM_HOME` 是相对家目录的 `.aterm`（`mkdir -p .aterm/…` 在登录 shell 的家目录下跑），
 * 而 `DaemonLocator.HOME_PREFIX` 是 `"$HOME"/`（双引号，交远端 shell 展开）。
 * 这里只认「前缀是不是我们的名字空间」，不对这两种写法之一表态。
 */
class NamespacePrefixTest {
    /**
     * 把所有会写远端的命令产出函数各打一次，从返回串里把写动词
     * （`mkdir` · `touch` · `mv` · `cp` · `install` · `>` · `>>`）后面那个路径抠出来，
     * 断言每一个都落在 `.aterm/` 下面。
     *
     * 期望前缀是这里手写的定值 [OUR_HOME]，不是 `ClaudeInvocation.ATERM_HOME`：拿常量当期望值的话，
     * 「常量本身被改成 `.cc-monitor`」这一刀会恒绿。覆盖面就是下面这张表，分母 [EXPECTED_CORE_WRITE_TARGETS] 写死。
     */
    @Test
    fun everyWriteCommandLandsUnderOurNamespace() {
        val sid = "0f9d3c2a-1111-4222-8333-444455556666"
        val session = PipeSession(RemoteCommandChannel { emptyFlow() }, sid)
        val commands: Map<String, String> =
            linkedMapOf(
                "ClaudeInvocation.pipeInvocation(新对话)" to ClaudeInvocation.pipeInvocation(null, sid),
                "ClaudeInvocation.pipeInvocation(带 workdir/resume)" to
                    ClaudeInvocation.pipeInvocation("cct", sid, "/home/u/proj", "plan", sid, null, "/home/u/.claude"),
                "PipeSession.idempotentAppendCommand" to session.idempotentAppendCommand("你好", "up-1"),
                "PipeSession.interruptCommand" to session.interruptCommand("req_1_deadbeef"),
                "ChatAttachment.mkdirCommand" to ChatAttachment.mkdirCommand(sid),
            )

        var targets = 0
        for ((who, cmd) in commands) {
            assertFalse("$who 的命令串里出现了 cc-monitor 的名字空间 `$CC_MONITOR`：\n$cmd", cmd.contains(CC_MONITOR))
            assertFalse("$who 的命令串里出现了 `/tmp`（重启即丢且是共享名字空间）：\n$cmd", cmd.contains("/tmp"))
            val ts = writeTargets(cmd)
            assertTrue("前提：$who 里一个写动词都没抠出来 ⇒ 抠取器瞎了、这条判据对它恒绿：\n$cmd", ts.isNotEmpty())
            targets += ts.size
            for (t in ts) {
                assertTrue(
                    "$who 往 `$t` 写东西：不在我们的名字空间（`$OUR_HOME/`）下。\n整条命令：\n$cmd",
                    t == OUR_HOME || t.startsWith("$OUR_HOME/") || t in ALLOWED_SINKS,
                )
            }
        }
        assertEquals("前提：抠出来的写落点总数与逐处读数对不上 ⇒ 先核抠取器再改这个数", EXPECTED_CORE_WRITE_TARGETS, targets)

        // SFTP 那条不是 shell 命令（`sftpUpload` 直接给远端路径），单独断言落点。
        val upload = ChatAttachment.remotePath(sid, "a.png")
        assertTrue("附件上传落点 `$upload` 不在 `$OUR_HOME/` 下", upload.startsWith("$OUR_HOME/"))
    }

    /**
     * 碰 cc-monitor 名字空间的那条路，真实输出里一个字节都没写。
     *
     * `DaemonLocator.candidates` 是唯一合法提到 `.cc-monitor` 的地方。这条把候选表真的渲染成远端命令，
     * 问它写了什么：除了 `/dev/null` 什么都没有（`command -v` 那两条把噪声丢进去），
     * 即只有 `[ -x … ]` 与 `command -v --` 这两种纯探测。
     *
     * 不替代 `DaemonCommands.presence` 自己那条逐字节判据（「绝不执行候选」），这里只答名字空间那一问。
     */
    @Test
    fun theDaemonLocatorCandidatesOnlyProbeAndNeverWrite() {
        val cands = DaemonLocator.candidates(null)
        assertTrue("前提：默认候选表得是非空的（空表 ⇒ 本条恒绿）", cands.size >= 2)
        val locate = DaemonCommands.locate(cands.mapIndexed { i, c -> DaemonCommands.presence(i, c.rendered, c.byName) })
        assertTrue("前提：这就该是碰 `$CC_MONITOR` 的那条路；不含它 ⇒ 量错东西了：\n$locate", locate.contains(CC_MONITOR))
        for (t in writeTargets(locate)) {
            assertTrue("cc-monitor 名字空间那条路只许读，却往 `$t` 写了东西：\n$locate", t in ALLOWED_SINKS)
        }
    }

    /**
     * 常量本身仍是那个前缀。
     *
     * [everyWriteCommandLandsUnderOurNamespace] 刻意用手写定值当期望，「常量被改」那一刀落在这里。
     */
    @Test
    fun theConstantsStillSayWhatTheCharterSays() {
        assertEquals("专属层路径前缀", OUR_HOME, ClaudeInvocation.ATERM_HOME)
        assertEquals("会话目录布局（见 `bridge/PROTOCOL.md`）", "$OUR_HOME/s/x", PipeCommands.sessionDir("x"))
    }

    /**
     * 抠取器自检：[everyWriteCommandLandsUnderOurNamespace] 的判别力全押在它身上。
     *
     * 它要是只认得出零个写动词，那条判据对任何命令串都恒绿；
     * 它要是把 `tail -f <路径>` 这种读也算成写，那条判据会开始误红。两种都要证不会。
     */
    @Test
    fun theWriteTargetExtractorFindsWritesAndOnlyWrites() {
        assertEquals(
            "五种写动词都要认出（含引号包着的路径与 `2>>` 这种带 fd 的重定向）",
            listOf("a/1", "b/2", "c/3", "d/4", "e/5", "f/6"),
            writeTargets("mkdir -p 'a/1' && touch 'b/2'; mv x 'c/3'; cp y 'd/4'; echo hi > 'e/5' 2>> 'f/6'"),
        )
        assertEquals("`tail -f` / `grep` 这种读一个都不许算成写", emptyList<String>(), writeTargets("tail -n 0 -f 'r/1' | grep -qF x 'r/2'"))
        assertEquals("贴着写的重定向（`2>/dev/null`）也要认出来", listOf("/dev/null"), writeTargets("tmux has-session -t '=x' 2>/dev/null"))
        assertEquals("`2>&1` 是复制 fd、不写文件，不许算成落点", emptyList<String>(), writeTargets("tmux kill-session -t 'x' 2>&1"))
        // 单引号里的空格不许把一个路径切成两半（`shellQuote` 出来的路径可能带空格）。
        assertEquals("引号内的空格不许切词", listOf("a b/c"), writeTargets("mkdir -p 'a b/c'"))
        // 整段被引号包住时，里面的写动词是参数不是命令，不该被算成本层的写。
        assertEquals("被引用的整段载荷是一个参数，不是本层的写", emptyList<String>(), writeTargets("tmux send-keys -t x 'mkdir -p q; echo z >> w' Enter"))
    }

    // ---- 抠取器 ---------------------------------------------------------------

    /**
     * 把一条 shell 命令串切成词，单引号内的空白不切（`shellQuote` 出来的路径可能带空格）。
     *
     * 只认单引号：所有路径都经 `shellQuote`（无条件单引号）。双引号那几条（`DaemonLocator.HOME_PREFIX`、
     * `-c "$cwd"`）是读或用户值，不在射程里。`;` 与 `|` 在引号外也是分词符，否则
     * `touch 'b/2'; mv …` 抠出来的落点会挂着个分号。
     *
     * app 侧 `TmuxSessionNameSourceTest` 里有一份拷贝（没配跨模块共享测试夹具），改一份要核另一份。
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
     * 从一条命令串里抠出被写的路径。
     *
     * 认两类：① 写命令（[WRITE_VERBS]）后面第一个非 flag 的词；② 重定向（`>` `>>`，可带 fd 前缀，
     * 目标贴着写或空格分开都认）后面那个词。
     *
     * 整段被单引号包住时（`send-keys '<载荷>'`），里面的写动词是一个参数，不下钻：
     * 那一层的落点由造那个载荷的函数自己那条判据管。
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
                    // `mv a b` / `cp a b` 写的是最后那个操作数；`mkdir` / `touch` 写第一个。
                    val last = if (tok in TWO_OPERAND_VERBS) j + 1 else j
                    if (last < t.size) out += t[last]
                }
                redirect != null -> {
                    val glued = redirect.groupValues[2]
                    when {
                        // `2>&1` 是复制文件描述符，不写任何文件，不许算成落点。
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

    companion object {
        /** 专属层前缀：手写定值，刻意不引 `ClaudeInvocation.ATERM_HOME`。 */
        private const val OUR_HOME = ".aterm"

        private const val CC_MONITOR = ".cc-monitor"

        /**
         * 允许出现的非路径写落点，只有一个：`/dev/null`（丢弃噪声，
         * 见 `DaemonCommands.presence` 的 `command -v … >/dev/null 2>&1`）。
         * 它不属于任何人的名字空间，也不留下任何东西。
         */
        private val ALLOWED_SINKS = setOf("/dev/null")

        /**
         * 上面五条命令串抠出来的写落点总数。分母写死：抠取器少认了一类，
         * 这个数对不上就红，而不是悄悄把覆盖面缩到零还全绿。
         *
         * 构成：`pipeInvocation` 两支各 4（`mkdir` · `touch` · `>>` · `2>>`）·
         * `idempotentAppendCommand` 2（`2>/dev/null` 的丢弃口 + `>>`）· `interruptCommand` 1 ·
         * `mkdirCommand` 1 = 12。
         */
        private const val EXPECTED_CORE_WRITE_TARGETS = 12

        /** 会写的命令。读那些（`tail` `grep` `find` `cat`）刻意不在表里。 */
        private val WRITE_VERBS = setOf("mkdir", "touch", "mv", "cp", "install")

        /** 写的是最后那个操作数的（`mv a b` / `cp a b`）。 */
        private val TWO_OPERAND_VERBS = setOf("mv", "cp", "install")

        /** `>` / `>>`，可带 fd 前缀（`2>>`），目标可贴着写（`2>/dev/null`）也可空格分开。 */
        private val REDIRECT_RE = Regex("""^([0-9]*>>?)(.*)$""")
    }
}
