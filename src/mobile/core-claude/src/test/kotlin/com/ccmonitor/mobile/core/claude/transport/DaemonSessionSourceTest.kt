package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * [DaemonSessionSource]：[DaemonTransport] 的消费方，把帧折成会话表。
 */
class DaemonSessionSourceTest {
    private val path = "/home/u/.cc-monitor/bin/cc-monitor-remote"

    private fun realHelloLine(ver: String): String {
        val f =
            File("../bridge/vectors/daemon-wire.ndjson").takeIf { it.isFile }
                ?: File("bridge/vectors/daemon-wire.ndjson")
        return f
            .readLines()
            .asSequence()
            .filter { it.contains("\"ver\":\"$ver\"") && it.contains("流模式首帧") }
            .mapNotNull {
                Regex("\"raw\":\"(.*)\"\\}$")
                    .find(it)
                    ?.groupValues
                    ?.get(1)
                    ?.replace("\\\"", "\"")
                    ?.replace("\\\\", "\\")
            }.firstOrNull() ?: error("fixture 里找不到 $ver 的流模式首帧")
    }

    /**
     * 记录每一次 exec 的命令串的假 channel。
     *
     * 忽略命令参数的假 channel 会让「探到什么发什么」这条接缝零覆盖：就算把裸命令甚至空串喂给
     * `DaemonTransport`，测试也全部照绿。
     */
    private class RecordingChannel(
        private val lines: List<String>,
    ) : RemoteCommandChannel {
        val commands = mutableListOf<String>()

        override fun exec(command: String) =
            flowOf((lines.joinToString("\n") + "\n").toByteArray()).also { commands += command }
    }

    /** 探测与主流用同一份行；真实场景是两次 exec，测试里简化为同一批。 */
    private fun chan(vararg lines: String) = RecordingChannel(lines.toList())

    @Test
    fun buildsSessionTableFromAddedStatusRemoved() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","path":"/p/s1.jsonl","cwd":"/proj","name":"活儿","status":"running"}""",
                        """{"kind":"session_added","sid":"s2","path":"/p/s2.jsonl","cwd":"/other","status":"idle"}""",
                        """{"kind":"session_status","sid":"s1","status":"idle","waiting_for":"user"}""",
                        """{"kind":"session_removed","sid":"s2"}""",
                    ),
                    path,
                ).states().toList()

            // 流正常结束会补发一个 fatal（daemon 退出），所以取倒数第二个看会话表
            val last = states[states.size - 2]
            assertNull("正常帧处理阶段不该有致命错误：${last.fatal}", last.fatal)
            assertEquals("s2 已被移除，只剩 s1", setOf("s1"), last.sessions.keys)
            val s1 = last.sessions.getValue("s1")
            assertEquals("状态应被 session_status 更新", "idle", s1.status)
            assertEquals("user", s1.waitingFor)
            assertEquals("added 里的字段不该被 status 帧抹掉", "/proj", s1.cwd)
            assertEquals("活儿", s1.name)
        }

    // ---- wire 层解析了、要进到 Session 的两个字段 ----------------------

    /**
     * `attachable` 的语义不是「活着吗」，是「attach 进去有没有意义」。
     *
     * 后端定义：`false` = 别给 attach / ↗ /「杀死空 tmux」这几个动作；省略 = `true`。
     * 它必须从 wire 一路进到 `Session`，否则总览面不可能正确显示它。
     *
     * 注意：`daemon-wire.ndjson` 里一条 `attachable` 都没有（它录自 `p1t-removal-cause`，早于该字段，
     * 最小 `BUILD_ID = p1v-attachable`），所以这里的 `session_added` 是手写的，hello 仍用 fixture 里那条
     * （`attachable` 是 additive 字段，与 hello 无关）。
     */
    @Test
    fun attachableReachesTheSessionAndDefaultsToTrueWhenOmitted() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","attachable":false}""",
                        """{"kind":"session_added","sid":"s2"}""",
                        """{"kind":"session_added","sid":"s3","attachable":"false"}""",
                    ),
                    path,
                ).states().toList()
            val last = states[states.size - 2]
            assertEquals("前提：三条都要建出来", setOf("s1", "s2", "s3"), last.sessions.keys)
            assertFalse("显式 false ⇒ 不给 attach 类动作", last.sessions.getValue("s1").attachable)
            assertTrue("省略 = true（不带这一格的会话照常可 attach）", last.sessions.getValue("s2").attachable)
            // 只认真布尔：字符串 "false" 不是布尔 ⇒ 落回缺席 ⇒ true（后端写的是真布尔）
            assertTrue("字符串不当布尔用", last.sessions.getValue("s3").attachable)
        }

    // ── `activity` 与 `background` ──────────

    /**
     * 活动档必须进会话表：不进的话总览面的红绿灯整片中性，而且不报错。
     *
     * 后端的 `SessionAdded` 与 `SessionStatus` 可能只带 `activity`（闭集三值 `working`/`needs_you`/`idle`），
     * 不带 `status`。这里只守「传输层真的把这一格端上来了」；灯读哪一格由总览面自己守。
     */
    @Test
    fun theActivityReachesTheSessionTable() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        // session_added：没有 status / session_kind，有 activity + background
                        """{"kind":"session_added","sid":"s1","cwd":"/a","activity":"needs_you","waiting_for":"permission prompt","background":true}""",
                        """{"kind":"session_added","sid":"s2","cwd":"/b","activity":"working"}""",
                        // session_status：同样只有 activity
                        """{"kind":"session_status","sid":"s2","activity":"idle"}""",
                    ),
                    path,
                ).states().toList()
            val last = states[states.size - 2]
            val s1 = last.sessions.getValue("s1")
            assertEquals("宣告时的活动档要进表", "needs_you", s1.activity)
            assertEquals("等什么那一格照旧", "permission prompt", s1.waitingFor)
            assertTrue("后台会话那一格要进表", s1.background)
            assertNull("不发 status ⇒ 这一格就是空的，不许编", s1.status)
            val s2 = last.sessions.getValue("s2")
            assertEquals("状态帧带来的活动档要盖掉宣告时那一个", "idle", s2.activity)
            assertFalse("缺席 ≡ false ≡ 交互会话", s2.background)
        }

    /**
     * 活动档缺席即清空，不跟 `status` 那条「缺席保留旧值」。
     *
     * 后端那一格「说不清 ⇒ 不上线」⇒ 缺席的意思是「说不好」。保留上一次的档 ⇒ 灯停在过去，
     * 而且现在可能已经不对；清成 null ⇒ 灯变中性，与 `SessionLight.Unknown`「不点误报的灯」一致。
     *
     * 写成 `activity = fr.activity ?: prev.activity` 时这条会红；那是最顺手的写法
     * （旁边 `status` 与 `livenessConfidence` 两格就是那么写的）。
     */
    @Test
    fun anAbsentActivityIsClearedInsteadOfKeepingAStaleOne() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","cwd":"/a","activity":"working"}""",
                        // 对端这一拍自己也说不清 ⇒ 不上线这一格
                        """{"kind":"session_status","sid":"s1","liveness_confidence":"authoritative"}""",
                    ),
                    path,
                ).states().toList()
            val s1 = states[states.size - 2].sessions.getValue("s1")
            assertNull("对端说不清 ⇒ 我们也说不好，不许留着上一个档", s1.activity)
            assertEquals("前提：这一帧确实生效了（别的格还在动）", "authoritative", s1.livenessConfidence)
            assertEquals("只清这一格，不许动这条会话本身", "/a", s1.cwd)
        }

    /**
     * `background` 只在 `session_added` 上（后端 `SessionStatus` 不带它）⇒ 状态帧到达时必须保住它。
     * 形与 [aStatusFrameMustNotResetAttachable] 同。
     */
    @Test
    fun aStatusFrameMustNotResetBackground() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","cwd":"/proj","background":true}""",
                        """{"kind":"session_status","sid":"s1","activity":"working"}""",
                    ),
                    path,
                ).states().toList()
            val s1 = states[states.size - 2].sessions.getValue("s1")
            assertEquals("前提：状态帧确实生效了", "working", s1.activity)
            assertTrue("状态帧不许把 background 冲回默认值", s1.background)
            assertEquals("added 的其他字段也不该被抹", "/proj", s1.cwd)
        }

    /**
     * 丢了 `session_status` ⇒ 活动档也要一起作废。
     *
     * `session_status` 上可能只有 `activity`，只清 `status` ＋ `waitingFor` 的作废就等于一格都没清
     * ⇒ 丢帧之后屏上会拿一个过期的 `needs_you` 继续说「在等」。后端把 `session_status` 判为 unrecoverable：
     * 丢了永远补不回来，这正是 `invalidateLostStatus` 存在的理由。
     *
     * 作废时不清 `activity` 的话这条红；[aLostStatusFrameVoidsThatConversationsWaitingState] 只喂带 `status` 的形，抓不到。
     */
    @Test
    fun aLostStatusFrameAlsoVoidsTheActivityNotJustTheStatus() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","cwd":"/a","activity":"needs_you","waiting_for":"permission prompt"}""",
                        """{"kind":"session_added","sid":"s2","cwd":"/b","activity":"needs_you","waiting_for":"input needed"}""",
                        """{"kind":"overflow","dropped":4,"lost":[{"kind":"session_status","subject":"s1"}]}""",
                    ),
                    path,
                ).states().toList()
            val last = states[states.size - 2]
            val s1 = last.sessions.getValue("s1")
            assertNull("点了名的那条：活动档必须一起作废", s1.activity)
            assertNull("等待态同样作废", s1.waitingFor)
            assertEquals("这条对话本身还在，不许删", "/a", s1.cwd)
            val s2 = last.sessions.getValue("s2")
            assertEquals("没被点名的一条不许跟着作废", "needs_you", s2.activity)
            assertEquals("input needed", s2.waitingFor)
        }

    /**
     * 生产面普查：没有任何生产 `want` 要那个硬 flag。
     *
     * 配对判据在 `DaemonCommandsTest.theHardFlagIsNeverSwallowedByTheCapabilityIntersection`（机制不会被交集吞掉）。
     * 这条守另一半：daemon 流的 `Line` 帧没有生产消费方（`DaemonSessionSource.apply` 对 `Line` 落 `else -> this`；
     * `TurnEndRoute.onFrame` 落 `else -> Unit`；读 `Line.raw` 的那几处走的是 `TailTransport` 与 bridge），
     * 要来的原文没人读，而它按行加一份原文是实打实的流量。
     *
     * 要接 daemon 流的正文时，这条就是拦截点：把 `DaemonTransport.parseLine` KDoc 里那三件一起做掉，
     * 别只动一侧（只动一侧的结果是整面空白且无任何诊断）。
     */
    @Test
    fun noProductionStreamAsksForTheRawFlag() =
        runTest {
            val ch = chan(realHelloLine("p1t"))
            DaemonSessionSource(ch, path).states().toList()
            assertEquals("前提：探测 + 主流两次 exec 都要量到", 2, ch.commands.size)
            val raw = "--with" + "-raw"
            ch.commands.forEach {
                assertFalse("总览那条流不要原文（理由见本条 KDoc）：$it", it.contains(raw))
            }
            assertFalse(
                "前台服务那条流（DaemonTurnEndSource）同样不要 —— 它只吃 turn_end 帧",
                DaemonCommands.WANT_RAW in DaemonTurnEndSource.WANT,
            )
            // 前提自检：这条判据不是因为那个词根本拼不出来才绿的
            assertTrue(
                "前提：机制得真的在（否则本条恒绿）",
                DaemonCommands
                    .stream(path, capabilities = emptyList(), want = listOf(DaemonCommands.WANT_RAW))
                    .contains(raw),
            )
        }

    /**
     * `superseded` 不是「会话死了」，是同一个 pidfile 原地换了 sid（`/branch`、`/clear`）。
     *
     * 后端的 `RemovalCause`：旧 sid 不是死了，是被顶替了，收到它必须直接归档，不要再去查 tmux 快照
     * （那份快照对这个场景恒错）。两种原因走同一条路的话，`/clear` 一下总览面表现得和会话崩了一模一样。
     */
    @Test
    fun aSupersededSessionIsArchivedNotTreatedAsDead() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"old","cwd":"/proj"}""",
                        """{"kind":"session_added","sid":"dead","cwd":"/other"}""",
                        """{"kind":"session_removed","sid":"old","cause":"superseded"}""",
                        """{"kind":"session_removed","sid":"dead"}""",
                    ),
                    path,
                ).states().toList()
            val last = states[states.size - 2]
            assertEquals("两条都要从活动表里下来", emptySet<String>(), last.sessions.keys)
            assertEquals("只有被顶替的进归档，真死的不进", setOf("old"), last.superseded.keys)
            assertEquals("归档要留住原来的 cwd，否则 UI 只剩一个光秃秃的 sid", "/proj", last.superseded.getValue("old").cwd)
            // 乘客读数：这台后端（p1t）说得出 `cause` ⇒ 第三档必须是空的。
            // 把三态改回两态时，这一行与上面两行照样全绿：它们守不住「说不出口」那一格。
            assertEquals("会说 cause 的后端上，第三档不该有东西", emptySet<String>(), last.removedUnknown.keys)
        }

    // ---- `cause` 缺席的诚实降级（三态）--------------------------------

    /**
     * daemon `p1r-event-liveness` 从不发 `cause`（二进制里 `grep -ac superseded` = 0）。
     *
     * wire 契约里 `cause` 缺席 = `"gone"` = 真死了 ⇒ 每一条 `session_removed` 都落进「真死了」那一支
     * ⇒ `/clear` 一下，那条对话从总览面无声消失，而「已被 /branch 或 /clear 接替」那一区永远空着。
     * 一句错话都没说，所以比显示成「死了」更难被发现。
     *
     * 所以有第三态：不知道。它挂在 `build_id` 闸门上，不看 `cause`（不发 `cause` 的 daemon 与
     * 说「gone」的 daemon 在 wire 上一模一样）。这里的 hello 是手抄的 `build_id`，不是真素材。
     */
    @Test
    fun aDaemonThatCannotSayWhyGetsItsOwnBucketInsteadOfDisappearing() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        """{"kind":"hello","v":1,"build_id":"p1r-event-liveness","host_arch":"x86_64","claude_dir":"/h/.claude"}""",
                        """{"kind":"session_added","sid":"cleared","cwd":"/proj","status":"busy"}""",
                        """{"kind":"session_removed","sid":"cleared"}""",
                    ),
                    path,
                ).states().toList()
            val last = states[states.size - 2]
            assertEquals("已经不在活动表里", emptySet<String>(), last.sessions.keys)
            assertEquals("不许当成被顶替 —— 它没说过那句话", emptySet<String>(), last.superseded.keys)
            assertEquals("也不许无声消失", setOf("cleared"), last.removedUnknown.keys)
            assertEquals("要留住最后一份快照，否则 UI 只剩一个光秃秃的 sid", "/proj", last.removedUnknown.getValue("cleared").cwd)
        }

    /**
     * 同一条路，素材换成 fixture 里真录下来的 hello（`p1h-bg-badge`，缺 capabilities/emits）。
     *
     * 上一条用的是手抄的 `p1r`；两条一起，「说不出口」这件事不只建在手抄的那一格上。
     */
    @Test
    fun aRecordedP1hHelloAlsoLandsInTheCannotSayBucket() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1h"),
                        """{"kind":"session_added","sid":"s1"}""",
                        """{"kind":"session_removed","sid":"s1","cause":"superseded"}""",
                    ),
                    path,
                ).states().toList()
            val last = states[states.size - 2]
            // 连帧上明写了 `superseded` 也不采信：这一版的 daemon 不该发得出这个字段，
            // 采信它就是把「推出来的版本序」凌驾于「daemon 真的说了什么」之上。
            assertEquals("认不出/太旧 ⇒ 一律落第三档", setOf("s1"), last.removedUnknown.keys)
            assertEquals(emptySet<String>(), last.superseded.keys)
        }

    /**
     * 「这台后端说不说得出 `cause`」全仓只有一种答案，由 [BuildIds] 给出。
     *
     * 两个比较器的话，总览的下线分派与 [DaemonProbe.Result.supports] 可能对同一台后端给出相反答案；
     * 分派那条答错的后果就是 [WireFeature.REMOVAL_CAUSE] 头注那句「从总览面无声消失」。
     *
     * 后端历史上 152 个 `BUILD_ID` 用两种比法零分歧（它的惯例是字母到 `z` 就进主版本：`p8z → p9a`）；
     * 分歧全在构造的名字上：同主版本的多字母段（`p1aa` · `p1at`）、四位主版本（`p1000a`）、
     * 后缀不是 `-`（`p1t_…`）、前导空白。`p9aa` 本身不分歧（主版本 9 > 1，轮不到比字母），留在表里把这句说死。
     *
     * 走生产路径（`states()` 的下线分派），并逐行对照 [DaemonProbe.Result.supports]。
     * 期望值是手写的绝对值，不是「两边相等」，否则两边一起错也是绿的。
     */
    @Test
    fun whetherABackendSpeaksCauseHasExactlyOneAnswerInTheWholeRepo() =
        runTest {
            // build_id → 说得出 `cause` 吗（true ⇒ 帧上的 superseded 采信、进归档；false ⇒ 落第三档）
            val table =
                listOf(
                    "p1aa-later" to true, // 多字母段 ＋ 同主版本：只看第一个字母会读成 'a' < 't'
                    "p1at-later" to true,
                    "p9aa-later" to true,
                    "p1000a-later" to true, // 四位主版本：`\d{1,3}` 认不出
                    "p9k-flicker" to true, // 部署过的一版
                    "p1t-removal-cause" to true,
                    "p1r-event-liveness" to false, // 早于门槛
                    "p1t_underscore" to false, // 后缀不是 `-` ⇒ 认不出 ⇒ 保守；只认前缀的比较器会判够新
                    null to false,
                    "" to false,
                    "dev" to false,
                    "2026.09.04" to false,
                    "px1" to false,
                    "P1T-REMOVAL-CAUSE" to false,
                )
            val wrong = mutableListOf<String>()
            for ((id, speaks) in table) {
                val buildIdField = if (id == null) "" else ""","build_id":"$id""""
                val last =
                    DaemonSessionSource(
                        chan(
                            """{"kind":"hello","v":1$buildIdField,"host_arch":"x86_64","claude_dir":"/h/.claude"}""",
                            """{"kind":"session_added","sid":"s1","cwd":"/proj"}""",
                            """{"kind":"session_removed","sid":"s1","cause":"superseded"}""",
                        ),
                        path,
                    ).states().toList().let { it[it.size - 2] }
                val dispatched = "s1" in last.superseded
                val probeSays = DaemonProbe.Result(JsonlFrame.Hello(buildId = id)).supports(WireFeature.REMOVAL_CAUSE)
                if (dispatched != speaks) wrong += "「$id」下线分派说 $dispatched，应为 $speaks"
                if (("s1" in last.removedUnknown) == dispatched) wrong += "「$id」既不在归档也不在第三档（或两边都在）"
                if (probeSays != dispatched) wrong += "「$id」下线分派说 $dispatched，DaemonProbe 说 $probeSays —— 两种答案"
            }
            assertEquals("每个名字全仓只许一种答案，且是对的那一种", emptyList<String>(), wrong)
        }

    /**
     * hello 里没带 `build_id` 时也必须落第三档。
     * 与上面那张表的区别：表里的下线帧都明写了 `cause`，这一条连 `cause` 都没有。
     */
    @Test
    fun aHelloWithoutABuildIdIsTreatedAsTooOldToSpeak() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        """{"kind":"hello","v":1,"host_arch":"x86_64","claude_dir":"/h/.claude"}""",
                        """{"kind":"session_added","sid":"s1"}""",
                        """{"kind":"session_removed","sid":"s1"}""",
                    ),
                    path,
                ).states().toList()
            val last = states[states.size - 2]
            assertEquals(setOf("s1"), last.removedUnknown.keys)
        }

    // ---- 丢掉的状态帧 ⇒ 等待态当场作废 --------------------------------

    /**
     * 后端把 `session_status` 判为 unrecoverable：丢了永远补不回来 ⇒ 手上那个 `waiting` 可能是过期的最后一帧。
     *
     * 不作废的话屏上继续说「在等」，而它可能早就跑完了；拦发送那道门也会一直拦着。
     */
    @Test
    fun aLostStatusFrameVoidsThatConversationsWaitingState() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","cwd":"/a","status":"waiting","waiting_for":"permission prompt"}""",
                        """{"kind":"session_added","sid":"s2","cwd":"/b","status":"waiting","waiting_for":"input needed"}""",
                        """{"kind":"overflow","dropped":4,"lost":[{"kind":"session_status","subject":"s1"}]}""",
                    ),
                    path,
                ).states().toList()
            val last = states[states.size - 2]
            val s1 = last.sessions.getValue("s1")
            assertNull("丢了状态帧 ⇒ 等待态作废", s1.waitingFor)
            assertNull("连 status 一起作废 —— 我们不知道它现在怎么样了", s1.status)
            assertEquals("但这条对话本身还在，不许删", "/a", s1.cwd)
            val s2 = last.sessions.getValue("s2")
            assertEquals("没被点名的一条不许跟着作废", "waiting", s2.status)
            assertEquals("input needed", s2.waitingFor)
            assertEquals("丢行数照旧要记", 4, last.dropped)
        }

    /**
     * 表被截断（或某项没带 subject）⇒ 点不出名 ⇒ 保守方向是全部作废。
     *
     * 代价写在被守对象的头注里：只是列表截断、其实没丢状态帧时，这一下会把整张表打成「说不好」。
     * 拥塞会立刻触发一次自动重连、每条新流重新宣告所有会话 ⇒ 它是短命的。
     */
    @Test
    fun anUnscopedLossVoidsEveryStatusBecauseWeCannotTellWhichOne() =
        runTest {
            val truncated = """{"kind":"overflow","dropped":9,"lost":[{"kind":"line","subject":"x"}],"lost_truncated":true}"""
            val noSubject = """{"kind":"overflow","dropped":9,"lost":[{"kind":"session_status"}]}"""
            for (frame in listOf(truncated, noSubject)) {
                val states =
                    DaemonSessionSource(
                        chan(
                            realHelloLine("p1t"),
                            """{"kind":"session_added","sid":"s1","status":"waiting","waiting_for":"input needed"}""",
                            """{"kind":"session_added","sid":"s2","status":"busy"}""",
                            frame,
                        ),
                        path,
                    ).states().toList()
                val last = states[states.size - 2]
                assertNull("形状=$frame：点不出名 ⇒ 一律作废", last.sessions.getValue("s1").waitingFor)
                assertNull("形状=$frame", last.sessions.getValue("s1").status)
                assertNull("形状=$frame：连不在等的那条也一起作废（保守方向）", last.sessions.getValue("s2").status)
            }
        }

    /**
     * 阴性对照：`overflow` 没说丢了什么（只发 `dropped`）时，状态一个字都不许动，
     * 否则每次拥塞都会把整张表打成「说不好」。
     */
    @Test
    fun anOverflowThatSaysNothingAboutWhatWasLostChangesNoStatus() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","status":"waiting","waiting_for":"input needed"}""",
                        """{"kind":"overflow","dropped":2}""",
                    ),
                    path,
                ).states().toList()
            val s1 = states[states.size - 2].sessions.getValue("s1")
            assertEquals("waiting", s1.status)
            assertEquals("input needed", s1.waitingFor)
        }

    /**
     * `attachable` 只在 `session_added` 里出现（后端 `SessionStatus` 不带它）⇒ 状态帧到达时必须保住它。
     *
     * 把 `prev.copy(...)` 换成重建 `Session(...)`，或补一句 `attachable = true`，别的测试全绿；这条补那个洞。
     */
    @Test
    fun aStatusFrameMustNotResetAttachable() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","cwd":"/proj","attachable":false}""",
                        """{"kind":"session_status","sid":"s1","status":"busy"}""",
                    ),
                    path,
                ).states().toList()
            val s1 = states[states.size - 2].sessions.getValue("s1")
            assertEquals("前提：状态帧确实生效了", "busy", s1.status)
            assertFalse("状态帧不许把 attachable 冲回默认值", s1.attachable)
            assertEquals("added 的其他字段也不该被抹", "/proj", s1.cwd)
        }

    /**
     * 同一个 sid 会收到不止一条 `session_added`（第二个 pidfile 会再宣告一次）。
     *
     * 整条重建 `Session(...)` 的话，两条 added 之间的 `session_status` 更新被抹掉，
     * 行会在「正在跑 / 需手动 / 其余」之间来回跳。
     */
    @Test
    fun aSecondAddedForTheSameSidMergesInsteadOfWipingWhatWeLearned() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","cwd":"/proj","name":"取了名","attachable":false}""",
                        """{"kind":"session_status","sid":"s1","status":"busy","waiting_for":"permission"}""",
                        // 第二个 pidfile 再宣告一次：只带 sid + path，其余字段缺席
                        """{"kind":"session_added","sid":"s1","path":"/p/s1.jsonl"}""",
                    ),
                    path,
                ).states().toList()
            val s1 = states[states.size - 2].sessions.getValue("s1")
            assertEquals("新帧带的要更新", "/p/s1.jsonl", s1.path)
            assertEquals("中途学到的状态不许被抹", "busy", s1.status)
            assertEquals("waitingFor 同理", "permission", s1.waitingFor)
            assertEquals("第一条 added 的字段也要留住", "取了名", s1.name)
            assertFalse("attachable 更不能被冲回默认 true —— 它是个安全门", s1.attachable)
        }

    /**
     * 「`waitingFor` 缺席即清空」是 `apply(SessionStatus)` 里三选一的例外：「不再等待」就该消失，
     * 保留旧值会让 UI 永远显示「等待用户」。写成 `fr.waitingFor ?: prev.waitingFor` 时这条红。
     */
    @Test
    fun waitingForIsClearedWhenAbsentUnlikeTheOtherFields() =
        runTest {
            val states =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","status":"waiting","waiting_for":"permission"}""",
                        """{"kind":"session_status","sid":"s1","status":"busy"}""",
                    ),
                    path,
                ).states().toList()
            val s1 = states[states.size - 2].sessions.getValue("s1")
            assertEquals("前提：先真的等过", "busy", s1.status)
            assertNull("缺席即清空 —— 否则永远显示「等待用户」", s1.waitingFor)
        }

    /**
     * `session_status` 先于 `session_added` 到达时，凭空建一条而不是丢掉。
     *
     * daemon 是事件驱动的，顺序不保证。丢掉的表现是「这个会话的状态永远不更新」：
     * UI 上一个永远显示旧状态的条目，且没有任何报错。
     */
    @Test
    fun statusArrivingBeforeAddedCreatesTheSessionInsteadOfDropping() =
        runTest {
            val last =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_status","sid":"ghost","status":"running"}""",
                    ),
                    path,
                ).states().toList().let { it[it.size - 2] }

            assertTrue("乱序到达的 status 必须建条目", last.sessions.containsKey("ghost"))
            assertEquals("running", last.sessions.getValue("ghost").status)
        }

    /**
     * `overflow` 必须变成显式的 degraded，绝不静默吞。
     *
     * daemon 拥塞会丢行。静默忽略的话，UI 会显示一份「看起来完整、实际缺行」的会话，没有任何线索。
     * 让上层自己决定怎么办（补齐 / 标脏 / 提示），而不是替它决定「没事」。
     */
    @Test
    fun overflowBecomesAnExplicitDegradedStateNotSilentlyIgnored() =
        runTest {
            val last =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","path":"/p/s1.jsonl"}""",
                        """{"kind":"overflow","dropped":42}""",
                    ),
                    path,
                ).states().toList().let { it[it.size - 2] }

            // 只出结构化事实，不出散文：上屏的话由上层决定。
            assertEquals("overflow 必须上抛为丢行数", 42, last.dropped)
            // 注意：`?.takeIf { false }` 恒为 null，对任何实现都成立，不能拿来当判据。
            assertEquals("本类自己永远不清 —— 它不负责补齐", 42, last.dropped)
        }

    /**
     * 探测失败 ⇒ 发一个 `fatal` 快照，不是静默返回空流。空流的表现是上层永远转圈。
     */
    @Test
    fun probeFailureEmitsFatalStateInsteadOfAnEmptyFlow() =
        runTest {
            val states = DaemonSessionSource(chan(""), path).states(probeTimeoutMs = 500).toList()
            assertEquals("必须恰好发一个快照告知失败", 1, states.size)
            assertNotNull("必须带 fatal", states[0].fatal)
            assertTrue("fatal 要带诊断信息：${states[0].fatal}", states[0].fatal!!.contains(path))
            assertTrue("会话表应为空", states[0].sessions.isEmpty())
        }

    /** 没有 capabilities 的 daemon 照常工作，只是能力集为空：它是正常情况不是错误。 */
    @Test
    fun aDaemonWithoutCapabilitiesWorksWithAnEmptySet() =
        runTest {
            val last =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1h"),
                        """{"kind":"session_added","sid":"s1","path":"/p/s1.jsonl"}""",
                    ),
                    path,
                ).states().toList().let { it[it.size - 2] }

            assertNull("旧 daemon 不是错误：${last.fatal}", last.fatal)
            assertEquals("能力集为空", emptyList<String>(), last.capabilities)
            assertTrue("会话照常建", last.sessions.containsKey("s1"))
        }

    /**
     * 能力协商的闭环：探到的 capabilities 必须真的传进主流的 exec 命令。
     *
     * 断言两次 exec：第一次不带任何能力 flag（探测时还不知道能力），第二次才带 flag，且只带 daemon 声明过的。
     * 两次都要过 `--` 那道门并显式发 `--stream`：裸命令 `'<path>'` 的含义是「起一个 claude」。
     */
    @Test
    fun negotiatedCapabilitiesActuallyReachTheMainStreamCommand() =
        runTest {
            val ch = chan(realHelloLine("p1t"))
            DaemonSessionSource(ch, path).states(want = listOf("bg")).toList()

            assertEquals("应恰好两次 exec（探测 + 主流）", 2, ch.commands.size)
            assertEquals("第一次不带能力 flag，但门与 --stream 照发", "'$path' -- --stream", ch.commands[0])
            assertEquals("第二次才带协商出来的 flag", "'$path' -- --stream --with-bg", ch.commands[1])
        }

    /**
     * 没有 capabilities 的 daemon 上，即使想要 bg，主流命令也一个能力 flag 都不许带。
     *
     * 发了它不认识的 flag ⇒ 落进一次性查询模式 ⇒ `exit 2` ⇒ 永远不发 hello ⇒ 无限重连、日志里什么都没有。
     * 「不带 flag」不等于「裸命令」：`-- --stream` 与能力协商无关，恒发。
     */
    @Test
    fun aDaemonWithoutCapabilitiesNeverGetsAFlagEvenWhenWanted() =
        runTest {
            val ch = chan(realHelloLine("p1h"))
            DaemonSessionSource(ch, path).states(want = listOf("bg", "tail-only")).toList()

            assertEquals(2, ch.commands.size)
            ch.commands.forEach {
                assertEquals("旧 daemon 的每一次 exec 都不许带能力 flag", "'$path' -- --stream", it)
            }
        }

    /**
     * 流正常结束 ≠ 一切正常：daemon 退出了，必须发 `fatal` 说出来。
     *
     * 裸 `collect` 的话 flow 静默完成，最后一个快照 `fatal=null`/`degraded=null`
     * ⇒ UI 看到一张停在过去某一刻、却自称健康的会话表。
     */
    @Test
    fun streamEndingIsReportedAsFatalNotSilentlyTreatedAsHealthy() =
        runTest {
            val last =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","path":"/p/s1.jsonl"}""",
                    ),
                    path,
                ).states().toList().last()

            assertNotNull("daemon 退出必须上抛 fatal，不能装作健康", last.fatal)
            assertTrue("要说明是对端退出：${last.fatal}", last.fatal!!.contains("退出"))
            assertTrue("已知的会话表应保留（供 UI 显示「最后已知状态」）", last.sessions.containsKey("s1"))
        }

    /** 未知帧型原样跳过，不崩、不污染会话表（daemon 加新帧不该让我们红）。 */
    @Test
    fun unknownFrameKindsAreSkippedWithoutBreakingTheTable() =
        runTest {
            val last =
                DaemonSessionSource(
                    chan(
                        realHelloLine("p1t"),
                        """{"kind":"session_added","sid":"s1","path":"/p/s1.jsonl"}""",
                        """{"kind":"brand_new_frame_from_the_future","whatever":1}""",
                        """{"kind":"turn_end","session_id":"s1","uuid":"u1"}""",
                    ),
                    path,
                ).states().toList().let { it[it.size - 2] }

            assertNull(last.fatal)
            assertEquals(setOf("s1"), last.sessions.keys)
        }
}
