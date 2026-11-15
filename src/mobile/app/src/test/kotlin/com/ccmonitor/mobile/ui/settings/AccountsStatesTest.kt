package com.ccmonitor.mobile.ui.settings

import com.ccmonitor.mobile.core.claude.link.Login
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.AccountRow
import com.ccmonitor.mobile.core.wire.gen.Failed
import com.ccmonitor.mobile.core.wire.gen.Tone
import com.ccmonitor.mobile.link.Problem
import com.ccmonitor.mobile.ui.Wires
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import com.ccmonitor.mobile.ui.chat.linkDown
import com.ccmonitor.mobile.ui.chat.linkUp
import kotlinx.coroutines.test.runTest
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
        assertEquals(Tone.FAIL, bar.tone)
        assertEquals(copyText("front.act.retry"), bar.fix?.label)
        assertEquals("d-bad", bar.detail)
        assertFalse(bad.loading)
    }

    @Test
    fun `号名那一行照摆那台写好的「MCP 登录不同步」那一句；缺 ⇒ 不出`() {
        fun row(note: String?) = Wire.bind<AccountRow>(Wires.accountRow("name" to "f", "label" to "f", "mcpLogins" to note))!!
        assertEquals(listOf("后端·不同步"), accountLineOneNotes(row("后端·不同步")))
        assertTrue(accountLineOneNotes(row(null)).isEmpty())
    }

    @Test
    fun `退出 · 状态问那台一次：只交号名（家由那台照号表认）；成了照摆那台写好的那一句，没成 ⇒ 那台那一句`() =
        runTest {
            val l = FakeCoreLink()
            l.answer = { cmd, _ ->
                if (cmd == Login.STATUS) Reply.Ok(mapOf("said" to "后端·x1 未登录")) else Reply.Ok(mapOf("said" to "后端·x1 已退出登录"))
            }
            assertEquals("后端·x1 未登录", loginOnce(l, "smokebox", Login.STATUS, "x1"))
            assertEquals("后端·x1 已退出登录", loginOnce(l, "smokebox", Login.OUT, "x1"))
            assertEquals(listOf(mapOf("account" to "x1")), l.calls(Login.STATUS))
            assertEquals(listOf(mapOf("account" to "x1")), l.calls(Login.OUT))
            l.answer = { _, _ -> Reply.Failed(Failed("failed", "后端·退出登录未完成", "d")) }
            val said = loginOnce(l, "smokebox", Login.OUT, "x1")
            assertTrue(said, said.contains("后端·退出登录未完成"))
        }
}
