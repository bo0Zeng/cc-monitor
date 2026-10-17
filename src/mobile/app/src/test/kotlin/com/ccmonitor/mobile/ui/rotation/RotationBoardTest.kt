package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.Changed
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.RotEdit
import com.ccmonitor.mobile.core.claude.link.Rotation
import com.ccmonitor.mobile.core.claude.link.wire.RotFrom
import com.ccmonitor.mobile.core.claude.link.wire.RotHintKind
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 轮换屏的接线（桌面 `rot-mount.ts` 同一套）：每一格读的是那台回包的哪一格（手机不算）· 会话侧的临时动作 · 设置页改一格交哪条命令、
 * `ifRev` 照那份的版本带 · 各种情形的那一句来自那台。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class RotationBoardTest {
    private fun cellOf(
        view: Map<String, Any?>,
        row: Int,
        key: String,
        window: String,
    ) = Rotation.ofView(view)!!.board.rows[row].cells.first { it.key == key && it.window == window }

    /** 设置页读的是哪一份（`rotation-view` 入参的 `target.strategy`）。 */
    private fun shownIn(args: Map<String, Any?>?): String = (args?.get("target") as Map<*, *>)["strategy"] as String

    private fun TestScope.board(
        sid: String? = "s1",
        view: (Map<String, Any?>?) -> Map<String, Any?> = { RotFixture.view() },
        strategies: () -> Map<String, Any?> = { RotFixture.strategies() },
        more: (String, Map<String, Any?>?) -> Reply? = { _, _ -> null },
        changes: MutableSharedFlow<Changed>? = null,
    ): Pair<FakeCoreLink, RotationBoard> {
        val link = FakeCoreLink()
        link.answer = { cmd, args ->
            more(cmd, args) ?: when (cmd) {
                Rotation.STRATEGIES -> Reply.Ok(strategies())
                Rotation.VIEW -> Reply.Ok(view(args))
                else -> Reply.Ok(emptyMap<String, Any?>())
            }
        }
        val b = RotationBoard(link, sid, "devbox", TestScope(UnconfinedTestDispatcher(testScheduler)), changes)
        return link to b
    }

    @Test
    fun `会话：读 rotation-view {sid} 推 12 小时；号牌照每个号身上那一枚（不另问账号清单）；四块画的全是回包里的那一份`() =
        runTest {
            val (link, b) = board()
            val s = b.state.value
            assertEquals(mapOf("target" to mapOf("sid" to "s1"), "hours" to 12), link.calls(Rotation.VIEW).single())
            assertEquals("spread", s.view?.strategy?.id)
            assertEquals(
                listOf("work", "team", "lab"),
                s.view!!
                    .accounts
                    .take(s.view.listed)
                    .map { it.account },
            )
            assertEquals("team", s.view.current)
            assertEquals("Work", b.label("work"))
            assertEquals("lab", b.label("lab"))
            assertEquals("视图里没有的号照原名", "nope", b.label("nope"))
            assertEquals("wo", s.plate("work")?.text)
            assertTrue("不另问账号清单", link.calls("accounts-list").isEmpty())
            assertEquals(listOf("spread", "night"), s.list?.strategies?.map { it.id })
        }

    @Test
    fun `设置页：看新会话用的那份 {strategy}；改一格交 {strategy, 意图, ifRev ＝ 那份的 rev}，那台当场存成新版、再读`() =
        runTest {
            val (link, b) =
                board(
                    sid = null,
                    strategies = { RotFixture.strategies(forNew = "night") },
                    view = { RotFixture.view(strategy = "night") },
                )
            assertEquals(mapOf("target" to mapOf("strategy" to "night"), "hours" to 12), link.calls(Rotation.VIEW).single())
            b.edit(RotEdit.set("smart", null, false))
            assertEquals(mapOf("strategy" to "night", "edit" to mapOf("set" to mapOf("key" to "smart", "v" to false)), "ifRev" to "h:night"), link.calls(Rotation.SETTINGS).single())
            assertEquals("再读不带别的", mapOf("target" to mapOf("strategy" to "night"), "hours" to 12), link.calls(Rotation.VIEW).last())
            b.forNew("spread")
            assertEquals(mapOf("strategy" to "spread"), link.calls(Rotation.FOR_NEW).single())
            assertTrue(link.calls(Rotation.USE).isEmpty())
        }

    @Test
    fun `策略改过了：会话［载入新版］⇒ rotation-use 同一份；设置页［都载入新版］⇒ rotation-reload`() =
        runTest {
            val (link, b) = board()
            b.loadNew()
            assertEquals(mapOf("sid" to "s1", "strategy" to "spread"), link.calls(Rotation.USE).single())
            val (page, p) = board(sid = null, strategies = { RotFixture.strategies(forNew = "night") }, view = { RotFixture.view(strategy = "night") })
            p.loadNew()
            assertEquals(mapOf("strategy" to "night"), page.calls(Rotation.RELOAD).single())
        }

    @Test
    fun `会话的顺序照那台排好的 lineup；勾去掉 ⇒ rotation-temp off；下拉选一份 ⇒ rotation-use 载入它`() =
        runTest {
            val (link, b) = board(view = { RotFixture.view(off = listOf("api")) })
            val v = b.state.value.view!!
            assertEquals(listOf("team", "work", "api", "lab"), v.lineup)
            assertEquals(listOf("api"), v.off)
            assertEquals(
                "夹具·work 的理由",
                v.decision!!
                    .why
                    .single()
                    .why,
            )
            assertEquals(listOf("夹具·work 的规则"), v.board.rows[0].rules)
            b.off("lab", true)
            assertEquals(mapOf("sid" to "s1", "op" to mapOf("off" to mapOf("account" to "lab", "off" to true))), link.calls(Rotation.TEMP).single())
            b.pick("night")
            assertEquals(mapOf("sid" to "s1", "strategy" to "night"), link.calls(Rotation.USE).single())
            assertNull(b.state.value.ask)
        }

    @Test
    fun `改格没成 ⇒ 那一格底下一句「改动保存失败 · 那台的话」；conflict ⇒ 不报、再读`() =
        runTest {
            var code = "refused"
            val (link, b) = board(sid = null, more = { cmd, _ -> if (cmd == Rotation.SETTINGS) RotFixture.failed(code, "延迟应在 0–300 分钟之间") else null })
            b.edit(RotEdit.set("wait", 1, 1), "wait@1")
            val f = b.state.value.cellFailed!!
            assertEquals("wait@1", f.cell)
            assertEquals(copyText("rot.fail.settings") + copyText("kit.text.sep") + "延迟应在 0–300 分钟之间", f.text)
            code = "conflict"
            val before = link.calls(Rotation.VIEW).size
            b.edit(RotEdit.set("smart", null, true), "smart@all")
            assertEquals(before + 1, link.calls(Rotation.VIEW).size)
            assertNull(b.state.value.failure)
        }

    @Test
    fun `临时条目：拖到最上那条 ⇒ rotation-temp place {account, at}；清掉 ⇒ op clear；第几段照那台推演的段数`() =
        runTest {
            val (link, b) = board()
            val uses =
                b.state.value.view!!
                    .timeline!!
                    .uses
            assertEquals("松在第二段里 ⇒ 排到它", 2, placeAt(uses, RotFixture.NOW, RotFixture.NOW + 6_000))
            assertEquals("松在此刻那段里 ⇒ 第一位", 1, placeAt(uses, RotFixture.NOW, RotFixture.NOW + 100))
            b.place("lab", 2)
            assertEquals(mapOf("sid" to "s1", "op" to mapOf("place" to mapOf("account" to "lab", "at" to 2))), link.calls(Rotation.TEMP).single())
            b.clearTemp()
            assertEquals(mapOf("sid" to "s1", "op" to "clear"), link.calls(Rotation.TEMP).last())
        }

    @Test
    fun `立即用：rotation-use-now；那台回 unusable ⇒ 按钮下照 rot·acct·refused 写那个号事实上的那一句`() =
        runTest {
            val (link, b) = board(more = { cmd, _ -> if (cmd == Rotation.USE_NOW) RotFixture.failed("unusable", "用不了") else null })
            b.useNow("lab")
            assertEquals(mapOf("sid" to "s1", "account" to "lab"), link.calls(Rotation.USE_NOW).single())
            assertEquals("lab" to copyText("rot.acct.refused", "account" to "lab", "why" to "5 小时用量已到上限"), b.state.value.refused)
        }

    @Test
    fun `号牌拖动 · 移出 · 加号 · 改一段：各交一个意图（怎么排 · 照谁改 · 写成谁的是那台的）`() =
        runTest {
            val (link, b) = board(sid = null)
            b.edit(RotEdit.move(2, 0))
            b.edit(RotEdit.remove(1))
            b.edit(RotEdit.add("api"))
            b.edit(RotEdit.same("caps", 0, "7d", false))
            assertEquals(
                listOf(
                    mapOf("move" to mapOf("from" to 2, "to" to 0)),
                    mapOf("remove" to mapOf("row" to 1)),
                    mapOf("add" to mapOf("account" to "api")),
                    mapOf("same" to mapOf("key" to "caps", "window" to "7d", "same" to false, "row" to 0)),
                ),
                link.calls(Rotation.SETTINGS).map { it!!["edit"] },
            )
        }

    @Test
    fun `一段一行：改第几段就带第几段；新加的那一行不带；钟点或上限写得不对 ⇒ 不交`() {
        val cap = cellOf(RotFixture.view(), 1, "caps", "5h")
        assertEquals(
            mapOf("seg" to mapOf("key" to "caps", "window" to "5h", "at" to 1, "seg" to mapOf("from" to 1200L, "to" to 1380L, "v" to 0.0), "row" to 1)),
            segEdit(cap, 1, 1, "20:00", "23:00", "0"),
        )
        assertEquals(
            mapOf("seg" to mapOf("key" to "caps", "window" to "5h", "seg" to mapOf("from" to 0L, "to" to 1440L, "v" to 50.0), "row" to 1)),
            segEdit(cap, 1, null, "0", "24:00", "50"),
        )
        assertEquals("删一段只带第几段", mapOf("seg" to mapOf("key" to "caps", "window" to "5h", "at" to 0)), RotEdit.seg("caps", null, "5h", 0, null))
        assertNull(segEdit(cap, 1, 0, "25:00", "23:00", "0"))
        assertNull(segEdit(cap, 1, 0, "20:00", "23:61", "0"))
        assertNull(segEdit(cap, 1, 0, "20:00", "23:00", ""))
        assertEquals(1230L, minutesOf("2030"))
        assertEquals(540L, minutesOf("9"))
        assertEquals(1440L, minutesOf("24:00"))
        assertNull(minutesOf("24:30"))
    }

    @Test
    fun `编辑小窗照那台给的格画：显示条件不满足的格不在（挂着的缩进）· 7 天同第一窗只出那颗钮 · 号名旁的记号`() {
        val v = Rotation.ofView(RotFixture.view())!!
        val keys = { i: Int -> paneCells(v.board.rows[i]).map { (c, indent) -> c.key + (if (indent) ">" else "") } }
        assertEquals("默认切入 · 默认切出：没有挂着的格", listOf("caps", "caps", "in", "out"), keys(0))
        assertEquals("延迟切入：延迟挂在切入下", listOf("caps", "caps", "in", "wait>", "out"), keys(1))
        assertEquals("强制切出：切出条件挂在切出下", listOf("caps", "caps", "in", "out", "outTotal>"), keys(2))
        val w7 = cellOf(RotFixture.view(), 0, "caps", "7d")
        assertEquals("夹具·同 5h", w7.same)
        assertTrue("同第一窗 ⇒ 不画段", !segsShown(w7))
        assertTrue(segsShown(cellOf(RotFixture.view(), 0, "caps", "5h")))
        assertEquals("夹具·合计达到", v.board.rows[2].cells.first { it.key == "outTotal" }.lead)
        assertEquals(emptyList<String>(), marksOf(v, "work").map { it.tip })
        assertEquals(listOf("↺", "⇥"), marksOf(v, "lab").map { it.glyph })
        assertEquals(listOf(true), marksOf(v, "team").map { it.dashed })
        assertEquals("号列表外的号没有记号", emptyList<Any>(), marksOf(v, "api"))
    }

    @Test
    fun `编辑小窗底行：改的是哪份 · 在用旧版那一句（那台写的）带［都载入新版］；没人在用写「没有会话在用」`() {
        val older = mapOf("hint" to mapOf("k" to "older", "n" to 2.0), "tone" to "plain", "text" to "夹具·2 个会话还在用旧版")
        val sep = copyText("kit.text.sep")
        val v = Rotation.ofView(RotFixture.view(hints = listOf(older)))!!
        assertEquals(copyText("rot.pane.foot", "name" to "spread") + sep + "夹具·2 个会话还在用旧版" to true, paneFoot(v))
        val idle = Rotation.ofView(RotFixture.view(strategy = "night"))!!
        assertEquals(copyText("rot.pane.foot", "name" to "night") + sep + copyText("rot.pane.noUse") to false, paneFoot(idle))
    }

    @Test
    fun `顺序里长按拖一行：松在第一格 ⇒ 现在用；别处 ⇒ rotation-temp set 拖完那一串；没挪 ⇒ 不发`() =
        runTest {
            val line = listOf("team", "work", "api", "lab")
            assertEquals(RowDrop.UseNow("api"), dropRow(line, 2, 0))
            assertEquals(RowDrop.Set(listOf("lab", "work", "api")), dropRow(line, 3, 1))
            assertEquals(RowDrop.Set(listOf("api", "work")), dropRow(line, 1, 2))
            assertNull(dropRow(line, 2, 2))
            val (link, b) = board()
            b.drop("lab", 3, 1)
            assertEquals(mapOf("sid" to "s1", "op" to mapOf("set" to listOf("lab", "work", "api"))), link.calls(Rotation.TEMP).single())
            b.drop("api", 2, 0)
            assertEquals(mapOf("sid" to "s1", "account" to "api"), link.calls(Rotation.USE_NOW).single())
        }

    @Test
    fun `设置页的下拉 ＝ 看哪一份：选一份只换看的那份（不改新会话用哪份）；选「不轮换」⇒ 新会话不轮换`() =
        runTest {
            val (link, b) = board(sid = null, view = { args -> RotFixture.view(strategy = shownIn(args)) })
            b.pick("night")
            assertEquals(mapOf("target" to mapOf("strategy" to "night"), "hours" to 12), link.calls(Rotation.VIEW).last())
            assertTrue(link.calls(Rotation.FOR_NEW).isEmpty())
            b.edit(RotEdit.set("smart", null, false))
            assertEquals("改的是看的那份", "night", link.calls(Rotation.SETTINGS).single()!!["strategy"])
            b.pick(null)
            assertEquals(mapOf("strategy" to null), link.calls(Rotation.FOR_NEW).single())
        }

    @Test
    fun `新建策略：只问名字 ⇒ rotation-strategy-add 不带原文；设置页接着看新建的那份`() =
        runTest {
            val (link, b) =
                board(
                    sid = null,
                    strategies = { RotFixture.strategies(extra = listOf("夜里")) },
                    more = { cmd, _ -> if (cmd == Rotation.ADD) Reply.Ok(mapOf("id" to "夜里")) else null },
                )
            b.create()
            val ask = b.state.value.ask as RotAsk.Name
            assertEquals(copyText("rot.ask.createTitle"), ask.title)
            assertEquals(copyText("rot.ask.createNote"), ask.note)
            assertEquals(copyText("rot.ask.createGo"), ask.action)
            b.named("夜里")
            assertEquals(mapOf("name" to "夜里"), link.calls(Rotation.ADD).single())
            assertEquals(mapOf("target" to mapOf("strategy" to "夜里"), "hours" to 12), link.calls(Rotation.VIEW).last())
        }

    @Test
    fun `会话里［编辑策略］跳过来：设置页先看那一份、开那个号的编辑小窗`() =
        runTest {
            val link = FakeCoreLink()
            link.answer = { cmd, _ ->
                when (cmd) {
                    Rotation.STRATEGIES -> Reply.Ok(RotFixture.strategies())
                    else -> Reply.Ok(RotFixture.view(strategy = "night"))
                }
            }
            val b = RotationBoard(link, null, "devbox", TestScope(UnconfinedTestDispatcher(testScheduler)), null, RotFocus("night", "lab"))
            assertEquals(mapOf("target" to mapOf("strategy" to "night"), "hours" to 12), link.calls(Rotation.VIEW).single())
            assertEquals("lab", b.state.value.open)
        }

    @Test
    fun `格照那台解好的画：每格的值 · 从哪来 · 字；分段每窗一格、段带字`() =
        runTest {
            val (_, b) = board()
            val v = b.state.value.view!!
            val rows = v.board.rows
            val team = rows[1]
            assertEquals("夹具·team 那一行", team.line)
            val wait = team.cells.first { it.key == "wait" }
            assertEquals(RotFrom.OWN to 30.0, wait.from to wait.num)
            assertEquals("in", wait.under)
            val cap = rows[0].cells.first { it.window == "5h" }
            assertEquals("caps.5h@0", cellId(cap, 0))
            assertEquals(listOf("夹具·90.0"), cap.segs!!.map { it.text })
            assertEquals(RotFrom.SAME, rows[0].cells.first { it.window == "7d" }.from)
            assertEquals("夹具·一有额度", rows[2].cells.first { it.key == "outTotal" }.nullable)
            assertEquals("夹具·智能切换的说明", v.board.all.single().help)
        }

    @Test
    fun `设时区 ⇒ rotation-strategy-tz {id, tz ＝ 提示里那台给的 zone, ifRev ＝ 策略的 rev}`() =
        runTest {
            val (link, b) = board(view = { RotFixture.view(hints = listOf(RotFixture.TZ)) })
            val n =
                b.state.value.view!!
                    .hints
                    .single()
            assertEquals(RotHintKind.TZ_UNKNOWN, n.hint.k)
            b.setTz(n.hint.zone!!)
            assertEquals(mapOf("id" to "spread", "tz" to "America/Los_Angeles", "ifRev" to "h:spread"), link.calls(Rotation.TZ).single())
        }

    @Test
    fun `提示的字是那台写好的；停发那一句的 {dur} 照回包的 now 与 until 填；颜色照那台给的语气`() =
        runTest {
            val (_, b) = board(view = { RotFixture.view(hints = listOf(RotFixture.hold(4 * 3600), RotFixture.ERROR, RotFixture.TZ)) })
            val (hold, err, tz) =
                b.state.value.view!!
                    .hints
            assertEquals(
                "已停发 · 18:30 team 恢复（还有 " +
                    com.ccmonitor.mobile.core.copy.DurationFormat
                        .short(4 * 3600 * 1000L) + "）",
                hintText(
                    hold,
                    b.state.value.view!!
                        .now,
                ),
            )
            assertEquals("策略「spread」出错：第 12 行，除数为 0", hintText(err, 0))
            assertEquals("原话", err.detail)
            assertEquals("/h/strategies/spread.rhai", err.hint.path)
            assertEquals(NoteTone.ERROR, toneOf(err.tone))
            assertEquals(NoteTone.WARN, toneOf(hold.tone))
            assertEquals(NoteTone.WARN, toneOf(tz.tone))
        }

    @Test
    fun `号与时间轴上的字照回包：号那一行 · 每段写的字`() =
        runTest {
            val (_, b) = board()
            val v = b.state.value.view!!
            assertEquals("夹具·work 那一行", v.board.rows[0].line)
            assertEquals("5 小时用量已到上限", v.accounts[2].blocked?.text)
            val segs =
                v.timeline!!
                    .lanes
                    .single()
                    .segs
            assertEquals(listOf("夹具·剩 42", "夹具·↻18:30"), segs.map { it.text })
            assertEquals(listOf("lab"), v.timeline!!.uses[1].skipped)
        }

    @Test
    fun `无法读取 ⇒ 红条写那台的 message，旧画面留着；重试读成了红条收起`() =
        runTest {
            var broken = false
            val (_, b) = board(more = { cmd, _ -> if (cmd == Rotation.VIEW && broken) RotFixture.failed("refused", "轮换记录无法读取 · 第 3 行格式不对") else null })
            broken = true
            b.refresh()
            assertEquals(
                "轮换记录无法读取 · 第 3 行格式不对",
                b.state.value.readFailed
                    ?.text,
            )
            assertEquals(
                "spread",
                b.state.value.view
                    ?.strategy
                    ?.id,
            )
            broken = false
            b.refresh()
            assertNull(b.state.value.readFailed)
        }

    @Test
    fun `流上 changed：这个会话的轮换 · 策略目录 ⇒ 再读；别的会话的不读`() =
        runTest {
            val changes = MutableSharedFlow<Changed>(extraBufferCapacity = 8)
            val (link, _) = board(changes = changes)
            val n = link.calls(Rotation.VIEW).size
            changes.emit(Changed(Rotation.TOPIC, "s2"))
            assertEquals(n, link.calls(Rotation.VIEW).size)
            changes.emit(Changed(Rotation.TOPIC, "s1"))
            changes.emit(Changed(Rotation.TOPIC_STRATEGIES, null))
            assertEquals(n + 2, link.calls(Rotation.VIEW).size)
        }

    @Test
    fun `策略操作：复制 · 改名（ifRev 照那份的 rev）· 删除先问 · 新会话用这份`() =
        runTest {
            val (link, b) = board()
            b.copy("spread")
            b.named("spread 副本")
            assertEquals(mapOf("id" to "spread", "name" to "spread 副本"), link.calls(Rotation.COPY).single())
            b.rename("spread", "h:spread")
            b.named("spread")
            assertTrue("名字没变不发", link.calls(Rotation.RENAME).isEmpty())
            b.rename("spread", "h:spread")
            b.named("均摊")
            assertEquals(mapOf("id" to "spread", "name" to "均摊", "ifRev" to "h:spread"), link.calls(Rotation.RENAME).single())
            b.remove("night")
            assertTrue(link.calls(Rotation.DELETE).isEmpty())
            b.answer(true)
            assertEquals(mapOf("id" to "night"), link.calls(Rotation.DELETE).single())
            b.forNew("night")
            assertEquals(mapOf("strategy" to "night"), link.calls(Rotation.FOR_NEW).single())
            b.add("mine.rhai", "// 说明")
            b.named("mine")
            assertEquals(mapOf("source" to "// 说明", "name" to "mine"), link.calls(Rotation.ADD).single())
        }
}
