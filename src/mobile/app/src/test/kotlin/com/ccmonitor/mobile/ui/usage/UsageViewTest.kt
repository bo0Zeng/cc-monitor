package com.ccmonitor.mobile.ui.usage

import com.ccmonitor.mobile.core.claude.link.QuotaRead
import com.ccmonitor.mobile.core.claude.link.wire.AccountRow
import com.ccmonitor.mobile.core.claude.link.wire.Badge
import com.ccmonitor.mobile.core.claude.link.wire.QuotaReadReply
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** 用量屏「回包 ⇒ 要画的那一份」：次序 · 条 · 卡头 · 结论全照回包，手机只按号名对上账号库那一行。 */
class UsageViewTest {
    private fun read(map: Map<String, Any?>): QuotaReadReply = QuotaRead.of(map) ?: error("金样解不开")

    private fun row(
        name: String,
        plate: String,
        color: Int,
    ) = AccountRow(name, name, color, plate, null, null, true, false, "/a/$name", "isolated", "subscription", Badge("已登录", false))

    /** 金样里出过数的那一号复制成 [names] 那几号（照给的先后），没出过数的照留。 */
    @Suppress("UNCHECKED_CAST")
    private fun renamed(vararg names: String): Map<String, Any?> {
        val r = QuotaGolden.reply("没出过数")
        val one = (r["accounts"] as List<Map<String, Any?>>).single()
        return r + ("accounts" to names.map { one + ("account" to it) })
    }

    @Test
    fun `卡的次序照回包：出过数的照先后，没出过数的接在后面`() {
        val ui = usageView(read(renamed("zeta", "alpha")), emptyList(), null)
        assertEquals(listOf("zeta", "alpha", "team", "api"), ui.cards.map { it.account })
    }

    @Test
    fun `条：长度照 pct 封顶 · 数旧画斜纹 · 按量号没有条、有用量那一格`() {
        val full = usageView(read(QuotaGolden.reply("订阅 · 超额在兜")), emptyList(), null).cards.single()
        assertEquals(listOf(1f, 0.78f), full.bars.map { it.fill })
        assertEquals(listOf("5h", "7d"), full.bars.map { it.label })
        val stale = usageView(read(QuotaGolden.reply("订阅 · 上一窗已过")), emptyList(), null).cards.single()
        assertTrue(stale.bars.all { it.stale })
        assertEquals(listOf("采样", "09:02 · 旧"), stale.seen?.map { it.text })
        val api = usageView(read(QuotaGolden.reply("按量")), emptyList(), null).cards.first()
        assertTrue(api.bars.isEmpty())
        assertEquals("按量", api.usage?.text)
    }

    @Test
    fun `卡头：字照第一行 · 号牌按号名对上账号库 · 只有默认号带默认`() {
        val ui = usageView(read(renamed("zeta", "alpha")), listOf(row("alpha", "AL", 3), row("zeta", "ZE", 5)), "alpha")
        val (z, a) = ui.cards
        assertEquals(("ZE" to 5) to ("AL" to 3), (z.plate to z.color) to (a.plate to a.color))
        assertEquals(listOf(false, true), listOf(z.isDefault, a.isDefault))
        assertEquals("卡头照那台写的那一行（夹具只换了号名）", listOf("personal", "订阅"), z.head.map { it.text })
        assertEquals(listOf("team", "订阅", "需登录"), ui.cards[2].head.map { it.text })
        assertNull("账号库里没有的号不画号牌", ui.cards[2].plate)
    }

    @Test
    fun `点开那一块是第一行之后的全部行（含更细的窗口）`() {
        val c = usageView(read(QuotaGolden.reply("订阅 · 更细的窗口")), emptyList(), null).cards.single()
        assertEquals(listOf("5h", "7d", "7d opus", "超额", "采样"), c.detail.map { it.first().text })
        assertEquals(
            "1% ≈ 2.1M",
            c.bars
                .first()
                .slot.perPctText,
        )
    }

    @Test
    fun `没记过账：没有结论两行，只画那台那一句；读不出带复制详情`() {
        val absent = usageView(read(QuotaGolden.reply("还没看到过任何回包")), emptyList(), null)
        assertNull(absent.verdict)
        assertEquals("无采样", absent.text)
        assertNull(absent.failed)
        val bad = usageView(read(QuotaGolden.reply("额度账读不出来")), emptyList(), null)
        assertEquals(QuotaGolden.reply("额度账读不出来")["text"], bad.text)
        assertTrue(bad.failed != null)
        val ok = usageView(read(QuotaGolden.reply("订阅 · 正常")), emptyList(), null)
        assertEquals("可发的号 1" to null, ok.verdict?.text to ok.text)
    }
}
