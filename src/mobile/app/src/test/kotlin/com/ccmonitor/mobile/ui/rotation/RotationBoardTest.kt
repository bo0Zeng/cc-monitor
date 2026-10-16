package com.ccmonitor.mobile.ui.rotation

import com.ccmonitor.mobile.core.claude.link.Changed
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.Rotation
import com.ccmonitor.mobile.core.claude.link.wire.RotHintKind
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.test.TestScope
import kotlinx.coroutines.test.UnconfinedTestDispatcher
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
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
    fun `设置页：看新会话用的那份 {strategy}；改一格只成手上的草稿、随读交（带它照哪版改的），不发 rotation-settings；存交草稿`() =
        runTest {
            val (link, b) = board(sid = null, strategies = { RotFixture.strategies(forNew = "night") }, view = { RotFixture.view(strategy = "night", baseRev = "h:night") })
            assertEquals(mapOf("target" to mapOf("strategy" to "night"), "hours" to 12), link.calls(Rotation.VIEW).single())
            val next = mapOf("tol" to 30.0)
            b.settings(next)
            assertTrue(link.calls(Rotation.SETTINGS).isEmpty())
            assertEquals(mapOf("target" to mapOf("strategy" to "night", "settings" to next, "rev" to "h:night"), "hours" to 12), link.calls(Rotation.VIEW).last())
            b.save()
            assertEquals(mapOf("target" to mapOf("strategy" to "night"), "ifRev" to "h:night", "settings" to next), link.calls(Rotation.SAVE).single())
            assertEquals("存完草稿清掉：再读不带 settings", mapOf("target" to mapOf("strategy" to "night"), "hours" to 12), link.calls(Rotation.VIEW).last())
            b.pick("spread")
            assertEquals(mapOf("strategy" to "spread"), link.calls(Rotation.FOR_NEW).single())
            assertTrue(link.calls(Rotation.USE).isEmpty())
        }

    @Test
    fun `会话里改一格 ⇒ rotation-settings {target sid, 整份设置, ifRev ＝ baseRev}；存 ⇒ rotation-save ifRev 照回的；还原 ⇒ rotation-revert`() =
        runTest {
            val (link, b) = board(view = { RotFixture.view(dirty = true, baseRev = "h:base7") })
            val next =
                put(
                    b.state.value.view!!
                        .settings,
                    b.state.value.view!!
                        .form
                        .first { it.key == "tol" },
                    null,
                    30.0,
                )
            b.settings(next, "tol")
            val sent = link.calls(Rotation.SETTINGS).single()!!
            assertEquals(mapOf("sid" to "s1"), sent["target"])
            assertEquals("h:base7", sent["ifRev"])
            assertEquals(30.0, (sent["settings"] as Map<*, *>)["tol"])
            assertEquals("别的格原样带上", 3, rowsOf(sent["settings"]).size)
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
            b.settings(mapOf("x" to 1), "delay@team")
            val f = b.state.value.cellFailed!!
            assertEquals("delay@team", f.cell)
            assertEquals(copyText("rot.fail.settings") + copyText("kit.text.sep") + "启用延迟应在 0–600 分钟之间", f.text)
            code = "conflict"
            val before = link.calls(Rotation.VIEW).size
            b.settings(mapOf("x" to 2), "tol")
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
    fun `号牌拖动 · 移出 · 加号：只动 settings·accounts 的先后，别的原样`() =
        runTest {
            val (_, b) = board()
            val s =
                b.state.value.view!!
                    .settings
            assertEquals(listOf("lab", "work", "team"), rowsOf(moved(s, 2, 0)).map { it["id"] })
            assertEquals(listOf("team", "work", "lab"), rowsOf(moved(s, 0, 2)).map { it["id"] })
            assertNull(moved(s, 1, 1))
            assertEquals(listOf("work", "lab"), rowsOf(removed(s, 1)).map { it["id"] })
            assertEquals(listOf("work", "team", "lab", "api"), rowsOf(added(s, "api")).map { it["id"] })
            assertEquals(20.0, added(s, "api")["tol"])
        }

    @Test
    fun `格的值照声明取：号另设的 · 跟总体（虚线）· 默认；可空的放 null 不是删`() =
        runTest {
            val (_, b) = board()
            val v = b.state.value.view!!
            val delay = v.form.first { it.key == "delay" }
            assertEquals(CellValue(30.0, false), valueOf(v.settings, delay, 1))
            assertEquals("work 没另设、inherit ⇒ 跟总体（总体没写 ⇒ 声明的默认）", CellValue(0.0, true), valueOf(v.settings, delay, 0))
            val tol = v.form.first { it.key == "tol" }
            assertEquals(CellValue(20.0, false), valueOf(v.settings, tol, null))
            assertTrue(put(v.settings, tol, null, null).containsKey("tol"))
            assertFalse(rowsOf(put(v.settings, delay, 1, null, unset = true))[1].containsKey("delay"))
            val cap = v.form.first { it.key == "cap" }
            assertEquals(90.0, dayOf(v.settings, cap, null, "5h").segs.single().v, 0.0)
            val own = v.form.first { it.key == "cap" && it.scope == com.ccmonitor.mobile.core.claude.link.wire.RotFormScope.EACH }
            val follows = dayOf(v.settings, own, 0, "5h")
            assertTrue("号没另设这一窗 ⇒ 跟总体", follows.follows)
            assertEquals("跟总体 ⇒ 画的是总体那一组", 90.0, follows.segs.single().v, 0.0)
        }

    @Test
    fun `24 小时条刷一段 ⇒ rotation-day-paint {segs, from, to, v}，拿回那台刷完的那一组`() =
        runTest {
            val (link, b) = board(more = { cmd, _ -> if (cmd == Rotation.DAY_PAINT) Reply.Ok(mapOf("segs" to listOf(mapOf("from" to 0.0, "to" to 1200.0, "v" to 90.0), mapOf("from" to 1200.0, "to" to 1380.0, "v" to 0.0)))) else null })
            val v = b.state.value.view!!
            val segs = dayOf(v.settings, v.form.first { it.key == "cap" }, null, "5h").segs
            val got = b.dayPaint(segs, 1200, 1380, 0.0)!!
            assertEquals(mapOf("segs" to listOf(mapOf("from" to 0L, "to" to 1440L, "v" to 90.0)), "from" to 1200L, "to" to 1380L, "v" to 0.0), link.calls(Rotation.DAY_PAINT).single())
            assertEquals(listOf(0L to 1200L, 1200L to 1380L), got.map { it.from to it.to })
            assertEquals(1200L, minuteAt(1200f / 1440f))
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
    fun `提示的字是那台写好的；停发那一句的 {dur} 照回包的 now 与 until 填；颜色照 k`() =
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
            assertEquals(NoteTone.ERROR, toneOf(err.hint.k))
            assertEquals(NoteTone.WARN, toneOf(hold.hint.k))
            assertTrue(needsHand(tz.hint.k))
        }

    @Test
    fun `号与时间轴上的字：用量照回的数照模板排，钟面照回的 …Text，此刻那段剩多少照 left`() =
        runTest {
            val (_, b) = board()
            val v = b.state.value.view!!
            assertEquals(copyText("rot.acct.line", "u5" to 80L, "u7" to 10L) + copyText("kit.text.sep") + copyText("rot.acct.reset", "at" to "16:07"), acctLine(v.accounts[0]))
            assertEquals("5 小时用量已到上限", v.accounts[2].blocked?.text)
            val segs =
                v.timeline!!
                    .lanes
                    .single()
                    .segs
            assertEquals(copyText("rot.sim.left", "n" to 42L), laneText(segs[0], first = true))
            assertEquals(copyText("rot.sim.reset", "at" to "18:30"), laneText(segs[1], first = false))
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
