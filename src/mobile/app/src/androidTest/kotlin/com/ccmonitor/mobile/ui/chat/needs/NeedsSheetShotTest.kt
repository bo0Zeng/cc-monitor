package com.ccmonitor.mobile.ui.chat.needs

import android.graphics.Bitmap
import androidx.compose.ui.semantics.SemanticsProperties
import androidx.compose.ui.test.assertIsDisplayed
import androidx.compose.ui.test.junit4.v2.createComposeRule
import androidx.compose.ui.test.onAllNodesWithTag
import androidx.compose.ui.test.onNodeWithTag
import androidx.compose.ui.test.performClick
import androidx.test.ext.junit.runners.AndroidJUnit4
import androidx.test.platform.app.InstrumentationRegistry
import com.ccmonitor.mobile.core.claude.link.NeedsPick
import com.ccmonitor.mobile.core.ui.theme.AppTheme
import com.ccmonitor.mobile.core.wire.gen.AskKind
import com.ccmonitor.mobile.core.wire.gen.NeedsAsk
import com.ccmonitor.mobile.core.wire.gen.NeedsChoice
import com.ccmonitor.mobile.core.wire.gen.NeedsChoiceRow
import com.ccmonitor.mobile.core.wire.gen.NeedsNote
import com.ccmonitor.mobile.core.wire.gen.NeedsQuestion
import com.ccmonitor.mobile.core.wire.gen.NeedsQuestionOption
import com.ccmonitor.mobile.core.wire.gen.Tone
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith
import java.io.File

/**
 * 答题抽屉排成多高（A14-5「手机答题那一屏太长」）：app 没引 Robolectric，抽屉高度只在设备上量得出。
 * 直接画 [AskSheetView]（不连后端），几份夹具各截一张整屏 PNG（存在 app 的外部文件目录 `needs-shots/`，`adb pull` 取）：
 * - 长选项：一题四项、每项说明一整句 —— 说明与选项字并排时被挤成窄列、一项竖着十几行，抽屉要滚好几屏；
 * - 多题：第 1/3 题、多选；
 * - 长命令：四十行的 Bash —— 全文块先收着，下面「展开 N 行」。
 * 判：这三种都不用滚就答得全；头一行有［关闭］、点了走 dismiss；全文展开后抽屉也不铺满整屏（上面留一截点了就收起）。
 * 要模拟器（同 SmokeTest，门禁 `android-device` 那一格跑）。
 */
@RunWith(AndroidJUnit4::class)
class NeedsSheetShotTest {
    @get:Rule
    val compose = createComposeRule()

    private val noop =
        AskActions(pick = {}, sendNote = {}, cancelNote = {}, toggle = {}, other = {}, next = {}, dismiss = {})

    private fun show(
        sheet: AskSheet,
        act: AskActions = noop,
    ) {
        // 抽屉头的「已等」每秒走一次（无尽的 delay 循环）：时钟手动走，不然等空闲等不到头。
        compose.mainClock.autoAdvance = false
        compose.setContent { AppTheme { AskSheetView(sheet, now = { 0L }, act = act) } }
        settle()
        compose.waitUntil(WALL_CEILING_MS) { compose.onAllNodesWithTag(NeedsTags.SHEET).fetchSemanticsNodes().isNotEmpty() }
        settle()
    }

    private fun settle() = compose.mainClock.advanceTimeBy(SETTLE_MS)

    /**
     * 截整屏（存 PNG），回屏高（像素）。抽屉是另一扇窗、内容随抽屉的位移往下挪：截抽屉那个节点只截得到挪之前那一框，
     * 截根又截不到另一扇窗 —— 只有系统那一张整屏的截得对。
     */
    private fun shot(name: String): Int {
        compose.waitForIdle()
        val img = InstrumentationRegistry.getInstrumentation().uiAutomation.takeScreenshot() ?: error("截屏没拿到")
        InstrumentationRegistry.getInstrumentation().targetContext.getExternalFilesDir("needs-shots")?.let { dir ->
            File(dir, "$name.png").outputStream().use { img.compress(Bitmap.CompressFormat.PNG, PNG_QUALITY, it) }
        }
        return img.height
    }

    /** 不滚就看得全：抽屉头一行底下那一截一点都不用滚（[name] 那张截图同时存下）。 */
    private fun assertNoScroll(name: String) {
        shot(name)
        val range =
            compose
                .onNodeWithTag(NeedsTags.BODY)
                .fetchSemanticsNode()
                .config[SemanticsProperties.VerticalScrollAxisRange]
        val max = range.maxValue()
        assertTrue("抽屉要滚 $max px 才答得全（末尾那几颗在一屏之外）", max == 0f)
    }

    @Test
    fun longOptions() {
        show(sheet(quiz(listOf(longQuestion("q0", multi = false))), picks = mapOf("q0" to NeedsPick(setOf("q0o0"), ""))))
        assertNoScroll("long-options")
    }

    @Test
    fun manyQuestions() {
        show(sheet(quiz(listOf(longQuestion("q0", multi = true), longQuestion("q1", multi = false), longQuestion("q2", multi = false)))))
        assertNoScroll("many-questions")
    }

    @Test
    fun longCommand() {
        val script = (1..COMMAND_LINES).joinToString("\n") { "echo \"step $it: rsync -av --delete ./build/out/part-$it/ backup:/srv/archive/2026/part-$it/\"" }
        show(sheet(approve(script)))
        compose.onNodeWithTag(NeedsTags.DETAIL_MORE).assertIsDisplayed()
        assertNoScroll("long-command")
    }

    @Test
    fun hideButton() {
        var hidden = 0
        val act = AskActions(pick = {}, sendNote = {}, cancelNote = {}, toggle = {}, other = {}, next = {}, dismiss = { hidden++ })
        show(sheet(approve("touch a.txt")), act)
        compose.onNodeWithTag(NeedsTags.HIDE).assertIsDisplayed().performClick()
        settle()
        assertEquals("普通请求的抽屉头上那颗［关闭］点了就收起（走 dismiss，不算答）", 1, hidden)
    }

    @Test
    fun notFullScreen() {
        val script = (1..COMMAND_LINES).joinToString("\n") { "echo step $it" }
        show(sheet(approve(script)))
        compose.onNodeWithTag(NeedsTags.DETAIL_MORE).performClick()
        settle()
        compose.onNodeWithTag(NeedsTags.DETAIL_MORE).assertDoesNotExist()
        val window = shot("expanded-command")
        val top =
            compose
                .onNodeWithTag(NeedsTags.HIDE)
                .fetchSemanticsNode()
                .positionInWindow.y
        assertTrue("全文展开后抽屉铺满了整屏（头一行在 $top px，窗高 $window px）：上面没留出点一下就收起的地方", top >= window * TOP_GAP)
    }

    private fun sheet(
        ask: NeedsAsk,
        picks: Map<String, NeedsPick> = emptyMap(),
    ) = AskSheet(ask, head = if (ask.kind == AskKind.APPROVE) "等批准 · Bash" else "等回答", waitedMs = 12_000L, receivedAt = 0L, picks = picks)

    private fun choice(
        id: String,
        text: String,
        row: NeedsChoiceRow,
        note: NeedsNote = NeedsNote.NONE,
        tone: Tone = Tone.PLAIN,
    ) = NeedsChoice(id, text, tone, note, row)

    private fun quiz(questions: List<NeedsQuestion>) =
        NeedsAsk(
            id = "ask-quiz",
            kind = AskKind.ANSWER,
            tool = "AskUserQuestion",
            run = null,
            what = null,
            detail = "",
            choices = listOf(choice("answer", "提交答案", NeedsChoiceRow.MAIN), choice("dismiss", "不回答", NeedsChoiceRow.RELEASE)),
            questions = questions,
        )

    private fun approve(script: String) =
        NeedsAsk(
            id = "ask-bash",
            kind = AskKind.APPROVE,
            tool = "Bash",
            run = null,
            what = null,
            detail = script,
            choices =
                listOf(
                    choice("allow", "允许", NeedsChoiceRow.MAIN),
                    choice("always-0", "允许，且允许访问 /srv/archive（本次会话）", NeedsChoiceRow.MAIN),
                    choice("deny", "拒绝", NeedsChoiceRow.HOLD),
                    choice("deny-note", "拒绝并说明…", NeedsChoiceRow.HOLD, NeedsNote.REQUIRED),
                    choice("deny-stop", "拒绝并停下", NeedsChoiceRow.HOLD, tone = Tone.FAIL),
                ),
            questions = emptyList(),
        )

    /** 一题四项、每项说明一整句（真 Claude 的 AskUserQuestion 每项都带 description）。 */
    private fun longQuestion(
        id: String,
        multi: Boolean,
    ) = NeedsQuestion(
        id = id,
        text = "迁移旧数据时，用户表的结构怎么处理？",
        header = "表结构",
        multi = multi,
        other = false,
        options =
            listOf(
                NeedsQuestionOption("${id}o0", "沿用现有表结构，只加一列（推荐）", "保留现在的 users 表，新增 last_seen 一列并补一条迁移；改动最小、回滚容易，但旧字段的命名问题还留着，以后再改名时还要再迁一次，相关的查询与报表也得跟着改。"),
                NeedsQuestionOption("${id}o1", "整张表重建", "按新的命名规范建 users_v2，写脚本把旧数据搬过去再切换读写；一次理顺，但迁移期间要停写几分钟，搬数据的脚本要先在副本上演练一遍，切换那一刻出错要能马上切回旧表。"),
                NeedsQuestionOption("${id}o2", "新旧两张表并存一段时间", "新写入同时落两张表，读先走旧表、逐步切到新表；不停机，代价是双写逻辑要维护到切完为止，两张表对不上时以哪张为准也得先定下来，排查起来比较费劲。"),
                NeedsQuestionOption("${id}o3", "先不动结构", "这一轮只迁数据、不改表，结构调整另开一轮；最稳，但下一轮还要再迁一次，这一轮写的迁移脚本到时候大半得重写，前后两轮加起来工作量更大。"),
            ),
    )

    private companion object {
        const val SETTLE_MS = 1_000L

        /** 等抽屉出来的上限：只防挂死（同 SmokeTest，与 `guard_core::wall::CEILING_SECS` 同一个数）。 */
        const val WALL_CEILING_MS = 300_000L
        const val COMMAND_LINES = 40
        const val PNG_QUALITY = 100

        /** 抽屉最高占到窗高的这么多以下：头一行至少在窗顶往下这一截（上面留出点一下就收起的地方）。 */
        const val TOP_GAP = 0.2f
    }
}
