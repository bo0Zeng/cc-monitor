package com.ccmonitor.mobile.ssh

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertSame
import org.junit.Assert.assertTrue
import org.junit.Test

/** OS 命令方言 seam（POSIX vs PowerShell 的 quote/cd/tmux 支持）——纯函数 JVM 测。 */
class ShellDialectTest {
    @Test
    fun hostOsMapping() {
        assertEquals(HostOs.POSIX, HostOs.of(null))
        assertEquals(HostOs.POSIX, HostOs.of("posix"))
        assertEquals(HostOs.POSIX, HostOs.of("linux")) // 未知 → 默认 POSIX
        assertEquals(HostOs.WINDOWS, HostOs.of("windows"))
        assertEquals(HostOs.WINDOWS, HostOs.of("Windows")) // 忽略大小写
    }

    // 能力位——POSIX 支持 tmux，PowerShell 不支持。UI 门控据此（阅读子页读那台核心，不看 shell）。
    @Test
    fun capabilityBits() {
        assertTrue("POSIX 支持 tmux", PosixDialect.supportsTmux)
        assertFalse("PowerShell 无 tmux", PowerShellDialect.supportsTmux)
        // 门控判据经 dialectFor：POSIX 主机全显、Windows 主机全隐。
        assertTrue(dialectFor(null).supportsTmux && dialectFor("linux").supportsTmux)
        assertFalse(dialectFor("windows").supportsTmux)
    }

    @Test
    fun posixDialect() {
        assertTrue("POSIX 有 tmux", PosixDialect.supportsTmux)
        assertEquals("POSIX 单引号（内嵌 ' → '\\''）", """'it'\''s'""", PosixDialect.quote("it's"))
        assertEquals("cd '/a b'", PosixDialect.cd("/a b"))
    }

    @Test
    fun powershellDialect() {
        assertFalse("Windows 无 tmux", PowerShellDialect.supportsTmux)
        assertEquals("PS 单引号（内嵌 ' → ''）", "'it''s'", PowerShellDialect.quote("it's"))
        assertEquals("PS cd 接单引号路径", """cd 'C:\Users'""", PowerShellDialect.cd("""C:\Users"""))
    }

    @Test
    fun dialectForByOs() {
        assertSame(PosixDialect, dialectFor(null))
        assertSame(PosixDialect, dialectFor("posix"))
        assertSame(PowerShellDialect, dialectFor("windows"))
    }

    @Test
    fun isWindowsHelper() {
        assertTrue(isWindows("windows"))
        assertTrue(isWindows("Windows"))
        assertFalse(isWindows(null))
        assertFalse(isWindows("posix"))
        assertFalse(isWindows("linux"))
    }
}
