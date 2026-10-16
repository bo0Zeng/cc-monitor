package com.ccmonitor.mobile.ui.chat.needs

import com.ccmonitor.mobile.core.claude.link.Changed
import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.wire.NeedsAsk
import com.ccmonitor.mobile.core.claude.link.wire.NeedsChoiceRow
import com.ccmonitor.mobile.core.claude.link.wire.Wire
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.squareup.moshi.Moshi
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.advanceTimeBy
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 一条会话的需手动：读 `sessions-needs` 里这一条、答走 `needs-answer`（同桌面那一条命令），那台推 `changed {needs}` 就重问。
 * 字 · 选项 · 次序 · 附话要求全照那台那一份（跨语言金样 `needs-ask.golden.json`），手机只排版。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class NeedsDeskTest {
    @Suppress("UNCHECKED_CAST")
    private val golden: Map<String, Any?> =
        Moshi
            .Builder()
            .build()
            .adapter(Any::class.java)
            .fromJson(File(repoRoot(), "tests/__fixtures__/needs-ask.golden.json").readText()) as Map<String, Any?>

    private fun repoRoot(): File =
        generateSequence(File("").absoluteFile) { it.parentFile }.first { File(it, "tests/__fixtures__").isDirectory && File(it, "src/backend/lib.rs").isFile }

    private fun needs(
        ask: String?,
        manual: String? = null,
        call: String? = null,
    ): Map<String, Any?> =
        mapOf(
            "kind" to if (ask == null) "unknown" else "approve",
            "call" to call,
            "text" to "等批准",
            "head" to "等批准 · Bash",
            "headCode" to "touch a.txt",
            "tone" to "need",
            "rank" to 0.0,
            "waitedMs" to 12_000.0,
            "ask" to ask?.let { golden[it] },
            "manualText" to manual,
        )

    /** 那台此刻的 `sessions-needs`：这一条会话挂着 [mine]（`null` ＝ 没有），另一条会话挂着一条批准。 */
    private class Board {
        var mine: Map<String, Any?>? = null

        /** 这一条会话上一条提问替人答过的那一句（`null` ＝ 没有）。 */
        var answered: String? = null

        fun reply(): Reply =
            Reply.Ok(
                mapOf(
                    "answered" to
                        listOfNotNull(
                            answered?.let { mapOf("sid" to "s1", "said" to it) },
                            mapOf("sid" to "other", "said" to "别的会话那一句"),
                        ),
                    "waiting" to
                        listOfNotNull(
                            mine?.let { mapOf("sid" to "s1", "needs" to it) },
                            mapOf("sid" to "other", "needs" to mapOf("kind" to "answer", "text" to "等回答", "head" to "等回答", "tone" to "need", "rank" to 1.0)),
                        ),
                ),
            )
    }

    private class Rig(
        val link: FakeCoreLink,
        val board: Board,
        val pushes: MutableSharedFlow<Changed>,
        val desk: NeedsDesk,
    )

    private fun TestScope.rig(
        mine: Map<String, Any?>?,
        answer: (Map<String, Any?>?) -> Reply = { Reply.Ok(mapOf("result" to "answered")) },
    ): Rig {
        val link = FakeCoreLink()
        val board = Board().also { it.mine = mine }
        link.answer = { cmd, args ->
            when (cmd) {
                "sessions-needs" -> board.reply()
                "needs-answer" -> answer(args)
                else -> Reply.LinkDown
            }
        }
        val pushes = MutableSharedFlow<Changed>(extraBufferCapacity = 16)
        val desk = NeedsDesk(link, pushes, MutableStateFlow("s1"), "box", backgroundScope, now = { 0L })
        runCurrent()
        advanceTimeBy(NeedsDesk.DEBOUNCE_MS + 1)
        runCurrent()
        return Rig(link, board, pushes, desk)
    }

    private fun TestScope.settle() {
        advanceTimeBy(NeedsDesk.DEBOUNCE_MS + 1)
        runCurrent()
    }

    @Test
    fun `钉条：这一条会话的那一份，字照那台；别的会话的不上`() =
        runTest {
            val r = rig(needs("approve"))
            val bar = r.desk.state.value.bar!!
            assertEquals("等批准 · Bash", bar.head)
            assertEquals("touch a.txt", bar.line2)
            assertTrue("挂着请求 ⇒ 有［答］", bar.canAnswer)
            assertEquals(copyText("sessionFace.state.waiting", "kind" to "等批准 · Bash", "waited" to DurationFormat.waited(12_000L, 0L, 0L)!!), bar.firstLine(0L))
        }

    @Test
    fun `替人答过的那一句：没在等时照那台那一句画一行、没有钉条；抽屉开着时那一份没了就写这一句；再挂上提问就换回钉条`() =
        runTest {
            val r = rig(needs("approve"))
            r.desk.open()
            assertNull(r.desk.state.value.answered)
            r.board.mine = null
            r.board.answered = "10 分钟没人答，已自动答：继续"
            r.pushes.emit(Changed("needs", "s1"))
            settle()
            val ui = r.desk.state.value
            assertNull(ui.bar)
            assertEquals("10 分钟没人答，已自动答：继续", ui.answered)
            assertEquals("10 分钟没人答，已自动答：继续", ui.sheet?.gone)
            r.desk.dismiss()
            r.board.mine = needs("approve")
            r.board.answered = null
            r.pushes.emit(Changed("needs", "s1"))
            settle()
            assertNull(r.desk.state.value.answered)
            assertNotNull(r.desk.state.value.bar)
        }

    @Test
    fun `流里在等的那一步：那台给了是哪个调用 ⇒ 交出调用 id 与那台写好的字；判不出 ⇒ 不标；不等了 ⇒ 撤掉`() =
        runTest {
            val r = rig(needs("approve", call = "toolu_1"))
            assertEquals(AwaitingStep("toolu_1", "等批准"), r.desk.state.value.awaiting)
            r.board.mine = needs("approve")
            r.pushes.emit(Changed("needs", "s1"))
            settle()
            assertNull("判不出是哪一步 ⇒ 不标", r.desk.state.value.awaiting)
            r.board.mine = null
            r.pushes.emit(Changed("needs", "s1"))
            settle()
            assertNull(r.desk.state.value.awaiting)
        }

    @Test
    fun `答不了的（ask 为空）：第二行写那台那一句，没有［答］，也不给在终端里打开`() =
        runTest {
            val r = rig(needs(null, manual = "只能在终端里答"))
            val bar = r.desk.state.value.bar!!
            assertEquals("只能在终端里答", bar.line2)
            assertFalse(bar.canAnswer)
            r.desk.open()
            assertNull("答不了的不升抽屉", r.desk.state.value.sheet)
        }

    @Test
    fun `推 changed needs（这一条）⇒ 重问；别的主题 · 别的会话不问`() =
        runTest {
            val r = rig(null)
            assertNull(r.desk.state.value.bar)
            val before = r.link.calls("sessions-needs").size
            r.pushes.emit(Changed("needs", "other"))
            r.pushes.emit(Changed("tasks", "s1"))
            settle()
            assertEquals(before, r.link.calls("sessions-needs").size)
            r.board.mine = needs("approve")
            r.pushes.emit(Changed("needs", "s1"))
            settle()
            assertEquals(before + 1, r.link.calls("sessions-needs").size)
            assertNotNull(r.desk.state.value.bar)
        }

    @Test
    fun `答批准：入参照那台的 id；答上了 ⇒ 抽屉收起，同一份不再给［答］，重问没了钉条才消失`() =
        runTest {
            val r = rig(needs("approve"))
            r.desk.open()
            assertEquals(
                "ask-1",
                r.desk.state.value.sheet!!
                    .ask.id,
            )
            r.desk.pick("always-0")
            runCurrent()
            assertEquals(listOf(mapOf("sid" to "s1", "ask" to "ask-1", "choice" to "always-0")), r.link.calls("needs-answer"))
            assertNull("答上了 ⇒ 抽屉收起", r.desk.state.value.sheet)
            assertFalse(
                "那一份已答过 ⇒ 不再给［答］",
                r.desk.state.value.bar!!
                    .canAnswer,
            )
            r.board.mine = null
            r.pushes.emit(Changed("needs", "s1"))
            settle()
            assertNull(r.desk.state.value.bar)
        }

    @Test
    fun `送出那一刻：那一颗在转、别的都不能点（再点不再交）`() =
        runTest {
            val gate = kotlinx.coroutines.CompletableDeferred<Reply>()
            val link = FakeCoreLink()
            val board = Board().also { it.mine = needs("approve") }
            link.answer = { cmd, _ -> if (cmd == "sessions-needs") board.reply() else Reply.LinkDown }
            val desk = NeedsDesk(link, MutableSharedFlow(), MutableStateFlow("s1"), "box", backgroundScope, now = { 0L }, answerCall = { _, _ -> gate.await() })
            settle()
            desk.open()
            desk.pick("allow")
            runCurrent()
            assertEquals(
                "allow",
                desk.state.value.sheet!!
                    .sending,
            )
            desk.pick("deny")
            runCurrent()
            assertEquals(
                "allow",
                desk.state.value.sheet!!
                    .sending,
            )
            gate.complete(Reply.Ok(mapOf("result" to "answered")))
            runCurrent()
            assertNull(desk.state.value.sheet)
        }

    @Test
    fun `别处先答了（点了才知道）：按钮全灰 ＋ 那台那一句 ＋ 只留关闭；随后重问`() =
        runTest {
            val r = rig(needs("approve")) { Reply.Failed(CoreFailure("closed_elsewhere", "已在终端答过或已关闭", "d")) }
            r.desk.open()
            val before = r.link.calls("sessions-needs").size
            r.desk.pick("allow")
            runCurrent()
            val sheet = r.desk.state.value.sheet!!
            assertEquals("已在终端答过或已关闭", sheet.gone)
            assertTrue(sheet.locked)
            settle()
            assertTrue("随后重问", r.link.calls("sessions-needs").size > before)
            r.desk.pick("deny")
            runCurrent()
            assertEquals("灰了不再交", 1, r.link.calls("needs-answer").size)
            r.desk.dismiss()
            assertNull(r.desk.state.value.sheet)
        }

    @Test
    fun `别处先答了（没点、重问里那条请求没了）：同一态，那一句同桌面`() =
        runTest {
            val r = rig(needs("approve"))
            r.desk.open()
            r.board.mine = null
            r.pushes.emit(Changed("needs", "s1"))
            settle()
            assertEquals(
                copyText("askBlock.gone.elsewhere"),
                r.desk.state.value.sheet!!
                    .gone,
            )
            assertNull("钉条随重问消失", r.desk.state.value.bar)
        }

    @Test
    fun `没答成（别的原因）：那一句 ＋ 详情，按钮照旧可点`() =
        runTest {
            val r = rig(needs("approve")) { Reply.Failed(CoreFailure("not_found", "未找到该请求", "detail-x")) }
            r.desk.open()
            r.desk.pick("allow")
            runCurrent()
            val sheet = r.desk.state.value.sheet!!
            assertEquals("未找到该请求", sheet.failed?.text)
            assertEquals("detail-x", sheet.failed?.detail)
            assertNull(sheet.sending)
            assertFalse(sheet.locked)
        }

    @Test
    fun `拒绝并说明：先换成输入框；必附的空着不交；写了就带上那一句`() =
        runTest {
            val r = rig(needs("approve"))
            r.desk.open()
            r.desk.pick("deny-note")
            runCurrent()
            assertEquals(
                "deny-note",
                r.desk.state.value.sheet!!
                    .noteFor,
            )
            assertTrue(r.link.calls("needs-answer").isEmpty())
            r.desk.sendNote("  ")
            runCurrent()
            assertTrue("必附的空着不交", r.link.calls("needs-answer").isEmpty())
            r.desk.cancelNote()
            assertNull(
                r.desk.state.value.sheet!!
                    .noteFor,
            )
            r.desk.pick("deny-note")
            r.desk.sendNote("别动 build/")
            runCurrent()
            assertEquals(listOf(mapOf("sid" to "s1", "ask" to "ask-1", "choice" to "deny-note", "note" to "别动 build/")), r.link.calls("needs-answer"))
        }

    @Test
    fun `批计划：继续规划可空着交`() =
        runTest {
            val r = rig(needs("plan"))
            r.desk.open()
            r.desk.pick("plan-keep")
            r.desk.sendNote("")
            runCurrent()
            assertEquals(listOf(mapOf("sid" to "s1", "ask" to "ask-3", "choice" to "plan-keep")), r.link.calls("needs-answer"))
        }

    @Test
    fun `答提问：一屏一题，这一题答了才能下一题，末题交每题的答`() =
        runTest {
            val r = rig(needs("question"))
            r.desk.open()
            val s0 = r.desk.state.value.sheet!!
            assertEquals(0, s0.page)
            assertFalse("没选不能下一题", s0.canNext)
            r.desk.toggle("q0o1")
            r.desk.toggle("q0o0") // 单选：换成这一项
            assertEquals(
                setOf("q0o0"),
                r.desk.state.value.sheet!!
                    .picks["q0"]
                    ?.options,
            )
            assertTrue(
                r.desk.state.value.sheet!!
                    .canNext,
            )
            r.desk.next()
            assertEquals(
                1,
                r.desk.state.value.sheet!!
                    .page,
            )
            assertTrue(
                r.desk.state.value.sheet!!
                    .last,
            )
            r.desk.toggle("q1o0")
            r.desk.toggle("q1o1") // 多选：两项都在
            r.desk.toggle("q1o0") // 再点 ⇒ 去掉
            r.desk.other("Kiwi")
            r.desk.pick("answer")
            runCurrent()
            assertEquals(
                listOf(
                    mapOf(
                        "sid" to "s1",
                        "ask" to "ask-4",
                        "choice" to "answer",
                        "picks" to mapOf("q0" to mapOf("options" to listOf("q0o0"), "text" to ""), "q1" to mapOf("options" to listOf("q1o1"), "text" to "Kiwi")),
                    ),
                ),
                r.link.calls("needs-answer"),
            )
        }

    @Test
    fun `答提问：单选写了其他 ⇒ 选项清掉；没答完不交提交答案`() =
        runTest {
            val r = rig(needs("question"))
            r.desk.open()
            r.desk.toggle("q0o0")
            r.desk.other("Green")
            assertEquals(
                emptySet<String>(),
                r.desk.state.value.sheet!!
                    .picks["q0"]
                    ?.options,
            )
            r.desk.pick("answer")
            runCurrent()
            assertTrue("第二题还没答 ⇒ 不交", r.link.calls("needs-answer").isEmpty())
        }

    @Test
    fun `选项分段：允许一族（首颗主按钮）· 拒绝一族另起（拒绝并停下红字）· 改到终端里答排末尾、次要；要附话的字后加省略`() {
        val sub = Wire.bind<NeedsAsk>(golden["subAgent"])!!
        val groups = choiceGroups(sub)
        assertEquals(listOf(listOf("allow", "always-0", "always-1"), listOf("deny", "deny-note", "deny-stop"), listOf("release")), groups.map { g -> g.map { it.choice.id } })
        assertEquals(listOf(ChoiceLook.PRIMARY, ChoiceLook.OUTLINE, ChoiceLook.OUTLINE), groups[0].map { it.look })
        assertEquals(listOf(ChoiceLook.OUTLINE, ChoiceLook.OUTLINE, ChoiceLook.DANGER), groups[1].map { it.look })
        assertEquals(ChoiceLook.QUIET, groups[2].single().look)
        assertEquals(copyText("askBlock.choice.more", "label" to "拒绝并说明"), groups[1][1].label)

        val plan = choiceGroups(Wire.bind<NeedsAsk>(golden["plan"])!!)
        assertEquals(listOf(listOf("plan-auto", "plan-edits", "plan-manual"), listOf("plan-keep")), plan.map { g -> g.map { it.choice.id } })

        val q = choiceGroups(Wire.bind<NeedsAsk>(golden["question"])!!)
        assertEquals(listOf(listOf("answer"), listOf("dismiss")), q.map { g -> g.map { it.choice.id } })
        assertEquals(ChoiceLook.QUIET, q[1].single().look)

        val moved = sub.copy(choices = sub.choices.map { if (it.id == "always-1") it.copy(row = NeedsChoiceRow.RELEASE) else it })
        assertEquals("段只照那台给的 row，不按 id 认", listOf("always-1", "release"), choiceGroups(moved).last().map { it.choice.id })
    }
}
