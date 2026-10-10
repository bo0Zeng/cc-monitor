package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.transport.DaemonCommands
import com.ccmonitor.mobile.core.claude.transport.DaemonConversationReader
import com.ccmonitor.mobile.core.claude.transport.DaemonSessionSource
import com.ccmonitor.mobile.core.claude.transport.SessionSignals
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOf
import kotlinx.coroutines.launch
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.resetMain
import kotlinx.coroutines.test.runTest
import kotlinx.coroutines.test.setMain
import kotlinx.coroutines.yield
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Before
import org.junit.Test
import java.io.File

/**
 * 翻页不打断实时流。
 *
 * 判据量的是「流被起过几次」（[CountingChannel.streamExecs]），不是「翻完页列表还在不在」：
 * `generation++` 重起流之后 daemon 会做一次 initial walkdir scan，会话被重新宣告一遍，
 * `running` 照样非空，那样的断言对要防的 bug 恒绿。
 */
@Suppress("DEPRECATION")
class SessionOverviewPagingTest {
    private val dispatcher = UnconfinedTestDispatcher()

    @Before fun setUp() = Dispatchers.setMain(dispatcher)

    @After fun tearDown() = Dispatchers.resetMain()

    /** 与 `SessionOverviewResyncTest` 同一个取法：不手编 hello，那会绕开能力协商。 */
    private fun realHello(): String {
        val f =
            File("../bridge/vectors/daemon-wire.ndjson").takeIf { it.isFile }
                ?: File("bridge/vectors/daemon-wire.ndjson")
        return f
            .readLines()
            .asSequence()
            .filter { it.contains("\"ver\":\"p1t\"") && it.contains("流模式首帧") }
            .mapNotNull { Regex("\"raw\":\"(.*)\"\\}$").find(it)?.groupValues?.get(1) }
            .first()
            .replace("\\\"", "\"")
            .replace("\\\\", "\\")
    }

    private fun vector(name: String): String {
        val f = File("../bridge/vectors/$name").takeIf { it.isFile } ?: File("bridge/vectors/$name")
        return f.readText()
    }

    /**
     * 一条会分派命令的假通道：daemon 流 / `--list-projects` / `--list-sessions` 各走各的，
     * 并分别计数。
     */
    private inner class CountingChannel(
        private val streamLines: List<String>,
    ) : RemoteCommandChannel {
        var streamExecs = 0
        var projectQueries = 0
        val sessionQueries = ArrayList<String>()

        override fun exec(command: String) =
            when {
                command.contains("--list-projects") -> {
                    projectQueries++
                    flowOf(vector("daemon-query-projects.ndjson").toByteArray())
                }
                command.contains("--list-sessions") -> {
                    sessionQueries += command.substringAfter("--list-sessions ").substringBefore(" &&")
                    flowOf(vector("daemon-query-sessions.ndjson").toByteArray())
                }
                // 血统探针不是流，不计数（同 SessionOverviewResyncTest）
                command.startsWith("for f in ") -> flowOf(ByteArray(0))
                else -> {
                    streamExecs++
                    flowOf((streamLines.joinToString("\n") + "\n").toByteArray())
                }
            }
    }

    private fun channel() =
        CountingChannel(
            listOf(realHello(), """{"kind":"session_added","sid":"s1","path":"/p/s1.jsonl","status":"busy"}"""),
        )

    private fun vm(ch: RemoteCommandChannel) =
        SessionOverviewViewModel(
            source = DaemonSessionSource(ch, "/opt/d"),
            // 这个参数没有默认值是有意的（见 VM 头注）：本条测试不量信号汇，
            //   但也不许有一个「忘了接线照样跑」的入口。
            signals = SessionSignals(),
            conversations = DaemonConversationReader(ch, "/opt/d"),
            probeTimeoutMs = 1_000,
        )

    /**
     * 连翻两页，实时流一次都不许被重起。
     */
    @Test
    fun pagingThroughHistoryNeverRestartsTheLiveStream() =
        runTest(dispatcher) {
            val ch = channel()
            val vm = vm(ch)
            val job = launch { vm.state.collect { } }
            yield()

            val afterFirstFrame = ch.streamExecs
            assertTrue("前提：流确实起过（探测+主流）：$afterFirstFrame", afterFirstFrame >= 2)

            vm.loadOlderConversations()
            yield()
            vm.loadOlderConversations()
            yield()

            assertTrue("前提：翻页确实发生了（发过 --list-sessions）", ch.sessionQueries.isNotEmpty())
            // 命门：翻两页之后，流被起的次数一次都不许涨
            assertEquals(
                "翻页绝不许重起实时流（重起一次要再扛一次上百 MB 的洪峰）",
                afterFirstFrame,
                ch.streamExecs,
            )
            job.cancel()
        }

    /**
     * 阴性对照：`reconnect()` 确实会重起流。
     *
     * 没有这条，上面那个 `assertEquals` 可能只是因为「这个桩下 `streamExecs` 根本不会涨」而恒绿。
     */
    @Test
    fun reconnectDoesRestartTheStreamSoTheCounterIsNotStuck() =
        runTest(dispatcher) {
            val ch = channel()
            val vm = vm(ch)
            val job = launch { vm.state.collect { } }
            yield()
            val before = ch.streamExecs
            vm.reconnect()
            yield()
            assertTrue(
                "阴性对照：reconnect 必须让计数涨（否则「翻页不重起流」那条是恒绿的）：$before → ${ch.streamExecs}",
                ch.streamExecs > before,
            )
            job.cancel()
        }

    /** 首页在进屏那一刻就要（进总览时才查的轻量查询）。 */
    @Test
    fun theFirstHistoryPageIsRequestedOnEnteringTheOverview() =
        runTest(dispatcher) {
            val ch = channel()
            val vm = vm(ch)
            val job = launch { vm.state.collect { } }
            yield()
            assertEquals("进屏就该问一次项目列表", 1, ch.projectQueries)
            val rows = vm.state.value.historyPaging.rows
            assertTrue("进屏就该翻到第一页", rows.isNotEmpty())
            job.cancel()
        }

    /**
     * 没接翻页（`conversations = null`）时不许显示「已是最早」：那是把「没这功能」讲成「已到顶」。
     */
    @Test
    fun withoutPagingAttachedTheScreenNeverClaimsTheOldestWasReached() =
        runTest(dispatcher) {
            val ch = channel()
            val vm = SessionOverviewViewModel(DaemonSessionSource(ch, "/opt/d"), SessionSignals(), probeTimeoutMs = 1_000)
            val job = launch { vm.state.collect { } }
            yield()
            val paging = vm.state.value.historyPaging
            assertFalse("没接翻页就不许说「已是最早」", paging.showsOldestReached)
            assertFalse("也不该说还能再翻", paging.canLoadMore)
            assertEquals("没接翻页时一次查询都不该发", 0, ch.projectQueries)
            job.cancel()
        }

    /**
     * 活会话为空、但历史有内容时，不许说「这台服务器上还没有对话」。
     *
     * 流上宣告的只是正在跑的那一小部分；历史里有的话，「空」这个判断就是错的。
     */
    @Test
    fun aHostWithHistoryIsNeverReportedAsHavingNoConversations() =
        runTest(dispatcher) {
            val ch = CountingChannel(listOf(realHello())) // 流上一条会话都不宣告
            val vm = vm(ch)
            val job = launch { vm.state.collect { } }
            yield()
            val st = vm.state.value
            assertTrue("前提：活会话确实是空的", st.isEmpty)
            assertTrue("前提：历史确实读到了", st.historyPaging.rows.isNotEmpty())
            assertFalse("有历史就不许报「还没有对话」", st.isCompletelyEmpty)
            job.cancel()
        }

    /** 查询面读不出来时要说出来，且不许把已翻到的行清空。 */
    @Test
    fun aFailedHistoryQueryIsReportedInsteadOfSilentlyShowingNothing() =
        runTest(dispatcher) {
            val ch =
                object : RemoteCommandChannel {
                    override fun exec(command: String) =
                        when {
                            // 没有正向标记 ⇒ 查询失败（老 daemon 不认识这个子命令时就是这样）
                            command.contains("--list-") -> flowOf(ByteArray(0))
                            command.startsWith("for f in ") -> flowOf(ByteArray(0))
                            else -> flowOf((realHello() + "\n").toByteArray())
                        }
                }
            val vm = vm(ch)
            val job = launch { vm.state.collect { } }
            yield()
            val paging = vm.state.value.historyPaging
            assertTrue("查不出来必须说出来，不许静默空白：$paging", paging.error != null)
            assertFalse("说完了还要留一条出路（不是 loading 卡住）", paging.loading)
            assertFalse("查询失败不许被讲成「已是最早」", paging.showsOldestReached)
            job.cancel()
        }

    /** 命令里要带正向标记：没有它就分不清「没有」和「问不出来」。 */
    @Test
    fun historyQueriesCarryThePositiveSuccessMarker() =
        runTest(dispatcher) {
            val seen = ArrayList<String>()
            val ch =
                object : RemoteCommandChannel {
                    override fun exec(command: String) =
                        flowOf(ByteArray(0)).also { if (command.contains("--list-")) seen += command }
                }
            val vm = vm(ch)
            val job = launch { vm.state.collect { } }
            yield()
            assertTrue("前提：确实发了查询", seen.isNotEmpty())
            assertTrue(
                "每条查询都要带正向标记：$seen",
                seen.all { it.contains(DaemonCommands.QUERY_OK_MARKER) },
            )
            job.cancel()
        }

    /**
     * `pagedProjects` 只装真的被翻到过的那几个项目（组头计数的生产接线）。
     *
     * `SectionsTest.theHeaderCounterOnlySpeaksForProjectsAlreadyPagedThrough` 钉的是纯函数 [projectGroupCounter]：
     * 给它一份「只含翻到过的项目」的 `pagedProjects`，它就只对那几个说数。可那份表是谁填的，那条判据管不着：
     * 把 `SessionOverviewViewModel` 里的 `projectTable.filter { it.dirName in p.fetched }` 改成 `projectTable`（整张表），
     * 别的判据一条都不红，屏上会对一个还没问过的项目报一个数。
     *
     * 阴性对照都在断言里：
     * ① 前提断言「项目表比翻到的多得多」，否则「只含翻到过的」恒真；
     * ② 前提断言「确实翻到了几个」，否则空表也算过；
     * ③ 翻第二页之后这个数必须变多，否则一个「算完就冻住」的实现照样过前两条。
     */
    @Test
    fun theHeaderCounterDataCoversOnlyTheProjectsActuallyPagedThrough() =
        runTest(dispatcher) {
            val ch = channel()
            val vm = vm(ch)
            val job = launch { vm.state.collect { } }
            yield()

            val tableSize = vector("daemon-query-projects.ndjson").lines().count { it.startsWith("{") }
            val queried = ch.sessionQueries.distinct()
            assertTrue("前提：确实翻到了几个项目，否则这条恒真", queried.isNotEmpty())
            assertTrue(
                "前提：项目表必须比翻到的多得多，否则「只含翻到过的」恒真：表 $tableSize / 翻到 ${queried.size}",
                tableSize > queried.size,
            )
            assertEquals(
                "组头的数只许对翻到过的项目说：翻了 ${queried.size} 个项目，pagedProjects 却装了 " +
                    "${vm.state.value.historyPaging.pagedProjects.size} 个（表里一共 $tableSize 个）",
                queried.size,
                vm.state.value.historyPaging.pagedProjects.size,
            )

            // 阴性对照③：再翻一页，翻到的项目数必须涨：挡住「算完就冻住」的实现
            val afterFirstPage = vm.state.value.historyPaging.pagedProjects.size
            vm.loadOlderConversations()
            yield()
            assertTrue(
                "翻了第二页，pagedProjects 却没变多（这个字段被冻住了）：$afterFirstPage → " +
                    "${vm.state.value.historyPaging.pagedProjects.size}",
                vm.state.value.historyPaging.pagedProjects.size > afterFirstPage,
            )
            assertEquals(
                "第二页之后也只许对翻到过的项目说数",
                ch.sessionQueries.distinct().size,
                vm.state.value.historyPaging.pagedProjects.size,
            )
            job.cancel()
        }
}
