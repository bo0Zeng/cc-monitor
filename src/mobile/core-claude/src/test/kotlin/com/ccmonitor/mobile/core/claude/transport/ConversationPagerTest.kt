package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.claude.transport.DaemonConversationCatalog.Conversation
import com.ccmonitor.mobile.core.claude.transport.DaemonConversationCatalog.Project
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 历史列表有第二页，且翻第二页不重传第一页。
 *
 * 判据量的是发出去几次查询，不是结果长什么样：「两页不相交」在「每次全量重下再本地切片」的实现下
 * 照样成立。有判别力的断言是 [ConversationPager.fetched]：第一页查过的项目，第二页一次都不许再查。
 */
class ConversationPagerTest {
    private fun project(
        dir: String,
        count: Int,
        activity: Long,
    ) = Project(dirName = dir, projectPath = "/p/$dir", sessionCount = count, lastActivityMs = activity)

    private fun conv(
        id: String,
        updated: Long = 0,
    ) = Conversation(
        sessionId = id,
        title = null,
        cwd = null,
        jsonlPath = "/p/$id.jsonl",
        messageCount = 1,
        startedAtMs = updated,
        updatedAtMs = updated,
        background = false,
    )

    /**
     * 五个项目、每个 10 条 ⇒ 共 50 条，pageSize=15。
     * 注意：桩必须多项目且总数大于一页，否则「两页不相交」在单项目桩上恒真（第二页恒空）。
     */
    private fun fivePeojects() =
        (1..5).map { project("p$it", count = 10, activity = it.toLong()) }

    private class RecordingFetcher(
        private val perProject: Int = 10,
        private val failFor: Set<String> = emptySet(),
    ) {
        val calls = ArrayList<String>()

        suspend fun fetch(dir: String): List<Conversation>? {
            calls += dir
            if (dir in failFor) return null
            return (1..perProject).map {
                Conversation(
                    sessionId = "$dir-c$it",
                    title = null,
                    cwd = null,
                    jsonlPath = "/p/$dir/c$it.jsonl",
                    messageCount = 1,
                    startedAtMs = 0,
                    updatedAtMs = it.toLong(),
                    background = false,
                )
            }
        }
    }

    /**
     * 第二页只查第一页没碰过的项目：翻多远下多远。
     */
    @Test
    fun theSecondPageQueriesOnlyProjectsTheFirstPageDidNotTouch() =
        runTest {
            val f = RecordingFetcher()
            val pager = ConversationPager(pageSize = 15) { f.fetch(it) }
            pager.start(fivePeojects())

            pager.loadNextPage()
            val afterFirst = f.calls.toList()
            assertTrue("前提：第一页确实查了几个项目", afterFirst.isNotEmpty())
            // 15 条一页、每项目 10 条 ⇒ 第一页应该只吃掉 2 个项目，不是全部 5 个
            assertEquals("第一页不许把所有项目都查一遍（那就是「全有或全无」）", 2, afterFirst.size)

            pager.loadNextPage()
            val secondPageCalls = f.calls.drop(afterFirst.size)
            assertTrue("前提：第二页确实发了查询", secondPageCalls.isNotEmpty())
            // 这一条是要害
            assertEquals(
                "第一页查过的项目，第二页一次都不许再查（重查=重传）",
                emptyList<String>(),
                secondPageCalls.filter { it in afterFirst },
            )
            assertTrue(
                "任何一个项目都不许被查两遍：${pager.fetched}",
                pager.fetched.values.all { it == 1 },
            )
        }

    /** 两页的内容不相交（去重那一半）。 */
    @Test
    fun twoPagesDoNotOverlap() =
        runTest {
            val f = RecordingFetcher()
            val pager = ConversationPager(pageSize = 15) { f.fetch(it) }
            pager.start(fivePeojects())
            val first = pager.loadNextPage().added.mapTo(HashSet()) { it.sessionId }
            val second = pager.loadNextPage().added.mapTo(HashSet()) { it.sessionId }
            assertTrue("前提：两页都非空", first.isNotEmpty() && second.isNotEmpty())
            assertEquals("两页不许有重复的对话", emptySet<String>(), first intersect second)
        }

    /** 一路翻到底 ⇒ 显式终止态，且每个项目恰好被查一次（不重不漏）。 */
    @Test
    fun pagingToTheEndCoversEveryProjectExactlyOnceAndSaysItIsExhausted() =
        runTest {
            val f = RecordingFetcher()
            val pager = ConversationPager(pageSize = 15) { f.fetch(it) }
            pager.start(fivePeojects())
            val all = ArrayList<String>()
            var pages = 0
            while (pager.hasMore && pages < 20) {
                all += pager.loadNextPage().added.map { it.sessionId }
                pages++
            }
            assertTrue("应该不止一页，否则这个桩没有判别力：$pages", pages >= 2)
            assertEquals("50 条一条不漏", 50, all.size)
            assertEquals("一条都不许重复", 50, all.toSet().size)
            assertEquals("每个项目恰好查一次", List(5) { "p${it + 1}" }.toSet(), f.calls.toSet())
            assertEquals("每个项目恰好查一次（次数）", 5, f.calls.size)
            assertFalse("翻完了 hasMore 必须为假", pager.hasMore)
            assertTrue("翻完了要显式说「已是最早」", pager.loadNextPage().exhausted)
        }

    /**
     * 查询失败的项目：进 failedProjects，且游标照常前进。
     *
     * 不前进的话一个坏项目会把翻页永久卡死在原地；不上报的话一份少了几十条的列表毫无线索。
     */
    @Test
    fun aFailingProjectIsReportedAndDoesNotWedgeThePager() =
        runTest {
            val f = RecordingFetcher(failFor = setOf("p5"))
            // pageSize=1 ⇒ 每页一个项目，好把失败那页单独拎出来
            val pager = ConversationPager(pageSize = 1) { f.fetch(it) }
            pager.start(fivePeojects())
            val pages = (1..5).map { pager.loadNextPage() }
            val failed = pages.flatMap { it.failedProjects }
            assertEquals("失败的项目必须被报上来", listOf("p5"), failed)
            assertFalse("失败之后游标要照常走完，不许卡死", pager.hasMore)
            assertEquals("其余 4 个项目的内容照常拿到", 40, pages.sumOf { it.added.size })
        }

    /** `sessionCount == 0` 的项目不查：白白多一个 SSH 往返，还会稀释掉一页。 */
    @Test
    fun emptyProjectsAreNeverQueried() =
        runTest {
            val f = RecordingFetcher()
            val pager = ConversationPager(pageSize = 100) { f.fetch(it) }
            pager.start(listOf(project("empty", count = 0, activity = 9), project("real", count = 3, activity = 1)))
            pager.loadNextPage()
            assertEquals("空项目不该被查", listOf("real"), f.calls)
        }

    /** 项目按最近活动倒序翻：往回翻的直觉是「从最近往更早」。 */
    @Test
    fun projectsArePagedMostRecentlyActiveFirst() =
        runTest {
            val f = RecordingFetcher(perProject = 1)
            val pager = ConversationPager(pageSize = 1) { f.fetch(it) }
            pager.start(
                listOf(
                    project("old", count = 1, activity = 1),
                    project("newest", count = 1, activity = 99),
                    project("mid", count = 1, activity = 50),
                ),
            )
            pager.loadNextPage()
            assertEquals("最近活动的项目要先翻到", listOf("newest"), f.calls)
        }
}
