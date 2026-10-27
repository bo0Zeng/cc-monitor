package com.ccmonitor.mobile.ui.plan

import com.ccmonitor.mobile.core.claude.link.PlanLook
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.ui.nav.Screen
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import java.net.URLDecoder

class PlanScreensTest {
    @Test
    fun lookPicksTheIconOnly() {
        assertEquals(Ph.Check, lookIcon(PlanLook.DONE))
        assertEquals(Ph.Circle, lookIcon(PlanLook.OPEN))
        assertEquals(Ph.WarningCircle, lookIcon(PlanLook.UNDECIDED))
        assertEquals(Ph.Circle, lookIcon(PlanLook.NONE))
    }

    /** 路由的查询串照 URL 编码：工作区路径带 `/` 与空格、片名是中文，来回不丢；没给的会话不写。 */
    @Test
    fun routesKeepPathsAndNamesIntact() {
        val r = Screen.PlanSlice.of("h1", "/srv/a b/ws", "账本", null)
        assertFalse(r.contains("sid="))
        val q =
            r.substringAfter('?').split('&').associate {
                val (k, v) = it.split('=', limit = 2)
                k to URLDecoder.decode(v, "UTF-8")
            }
        assertEquals(mapOf("ws" to "/srv/a b/ws", "slice" to "账本"), q)
        assertEquals("plan-slice/h1", r.substringBefore('?'))
        assertEquals("plan/h1", Screen.Plan.of("h1", null))
        assertEquals("plan/h1?sid=s%201", Screen.Plan.of("h1", "s 1"))
        assertEquals("plan-cell/h1?ws=%2Fw&slice=x&id=A1-2", Screen.PlanCell.of("h1", "/w", "x", "A1-2"))
    }
}
