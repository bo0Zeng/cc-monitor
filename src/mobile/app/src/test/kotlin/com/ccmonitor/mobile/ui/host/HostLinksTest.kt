package com.ccmonitor.mobile.ui.host

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 主机设置页「后端」那一行下面那一句：怎么连过去的，字照核心交的成品 `route` 原样（不拼字）；直连不经代理（没有 `route`）⇒ 不画。
 * 每条连接的读数不上这一页（在复制详情里）。
 *
 * 判别力边界：那一句几个字由核心写（`ssh_dial::pool::way_of`，那边有判据）；这里只验照摆，画在不在 `BackendRow` 里读源码。
 */
class HostLinksTest {
    private fun up(route: String): LinkLine? =
        Wire.read<LinkLine>(
            """{"state":"up","epoch":1,"machine":{"detail":null,"expectedHostKey":null,"fixes":[],"name":"deep","offline":false,""" +
                """"offlineSaid":"deep 离线（夹具）","os":null,"reason":null,"said":"","seenHostKey":null,"stage":null,"state":"up",""" +
                """"version":null,"versionRelation":"same","word":""$route}}""",
        )

    @Test
    fun `经跳板 ⇒ 一句，字照成品原样`() {
        assertEquals("经跳板 edge · 经代理（夹具）", routeLine(up(""","route":"经跳板 edge · 经代理（夹具）"""")))
    }

    @Test
    fun `直连不经代理 · 还没有那一行 ⇒ 不画`() {
        assertNull(routeLine(up("")))
        assertNull(routeLine(null))
    }

    @Test
    fun `后端那一行把那一句画出来`() {
        val code =
            KotlinSourceScanner.codeOnlyKeepingLiterals(
                (File(HOST_SCREENS).takeIf { it.isFile } ?: File("app/$HOST_SCREENS")).readText(),
            )
        val row = code.substringAfter("private fun BackendRow(").substringBefore("\n}\n")
        assertTrue("BackendRow 里没画 routeLine(link)", "routeLine(link)" in row)
        assertTrue("BackendRow 里还在画每条连接", "linkLines" !in code)
    }

    private companion object {
        const val HOST_SCREENS = "src/main/kotlin/com/ccmonitor/mobile/ui/host/HostScreens.kt"
    }
}
