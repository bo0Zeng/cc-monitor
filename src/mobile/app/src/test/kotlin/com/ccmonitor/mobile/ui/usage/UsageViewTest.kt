package com.ccmonitor.mobile.ui.usage

import com.ccmonitor.mobile.core.claude.link.QuotaRead
import com.ccmonitor.mobile.core.wire.gen.AccountRow
import com.ccmonitor.mobile.core.wire.gen.Badge
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import com.ccmonitor.mobile.core.wire.gen.QuotaRead as QuotaReadReply

/** 用量屏「回包 ⇒ 要画的那一份」：次序 · 条 · 卡头 · 结论全照回包，手机只按号名对上账号库那一行。 */
class UsageViewTest {
    private fun read(map: Map<String, Any?>): QuotaReadReply = QuotaRead.of(map) ?: error("金样解不开")

    private fun view(
        reply: QuotaReadReply,
        accounts: List<AccountRow> = emptyList(),
        defaultAccount: String? = null,
    ): UsageUi = usageView(reply, accounts, defaultAccount) ?: error("金样被判成契约对不上")

    private fun row(
        name: String,
        plate: String,
        color: Int,
    ) = AccountRow(
        keyMasked = null,
        endpointHost = null,
        badge = Badge("已登录", false),
        plate = plate,
        kindLine = "",
        slots = emptyList(),
        unselectable = null,
        login = false,
        newSession = true,
        name = name,
        label = name,
        email = "",
        configDir = null,
        configDirShown = null,
        isDefault = false,
        mode = "subscription",
        exists = true,
        loggedIn = true,
        authKind = "subscription",
        authReady = true,
        selectable = true,
        color = color,
    )

    /** 金样里出过数的那一号复制成 [names] 那几号（照给的先后），没出过数的照留。 */
    @Suppress("UNCHECKED_CAST")
    private fun renamed(vararg names: String): Map<String, Any?> {
        val r = QuotaGolden.reply("没出过数")
        val one = (r["accounts"] as List<Map<String, Any?>>).single()
        return r + ("accounts" to names.map { one + ("account" to it) })
    }

    @Test
    fun `卡的次序照回包：出过数的照先后，没出过数的接在后面`() {
        val ui = view(read(renamed("zeta", "alpha")))
        assertEquals(listOf("zeta", "alpha", "team", "api"), ui.cards.map { it.account })
    }

    @Test
    fun `条：长度照 pct 封顶 · 数旧画斜纹 · 按量号没有条、有用量那一格`() {
        val full = view(read(QuotaGolden.reply("订阅 · 超额在兜"))).cards.single()
        assertEquals(listOf(1f, 0.78f), full.bars.map { it.fill })
        assertEquals(listOf("5h", "7d"), full.bars.map { it.label })
        val stale = view(read(QuotaGolden.reply("订阅 · 上一窗已过"))).cards.single()
        assertTrue(stale.bars.all { it.stale })
        assertEquals(listOf("采样", "09:02 · 旧"), stale.seen?.map { it.text })
        val api = view(read(QuotaGolden.reply("按量"))).cards.first()
        assertTrue(api.bars.isEmpty())
        assertEquals("按量", api.usage?.text)
    }

    /** 金样「订阅 · 正常」那一号的 5h 那一格摘掉 pct（那台说不出百分比）。 */
    @Suppress("UNCHECKED_CAST")
    private fun noPct(): Map<String, Any?> {
        val r = QuotaGolden.reply("订阅 · 正常")
        val one = (r["accounts"] as List<Map<String, Any?>>).single()
        val slots = (one["slots"] as List<Map<String, Any?>>).map { if (it["slot"] == "5h") it - "pct" else it }
        return r + ("accounts" to listOf(one + ("slots" to slots)))
    }

    @Test
    fun `条：pct 缺 ⇒ 不出条长（不当 0），右边那一格照回包`() {
        val c = view(read(noPct())).cards.single()
        assertEquals(listOf(null, 0.41f), c.bars.map { it.fill })
        assertEquals(listOf("63%", "41%"), c.bars.map { it.slot.text })
    }

    @Test
    fun `位名：回包的 names 里没有那个语义位 ⇒ 契约对不上，不拿码当名字`() {
        @Suppress("UNCHECKED_CAST")
        val r = QuotaGolden.reply("订阅 · 正常").let { it + ("names" to ((it["names"] as Map<String, Any?>) + ("slots" to mapOf("7d" to "7d")))) }
        assertNull(usageView(read(r), emptyList(), null))
    }

    @Test
    fun `卡头：字照第一行 · 号牌按号名对上账号库 · 只有默认号带默认`() {
        val ui = view(read(renamed("zeta", "alpha")), listOf(row("alpha", "AL", 3), row("zeta", "ZE", 5)), "alpha")
        val (z, a) = ui.cards
        assertEquals(("ZE" to 5) to ("AL" to 3), (z.plate to z.color) to (a.plate to a.color))
        assertEquals(listOf(false, true), listOf(z.isDefault, a.isDefault))
        assertEquals("卡头照那台写的那一行（夹具只换了号名）", listOf("personal", "订阅"), z.head.map { it.text })
        assertEquals(listOf("team", "订阅", "需登录"), ui.cards[2].head.map { it.text })
        assertNull("账号库里没有的号不画号牌", ui.cards[2].plate)
    }

    @Test
    fun `点开那一块是第一行之后的全部行（含更细的窗口）`() {
        val c = view(read(QuotaGolden.reply("订阅 · 更细的窗口"))).cards.single()
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
        val absent = view(read(QuotaGolden.reply("还没看到过任何回包")))
        assertNull(absent.verdict)
        assertEquals("无采样", absent.text)
        assertNull(absent.failed)
        val bad = view(read(QuotaGolden.reply("额度账读不出来")))
        assertEquals(QuotaGolden.reply("额度账读不出来")["text"], bad.text)
        assertTrue(bad.failed != null)
        val ok = view(read(QuotaGolden.reply("订阅 · 正常")))
        assertEquals("可发的号 1" to null, ok.verdict?.text to ok.text)
    }
}
