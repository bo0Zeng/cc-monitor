package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * [DaemonLocator]：后端二进制怎么找到。
 *
 * 每条要害判据旁边放一条故意写松的「乘客」对照（`contains` / 集合相等 / 只验非空）：
 * 在对应的错法下，要害那条红、乘客那条绿，这个差就是松写法守不住的证据。
 *
 * 注意：这里结构上证不了「名字是对的」，只能证「表变了会被迫停下来」。[fakeRemote] 认的是我们自己的模板；
 * 后端改落点那天这里照样全绿，名字对不对只能去真机上 `ls ~/.cc-monitor/bin/`（读数在 `DaemonLocator` 的类注释里）。
 *
 * [fakeRemote] 把远端脚本化，造出四种远端现实：只有真落点 / 只有 PATH 名字 / 两个都在 / 都不在；
 * 另加一档「不在表上的名字装着」，证明它不被找到。
 */
class DaemonLocatorTest {
    private val home = "/home/u"

    // ── 远端脚本化：一个只认得懂我们自己那套模板的迷你 shell ────────────────────────

    /**
     * 假远端。给一组「远端真实存在的东西」（绝对路径 或 PATH 上的名字），
     * 按真正发出去的那条命令逐句判定，产出 daemon 那侧会打回来的 stdout。
     *
     * 它是解释器不是复读机：不知道候选表长什么样，只认模板的两种句型。生产那侧把模板改宽（`|| X`）、
     * 把 `$HOME` 引死（[com.ccmonitor.mobile.core.remote.shellQuote]）、或把 `; true` 去掉，它都会跟着表现不同。
     */
    private fun fakeRemote(
        existing: Set<String>,
        seen: MutableList<String> = mutableListOf(),
    ): RemoteCommandChannel =
        RemoteCommandChannel { cmd ->
            seen += cmd
            val out = StringBuilder()
            for (clause in cmd.split("; ")) {
                // 收尾那句 `true && printf '\nATERM_Q_OK\n'`：它就是「命令跑完了」的肯定证据。
                if (OK_CLAUSE.matches(clause)) {
                    out.append("\n").append(DaemonCommands.QUERY_OK_MARKER).append("\n")
                    continue
                }
                val m = PATH_CLAUSE.matchEntire(clause) ?: NAME_CLAUSE.matchEntire(clause) ?: continue
                val target = expand(m.groupValues[1])
                if (target !in existing) continue
                out
                    .append(DaemonCommands.LOCATE_HIT_MARKER)
                    .append(" ")
                    .append(m.groupValues[2])
                    .append(" ")
                    .append(expand(m.groupValues[3]))
                    .append("\n")
            }
            flowOf(out.toString().toByteArray())
        }

    /** 远端 shell 的展开规则：只有双引号的 `"$HOME"/` 会展开；单引号里的一律字面。 */
    private fun expand(word: String): String =
        when {
            word.startsWith(DaemonLocator.HOME_PREFIX) -> "$home/" + unquote(word.removePrefix(DaemonLocator.HOME_PREFIX))
            else -> unquote(word)
        }

    private fun unquote(s: String): String = if (s.startsWith("'") && s.endsWith("'") && s.length >= 2) s.substring(1, s.length - 1) else s

    // ── 候选表：是序不是集合 ──────────────────────────────────────────────

    /**
     * 候选表整体的定值钉：顺序 + 内容，逐项比对。
     *
     * 改这张表之前先过四条：
     * ① 它是后端本体还是 bash 启动器？② 真机上裸跑会不会多起一条会话？
     * ③ `NewUiSettingsTest` 那份启动器黑名单要不要跟着加？④ 它认不认 `--` 那道分派门？
     *
     * 注意：这条钉子对「后端改名」恒哑，它只在我们自己改表时红。不许把它引作「改名不会打到我们」的证据。
     */
    @Test
    fun theCandidateTableIsPinnedInOrder() {
        val table = DaemonLocator.candidates(userPath = null)
        assertEquals(
            "改候选表之前先过四条（见本测试 KDoc），过完了再把新表抄到这里",
            listOf(
                "\"\$HOME\"/.cc-monitor/bin/ccm" to false,
                "'ccm'" to true,
            ),
            table.map { it.rendered to it.byName },
        )
    }

    /**
     * 首位必须是真机上量到过的那个落点，且是一条路径，不是 PATH 上的名字。
     *
     * 定值表换成任何一张新表都能被抄绿，所以单列：先问量到过的那条路径。
     * PATH 上那条有歧义（可能有一份同名旧 bash 启动器），排在它后面才安全。
     */
    @Test
    fun theFirstCandidateIsTheLandingWeActuallyMeasured() {
        val first = DaemonLocator.candidates(userPath = null).first()
        assertEquals("首位必须是 10-02 `ls ~/.cc-monitor/bin/` 现打的那条", "\"\$HOME\"/.cc-monitor/bin/ccm", first.rendered)
        assertFalse("首位必须走 `[ -x 路径 ]`，不是 `command -v 名字`（后者有歧义）", first.byName)
    }

    /**
     * 两个不再存在的名字一个都不许出现在命令串里。
     *
     * `cc-monitor-backend` 与 `cc-monitor-remote` 都不认 `--` 那道分派门，而我们发的每条命令都带它，
     * 所以它们是结构上永远不可能成功的候选，只会把「找过这 N 个地方」那句话撑胖。
     *
     * 自检在先：先证明扫描面里真有活着的那个名字，否则两条 `assertFalse` 对空串也绿。
     */
    @Test
    fun theUnsupportedBackendNamesAreNotInTheCandidateTable() {
        val cmd = DaemonLocator.probeCommand(DaemonLocator.candidates(userPath = null))
        assertTrue("自检：扫描面里得有活着的 `ccm`，否则下面两条对任何东西都绿：\n$cmd", cmd.contains(DaemonLocator.BACKEND_NAME))
        assertFalse("`cc-monitor-backend` 不许出现：\n$cmd", cmd.contains("cc-monitor-backend"))
        assertFalse("`cc-monitor-remote` 不许出现：\n$cmd", cmd.contains("cc-monitor-remote"))
    }

    /**
     * 乘客对照：只钉「集合相等」的写法。
     *
     * 它在「顺序颠倒」下是绿的，而顺序正是要紧的那一维（确定的路径必须排在有歧义的 PATH 名字前面）。
     */
    @Test
    fun passengerTheCandidateSetIgnoresOrder() {
        val rendered = DaemonLocator.candidates(userPath = null).map { it.rendered }.toSet()
        assertEquals(
            setOf(
                "\"\$HOME\"/.cc-monitor/bin/ccm",
                "'ccm'",
            ),
            rendered,
        )
    }

    /** 设置里填了 ⇒ 只有它一条，不回退（回退会把显式选择变成猜）。 */
    @Test
    fun anExplicitPathIsUsedAloneWithNoFallback() {
        val table = DaemonLocator.candidates("/opt/ccm/cc-monitor-backend")
        assertEquals(1, table.size)
        assertEquals("'/opt/ccm/cc-monitor-backend'", table.single().rendered)
    }

    /**
     * 设置栏里填 `~/…` 不许被 `shellQuote` 引死。
     *
     * `shellQuote` 无条件加单引号 ⇒ `'~/…'` 必然 no-such-file，而填的路径明明是对的，得不到任何提示。
     */
    @Test
    fun aTildePathIsExpandedByTheRemoteShellNotQuotedToDeath() {
        assertEquals(
            "\"\$HOME\"/'.aterm/bin/cc-monitor-backend'",
            DaemonLocator.candidates("~/.aterm/bin/cc-monitor-backend").single().rendered,
        )
        assertEquals(
            "\"\$HOME\"/'.aterm/bin/cc-monitor-backend'",
            DaemonLocator.candidates("\$HOME/.aterm/bin/cc-monitor-backend").single().rendered,
        )
    }

    /** 界面上那个占位名 / 空 / 全空白 ⇒ 「没填」，走整张表。 */
    @Test
    fun thePlaceholderMeansUnsetNotAChoice() {
        assertEquals(null, DaemonLocator.userPathOrNull(DaemonLocator.UNSET_PLACEHOLDER))
        assertEquals(null, DaemonLocator.userPathOrNull(""))
        assertEquals(null, DaemonLocator.userPathOrNull("   "))
        assertEquals("/opt/x", DaemonLocator.userPathOrNull("  /opt/x  "))
    }

    // ── 命令串：白名单式，逐字节 ────────────────────────────────────────

    /**
     * 定位不许把候选跑起来，且命令串逐字节等于模板。
     *
     * 必须逐字节而不是 `contains`：`assertTrue(cmd.contains("command -v"))` 在 `command -v X || X` 下照样绿，
     * 而裸跑一个 bash 启动器会在远端真起一条 Claude 会话。
     *
     * 同时钉住 `$HOME` 那两条不走 shellQuote：引死之后拼出来的是 `'$HOME/…'` 这种必然 no-such-file 的字面路径。
     */
    @Test
    fun theLocateCommandIsByteForByteTheTemplateAndNeverRunsACandidate() {
        assertEquals(
            "定位命令必须逐字节等于模板——多出来的任何片段（尤其 `|| X` 这种顺手跑一下）都要红",
            EXPECTED_LOCATE_COMMAND,
            DaemonLocator.probeCommand(DaemonLocator.candidates(userPath = null)),
        )
    }

    /**
     * 乘客对照：三条「明显在验定位」的松写法。
     *
     * 它们在「模板写成 `command -v X || X`」和「`$HOME` 改走 shellQuote」两种错法下全是绿的。
     */
    @Test
    fun passengerTheLooseAssertionsAboutTheLocateCommand() {
        val cmd = DaemonLocator.probeCommand(DaemonLocator.candidates(userPath = null))
        assertTrue("① 命令非空", cmd.isNotEmpty())
        assertTrue("② 用了 command -v", cmd.contains("command -v"))
        assertTrue("③ 带了肯定标记", cmd.contains(DaemonCommands.QUERY_OK_MARKER))
    }

    /**
     * 白名单式的第二半：整条命令里只许出现这几个动词。
     *
     * 逐字节那条已经覆盖了默认表；这一条覆盖设置里填任意路径那一支
     * （那一支的 rendered 是外部输入拼出来的，逐字节钉不住）。
     */
    @Test
    fun noCandidateIsEverExecutedEvenForAUserSuppliedPath() {
        val cmd = DaemonLocator.probeCommand(DaemonLocator.candidates("/opt/ccm/ccm"))
        assertEquals(
            "命令里只许有 `[ -x … ]` / `command -v --` / `printf` / `true` 四种成分",
            "[ -x '/opt/ccm/ccm' ] && printf 'ATERM_LOC_HIT 0 %s\\n' '/opt/ccm/ccm'; true && printf '\\n${DaemonCommands.QUERY_OK_MARKER}\\n'",
            cmd,
        )
        // 裸跑候选的两种最常见写法，一个都不许出现
        assertFalse("不许 `|| 候选`（顺手跑一下）", cmd.contains("|| '/opt/ccm/ccm'"))
        assertFalse("不许 `候选 --version`", cmd.contains("'/opt/ccm/ccm' --version"))
    }

    // ── 远端存在性：四种远端现实 ────────────────────────────────────────────────

    /**
     * 确定的那条路径在前，有歧义的 PATH 名字在后。
     *
     * `~/.cc-monitor/bin/ccm` 是真机上量到过的落点；PATH 上的 `ccm` 可能是 `~/.local/bin/` 里那份旧 bash 启动器。
     * 两者都在时咬了后者 ⇒ 咬到的可能根本不是后端。
     *
     * 这一档只能脚本化：真机上造不出「同时存在且是两个不同东西」的盘面。
     */
    @Test
    fun theMeasuredLandingWinsOverTheAmbiguousPathName() =
        runTest {
            val out =
                DaemonLocator.locate(
                    fakeRemote(setOf("$home/.cc-monitor/bin/ccm", "ccm")),
                    configured = DaemonLocator.UNSET_PLACEHOLDER,
                    timeoutMs = 1_000,
                )
            assertEquals(
                "两条都在时必须先咬那条量过的路径——PATH 上那个名字可能是旧 bash 启动器",
                DaemonLocator.Outcome.Found(
                    "$home/.cc-monitor/bin/ccm",
                    DaemonLocator.candidates(null)[0],
                    DaemonLocator.candidates(null),
                ),
                out,
            )
        }

    /** 只有那条真落点（真机上的常见盘面）⇒ 什么都不用做。 */
    @Test
    fun theRealLandingAloneIsFound() =
        runTest {
            val out = DaemonLocator.locate(fakeRemote(setOf("$home/.cc-monitor/bin/ccm")), DaemonLocator.UNSET_PLACEHOLDER, 1_000)
            assertEquals("$home/.cc-monitor/bin/ccm", (out as DaemonLocator.Outcome.Found).path)
        }

    /**
     * 不在候选表上的名字即使装着也不被找到。
     *
     * [theUnsupportedBackendNamesAreNotInTheCandidateTable] 只看命令串，这条看行为。
     * 叫那些名字的二进制不认 `--` 分派门，找到了也只会 `unknown argument` ＋ exit 2；
     * 与其显示「找到了却连不上」，不如如实说「没找到」并逐条列出找过哪里。
     */
    @Test
    fun aBinaryUnderAnUnsupportedNameIsNotFoundEvenIfInstalled() =
        runTest {
            val stillThere =
                setOf(
                    "$home/.cc-monitor/bin/cc-monitor-remote",
                    "$home/.cc-monitor/bin/cc-monitor-backend",
                    "cc-monitor-backend",
                )
            val out = DaemonLocator.locate(fakeRemote(stillThere), DaemonLocator.UNSET_PLACEHOLDER, 1_000)
            assertTrue("那个名字不在候选表上 ⇒ 必须落 NotFound，不许「找到」一个连不上的东西。实际：$out", out is DaemonLocator.Outcome.NotFound)
        }

    /** PATH 上的名字也认（有歧义的那条兜底）。 */
    @Test
    fun aNameOnThePathIsFound() =
        runTest {
            val out = DaemonLocator.locate(fakeRemote(setOf("ccm")), DaemonLocator.UNSET_PLACEHOLDER, 1_000)
            assertEquals("ccm", (out as DaemonLocator.Outcome.Found).path)
        }

    /**
     * 都没有的时候，逐条说出试过哪几个。
     *
     * 只说一句「找不到」，既不知道找过哪里、也不知道该往哪里放，一次可预测的迁移会被说成配置问题。
     */
    @Test
    fun nothingFoundListsEveryPlaceWeTried() =
        runTest {
            val out = DaemonLocator.locate(fakeRemote(emptySet()), DaemonLocator.UNSET_PLACEHOLDER, 1_000)
            val notFound = out as DaemonLocator.Outcome.NotFound
            assertEquals(2, notFound.tried.size)
            val msg = DaemonLocator.notFoundMessage(notFound.tried)
            for (c in notFound.tried) {
                assertTrue("「找不到」那句话必须逐条列出试过的地方，缺了：${c.where}\n实际：$msg", msg.contains(c.where))
            }
            assertTrue("还要说清楚下一步动作", msg.contains("去设置里填"))
        }

    /**
     * 「问不出来」与「没装」必须分开。
     *
     * 把通道故障说成「没装」，会让人去改一个本来就对的配置。
     * 这里靠 [DaemonCommands.QUERY_OK_MARKER] 那个肯定的成功证据分：
     * 没见到标记 ⇒ 命令根本没跑完。
     */
    @Test
    fun aChannelThatSaysNothingIsNotTheSameAsNotInstalled() =
        runTest {
            val dead = RemoteCommandChannel { flowOf(ByteArray(0)) }
            val out = DaemonLocator.locate(dead, DaemonLocator.UNSET_PLACEHOLDER, 1_000)
            assertTrue("空回答 ⇒ 通道失败，不是「没装」。实际：$out", out is DaemonLocator.Outcome.ChannelFailed)
        }

    /** 空候选表 = 悄悄放弃定位，结构上不许存在。 */
    @Test
    fun anEmptyCandidateTableIsRefusedLoudly() {
        val e = runCatching { DaemonCommands.locate(emptyList()) }.exceptionOrNull()
        assertTrue("空候选表必须响亮地拒绝，实际：$e", e is IllegalArgumentException)
    }

    /** 三个失败终态的第一句，与另外两句不许是同一条文案（另两句在 `DaemonProbeTest`）。 */
    @Test
    fun theNotFoundLineIsItsOwnWording() {
        val a = DaemonLocator.notFoundMessage(DaemonLocator.candidates(null))
        val b = DaemonProbe.tooOldMessage("/x/y", "p1r-event-liveness", WireFeature.CONTROL_PLANE)
        assertNotEquals("「没找到」与「太旧」不许共用同一条文案常量", a, b)
    }

    private companion object {
        /** `[ -x <路径> ] && printf 'ATERM_LOC_HIT <序号> %s\n' <路径>` */
        val PATH_CLAUSE = Regex("""^\[ -x (.+) ] && printf 'ATERM_LOC_HIT (\d+) %s\\n' (.+)$""")

        /** `command -v -- <名字> >/dev/null 2>&1 && printf 'ATERM_LOC_HIT <序号> %s\n' <名字>` */
        val NAME_CLAUSE = Regex("""^command -v -- (.+) >/dev/null 2>&1 && printf 'ATERM_LOC_HIT (\d+) %s\\n' (.+)$""")

        /** 收尾那句 `true && printf '\nATERM_Q_OK\n'`。 */
        val OK_CLAUSE = Regex("""^true && printf '\\n${DaemonCommands.QUERY_OK_MARKER}\\n'$""")

        /**
         * 模板逐字节，手写在这里，不从生产代码拼：拼出来的话生产改模板时它跟着改，判据恒绿。
         */
        const val EXPECTED_LOCATE_COMMAND: String =
            "[ -x \"\$HOME\"/.cc-monitor/bin/ccm ] && printf 'ATERM_LOC_HIT 0 %s\\n' \"\$HOME\"/.cc-monitor/bin/ccm; " +
                "command -v -- 'ccm' >/dev/null 2>&1 && printf 'ATERM_LOC_HIT 1 %s\\n' 'ccm'; " +
                "true && printf '\\nATERM_Q_OK\\n'"
    }
}
