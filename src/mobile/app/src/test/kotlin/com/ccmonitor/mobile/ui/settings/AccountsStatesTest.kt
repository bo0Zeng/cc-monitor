package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.ui.chat.linkDown
import com.ccmonitor.mobile.ui.chat.linkUp
import com.ccmonitor.mobile.ui.common.BarTone
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 账号屏照用量屏分几种结局：读中 ⇒ 骨架；那台离线 ⇒ 机器条（各屏同一条）＋ 手上那一份（上次所知、用量格不出数），没有那一份 ⇒ 只机器条；
 * 读不出 ⇒ 红条 ＋［重试］［复制详情］。不再只有一行红字。夹具只造结构（号名 · 占位句子）。
 */
class AccountsStatesTest {
    @Test
    fun `还没答过 ⇒ 读中（骨架）；答了 ⇒ 不读中`() {
        assertTrue(accountsLook(MachineSettingsUi(), linkUp()).loading)
        assertFalse(accountsLook(MachineSettingsUi(accountsRead = true), linkUp()).loading)
    }

    @Test
    fun `那台离线 ⇒ 不读中、不出红条，行照手上那一份、标上次所知；手上没有 ⇒ 没有行`() {
        val off = accountsLook(MachineSettingsUi(accountsFailed = Problem("s-off", "")), linkDown("s-off"))
        assertFalse(off.loading)
        assertNull("离线那一句在机器条上，不另出红字", off.failed)
        assertTrue(off.stale)
        assertTrue(off.rows.isEmpty())
    }

    @Test
    fun `读不出 ⇒ 红条：那一句 ＋［重试］＋ 复制详情`() {
        val bad = accountsLook(MachineSettingsUi(accountsFailed = Problem("s-bad", "d-bad")), linkUp())
        val bar = bad.failed!!
        assertEquals("s-bad", bar.text)
        assertEquals(BarTone.FAULT, bar.tone)
        assertEquals(copyText("front.act.retry"), bar.fix?.label)
        assertEquals("d-bad", bar.detail)
        assertFalse(bad.loading)
    }
}
