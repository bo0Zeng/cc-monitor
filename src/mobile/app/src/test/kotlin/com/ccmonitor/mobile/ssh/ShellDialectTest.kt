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

    // 能力位——POSIX 支持 tmux + Claude 阅读（tail -F 工具链），PowerShell 都不支持。UI 门控据此。
    @Test
    fun capabilityBits() {
        assertTrue("POSIX 支持 tmux", PosixDialect.supportsTmux)
        assertTrue("POSIX 支持 Claude 阅读", PosixDialect.supportsClaudeReading)
        assertFalse("PowerShell 无 tmux", PowerShellDialect.supportsTmux)
        assertFalse("PowerShell 无 Claude 阅读", PowerShellDialect.supportsClaudeReading)
        // 门控判据经 dialectFor：POSIX 主机全显、Windows 主机全隐。
        assertTrue(dialectFor(null).supportsTmux && dialectFor("linux").supportsClaudeReading)
        assertFalse(dialectFor("windows").supportsTmux || dialectFor("windows").supportsClaudeReading)
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

    // SFTP 路径 → shell cd 路径归一。
    @Test
    fun posixSftpCwdIdentity() {
        // POSIX：SFTP 路径即 shell 路径，恒原样。
        assertEquals("/home/me/proj", PosixDialect.sftpCwdToShell("/home/me/proj"))
        assertEquals("/", PosixDialect.sftpCwdToShell("/"))
        assertEquals(".", PosixDialect.sftpCwdToShell("."))
        assertEquals("/C:/x", PosixDialect.sftpCwdToShell("/C:/x")) // POSIX 不动盘符形
    }

    @Test
    fun windowsSftpCwdStripsDriveLeadingSlash() {
        // Windows：sshj SFTP 返 /C:/Users/... → 去盘符前导 / → C:/Users/...（cd 'C:/...' 对）。
        assertEquals("C:/Users/me", PowerShellDialect.sftpCwdToShell("/C:/Users/me"))
        assertEquals("D:/data", PowerShellDialect.sftpCwdToShell("/D:/data"))
        assertEquals("c:/lower", PowerShellDialect.sftpCwdToShell("/c:/lower")) // 小写盘符
        // 盘符根（面包屑上溯 parentOf 可达，无尾斜杠）→ 去前导 /：cd 'C:' 合法，cd '/C:' 不合法。
        assertEquals("C:", PowerShellDialect.sftpCwdToShell("/C:"))
        assertEquals("C:/", PowerShellDialect.sftpCwdToShell("/C:/"))
    }

    @Test
    fun windowsSftpCwdKeepsNonDrivePath() {
        // 非 /盘符:/ 前缀保持原样（不误伤）。
        assertEquals("/foo/bar", PowerShellDialect.sftpCwdToShell("/foo/bar")) // 无盘符
        assertEquals("C:/x", PowerShellDialect.sftpCwdToShell("C:/x")) // 已无前导 /
        assertEquals(".", PowerShellDialect.sftpCwdToShell("."))
        assertEquals("/CD:/x", PowerShellDialect.sftpCwdToShell("/CD:/x")) // 双字母非盘符 → 不动
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
