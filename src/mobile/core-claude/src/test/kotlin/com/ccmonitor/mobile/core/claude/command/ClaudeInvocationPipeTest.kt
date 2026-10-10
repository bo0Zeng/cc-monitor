package com.ccmonitor.mobile.core.claude.command

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 常驻管道的命令构造。只钉命令串，不跑真 `claude`；信封那段脚本交给真 `/bin/sh` 跑。
 */
class ClaudeInvocationPipeTest {
    private val sid = "abc123-DEF_456"

    private companion object {
        /** 逐行流式那条判据的上限。攒着不吐 ⇒ 几秒内打红，不许挂着。 */
        const val STREAMING_TIMEOUT_MS = 10_000L

        /**
         * 跟 `in.ndjson` 那条 `tail` 的逐字形态。
         *
         * 只在定位（`indexOf`/顺序断言）时用它；「它是不是这个形态」由
         * [theTailDoesNotReplayOldUplinkLinesOnRestart] 用字面量单独钉：顺序断言引常量、形态断言引字面量，
         * 改了常量才不会两边一起绿。
         *
         * 注意：顺序断言必须先确认它找得到。`indexOf` 找不到给 -1，`a > indexOf(...)` 那种写法会恒真。
         */
        const val TAIL = "tail -n 0 -f"
    }

    /**
     * 起管道那条 `tail` 必须带 `-n 0`，否则重建时会重放旧上行。
     *
     * `tail -f` 默认是 `-n 10`：先把文件最后 10 行重放一遍再跟
     * （`printf 'l1\nl2\nl3\n' > f; timeout 1 tail -f f` 吐出 `l1 l2 l3`，加上 `-n 0` 之后吐空）。
     * `in.ndjson` 的最后 10 行正是最近发的话与控制帧 ⇒ 每重建一次管道 = 把它们重新喂给 `claude`
     * = 重复花额度、重复干活，而且那几条里可能就有一条 `control_request`（刚起来就被自己打断）。
     *
     * 首起时 `in.ndjson` 刚 `touch` 出来是空的，加不加 `-n 0` 一样。
     */
    @Test
    fun theTailDoesNotReplayOldUplinkLinesOnRestart() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid)
        assertTrue("必须逐字是 `tail -n 0 -f`：$cmd", cmd.contains("tail -n 0 -f '.aterm/s/$sid/in.ndjson'"))
        // 两侧都钉：不许有任何一条裸 `tail -f <文件>` 混在这条命令里
        assertFalse("绝不许退回裸 `tail -f`（默认 -n 10 ⇒ 重建时重放最近 10 行）：$cmd", cmd.contains("tail -f '"))
    }

    /** 整串逐段钉住：这条命令里每一段都有它的理由。 */
    @Test
    fun thePipeCommandIsPinnedByteForByte() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid)
        assertTrue("目录相对、且 quote", cmd.contains("mkdir -p '.aterm/s/$sid' &&"))
        assertTrue("管道那条在", cmd.contains("$TAIL '.aterm/s/$sid/in.ndjson' | env claude ${ClaudeInvocation.PIPE_FLAGS}"))
        assertTrue("落进契约文件", cmd.contains("} >> '.aterm/s/$sid/${PipeCommands.EVENTS}'"))
        assertTrue("首行是 __meta__ 契约头", cmd.contains("\"source\":\"pipeline\""))
        assertTrue("每行套 t_ns 信封：$cmd", cmd.contains("t_ns"))
        assertTrue("每行套 event 信封", cmd.contains("event"))
        // 信封是零 fork 的 shell 循环 + 只取一次基准时刻（见 ENVELOPE_LOOP）
        assertTrue("信封靠逐行读原样包，不改内容", cmd.contains(PipeCommands.ENVELOPE_LOOP))
    }

    /**
     * 信封不许赌 GNU 扩展。
     *
     * `date +%s%N` 里的 `%N` 是 GNU 扩展，BusyBox 上原样吐 `1785746457%N` ⇒ 每行都不是合法 JSON
     * ⇒ 被 `PipeSession` 逐行静默丢弃 ⇒ 下行全空，且没有一个字的诊断。
     *
     * 判据落在「命令里不许出现那个赌注」上，而不是「有没有写 awk」：换别的可移植写法同样该绿。
     */
    @Test
    fun theEnvelopeDoesNotBetOnAGnuOnlyExtension() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid)
        assertFalse("不许再用 `date +%N`（GNU 扩展，BusyBox 上吐字面量）：$cmd", cmd.contains("%N"))

        // 也不许每行 fork：200 次 fork 要 1.3 秒，一轮几千行就是白烧几秒。
        // 取一次基准时刻是应该的，要禁的是循环体里的那种，所以按位置判：`while` 之后不许再出现。
        val loop = PipeCommands.ENVELOPE_LOOP
        val body = loop.substringAfter("while ")
        assertTrue("前提：信封确实是个循环，否则下面这条判据判的是空气", loop.contains("while "))
        // `$((…))` 是算术展开（shell 内建、零 fork），不能跟命令替换 `$(…)` 混为一谈。
        val commandSubstitution = Regex("""\$\((?!\()""")
        assertFalse(
            "循环体里不许有命令替换（那就是 per-line fork）：$body",
            commandSubstitution.containsMatchIn(body),
        )
    }

    /**
     * 真跑那段脚本：产出的行必须是合法 JSON。
     *
     * 上面那条只钉「不许赌 GNU 扩展」，换个同样不可移植的东西照样绿。这条把脚本喂给本机真的 `sh`，
     * 断言吐出来的能被解析、`t_ns` 是个数、`event` 原样保留：那正是 `PipeSession` 逐行要做的事。
     *
     * 没有 `/bin/sh` 的机器会跳过这条；BusyBox 那一格是手工验过的（见 [PipeCommands.ENVELOPE_LOOP] 的注释），这里不覆盖。
     */
    @Test
    fun theEnvelopeScriptActuallyProducesValidJson() {
        val sh = java.io.File("/bin/sh").takeIf { it.canExecute() }
        org.junit.Assume.assumeTrue("本机没有 /bin/sh，跳过", sh != null)

        // 从生产常量里取出那段脚本，不在测试里另抄一份
        val out = runShell(PipeCommands.ENVELOPE_LOOP, """{"type":"assistant","message":{"id":"m1"}}""")
        assertTrue("前提：脚本真的吐了东西出来", out.isNotEmpty())

        val parsed =
            com.squareup.moshi.Moshi
                .Builder()
                .build()
                .adapter(Any::class.java)
                .fromJson(out.first()) as? Map<*, *>
        assertNotNull("产出的行必须是合法 JSON，实得：${out.first()}", parsed)
        assertTrue("t_ns 必须是个数，实得 ${parsed!!["t_ns"]}", parsed["t_ns"] is Number)
        assertTrue("要像个真时间戳（秒级 ×10^9 起步）", (parsed["t_ns"] as Number).toDouble() > 1.0e18)
        assertEquals("event 必须原样保留", "m1", ((parsed["event"] as Map<*, *>)["message"] as Map<*, *>)["id"])
    }

    /**
     * 信封必须逐行出，不许攒着。
     *
     * 一个 `awk` 进程（`systime()` + `fflush()`）的写法单测全绿，但真管道上一行都不出来：
     * mawk 对 stdin 是块缓冲，而 `fflush()` 只管输出那侧。后果是逐字流式整个没了，界面要等一整轮结束才蹦出来。
     *
     * 判据是「n 行进去，就得逐行出 n 行」：喂一行、读一行；攒着不吐的实现读第一行就会阻塞。
     * 必须带 `timeout`：攒着不吐的正确表现是几秒内打红，不是把 CI 卡住。
     */
    @Test(timeout = STREAMING_TIMEOUT_MS)
    fun theEnvelopeEmitsLineByLineInsteadOfBufferingUntilTheEnd() {
        val sh = java.io.File("/bin/sh").takeIf { it.canExecute() }
        org.junit.Assume.assumeTrue("本机没有 /bin/sh，跳过", sh != null)

        val proc =
            ProcessBuilder("/bin/sh", "-c", PipeCommands.ENVELOPE_LOOP)
                .redirectErrorStream(true)
                .start()
        val reader = proc.inputStream.bufferedReader()
        val got = mutableListOf<String>()
        for (i in 1..3) {
            proc.outputStream.write("{\"i\":$i}\n".toByteArray())
            proc.outputStream.flush()
            // 还没关 stdin 就要能读到这一行：攒到 EOF 才吐的实现会卡死在这里
            got += reader.readLine() ?: break
        }
        proc.outputStream.close()
        proc.waitFor()

        assertEquals("喂几行就该出几行（攒着不吐的话这里读不满）", 3, got.size)
        // 顺带钉住排序：行序必须严格递增，那是 `t_ns` 唯一被指望的事
        val ts = got.map { it.substringAfter("\"t_ns\":").substringBefore(',').toLong() }
        assertEquals("t_ns 必须严格递增（它是排序键）", ts.sorted(), ts)
        assertEquals("不许有重复", ts.size, ts.toSet().size)
    }

    /**
     * 信封必须能挂在管道右边。
     *
     * 单独 `sh -c '<信封>'` 跑怎么写都对。而生产里它是 `… | claude … | <信封>` 的最后一段：
     * 信封以 `ATERM_T0=$(date +%s); …` 开头时，`|` 只绑住第一个命令（那个赋值），后面的 `while`
     * 变成另一条语句、读的是外层 stdin ⇒ claude 的输出被丢进赋值里，`events.ndjson` 里永远只有那行 `__meta__`。
     */
    @Test(timeout = STREAMING_TIMEOUT_MS)
    fun theEnvelopeStillWorksWhenItIsTheRightHandSideOfAPipe() {
        val sh = java.io.File("/bin/sh").takeIf { it.canExecute() }
        org.junit.Assume.assumeTrue("本机没有 /bin/sh，跳过", sh != null)

        // 数据只能经由管道到达：生产者自己产，`sh` 的 stdin 是空的。
        // 若把数据喂给 `sh` 的 stdin，没分组时 `while` 从同一个 stdin 也读得到，这条判据就没有判别力。
        val piped = "printf '%s\\n' '{\"a\":1}' '{\"a\":2}' | ${PipeCommands.ENVELOPE_LOOP}"
        val out = runShell(piped)
        assertEquals("挂在管道右边时也得一行不少地出来，实得 $out", 2, out.size)
        assertTrue("内容要原样保留：$out", out[0].contains("\"event\":{\"a\":1}"))
    }

    /** 跑一段 shell、喂几行、收全部输出。 */
    private fun runShell(
        script: String,
        vararg lines: String,
    ): List<String> {
        val proc = ProcessBuilder("/bin/sh", "-c", script).redirectErrorStream(true).start()
        proc.outputStream.write(lines.joinToString("") { "$it\n" }.toByteArray())
        proc.outputStream.close()
        val out =
            proc.inputStream
                .readBytes()
                .decodeToString()
                .trim()
        proc.waitFor()
        return out.lines().filter { it.isNotBlank() }
    }

    /** 命令里不许出现 `$HOME`：见 `PipeCommands.sessionDir` 的注释。 */
    @Test
    fun thePathHasNoHomeVariableSoBothEndsAgreeByConstruction() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid)
        // 要守的是「起管道的一端与读写的一端指同一个目录」。用 `$HOME` 的话一端双引号（展开）、
        // 另一端 `shellQuote`（不展开）；相对路径让两端由构造保证一致，不管谁加了哪种引号。
        assertFalse("命令里不许出现 \$HOME", cmd.contains("\$HOME"))
        assertEquals("与 PipeSession 用的是同一个目录串", ".aterm/s/$sid", PipeCommands.sessionDir(sid))
    }

    /**
     * 嵌套环境标记必须 unset：继承时 Claude 自认子会话，不写 JSONL、不注册 pidfile、关窗即丢。
     * `CLAUDE_CONFIG_DIR` 刻意不在名单里。
     */
    @Test
    fun theNestedMarkersAreUnsetButConfigDirIsNot() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid)
        assertTrue(cmd.startsWith("unset "))
        for (v in listOf("CLAUDECODE", "CLAUDE_CODE_ENTRYPOINT", "CLAUDE_CODE_SESSION_ID", "CLAUDE_CODE_CHILD_SESSION")) {
            assertTrue("$v 必须被 unset", cmd.contains(v))
        }
        assertFalse("CLAUDE_CONFIG_DIR 刻意保留", cmd.contains("CLAUDE_CONFIG_DIR"))
    }

    /** `--include-partial-messages` 不能省：没有它就没有增量帧，手机上会整段整段地蹦。 */
    @Test
    fun partialMessagesIsNotOptional() {
        assertTrue(ClaudeInvocation.pipeInvocation(null, sid).contains("--include-partial-messages"))
        assertTrue(ClaudeInvocation.PIPE_FLAGS.contains("--input-format stream-json"))
        assertTrue(ClaudeInvocation.PIPE_FLAGS.contains("--output-format stream-json"))
    }

    /** 启动命令过白名单，fail-closed 回退 `claude`（用 `sanitizeLaunchCommand`，不另写一套）。 */
    @Test
    fun anInjectableLaunchCommandFallsBackToPlainClaude() {
        assertTrue(ClaudeInvocation.pipeInvocation("cct", sid).contains("| env cct "))
        assertTrue("含元字符 ⇒ 回退", ClaudeInvocation.pipeInvocation("claude; rm -rf /", sid).contains("| env claude "))
        assertFalse(ClaudeInvocation.pipeInvocation("claude; rm -rf /", sid).contains("rm -rf"))
    }

    /** 非法 sessionId 必须当场拒：它要进路径，不能靠「后面某处会检查」。 */
    @Test
    fun anInvalidSessionIdIsRejectedAtConstruction() {
        for (bad in listOf("", "../../etc", "a b", "a;b", "a\$b")) {
            val threw = runCatching { ClaudeInvocation.pipeInvocation(null, bad) }.isFailure
            assertTrue("「$bad」必须被拒", threw)
        }
    }

    /** 目录要先建：`>>` 不会建父目录；`in.ndjson` 也要先 touch，否则 `tail -f` 立刻退出。 */
    @Test
    fun theDirectoryAndInputFileAreCreatedBeforeThePipeStarts() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid)
        val mkdirAt = cmd.indexOf("mkdir -p")
        val touchAt = cmd.indexOf("touch '.aterm/s/$sid/in.ndjson'")
        val tailAt = cmd.indexOf(TAIL)
        assertTrue("前提：得先真找得到那条 tail，否则下面是空真断言（见 TAIL 的 KDoc）", tailAt >= 0)
        assertTrue("mkdir 要在最前", mkdirAt in 0 until touchAt)
        assertTrue("touch 要在 tail 之前 —— 文件不存在时 tail -f 会立刻退出、管道当场塌", touchAt < tailAt)
    }

    /**
     * Claude 得在用户项目里干活，不是在会话目录里。
     *
     * `cd <会话目录>` 放在整条命令最前面的话，Claude 的 cwd 就是 `.aterm/s/<sid>/`：项目的 `CLAUDE.md`、代码、
     * git 仓一个都看不见；而管道本身跑得好好的，门禁和真机验证都发现不了。
     *
     * 判据分两半，缺一不可：
     * 1. `cd` 必须收在子 shell 里（只换 claude 那个进程的 cwd）；
     * 2. 文件通道的三个重定向仍是相对路径，由外层 shell（cwd=家目录）解析。
     */
    @Test
    fun onlyClaudeChangesDirectorySoTheFileChannelStaysWhereBothEndsAgree() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid, workdir = "/home/u/proj")
        assertTrue("cd 要在子 shell 里且只包住 claude：$cmd", cmd.contains("| ( cd '/home/u/proj' && exec env claude "))
        // 外层不许有裸 `cd` —— 有的话文件通道就跟着漂走了
        assertFalse("外层不许 cd", cmd.replace("( cd '/home/u/proj'", "").contains(" cd "))
        for (f in listOf(PipeCommands.IN, PipeCommands.EVENTS, PipeCommands.LOG)) {
            assertTrue("通道文件 $f 必须是相对路径", cmd.contains("'.aterm/s/$sid/$f'"))
        }
        assertFalse("相对路径世界里不许混进 \$HOME", cmd.contains("\$HOME"))
    }

    // ---- 赋值前缀的启动命令，配不配工作目录都得能跑 ------------------

    /**
     * `exec` 后面第一个词不能是赋值。
     *
     * 启动命令白名单（`LAUNCH_UNSAFE`）不禁 `=` ⇒ `FOO=1 claude` 过得了校验。而配了工作目录那支用 `exec`，
     * `exec FOO=1 claude` 直接 `sh: exec: FOO=1: not found`：同一条命令，配了工作目录就死、不配反而能跑，
     * 诊断只落 `bridge.log`。
     *
     * 判据落在「`exec` 之后那个词是不是一个能执行的东西」上，不是「有没有写 env」：
     * 换别的写法（比如把赋值挪到子 shell 里）同样该绿。
     */
    @Test
    fun anAssignmentPrefixedLaunchCommandIsNotHandedToExecAsIfItWereAProgram() {
        val launch = "FOO=1 claude"
        assertEquals("前提：这种形态确实过得了白名单，所以只靠白名单拦不住", launch, ClaudeInvocation.sanitizeLaunchCommand(launch))

        val cmd = ClaudeInvocation.pipeInvocation(launch, sid, workdir = "/p")
        val firstWordAfterExec = cmd.substringAfter("&& exec ").substringBefore(' ')
        assertFalse("exec 后面第一个词是赋值 ⇒ 起不来：$firstWordAfterExec", firstWordAfterExec.contains('='))
        assertTrue("但那个赋值本身不能被丢掉", cmd.contains("FOO=1"))
    }

    // ---- 设置里那个「Claude 配置目录」要管得到起那侧 -------------

    /**
     * 覆盖过配置目录时，起出来的那个 Claude 必须认它。
     *
     * 读那侧按 `ClaudePaths.resolveClaudeDir`（主机覆盖 > 应用默认 > 远端自己的）解析；起那侧什么都不设的话
     * 读的是 A、起出来那个写的是 B。远端做了账号隔离时，管道起的 Claude 会是未登录态。
     */
    @Test
    fun anOverriddenConfigDirReachesTheLaunchedClaude() {
        val dir = "/home/u/.claude-b"
        val cmd = ClaudeInvocation.pipeInvocation(null, sid, claudeDir = dir)
        assertTrue("要传给起出来的那个 Claude：$cmd", cmd.contains("CLAUDE_CONFIG_DIR='$dir'"))
        // 它必须落在起 Claude 那一段，而不是整条命令最前面：那样会连带影响 tail/信封那些进程。
        // 先确认 tail 找得到：`indexOf` 找不到给 -1 ⇒ `> -1` 恒真。
        val tailAt = cmd.indexOf(TAIL)
        assertTrue("前提：得先真找得到那条 tail，否则下面是空真断言", tailAt >= 0)
        assertTrue("位置要在管道右侧", cmd.indexOf("CLAUDE_CONFIG_DIR") > tailAt)
    }

    /** 字面路径要 quote 住：含空格/引号的覆盖值不能把命令劈开。 */
    @Test
    fun aConfigDirWithSpacesOrQuotesCannotBreakTheCommand() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid, claudeDir = "/home/u/it's here")
        assertTrue("要 POSIX 转义：$cmd", cmd.contains("CLAUDE_CONFIG_DIR='/home/u/it'\\''s here'"))
    }

    /**
     * 没覆盖过就什么都不设，不把远端自己的选择覆盖掉。
     *
     * 默认值本身是 shell 表达式 `${'$'}{CLAUDE_CONFIG_DIR:-${'$'}HOME/.claude}`，语义就是「听远端自己的」。
     */
    @Test
    fun withoutAnOverrideNothingIsSetAndTheCommandIsUnchanged() {
        assertEquals(
            "不覆盖 ⇒ 与不传这个参数的产物逐字节相同",
            ClaudeInvocation.pipeInvocation("cct", sid, workdir = "/p"),
            ClaudeInvocation.pipeInvocation("cct", sid, workdir = "/p", claudeDir = null),
        )
        assertFalse("不覆盖就不许出现它", ClaudeInvocation.pipeInvocation(null, sid).contains("CLAUDE_CONFIG_DIR"))
    }

    /** 没给 workdir 就在会话目录里跑：行为要显式，不能是半个 `cd` 悬着。 */
    @Test
    fun withoutAWorkdirThereIsNoSubshellAtAll() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid)
        assertFalse("没 workdir 就不该有子 shell：$cmd", cmd.contains("( cd "))
        assertTrue("claude 仍在管道中间", cmd.contains("| env claude ${ClaudeInvocation.PIPE_FLAGS} |"))
    }

    /** workdir 是字面路径 ⇒ 必须被 quote 住，不能让空格/引号把命令劈开。 */
    @Test
    fun aWorkdirWithSpacesOrQuotesCannotBreakTheCommand() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid, workdir = "/home/u/it's here")
        assertTrue("要 POSIX 转义：$cmd", cmd.contains("cd '/home/u/it'\\''s here'"))
    }

    /**
     * `bypassPermissions` 刻意不提供。
     *
     * 官方移动端文档：「You can't select Bypass permissions from the app」。手机上屏小、误触多、
     * 常在路上没法细看要跑什么。它在 `claude --help` 的合法取值里，所以这条不是「不合法所以过不去」，
     * 是主动不提供；判据钉住这个区别。
     */
    @Test
    fun bypassPermissionsIsDeliberatelyNotOffered() {
        assertFalse(
            "不许出现在我们提供的清单里：${ClaudeInvocation.PERMISSION_MODES}",
            "bypassPermissions" in ClaudeInvocation.PERMISSION_MODES,
        )
        // 就算调用方硬传，也不许拼进命令行
        assertEquals("硬传也要被挡掉", "", ClaudeInvocation.permissionModeFlag("bypassPermissions"))
        assertFalse(
            "整条命令里不许出现它",
            ClaudeInvocation.pipeInvocation(null, sid, permissionMode = "bypassPermissions").contains("bypassPermissions"),
        )
    }

    /**
     * 非法值 fail-closed 到「不带这个参数」，不是原样拼进去。
     *
     * 拼一个 `claude` 不认的值进去它会当场退出，表现成「管道起来了但永远没有下行」，最难查。
     */
    @Test
    fun anUnknownModeFallsBackToNotPassingTheFlagAtAll() {
        for (bad in listOf("", "Full", "yolo", "acceptedits", "auto", "dontAsk", "--dangerously-skip-permissions")) {
            assertEquals("「$bad」必须被挡：", "", ClaudeInvocation.permissionModeFlag(bad))
        }
        assertEquals("null 也一样", "", ClaudeInvocation.permissionModeFlag(null))
    }

    /** 提供的那几个要真的进命令行，形状是 `--permission-mode <值>`（`claude --help` 的写法）。 */
    @Test
    fun anOfferedModeReachesTheCommandLine() {
        assertTrue("前提：清单非空", ClaudeInvocation.PERMISSION_MODES.isNotEmpty())
        for (mode in ClaudeInvocation.PERMISSION_MODES) {
            val cmd = ClaudeInvocation.pipeInvocation(null, sid, permissionMode = mode)
            assertTrue("「$mode」要进命令：$cmd", cmd.contains("--permission-mode $mode"))
        }
    }

    /**
     * 不选模式时，整条命令与不传这个参数的产物逐字节相等。判据写成关系式，不钉字面串。
     */
    @Test
    fun withoutAModeTheCommandIsByteForByteUnchanged() {
        assertEquals(
            "不选 ⇒ 与不传参数的产物完全相同",
            ClaudeInvocation.pipeInvocation("cct", sid, workdir = "/p"),
            ClaudeInvocation.pipeInvocation("cct", sid, workdir = "/p", permissionMode = null),
        )
        assertFalse(
            "不选就不许出现这个参数",
            ClaudeInvocation.pipeInvocation(null, sid).contains("--permission-mode"),
        )
    }

    /**
     * 接着已有对话跑，而不是起一条同名的空会话。
     *
     * 没有 `--resume` 时，点一条历史对话得到的是一条以那个 id 命名、内容全新的空会话：
     * 以为回到了那段对话，实际 Claude 什么都不记得。
     */
    @Test
    fun resumingAnExistingConversationPassesTheResumeFlag() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid, resumeSessionId = "abc123-DEF_456")
        assertTrue("要接着那条对话跑：$cmd", cmd.contains("--resume abc123-DEF_456"))
    }

    /** 不给 resume ⇒ 命令就是起新对话的那条。 */
    @Test
    fun withoutResumeTheCommandIsByteForByteUnchanged() {
        assertEquals(
            "不 resume ⇒ 与不传这个参数的产物完全相同",
            ClaudeInvocation.pipeInvocation("cct", sid, workdir = "/p"),
            ClaudeInvocation.pipeInvocation("cct", sid, workdir = "/p", resumeSessionId = null),
        )
        assertFalse("不 resume 就不许出现这个参数", ClaudeInvocation.pipeInvocation(null, sid).contains("--resume"))
    }

    /**
     * 非法 sid fail-closed 到「不带这个参数」，不是原样拼进命令行。
     *
     * sid 要进 shell 命令行 ⇒ 不过白名单就是注入向量；拼一个 `claude` 不认的值进去它会当场退出，
     * 表现成「管道起来了但永远没有下行」。
     */
    @Test
    fun anInvalidResumeIdIsRefusedRatherThanInterpolated() {
        for (bad in listOf("", "a b", "a;rm -rf /", "../../etc", "a\$b", "a`id`")) {
            assertEquals("「$bad」必须被挡：", "", ClaudeInvocation.resumeFlag(bad))
            assertFalse(
                "整条命令里不许出现它",
                ClaudeInvocation.pipeInvocation(null, sid, resumeSessionId = bad).contains("--resume"),
            )
        }
    }

    /**
     * stderr 必须有落点，且不许污染 `events.ndjson`。
     *
     * 最常见的失败是 `claude` 打一句 "Not logged in" 到 stderr 就退出。没有落点的话，管道「起成功了」、
     * `events.ndjson` 永远只有那行 `__meta__`、界面一直空白，没有任何地方说得出为什么。
     * 用 `2>&1` 又会把非 JSON 行混进契约文件、被下游逐行静默丢弃，诊断信息和数据一起毁掉。
     */
    @Test
    fun stderrGoesToItsOwnLogAndNeverIntoTheContractFile() {
        val cmd = ClaudeInvocation.pipeInvocation(null, sid)
        assertTrue("stderr 要有落点：$cmd", cmd.contains("2>> '.aterm/s/$sid/${PipeCommands.LOG}'"))
        assertFalse("不许 2>&1 —— 会把非 JSON 行混进契约文件", cmd.contains("2>&1"))
        assertTrue("落点与契约文件必须是两个文件", PipeCommands.LOG != PipeCommands.EVENTS)
    }

    // ---- 新对话不许分叉出两个编号 ---------------------------------------

    /**
     * 新对话必须把编号指定给 Claude。
     *
     * 否则新对话有两个编号：我们发一个（远端目录名 + 那条常驻命令的标识），Claude 自己另发一个
     * （`init.sid`、`--resume` 认的那个、会话记录的文件名）。两者对不上 ⇒ 下次从列表里找不回这条对话。
     *
     * 按常驻管道形态传 `--session-id <uuid>`，产出的每一行（连启动的 `hook_started`）`session_id` 都是给的那个。
     */
    @Test
    fun aNewConversationHandsItsIdToClaude() {
        val uuid = "6faeb51d-125c-43a9-b1cf-77a2319009de"
        val cmd = ClaudeInvocation.pipeInvocation(null, uuid, newSessionId = uuid)
        assertTrue("要把编号指定给 Claude：$cmd", cmd.contains("--session-id $uuid"))
        assertFalse("新对话不许同时 resume", cmd.contains("--resume"))
        // 前提：目录名用的是同一个编号，不然「一个编号」这句话就不成立
        assertTrue("远端目录也该是这个编号", cmd.contains("'.aterm/s/$uuid/"))
    }

    /**
     * 非法 UUID 当场炸，不是静默丢。
     *
     * 与 [ClaudeInvocation.resumeFlag] 的 fail-closed 刻意相反：resume 丢掉最多是「没接上」，
     * 而这里丢掉会让两个编号重新分叉，且从代码上看不出来。
     */
    @Test
    fun aNonUuidNewConversationIdIsRejectedLoudly() {
        // `aterm-<uuid>` 过得了 sessionId 白名单，过不了 UUID 这关
        val oldShape = "aterm-6faeb51d-125c-43a9-b1cf-77a2319009de"
        assertTrue("前提：它确实过得了 sessionId 白名单，所以只靠那道关拦不住", ClaudeInvocation.isValidSessionId(oldShape))
        for (bad in listOf(oldShape, "abc123-DEF_456", "", "not-a-uuid")) {
            val threw = runCatching { ClaudeInvocation.pipeInvocation(null, "s1", newSessionId = bad) }.isFailure
            assertTrue("「$bad」必须当场炸，不许静默丢", threw)
        }
    }

    /**
     * 编号已经有记录时，必须接着跑，不能再把它「指定」给 Claude。
     *
     * 把 `--session-id <uuid>` 传给一个已有会话记录的编号：
     * ```
     * Error: Session ID <uuid> is already in use.
     * ```
     * CLI 当场退出（stdout 空；记录本身没被改动）。
     *
     * 可达路径：新对话聊过之后那条常驻命令死了，从返回栈回到这一屏 ⇒ 又传一次 ⇒ CLI 退出，
     * 而 tmux 会话确实建起来了 ⇒ 报「起成功」⇒ 界面永远空白，原因只落在 `bridge.log`。
     *
     * 所以得问远端自己（记录在不在只有它知道），且在同一条命令里问，不另开往返。
     * 判据落在「两条路都得在，且分别指向正确的 flag」上。
     */
    @Test
    fun aNewConversationAsksTheRemoteWhetherItAlreadyHasARecord() {
        val uuid = "6faeb51d-125c-43a9-b1cf-77a2319009de"
        val cmd = ClaudeInvocation.pipeInvocation(null, uuid, workdir = "/home/u/proj", newSessionId = uuid)

        assertTrue("要去看那条记录在不在：$cmd", cmd.contains("[ -e ") && cmd.contains("/projects/-home-u-proj/$uuid.jsonl"))
        assertTrue("有记录 ⇒ 接着跑", cmd.contains("--resume $uuid"))
        assertTrue("没记录 ⇒ 把编号交出去", cmd.contains("--session-id $uuid"))
        // 判定必须发生在起 Claude 之前
        val tailAt = cmd.indexOf(TAIL)
        assertTrue("前提：得先真找得到那条 tail，否则下面是空真断言", tailAt >= 0)
        // 两侧都要护：`[ -e ` 这一侧缺失时 `indexOf` 也给 -1，而 `-1 < tailAt` 恒真，
        // 「判定要在管道之前」在「判定整个消失了」时照样绿。
        val probeAt = cmd.indexOf("[ -e ")
        assertTrue("前提：得先真找得到那条判定，否则下面是空真断言", probeAt >= 0)
        assertTrue("判定要在管道之前", probeAt < tailAt)
        // 而且真正交给 Claude 的是那个变量，不是写死的某一个
        assertTrue("命令行上用的必须是判定结果", cmd.contains("\$ATERM_SID_FLAG"))
    }

    /**
     * 没有工作目录时问不了：记录落在 `<配置目录>/projects/<编码后的 cwd>/` 下，
     * 而那时 cwd 是登录目录、不知道它是什么 ⇒ 原样传 `--session-id`。
     *
     * 那一格里 `--resume` 本来也不可靠（它按 cwd 划分作用域），这一格没有更好的答案。
     */
    @Test
    fun withoutAWorkdirTheRecordCannotBeProbedSoTheIdIsSimplyHandedOver() {
        val uuid = "6faeb51d-125c-43a9-b1cf-77a2319009de"
        val cmd = ClaudeInvocation.pipeInvocation(null, uuid, newSessionId = uuid)
        assertTrue("退回直接指定", cmd.contains("--session-id $uuid"))
        assertFalse("探不了就不许假装探过", cmd.contains("[ -e "))
        assertFalse("也不该出现 resume", cmd.contains("--resume"))
    }

    /** 覆盖过配置目录时，探的必须是那个目录下的记录：探错地方等于没探。 */
    @Test
    fun theRecordIsProbedUnderTheOverriddenConfigDir() {
        val uuid = "6faeb51d-125c-43a9-b1cf-77a2319009de"
        val cmd =
            ClaudeInvocation.pipeInvocation(null, uuid, workdir = "/p", newSessionId = uuid, claudeDir = "/home/u/acct")
        assertTrue("要在覆盖值下面找：$cmd", cmd.contains("[ -e '/home/u/acct'/projects/-p/$uuid.jsonl ]"))
    }

    /** 两者互斥：一条对话要么接着已有的跑，要么是新开的。都给 = 接线错了，必须炸。 */
    @Test
    fun resumingAndHandingOutAnIdAreMutuallyExclusive() {
        val uuid = "6faeb51d-125c-43a9-b1cf-77a2319009de"
        val threw =
            runCatching {
                ClaudeInvocation.pipeInvocation(null, uuid, resumeSessionId = uuid, newSessionId = uuid)
            }.isFailure
        assertTrue("两个都给必须炸", threw)
    }

    /** 不给新编号 ⇒ 命令就是不带 `--session-id` 的那条。 */
    @Test
    fun withoutANewIdTheCommandIsByteForByteUnchanged() {
        assertEquals(
            ClaudeInvocation.pipeInvocation("cct", sid, workdir = "/p"),
            ClaudeInvocation.pipeInvocation("cct", sid, workdir = "/p", newSessionId = null),
        )
        assertFalse("不指定就不许出现这个参数", ClaudeInvocation.pipeInvocation(null, sid).contains("--session-id"))
    }
}
