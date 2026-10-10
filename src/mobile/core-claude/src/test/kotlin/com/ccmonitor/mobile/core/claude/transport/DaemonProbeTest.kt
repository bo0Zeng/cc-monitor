package com.ccmonitor.mobile.core.claude.transport

import kotlinx.coroutines.flow.flow
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * [DaemonProbe]：能力探测与版本前置。
 *
 * 每条断言背后都有一个具体的失败形态，最常见的是：发了 daemon 不认识的 flag ⇒ 落进一次性查询模式
 * ⇒ exit 2 ⇒ 永远不发 hello ⇒ 无限重连、日志里什么都没有。
 */
class DaemonProbeTest {
    private val path = "/home/u/.cc-monitor/bin/cc-monitor-remote"

    /** 从真录制的 fixture 里取一版 hello 的原始行。 */
    private fun realHelloLine(ver: String): String {
        val f =
            File("../bridge/vectors/daemon-wire.ndjson").takeIf { it.isFile }
                ?: File("bridge/vectors/daemon-wire.ndjson")
        assertTrue("daemon wire golden 缺失：${f.absolutePath}", f.isFile)
        return f
            .readLines()
            .asSequence()
            .filter { it.isNotBlank() }
            .mapNotNull { line ->
                // 只要 {"ver":…,"cmd":"(流模式首帧)","stream":"stdout","raw":"…"} 那种行
                if (!line.contains("\"ver\":\"$ver\"") || !line.contains("流模式首帧")) {
                    null
                } else {
                    Regex("\"raw\":\"(.*)\"\\}$")
                        .find(line)
                        ?.groupValues
                        ?.get(1)
                        ?.replace("\\\"", "\"")
                        ?.replace("\\\\", "\\")
                }
            }.firstOrNull() ?: error("fixture 里找不到 $ver 的流模式首帧")
    }

    private fun chanOf(vararg lines: String) =
        com.ccmonitor.mobile.core.remote.RemoteCommandChannel {
            flowOf(lines.joinToString("\n").plus("\n").toByteArray())
        }

    /**
     * 探测不带任何能力 flag：此刻还不知道 daemon 能力，发多余 flag 会落进一次性查询模式
     * ⇒ exit 2 ⇒ 永远不发 hello ⇒ 无限重连。
     *
     * 但裸路径的含义是「起一个 claude」，所以不带能力 flag 的正确形是 `'<path>' -- --stream`：
     * 门与 `--stream` 始终要发。断言的是 [DaemonProbe] 实际发给 channel 的命令串。
     */
    @Test
    fun probeSendsNoCapabilityFlagsButStillGoesThroughTheArgvGate() =
        runTest {
            var seen: String? = null
            val chan =
                com.ccmonitor.mobile.core.remote.RemoteCommandChannel { cmd ->
                    seen = cmd
                    flowOf((realHelloLine("p1t") + "\n").toByteArray())
                }
            DaemonProbe(chan, path).probe(timeoutMs = 5_000)
            assertEquals("探测必须过 `--` 那道门并显式发 --stream，但一个能力 flag 都不带", "'$path' -- --stream", seen)
        }

    /** `p1t`：能拿到 capabilities 与 emits。 */
    @Test
    fun probeOnADaemonWithCapabilitiesYieldsThem() =
        runTest {
            val r = DaemonProbe(chanOf(realHelloLine("p1t")), path).probe(5_000)
            assertTrue("应探测成功：${r.failure}", r.ok)
            assertEquals(listOf("bg", "tail-only"), r.capabilities)
            assertTrue("emits 应有 8 种", r.emits.size == 8)
        }

    /**
     * `hello` 缺 `capabilities` ⇒ 空集，而不是「探测失败」也不是默认值。
     *
     * 这是正常情况不是错误，判成失败会让人以为坏了。
     */
    @Test
    fun probeOnADaemonWithoutCapabilitiesSucceedsWithAnEmptySet() =
        runTest {
            val r = DaemonProbe(chanOf(realHelloLine("p1h")), path).probe(5_000)
            assertTrue("旧 daemon 也算探测成功（它只是能力少）：${r.failure}", r.ok)
            assertEquals("缺 capabilities ⇒ 空集", emptyList<String>(), r.capabilities)
            assertEquals("缺 emits ⇒ 空集", emptyList<String>(), r.emits)
        }

    /**
     * 能力协商的闭环：探到什么就只发什么。
     *
     * 没声明能力的 daemon 上，即使想要 bg/tail-only，也一个能力 flag 都不能发。
     * 「不发 flag」≠「裸路径」：`-- --stream` 那两个词与能力协商无关，恒发。
     */
    @Test
    fun streamCommandOnlySendsFlagsThePeerDeclared() =
        runTest {
            val (newCmd, _) =
                DaemonProbe(chanOf(realHelloLine("p1t")), path)
                    .streamCommand(5_000, want = listOf("bg", "tail-only"))
            assertEquals("'$path' -- --stream --with-bg --tail-only", newCmd)

            val (oldCmd, r) =
                DaemonProbe(chanOf(realHelloLine("p1h")), path)
                    .streamCommand(5_000, want = listOf("bg", "tail-only"))
            assertEquals("旧 daemon：想要也不能发能力 flag，但门与 --stream 照发", "'$path' -- --stream", oldCmd)
            assertTrue(r.ok)
            assertFalse("绝不能出现 --with-bg", oldCmd!!.contains("--with-bg"))
        }

    /**
     * 流结束却没有 hello：必须响亮失败并带可诊断信息。
     *
     * 这正是「命令里混进了不认识的参数 → 落进查询模式 → exit 2 → stdout 空」的表现。
     * 三种根因（未部署 / 路径错 / 参数错）表现完全一样，所以错误信息必须把三种都点出来，
     * 否则只会看到「连接失败」然后无从查起。
     */
    @Test
    fun streamEndingWithoutHelloFailsLoudlyWithDiagnostics() =
        runTest {
            val r = DaemonProbe(chanOf(""), path).probe(5_000)
            assertFalse(r.ok)
            assertNotNull(r.failure)
            val msg = r.failure!!
            assertTrue("错误信息要点出「路径不对」这种可能：$msg", msg.contains("路径"))
            assertTrue("错误信息要点出「未部署」这种可能：$msg", msg.contains("未部署"))
            assertTrue("错误信息要带上实际命令，否则无从查起：$msg", msg.contains(path))
        }

    /** 超时同样要响亮，且带上等了多久与实际命令。 */
    @Test
    fun timeoutFailsLoudlyToo() =
        runTest {
            val hang =
                com.ccmonitor.mobile.core.remote.RemoteCommandChannel {
                    flow { kotlinx.coroutines.delay(1_000_000) }
                }
            val r = DaemonProbe(hang, path).probe(timeoutMs = 50)
            assertFalse(r.ok)
            assertTrue("要说明是超时：${r.failure}", r.failure!!.contains("超时"))
            assertTrue("要带上命令：${r.failure}", r.failure!!.contains(path))
        }

    /** 探测失败时 `streamCommand` 必须返回 null，绝不能拿失败结果去拼命令。 */
    @Test
    fun failedProbeYieldsNullCommandNotABareGuess() =
        runTest {
            val (cmd, r) = DaemonProbe(chanOf(""), path).streamCommand(5_000, listOf("bg"))
            assertNull("探测失败就不该给出命令——那会掩盖故障", cmd)
            assertFalse(r.ok)
        }

    // ══════════════ 版本前置：接控制面之前先知道 daemon 是哪一版 ══════════════

    /**
     * `BUILD_ID` 的序：四格都要覆盖。
     *
     * 只验「明显的比较关系」的话全绿也证明不了这条要防的东西（见 [passengerTheObviousComparisons]）。
     */
    @Test
    fun theBuildIdOrderCoversAllFourCells() {
        // ① 同 major，比字母段
        assertEquals(BuildIdVerdict.TOO_OLD, BuildIds.atLeast("p1r-event-liveness", "p1t-removal-cause"))
        // ② 跨 major：字母段单独看是 z > a，但 major 说了算
        assertEquals(BuildIdVerdict.NEW_ENOUGH, BuildIds.atLeast("p2a-whatever", "p1z-whatever"))
        // ③ build_id 缺席
        assertEquals(BuildIdVerdict.UNRECOGNIZED, BuildIds.atLeast(null, "p1t-removal-cause"))
        // ④ 格式完全不认识
        for (weird in listOf("dev", "2026.09.04", "", "   ", "P1T-REMOVAL-CAUSE", "1t")) {
            assertEquals("认不出的版本号「$weird」必须落进 UNRECOGNIZED", BuildIdVerdict.UNRECOGNIZED, BuildIds.atLeast(weird, "p1t-removal-cause"))
        }
        // 字母段：先长度后字典序，纯字典序会把 p1aa 排到 p1b 之前
        assertEquals(BuildIdVerdict.NEW_ENOUGH, BuildIds.atLeast("p1aa-x", "p1z-x"))
        assertEquals(BuildIdVerdict.NEW_ENOUGH, BuildIds.atLeast("p1t-removal-cause", "p1t-removal-cause"))
    }

    /**
     * 乘客对照：两条「明显在验比较器」的断言。
     *
     * 它们在「fail-safe 方向翻过来（认不出 ⇒ 当成够新）」下都是绿的：
     * 把明显的比较关系验了几条，不等于守住了这条判据真正要防的东西。
     */
    @Test
    fun passengerTheObviousComparisons() {
        assertEquals(BuildIdVerdict.TOO_OLD, BuildIds.atLeast("p1r-event-liveness", "p1t-removal-cause"))
        assertEquals(BuildIdVerdict.NEW_ENOUGH, BuildIds.atLeast("p2a-whatever", "p1z-whatever"))
    }

    /**
     * fail-safe 方向：认不出 / 没说，一律并进「不够新」。
     *
     * 反过来的话：对一台未知版本的 daemon 发控制命令 ⇒ 它不认识的参数 ⇒ 落进一次性查询模式
     * ⇒ exit 2 ⇒ 永不发 hello ⇒ 无限重连且日志里什么都没有。
     */
    @Test
    fun anUnreadableBuildIdIsNeverTreatedAsNewEnough() {
        for (f in WireFeature.entries) {
            for (weird in listOf(null, "", "dev", "2026.09.04", "nightly-main")) {
                val r = DaemonProbe.Result(JsonlFrame.Hello(buildId = weird))
                assertFalse("版本号「$weird」认不出，却对 ${f.name} 放行了——fail-safe 方向反了", r.supports(f))
            }
        }
    }

    /** 门槛表每一行都要带住址，否则它就是第二份真相。 */
    @Test
    fun everyThresholdRowCarriesItsAddress() {
        for (f in WireFeature.entries) {
            assertTrue("${f.name} 的门槛没写住址——对账时没人知道该去核哪一条", f.address.length > 10)
            assertNotNull("${f.name} 的门槛我们自己都认不出", BuildIds.rank(f.minBuildId))
            assertTrue("${f.name} 没写人话说明，那句「太旧」会说不清是什么功能", f.what.isNotEmpty())
        }
    }

    // ── 三档失败，三句不同的话 ────────────────────────────────────────────────

    /**
     * ①找不到 / ②太旧 / ③没说话：两两不相等。
     *
     * 压成一句会让人按错的方向去修：「没装」要去装，「太旧」要去升级（升级它的不是这个应用，
     * 我们不动共用落点），「装了但不说话」是另一码事。
     */
    @Test
    fun theThreeFailureLinesAreThreeDifferentSentences() {
        val notInstalled = DaemonProbe.Readiness.NotInstalled(DaemonLocator.candidates(null)).message
        val tooOld = DaemonProbe.Readiness.TooOld("/home/u/.cc-monitor/bin/cc-monitor-remote", "p1r-event-liveness", WireFeature.CONTROL_PLANE).message
        val speechless = DaemonProbe.Readiness.Speechless("/home/u/.cc-monitor/bin/cc-monitor-remote", "daemon 流结束但从未发出 hello。…").message
        assertNotEquals("①找不到 与 ②太旧 不许共用同一条文案", notInstalled, tooOld)
        assertNotEquals("①找不到 与 ③没说话 不许共用同一条文案", notInstalled, speechless)
        assertNotEquals("②太旧 与 ③没说话 不许共用同一条文案", tooOld, speechless)
    }

    /**
     * 乘客对照：三条「各自出现了正确的话」的写法。
     *
     * 把 ② 的文案换成与 ① 同一条常量之后，这三条里仍会有两条是绿的
     * （①命中它自己；②抄过来的那句里含着①的子串）。
     */
    @Test
    fun passengerEachLineContainsItsOwnWords() {
        val notInstalled = DaemonProbe.Readiness.NotInstalled(DaemonLocator.candidates(null)).message
        val tooOld = DaemonProbe.Readiness.TooOld("/x", "p1r-event-liveness", WireFeature.CONTROL_PLANE).message
        val speechless = DaemonProbe.Readiness.Speechless("/x", "daemon 流结束但从未发出 hello").message
        assertTrue("① 要说「没找到」", notInstalled.contains("没找到"))
        assertTrue("② 要说「找过这」", notInstalled.contains("找过这"))
        assertTrue("③ 要说 hello", speechless.contains("hello"))
    }

    /** ② 那句必须带上：实际版本、要求版本、装在哪，以及「升级它的不是这个应用」。 */
    @Test
    fun theTooOldLineSaysWhatItIsWhatIsNeededAndWhoUpgradesIt() {
        val msg = DaemonProbe.Readiness.TooOld("/home/u/.cc-monitor/bin/cc-monitor-remote", "p1r-event-liveness", WireFeature.REMOVAL_CAUSE).message
        assertTrue("要说实际是哪一版：$msg", msg.contains("p1r-event-liveness"))
        assertTrue("要说要求哪一版：$msg", msg.contains("p1t-removal-cause"))
        assertTrue("要说它装在哪：$msg", msg.contains("/home/u/.cc-monitor/bin/cc-monitor-remote"))
        assertTrue("不许出现我们做不到的动作，只能如实说这句：$msg", msg.contains("升级它的不是这个应用"))
    }

    // ── ready()：三个失败终态跑通 ───────────────────────────────────────────────────

    /** ① 候选表逐条问过，一个都不在。 */
    @Test
    fun readyReportsNotInstalledWhenNoCandidateExists() =
        runTest {
            val r = DaemonProbe(fakeHost(LOCATE_NOTHING), DaemonLocator.UNSET_PLACEHOLDER).ready(WireFeature.CONTROL_PLANE, 1_000)
            assertTrue("实际：$r", r is DaemonProbe.Readiness.NotInstalled)
            // 「试过哪几个」里出现的就是候选表里的 `ccm`
            assertTrue("要逐条列出试过哪几个：${r.message}", r.message!!.contains(DaemonLocator.BACKEND_NAME))
        }

    /**
     * ② 找到了但太旧（`p1r-event-liveness`）。
     *
     * 这一档好造，恰恰因此藏着恒绿陷阱：换到新 daemon 之后「②还成不成立」在新机器上造不出来。
     * 那一格由 [readyIsSatisfiedByTheBackendWeInstalledOurselves] 用 `p2d-relay` 的真 hello 守着。
     */
    @Test
    fun readyReportsTooOldOnP1r() =
        runTest {
            val host = fakeHost(locateHit(1, "/home/u/.cc-monitor/bin/cc-monitor-remote"), mapOf("/home/u/.cc-monitor/bin/cc-monitor-remote" to HELLO_P1R))
            val r = DaemonProbe(host, DaemonLocator.UNSET_PLACEHOLDER).ready(WireFeature.CONTROL_PLANE, 1_000)
            assertEquals(
                DaemonProbe.Readiness.TooOld("/home/u/.cc-monitor/bin/cc-monitor-remote", "p1r-event-liveness", WireFeature.CONTROL_PLANE),
                r,
            )
        }

    /**
     * 「够新的 daemon 上不出现②」那一格：用装在 `~/.aterm/bin/` 的那份后端的真 hello 守。
     *
     * 联调用的后端装在我们自己的目录，不写 `~/.cc-monitor/bin/`。这行 hello 录自隔离环境，只脱敏了 `claude_dir`。
     */
    @Test
    fun readyIsSatisfiedByTheBackendWeInstalledOurselves() =
        runTest {
            val host = fakeHost(locateHit(0, "/home/u/.aterm/bin/cc-monitor-backend"), mapOf("/home/u/.aterm/bin/cc-monitor-backend" to HELLO_P2D))
            val r = DaemonProbe(host, DaemonLocator.UNSET_PLACEHOLDER).ready(WireFeature.CONTROL_PLANE, 1_000)
            assertTrue("p2d-relay 应当够新，实际：$r", r is DaemonProbe.Readiness.Ready)
            assertEquals("p2d-relay", (r as DaemonProbe.Readiness.Ready).hello.buildId)
            assertNull("够新的时候屏上不该有话要说", r.message)
        }

    /** ③ 找到了但它不说话：给出那句三选一诊断。 */
    @Test
    fun readyReportsSpeechlessWhenFoundButSilent() =
        runTest {
            val host = fakeHost(locateHit(1, "/home/u/.cc-monitor/bin/cc-monitor-remote"), mapOf("/home/u/.cc-monitor/bin/cc-monitor-remote" to ""))
            val r = DaemonProbe(host, DaemonLocator.UNSET_PLACEHOLDER).ready(WireFeature.CONTROL_PLANE, 1_000)
            assertTrue("实际：$r", r is DaemonProbe.Readiness.Speechless)
            assertTrue("要保留那句三选一诊断：${r.message}", r.message!!.contains("未部署"))
        }

    /**
     * golden 里的真样本 `p1h-bg-badge` ⇒ 落「太旧」。
     *
     * 它缺的是 `capabilities` / `emits`，`build_id` 在。「缺 `build_id`」那一档没有真样本，由下一条构造的用例守。
     */
    @Test
    fun theRecordedP1hDaemonFallsIntoTooOld() =
        runTest {
            val host = fakeHost(locateHit(1, "/home/u/.cc-monitor/bin/cc-monitor-remote"), mapOf("/home/u/.cc-monitor/bin/cc-monitor-remote" to realHelloLine("p1h")))
            val r = DaemonProbe(host, DaemonLocator.UNSET_PLACEHOLDER).ready(WireFeature.REMOVAL_CAUSE, 1_000)
            assertTrue("实际：$r", r is DaemonProbe.Readiness.TooOld)
            assertEquals("p1h-bg-badge", (r as DaemonProbe.Readiness.TooOld).buildId)
        }

    /**
     * `build_id` 完全缺席 ⇒ 落「太旧」，不是「够新」也不是崩。这一档是构造的，没有真样本。
     */
    @Test
    fun aHelloWithoutBuildIdFallsIntoTooOld() =
        runTest {
            val noBuildId = """{"kind":"hello","v":1,"host_arch":"x86_64","capabilities":["bg"]}"""
            val host = fakeHost(locateHit(1, "/home/u/.cc-monitor/bin/cc-monitor-remote"), mapOf("/home/u/.cc-monitor/bin/cc-monitor-remote" to noBuildId))
            val r = DaemonProbe(host, DaemonLocator.UNSET_PLACEHOLDER).ready(WireFeature.REMOVAL_CAUSE, 1_000)
            assertTrue("实际：$r", r is DaemonProbe.Readiness.TooOld)
            assertNull("它确实没说版本号", (r as DaemonProbe.Readiness.TooOld).buildId)
            assertTrue("屏上那句话要如实说它说不出版本号：${r.message}", r.message.contains("说不出版本号"))
        }

    // ── 生产可达性：定位不是只有测试在引 ────────────────────────────────────────

    /**
     * 生产可达性：没填路径时，`probe()` 会先定位。
     *
     * 生产路径：`AppModule` → `DaemonSessionSource.states()` → `DaemonProbe.streamCommand()` → `probe()` → [DaemonLocator]。
     */
    @Test
    fun probeGoesThroughTheLocatorWhenNobodyFilledThePath() =
        runTest {
            val seen = mutableListOf<String>()
            val host = fakeHost(locateHit(1, "/home/u/.cc-monitor/bin/cc-monitor-remote"), mapOf("/home/u/.cc-monitor/bin/cc-monitor-remote" to HELLO_P1R), seen)
            val r = DaemonProbe(host, DaemonLocator.UNSET_PLACEHOLDER).probe(1_000)
            assertTrue("没填路径时第一条命令必须是定位，实际：${seen.firstOrNull()}", seen.first().contains(DaemonCommands.LOCATE_HIT_MARKER))
            assertTrue("定位过的路径才拿去问 hello", r.ok)
            assertEquals("/home/u/.cc-monitor/bin/cc-monitor-remote", r.resolvedPath)
        }

    /** 设置里填了就只用它，不多跑一次定位（回退会把显式选择变成猜，也会白花一次往返）。 */
    @Test
    fun anExplicitPathSkipsTheLocatorEntirely() =
        runTest {
            val seen = mutableListOf<String>()
            val host = fakeHost(LOCATE_NOTHING, mapOf(path to realHelloLine("p1t")), seen)
            val r = DaemonProbe(host, path).probe(1_000)
            assertEquals("用户填了就不该有定位那一趟", 1, seen.size)
            assertEquals("'$path' -- --stream", seen.single())
            assertEquals(path, r.resolvedPath)
        }

    /**
     * 主流命令必须用定位定下来的那条路径。
     *
     * 拿占位名去拼 ⇒ 送到远端的是 `'cc-monitor-remote'`，而它不在 PATH 上。
     */
    @Test
    fun streamCommandUsesTheLocatedPathNotThePlaceholder() =
        runTest {
            val found = "/home/u/.cc-monitor/bin/cc-monitor-remote"
            val host = fakeHost(locateHit(1, found), mapOf(found to realHelloLine("p1t")))
            val (cmd, _) = DaemonProbe(host, DaemonLocator.UNSET_PLACEHOLDER).streamCommand(1_000, listOf("bg"))
            assertEquals("'$found' -- --stream --with-bg", cmd)
            assertFalse("绝不许把界面上那个占位名送到远端：$cmd", cmd!!.contains("'${DaemonLocator.UNSET_PLACEHOLDER}'"))
        }

    // ── 脚本化的远端：定位一条回答、流模式一条回答 ────────────────────────────────────

    /** 定位命中一行 + 成功标记。字面拼的，不从生产代码取形状。 */
    private fun locateHit(
        index: Int,
        path: String,
    ) = "${DaemonCommands.LOCATE_HIT_MARKER} $index $path\n\n${DaemonCommands.QUERY_OK_MARKER}\n"

    /**
     * 一台脚本化的远端主机：定位命令回 [locateStdout]，流模式命令按路径回对应的 hello。
     *
     * 判定谁是谁只看命令里有没有 [DaemonCommands.LOCATE_HIT_MARKER] —— 那是定位命令独有的。
     */
    private fun fakeHost(
        locateStdout: String,
        helloByPath: Map<String, String> = emptyMap(),
        seen: MutableList<String> = mutableListOf(),
    ) = com.ccmonitor.mobile.core.remote.RemoteCommandChannel { cmd ->
        seen += cmd
        if (cmd.contains(DaemonCommands.LOCATE_HIT_MARKER)) {
            flowOf(locateStdout.toByteArray())
        } else {
            // 流模式命令是 `'<路径>' -- --stream [flag…]` ⇒ 取第一对单引号之间那一截当路径
            // （这里用的路径里没有嵌套引号）。
            val p = cmd.substringAfter('\'').substringBefore('\'')
            flowOf((helloByPath[p]?.takeIf { it.isNotEmpty() }?.plus("\n") ?: "").toByteArray())
        }
    }

    private companion object {
        /** 定位跑完了、一条都没命中。 */
        const val LOCATE_NOTHING: String = "\nATERM_Q_OK\n"

        /**
         * `p1r-event-liveness` 的 hello：手抄的，不是录的。
         *
         * 素材来自那份二进制的连续 `.rodata`（只读，没有执行它）：
         * ```
         * p1r-event-livenessbgtail-onlysession_addedsession_statussession_removedtmux_sessionstmux_session_closed
         * ```
         * 注意：手抄的跟着对 wire 的理解走，不跟着 daemon 走。
         * - `build_id` / `capabilities` 是逐字节读到的，这里的判据只用这两样。
         * - `emits` 只填直接读到的那五个。`.rodata` 会把短 token 去重（`line` / `overflow` / `turn_end`
         *   在别处也出现），所以这行不许被引作 p1r 的 emits 的证据。
         */
        const val HELLO_P1R: String =
            """{"kind":"hello","v":1,"build_id":"p1r-event-liveness","host_arch":"x86_64","claude_dir":"/home/USER/REDACTED","capabilities":["bg","tail-only"],""" +
                """"emits":["session_added","session_status","session_removed","tmux_sessions","tmux_session_closed"]}"""

        /**
         * `p2d-relay` 的 hello：真录的。
         *
         * 来源是装在 `~/.aterm/bin/` 的那份后端，在隔离环境（临时 `HOME`）里跑一次拿的第一帧；
         * 只脱敏了 `claude_dir`，其余逐字节原样。
         *
         * 它有第四条字段 `commands`（`launch` / `resolve` / `ping` / `cancel` / `kill` / `bus-*`），
         * 是控制面存在的直接证据，`WireFeature.CONTROL_PLANE` 的门槛（`p2d-relay`）由此而来。
         * 注意：这说明说了算的有两处，门槛表（buildId，序是我们推的）与 `commands`（daemon 自己说的，更硬）；
         * 接控制面应当先问 `commands`。
         */
        const val HELLO_P2D: String =
            """{"kind":"hello","v":1,"build_id":"p2d-relay","host_arch":"x86_64","claude_dir":"/home/USER/REDACTED/.claude",""" +
                """"capabilities":["bg","tail-only"],""" +
                """"emits":["line","session_added","session_status","session_removed","overflow","turn_end","tmux_sessions","tmux_session_closed"],""" +
                """"commands":["bus-kill","bus-list","bus-send","cancel","kill","launch","ping","resolve"]}"""
    }
}
