package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.catalog.SessionLight
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.time.LocalDate
import java.time.LocalTime
import java.time.ZoneId

/**
 * 总览面的分区与按项目分组。
 *
 * 每条断言都要能回答「它要防的 bug 存在时会不会红」，所以头注写明它对应哪条变异。
 * 三条最容易写成恒绿的地方，逐条防住：
 * 1. 「各段行数之和 == 输入总行数」在行被复制时也成立，因为「同一条同时进分区 1 和按项目那段」
 *    是设计要求。所以分两条断言：跨段比集合（没丢）+ 段内查重复（没多）。
 * 2. 排序断言必须有同档多行：每档只放一行的话，任何稳定排序都过，「同档按时间倒序」那半是空真。
 * 3. 「只对翻到过的项目说数」的夹具必须真的带一个没翻到的项目，全喂成翻到过的话那条恒真。
 */
class SectionsTest {
    // ── 夹具 ───────────────────────────────────────────────────────────────

    private fun live(
        id: String,
        light: SessionLight = SessionLight.Idle,
        cwd: String? = "/p/a",
        archived: Boolean = false,
        waitingFor: String? = null,
        attachable: Boolean = true,
    ) = SessionRow(
        sessionId = id,
        title = "标题-$id",
        cwd = cwd,
        light = light,
        attachable = attachable,
        waitingFor = waitingFor,
        archived = archived,
    )

    private fun hist(
        id: String,
        cwd: String? = "/p/a",
        updatedAtMs: Long = 1_000L,
        messageCount: Int = 0,
    ) = ConversationRow(
        sessionId = id,
        title = "历史-$id",
        cwd = cwd,
        jsonlPath = "/j/$id.jsonl",
        messageCount = messageCount,
        updatedAtMs = updatedAtMs,
    )

    /** 五批行各来一点，两个项目 + 一条认不出项目的。 */
    private fun mixed() =
        SessionOverviewUiState(
            needsYou = listOf(live("w1", SessionLight.WaitingInput, waitingFor = "permission prompt")),
            running = listOf(live("r1", SessionLight.Working)),
            others = listOf(live("o1", SessionLight.Idle, cwd = "/p/b"), live("o2", SessionLight.Shell, cwd = null)),
            superseded = listOf(live("s1", archived = true)),
            historyPaging =
                HistoryPaging(
                    rows = listOf(hist("h1"), hist("h2", cwd = "/p/b"), hist("h3", cwd = null)),
                    attached = true,
                ),
        )

    private fun idsOf(section: OverviewSection) = section.rows.map { it.sessionId }

    private fun byProject(state: SessionOverviewUiState) = sections(state)[3] as OverviewSection.ByProject

    /** 「说不出为什么下线」那一段。按类型取而不是按下标：下标由 [thereAreExactlyFourSectionsNamedInOrder] 单独钉。 */
    private fun unclearSectionOf(state: SessionOverviewUiState): OverviewSection.UnclearRemoval? {
        val all = sections(state)
        return all.filterIsInstance<OverviewSection.UnclearRemoval>().singleOrNull()
    }

    /** 一条下线了但说不出为什么的行（第三档）。刻意喂一个会点灯的 `light`。 */
    private fun goneUnclear(
        id: String,
        cwd: String? = "/p/a",
        light: SessionLight = SessionLight.Working,
    ) = live(id, light, cwd = cwd).copy(removalUnknown = true)

    // ── 分区 ───────────────────────────────────────────────────────────────

    /**
     * 分区恰好四个，段名与顺序逐字。空输入也是四段：「空段不返回」会让「恰好四段」变成一句随输入变化的话。
     *
     * 「说不出为什么下线」那段（[UNCLEAR_REMOVAL_TITLE]）在「已被接替」之后、「按项目/历史」之前。
     */
    @Test
    fun thereAreExactlyFourSectionsNamedInOrder() {
        assertEquals(
            listOf(SECTION_NEEDS_MANUAL, SECTION_RUNNING, UNCLEAR_REMOVAL_TITLE, SECTION_BY_PROJECT),
            sections(mixed()).map { it.title },
        )
        assertEquals(
            "空输入也必须是四段，否则「恰好四段」是句空话",
            4,
            sections(SessionOverviewUiState()).size,
        )
    }

    /**
     * 第三档：下线了但说不出为什么的那几条，有自己的一段。
     *
     * 后端不发 `cause` 时每条下线都可能被读成「真死了」，`/clear` 一下对话就从总览面无声消失。
     * 这一档在 `SessionOverviewUiState.removedUnknown` 里；`sections()` 漏掉它的话，屏上那一段就没了。
     *
     * 四条各守各的：
     * ① 这一段存在、且装的正是那几行（把 `UnclearRemoval` 那一项从 `sections()` 里删掉 ⇒ 红）；
     * ② 它不并进分区 1/2（并进去就成了「还在，只是没在跑」）；
     * ③ 它也在按项目那段里（项目那个轴上它是 X 项目的一条对话，漏掉的话
     *    唯一能按项目回溯的地方又把它吞了）；
     * ④ 记号与状态词一盏灯都不许点，见下面那条。
     */
    @Test
    fun theRemovalsWeCannotExplainGetTheirOwnSection() {
        val state =
            SessionOverviewUiState(
                running = listOf(live("r1", SessionLight.Working)),
                removedUnknown = listOf(goneUnclear("gone1"), goneUnclear("gone2")),
            )
        val unclear = unclearSectionOf(state)
        assertTrue("`sections()` 里根本没有「说不出为什么下线」这一段", unclear != null)
        assertEquals("这一段装的必须正是那几行", listOf("gone1", "gone2"), idsOf(unclear!!))
        assertEquals("段名必须是那个常量，不许另写一句", UNCLEAR_REMOVAL_TITLE, unclear.title)

        // ② 不许并进「在跑」那一段（那一段说的是「它还在，而且正在干活」）
        assertEquals(listOf("r1"), idsOf(sections(state)[1]))
        // ③ 项目那个轴上它照样在
        assertTrue(
            "按项目那段把它漏了：唯一能按项目回溯的地方又把它吞了",
            idsOf(byProject(state)).containsAll(listOf("gone1", "gone2")),
        )
    }

    /**
     * 第三档不点状态灯（同归档行那条规矩）。
     *
     * 喂 `lightForStatus(_, alive = false)` 会恒得 `Stopped`，屏上就写成「× 已停」。
     * 夹具里那行的 `light` 是 `Working`（真实数据里它就是下线前最后一次的状态）；
     * 喂 `Unknown` 的话「不点灯」那半是空真。
     */
    @Test
    fun aRemovalWeCannotExplainLightsNoStatusLamp() {
        val state = SessionOverviewUiState(removedUnknown = listOf(goneUnclear("gone1")))
        // 前提先断出来：不然这一段被漏掉时本条死在 `NoSuchElementException` 上，
        //   那读起来是崩溃，不是一次读数。
        val rows = unclearSectionOf(state)?.rows.orEmpty()
        assertEquals("前提：那一段得真有这一行（否则下面读的是空气）", listOf("gone1"), rows.map { it.sessionId })
        val row = rows.single()
        val mark = markFor(row)
        val word = statusWordFor(row)
        listOf("◆", "●", "▸", "○", "·", "×").forEach {
            assertFalse("第三档不许点任何一盏灯（撞上「$it」）：'$mark'", mark.contains(it))
        }
        listOf("已停", "结束", "崩", "死", "在跑", "空闲").forEach {
            assertFalse("状态词不许替对端把话说死（撞上「$it」）：'$word'", word.contains(it))
        }
        assertTrue("要说它下线了：'$word'", word.contains("下线"))
        assertTrue("要有一个记号，不能什么都不给（那是纯历史行那一档）：'$mark'", mark.isNotEmpty())
    }

    /**
     * 变异靶子：把 `superseded`（或任何一批）从按项目那段漏掉，这条必红。
     *
     * 逐批点名而不是只比总集合：只比总集合的话，漏掉的那一批如果碰巧
     * 与别的批有交集，断言会侥幸通过。
     */
    @Test
    fun sectionThreeCarriesEveryOtherSupersededAndHistoryRow() {
        val state = mixed()
        val third = idsOf(byProject(state)).toSet()
        listOf(
            "others" to state.others.map { it.sessionId },
            "superseded" to state.superseded.map { it.sessionId },
            "history" to state.historyPaging.rows.map { it.sessionId },
            // 分区 1/2 的行也要在（两个区的组织轴不同，各占一格不是重复）
            "needsYou" to state.needsYou.map { it.sessionId },
            "running" to state.running.map { it.sessionId },
        ).forEach { (name, ids) ->
            ids.forEach { id ->
                assertTrue("第三段丢了 $name 的 $id：$third", id in third)
            }
        }
    }

    /** 三段合起来一行都不丢（按 `sessionId` 去重后比集合）。 */
    @Test
    fun noConversationDisappearsFromAllThreeSections() {
        val state = mixed()
        val input =
            (state.needsYou + state.running + state.others + state.superseded).map { it.sessionId }.toSet() +
                state.historyPaging.rows
                    .map { it.sessionId }
                    .toSet()
        val shown = sections(state).flatMap { idsOf(it) }.toSet()
        assertEquals(input, shown)
    }

    /**
     * 去重断言的阴性对照：往按项目那段塞「每行复制两遍」，这条必红。
     *
     * 上面那条比的是集合，复制不改变集合，它对复制恒绿。真正接住复制的是这条：段内 `sessionId` 必须互不相同。
     * 分区 1/2 与按项目那段之间的重复是设计要求（组织轴不同），所以只查段内。
     */
    @Test
    fun noSectionShowsTheSameConversationTwice() {
        // 同一条对话既在流上宣告（running）又被历史翻到 —— 这是最容易重复的真实形态
        val state =
            mixed().let {
                it.copy(historyPaging = it.historyPaging.copy(rows = it.historyPaging.rows + hist("r1")))
            }
        sections(state).forEach { s ->
            val ids = idsOf(s)
            assertEquals("「${s.title}」段里同一条出现了不止一次：$ids", ids.size, ids.toSet().size)
        }
    }

    /**
     * 按项目那段按 `cwd` 分组；`cwd` 缺席的行进 [UNKNOWN_PROJECT_GROUP]，不许丢也不许猜。
     */
    @Test
    fun rowsAreGroupedUnderTheirOwnProjectAndTheNamelessOnesAreKept() {
        val groups = byProject(mixed()).groups.associateBy { it.name }
        assertEquals(setOf("a", "b", UNKNOWN_PROJECT_GROUP), groups.keys)
        assertEquals(
            setOf("w1", "r1", "s1", "h1"),
            groups
                .getValue("a")
                .rows
                .map { it.sessionId }
                .toSet(),
        )
        assertEquals(
            setOf("o1", "h2"),
            groups
                .getValue("b")
                .rows
                .map { it.sessionId }
                .toSet(),
        )
        assertEquals(
            setOf("o2", "h3"),
            groups
                .getValue(UNKNOWN_PROJECT_GROUP)
                .rows
                .map { it.sessionId }
                .toSet(),
        )
    }

    /** `⟨还没认出项目的对话⟩` 永远排最后。 */
    @Test
    fun theNamelessGroupIsAlwaysLast() {
        assertEquals(UNKNOWN_PROJECT_GROUP, byProject(mixed()).groups.last().name)
    }

    /**
     * 组内排序：先按记号档次，同档按时间倒序。
     *
     * 夹具刻意做成：`h*` 三行同档（都是纯历史行 ⇒ 无记号），且输入顺序
     * （100 · 300 · 200）与期望顺序（300 · 200 · 100）不同，否则「同档按时间倒序」那半是空真。
     */
    @Test
    fun insideAGroupMarksComeFirstAndThenNewestFirstWithinTheSameMark() {
        val state =
            SessionOverviewUiState(
                needsYou = listOf(live("w1", SessionLight.WaitingInput)),
                running = listOf(live("r1", SessionLight.Working)),
                others = listOf(live("i1", SessionLight.Idle)),
                superseded = listOf(live("s1", archived = true)),
                historyPaging =
                    HistoryPaging(
                        rows = listOf(hist("hA", updatedAtMs = 100), hist("hB", updatedAtMs = 300), hist("hC", updatedAtMs = 200)),
                        attached = true,
                    ),
            )
        assertEquals(
            listOf("w1", "r1", "i1", "hB", "hC", "hA", "s1"),
            byProject(state)
                .groups
                .single()
                .rows
                .map { it.sessionId },
        )
    }

    /**
     * 分区 1 按危险度排，不按输入顺序。
     *
     * 夹具里有两条同危险度（`permission prompt`）：没有它的话
     * 「同档保持输入顺序」那半是空真；输入顺序与期望顺序也刻意不同。
     */
    @Test
    fun needsYouIsOrderedByHowBadItIsToGetItWrong() {
        val state =
            SessionOverviewUiState(
                needsYou =
                    listOf(
                        live("d", SessionLight.WaitingInput, waitingFor = "dialog open"),
                        live("p1", SessionLight.WaitingInput, waitingFor = "permission prompt"),
                        live("sb", SessionLight.WaitingInput, waitingFor = "sandbox request"),
                        live("p2", SessionLight.WaitingInput, waitingFor = "permission prompt"),
                        live("x", SessionLight.WaitingInput, waitingFor = null),
                    ),
            )
        assertEquals(listOf("sb", "p1", "p2", "d", "x"), idsOf(sections(state)[0]))
    }

    /** 已知值之外的新值不当成最危险，与 `null` 同档：猜它危险也是猜。 */
    @Test
    fun anUnknownWaitingReasonIsNotGuessedToBeTheMostDangerous() {
        assertTrue(dangerRank("某种我们没见过的等待") > dangerRank("dialog open"))
        assertEquals(dangerRank(null), dangerRank("某种我们没见过的等待"))
    }

    // ── 按项目分组 ───────────────────────────────────────────────────────────────

    /**
     * 变异靶子：让 [markFor] 对纯历史行返回 `× 已停`，这条必红。
     *
     * 磁盘上的一条对话没有活性信息，它可能正在别的电脑上跑着；「流上没说」渲染成「已停」是一句假话。
     */
    @Test
    fun aConversationThatOnlyExistsOnDiskCarriesNoMarkAtAll() {
        val row =
            byProject(mixed())
                .groups
                .first { it.name == "a" }
                .rows
                .single { it.sessionId == "h1" }
        assertEquals("纯历史行的记号必须是空串", "", markFor(row))
        assertEquals("纯历史行也不许有状态词", "", statusWordFor(row))
        val sub = overviewRowSubtitle(row, nowMs = 2_000L)
        listOf("已停", "未知", "在跑", "空闲").forEach {
            assertFalse("纯历史行的副标题里冒出了「$it」：'$sub'", sub.contains(it))
        }
    }

    /**
     * 一条对话同时被流宣告 + 被历史翻到时，它在按项目那段里那一行必须带活的记号，
     * 而不是退化成一条无记号的历史行（历史行与分区 1/2 用同一套记号）。
     */
    @Test
    fun aRunningConversationKeepsItsLiveMarkInsideTheProjectGroup() {
        val state =
            SessionOverviewUiState(
                running = listOf(live("dup", SessionLight.Working)),
                historyPaging = HistoryPaging(rows = listOf(hist("dup", messageCount = 38)), attached = true),
            )
        val row =
            byProject(state)
                .groups
                .single()
                .rows
                .single()
        assertEquals("●", markFor(row))
        assertEquals("在跑", statusWordFor(row))
        // 历史行独有的事实要被带过来，不能因为「活行赢」就丢掉
        assertTrue("合并时把历史行的消息数丢了", overviewRowSubtitle(row, 2_000L).contains("38 条"))
    }

    /** `cwd` 缺席的活行，靠历史行认出它属于哪个项目 —— 不然它会白白掉进 `⟨还没认出⟩`。 */
    @Test
    fun aLiveRowWithoutAPathBorrowsTheProjectFromItsHistoryRow() {
        val state =
            SessionOverviewUiState(
                running = listOf(live("dup", SessionLight.Working, cwd = null)),
                historyPaging = HistoryPaging(rows = listOf(hist("dup", cwd = "/p/z")), attached = true),
            )
        assertEquals(listOf("z"), byProject(state).groups.map { it.name })
    }

    /**
     * 组头的「已显示 / 共」只对已经翻到过的项目说。
     *
     * 夹具里 `/p/b` 是没翻到过的项目（它只出现在 `pagedProjects` 之外），
     * 并且下面第三条断言证明它确实没被算进任何一个数；全喂成翻到过的话这条恒真。
     */
    @Test
    fun theHeaderCounterOnlySpeaksForProjectsAlreadyPagedThrough() {
        val state =
            SessionOverviewUiState(
                historyPaging =
                    HistoryPaging(
                        rows = listOf(hist("h1", cwd = "/p/a"), hist("h2", cwd = "/p/a"), hist("hb", cwd = "/p/b")),
                        attached = true,
                        pagedProjects = listOf(ProjectSummary(projectPath = "/p/a", sessionCount = 47, lastActivityMs = 9)),
                    ),
            )
        val groups = byProject(state).groups.associateBy { it.name }
        assertEquals("已显示 2 / 共 47", projectGroupCounter(groups.getValue("a")))
        assertNull("没翻到过的项目一个数都不许说", projectGroupCounter(groups.getValue("b")))
        // 阴性对照：没翻到的那一条确实存在（不是因为夹具漏了才没被算进去）
        assertEquals(listOf("hb"), groups.getValue("b").rows.map { it.sessionId })
        assertEquals("别的项目的行被算进了 a 的「已显示」", 2, groups.getValue("a").shown)
    }

    /**
     * `sections()` 在任何输入下都不产出一个全局的「共 N 条对话」。
     *
     * 变异：把 `"$SECTION_BY_PROJECT（共 ${'$'}{...} 条对话）"` 塞进按项目那段的标题 ⇒ 这条红。
     */
    @Test
    fun sectionsNeverAnnounceAGlobalConversationCount() {
        val state =
            mixed().let {
                it.copy(
                    historyPaging =
                        it.historyPaging.copy(
                            pagedProjects = listOf(ProjectSummary("/p/a", 47, 9), ProjectSummary("/p/b", 3, 8)),
                        ),
                )
            }
        val onScreen =
            sections(state).flatMap { s ->
                listOf(s.title) +
                    ((s as? OverviewSection.ByProject)?.groups.orEmpty().flatMap { listOfNotNull(it.name, projectGroupCounter(it)) })
            }
        assertEquals("按项目那段的标题只有三个字，任何计数都不许挂上去", SECTION_BY_PROJECT, sections(state)[3].title)
        onScreen.forEach {
            assertFalse("出现了全局计数：'$it'", it.contains("条对话"))
            assertFalse("出现了「共 N 条」：'$it'", Regex("共\\s*\\d+\\s*条").containsMatchIn(it))
        }
    }

    // ── 组序 / 文案 / 时间 ──────────────────────────────────────────────────

    /** 翻到过的项目按 `lastActivityMs` 倒序，与 `ConversationPager.start` 的项目序一致。 */
    @Test
    fun pagedProjectsFollowTheSameOrderAsTheParagraphPager() {
        val state =
            SessionOverviewUiState(
                historyPaging =
                    HistoryPaging(
                        rows = listOf(hist("x", cwd = "/p/old"), hist("y", cwd = "/p/new")),
                        attached = true,
                        pagedProjects =
                            listOf(
                                ProjectSummary("/p/old", 1, lastActivityMs = 100),
                                ProjectSummary("/p/new", 1, lastActivityMs = 900),
                            ),
                    ),
            )
        assertEquals(listOf("new", "old"), byProject(state).groups.map { it.name })
    }

    /**
     * 组里最急的那一行决定组序：危险度从行抬到组。
     *
     * 「有没有被翻到」不是「急不急」的代理。夹具刻意做成：`urgent` 组没被翻到（`pagedProjects` 里没有它），
     * `calm` 组被翻到过且 `lastActivityMs` 更大，所以「没翻到的排前面」和「纯按 lastActivityMs」
     * 都会把 `calm` 排前面，这条才不是空真。
     */
    @Test
    fun theProjectWithTheMostUrgentRowComesFirst() {
        val state =
            SessionOverviewUiState(
                needsYou = listOf(live("u", SessionLight.WaitingInput, cwd = "/p/urgent")),
                others = listOf(live("c", SessionLight.Idle, cwd = "/p/calm")),
                historyPaging =
                    HistoryPaging(
                        rows = listOf(hist("hc", cwd = "/p/calm")),
                        attached = true,
                        pagedProjects = listOf(ProjectSummary("/p/calm", 9, lastActivityMs = 999_999)),
                    ),
            )
        assertEquals(listOf("urgent", "calm"), byProject(state).groups.map { it.name })
    }

    /** 项目名 = 路径最后一段；缺席才是 `⟨还没认出⟩`（认得出就不许说认不出）。 */
    @Test
    fun theProjectNameIsTheLastPathSegment() {
        assertEquals("android-terminal", projectDisplayName("/home/u/project/android-terminal"))
        assertEquals("android-terminal", projectDisplayName("/home/u/project/android-terminal/"))
        assertEquals(UNKNOWN_PROJECT_GROUP, projectDisplayName(null))
        assertEquals("/", projectDisplayName("/"))
    }

    /** 「不可接入」是黑话，要说人话。 */
    @Test
    fun theCannotAttachLineIsSaidInPlainWords() {
        val state = SessionOverviewUiState(others = listOf(live("n", attachable = false)))
        val sub =
            overviewRowSubtitle(
                byProject(state)
                    .groups
                    .single()
                    .rows
                    .single(),
                2_000L,
            )
        assertTrue("实际是：'$sub'", sub.contains("这条在电脑上不接受远程接入"))
        assertFalse("「不可接入」是黑话：'$sub'", sub.contains("不可接入"))
    }

    /** 时间说的是绝对时刻，不说「几分钟前」。没有时间就留白。 */
    @Test
    fun theTimestampIsAbsoluteAndMissingTimeStaysBlank() {
        val zone = ZoneId.systemDefault()
        val today = LocalDate.of(2026, 9, 4)

        fun at(
            d: LocalDate,
            h: Int,
            m: Int,
        ) = d
            .atTime(LocalTime.of(h, m))
            .atZone(zone)
            .toInstant()
            .toEpochMilli()
        val now = at(today, 18, 30)
        assertEquals("09:07", relativeWhen(at(today, 9, 7), now))
        assertEquals("昨天", relativeWhen(at(today.minusDays(1), 23, 59), now))
        assertEquals("8月29日", relativeWhen(at(today.minusDays(6), 12, 0), now))
        assertNull("没有时间就留白，不许填占位符", relativeWhen(0L, now))
    }
}
