package com.ccmonitor.mobile.ui.host

import com.ccmonitor.mobile.core.wire.Wire
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 主机设置页「后端」那一行下面的每跳读数：到那台的每一条连接一行，字照核心交的成品 `links[].said` 原样（不拼字），先后照成品。
 *
 * 判别力边界：一行几个字由核心写（`ssh_dial::pool::rows_of`，那边有判据）；这里只验照摆与先后，画在不在 `BackendRow` 里读源码。
 */
class HostLinksTest {
    private fun up(links: String): LinkLine? =
        Wire.read<LinkLine>(
            """{"state":"up","epoch":1,"machine":{"detail":null,"expectedHostKey":null,"fixes":[],"name":"deep","offline":false,""" +
                """"offlineSaid":"deep 离线（夹具）","os":null,"reason":null,"said":"","seenHostKey":null,"stage":null,"state":"up",""" +
                """"version":null,"versionRelation":"same","word":""$links}}""",
        )

    @Test
    fun `经跳板连着 ⇒ 两行：那台一行、跳板一行，字照成品原样`() {
        val link =
            up(
                ""","links":[""" +
                    """{"endpoint":"deep.test:22","jump":false,"compressed":true,"viaProxy":false,"bytesIn":4096,"bytesOut":300,"said":"那台那一句（夹具）"},""" +
                    """{"endpoint":"edge.test:22","jump":true,"compressed":false,"viaProxy":true,"bytesIn":9000,"bytesOut":800,"said":"跳板那一句（夹具）"}]""",
            )
        assertEquals(listOf("那台那一句（夹具）", "跳板那一句（夹具）"), linkLines(link))
    }

    @Test
    fun `成品没带连接 · 还没有那一行 ⇒ 一行都不画`() {
        assertEquals(emptyList<String>(), linkLines(up("")))
        assertEquals(emptyList<String>(), linkLines(null))
    }

    @Test
    fun `后端那一行把每跳读数画出来`() {
        val code =
            KotlinSourceScanner.codeOnlyKeepingLiterals(
                (File(HOST_SCREENS).takeIf { it.isFile } ?: File("app/$HOST_SCREENS")).readText(),
            )
        val row = code.substringAfter("private fun BackendRow(").substringBefore("\n}\n")
        assertTrue("BackendRow 里没画 linkLines(link)", "linkLines(link)" in row)
    }

    private companion object {
        const val HOST_SCREENS = "src/main/kotlin/com/ccmonitor/mobile/ui/host/HostScreens.kt"
    }
}
