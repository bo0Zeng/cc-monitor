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
 * 轮换屏的接线（桌面 `rot-mount.ts` 同一套）：每一格读的是那台回包的哪一格（手机不算）· 改的那几格交哪条命令、`ifRev` 照回的带 ·
 * 各种情形的那一句来自那台。
 */
@OptIn(ExperimentalCoroutinesApi::class)
class RotationBoardTest {
    private fun TestScope.board(
        sid: String? = "s1",
        view: () -> Map<String, Any?> = { RotFixture.view() },
        strategies: () -> Map<String, Any?> = { RotFixture.strategies() },
        more: (String, Map<String, Any?>?) -> Reply? = { _, _ -> null },
        changes: MutableSharedFlow<Changed>? = null,
    ): Pair<FakeCoreLink, RotationBoard> {
        val link = FakeCoreLink()
        link.answer = { cmd, args ->
            more(cmd, args) ?: when (cmd) {
                Rotation.STRATEGIES -> Reply.Ok(strategies())
                Rotation.VIEW -> Reply.Ok(view())
                "accounts-list" -> Reply.Ok(mapOf("accounts" to listOf(mapOf("name" to "work", "label" to "Work", "color" to 2.0, "configDir" to "/c", "mode" to "isolated", "selectable" to true)), "meta" to emptyMap<String, Any?>()))
                else -> Reply.Ok(emptyMap<String, Any?>())
            }
        }
        val b = RotationBoard(link, sid, "devbox", TestScope(UnconfinedTestDispatcher(testScheduler)), changes)
        return link to b
    }

    @Test
    fun `会话：读 rotation-view {sid} 推 12 小时；号牌名字照账号库的 label；四块画的全是回包里的那一份`() =
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
            assertEquals(listOf("spread", "night"), s.list?.strategies?.map { it.id })
        }

    @Test
    fun `设置页：看新会话用的那份 {strategy}；改一格交意图、那台回改完的那份当草稿存着、随读交（带它照哪版改的）；存交草稿`() =
        runTest {
            val next = mapOf("tol" to 30.0)
            val (link, b) =
                board(
                    sid = null,
                    strategies = { RotFixture.strategies(forNew = "night") },
                    view = { RotFixture.view(strategy = "night", baseRev = "h:night") },
                    more = { cmd, _ -> if (cmd == Rotation.SETTINGS) Reply.Ok(mapOf("dirty" to true, "settings" to next)) else null },
                )
            assertEquals(mapOf("target" to mapOf("strategy" to "night"), "hours" to 12), link.calls(Rotation.VIEW).single())
            b.edit(RotEdit.set("tol", null, 30.0))
            assertEquals(mapOf("target" to mapOf("strategy" to "night"), "edit" to mapOf("set" to mapOf("key" to "tol", "v" to 30.0))), link.calls(Rotation.SETTINGS).single())
            b.edit(RotEdit.KEEP)
            assertEquals("下一笔照上一次回的那份改", next, link.calls(Rotation.SETTINGS).last()!!["settings"])
            assertEquals(mapOf("target" to mapOf("strategy" to "night", "settings" to next, "rev" to "h:night"), "hours" to 12), link.calls(Rotation.VIEW).last())
            b.save()
            assertEquals(mapOf("target" to mapOf("strategy" to "night"), "ifRev" to "h:night", "settings" to next), link.calls(Rotation.SAVE).single())
            assertEquals("存完草稿清掉：再读不带 settings", mapOf("target" to mapOf("strategy" to "night"), "hours" to 12), link.calls(Rotation.VIEW).last())
            b.pick("spread")
            assertEquals(mapOf("strategy" to "spread"), link.calls(Rotation.FOR_NEW).single())
            assertTrue(link.calls(Rotation.USE).isEmpty())
        }

    @Test
    fun `会话里改一格 ⇒ rotation-settings {target sid, 那个意图, ifRev ＝ baseRev}；存 ⇒ rotation-save ifRev 照回的；还原 ⇒ rotation-revert`() =
        runTest {
            val (link, b) = board(view = { RotFixture.view(dirty = true, baseRev = "h:base7") })
            b.edit(RotEdit.set("tol", null, 30.0), "tol@all")
            assertEquals(mapOf("target" to mapOf("sid" to "s1"), "edit" to mapOf("set" to mapOf("key" to "tol", "v" to 30.0)), "ifRev" to "h:base7"), link.calls(Rotation.SETTINGS).single())
            b.save()
            assertEquals(mapOf("target" to mapOf("sid" to "s1"), "ifRev" to "h:base7"), link.calls(Rotation.SAVE).single())
            b.revert()
            assertEquals(mapOf("sid" to "s1"), link.calls(Rotation.REVERT).single())
            assertEquals("每次写完照回包再读", 4, link.calls(Rotation.VIEW).size)
        }

    @Test
    fun `改格没成 ⇒ 那一格底下一句「改动保存失败 · 那台的话」；conflict ⇒ 不报、再读`() =
        runTest {
            var code = "refused"
            val (link, b) = board(more = { cmd, _ -> if (cmd == Rotation.SETTINGS) RotFixture.failed(code, "启用延迟应在 0–600 分钟之间") else null })
            b.edit(RotEdit.set("delay", 1, 1), "delay@team")
            val f = b.state.value.cellFailed!!
            assertEquals("delay@team", f.cell)
            assertEquals(copyText("rot.fail.settings") + copyText("kit.text.sep") + "启用延迟应在 0–600 分钟之间", f.text)
            code = "conflict"
            val before = link.calls(Rotation.VIEW).size
            b.edit(RotEdit.set("tol", null, 2), "tol@all")
            assertEquals(before + 1, link.calls(Rotation.VIEW).size)
            assertNull(b.state.value.failure)
        }

    @Test
    fun `换策略：rotation-use discard false；那台回 dirty ⇒ 先问；点头 ⇒ 再发 discard true；摇头 ⇒ 不发`() =
        runTest {
            var dirty = true
            val (link, b) = board(more = { cmd, args -> if (cmd == Rotation.USE && dirty && args!!["discard"] == false) RotFixture.failed("dirty", "有没存的设置") else null })
            b.pick("night")
            assertEquals(mapOf("sid" to "s1", "strategy" to "night", "discard" to false), link.calls(Rotation.USE).single())
            assertEquals(RotAsk.Discard("night"), b.state.value.ask)
            b.answer(false)
            assertEquals(1, link.calls(Rotation.USE).size)
            b.pick(null)
            b.answer(true)
            assertEquals(mapOf("sid" to "s1", "strategy" to null, "discard" to true), link.calls(Rotation.USE).last())
            dirty = false
            b.pick("spread")
            assertNull(b.state.value.ask)
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
    fun `号牌拖动 · 移出 · 加号 · 刷段：各交一个意图（怎么排 · 照谁刷 · 写成谁的是那台的）`() =
        runTest {
            val (link, b) = board()
            b.edit(RotEdit.move(2, 0))
            b.edit(RotEdit.remove(1))
            b.edit(RotEdit.add("api"))
            b.edit(RotEdit.day("cap", 0, "5h", 1200, 1380, 0.0), "cap.5h@0")
            assertEquals(
                listOf(
                    mapOf("move" to mapOf("from" to 2, "to" to 0)),
                    mapOf("remove" to mapOf("row" to 1)),
                    mapOf("add" to mapOf("account" to "api")),
                    mapOf("day" to mapOf("key" to "cap", "window" to "5h", "from" to 1200L, "to" to 1380L, "v" to 0.0, "row" to 0)),
                ),
                link.calls(Rotation.SETTINGS).map { it!!["edit"] },
            )
            assertEquals(1200L, minuteAt(1200f / 1440f))
        }

    @Test
    fun `格照那台解好的画：每格的值 · 从哪来（跟总体 ⇒ 虚线）· 字；24 小时条每窗一格、段带字`() =
        runTest {
            val (_, b) = board()
            val v = b.state.value.view!!
            val rows = v.board.rows
            val all = v.board.all
            val team = rows[1]
            assertEquals("夹具·team 那一行", team.line)
            val delay = team.cells.first { it.key == "delay" }
            assertEquals(RotFrom.OWN to 30.0, delay.from to delay.num)
            assertEquals(RotFrom.ALL, rows[0].cells.first { it.key == "delay" }.from)
            val cap = rows[0].cells.first { it.window == "5h" }
            assertEquals("cap.5h@0", cellId(cap, 0))
            assertEquals(listOf("夹具·90.0"), cap.segs!!.map { it.text })
            assertEquals("不强制切换", all.first { it.key == "tol" }.nullable)
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
