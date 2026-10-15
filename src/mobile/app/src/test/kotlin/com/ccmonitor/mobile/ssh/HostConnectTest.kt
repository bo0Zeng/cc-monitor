package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.db.Host
import org.junit.Assert.assertEquals
import org.junit.Test

class HostConnectTest {
    private fun host(
        wd: String? = null,
        tmux: Boolean = false,
        os: String? = null,
    ) = Host(
        id = "h",
        label = "h",
        host = "192.0.2.2",
        port = 22,
        username = "pi",
        defaultWorkingDir = wd,
        sessionManager = if (tmux) "tmux" else null,
        os = os,
    )

    // === Windows 主机：跳过 tmux（无等价），cd 走 PowerShell 方言（单引号 '' 转义）===
    @Test fun windowsSkipsTmuxEvenIfSessionManagerTmux() =
        assertEquals(
            "Windows 无 tmux → 不生成 tmux 命令（否则首条卡超时）",
            listOf("cd 'C:\\proj'"),
            host(wd = "C:\\proj", tmux = true, os = "windows").initialCommands(),
        )

    @Test fun windowsNoWdNoCommands() =
        assertEquals(emptyList<String>(), host(tmux = true, os = "windows").initialCommands())

    @Test fun windowsOverrideWorkingDirUsesPsCd() =
        assertEquals(
            "「在此打开终端」到 Windows 主机：override cwd 走 PS cd",
            listOf("""cd 'C:\a b'"""),
            host(os = "windows").initialCommands(workingDir = """C:\a b"""),
        )

    @Test fun posixDefaultStillTmux() =
        assertEquals(listOf("tmux -u new-session -A -s 'main'"), host(tmux = true, os = null).initialCommands())

    // === 无 workingDir：tmux 会话名带引号 'main' ===
    @Test fun noOverrideNoTmuxNoWd() = assertEquals(emptyList<String>(), host().initialCommands())

    @Test fun noOverridePlainCdWorkingDir() =
        assertEquals(listOf("cd '/home/pi'"), host(wd = "/home/pi").initialCommands())

    @Test fun noOverrideTmuxNoWd() =
        assertEquals(listOf("tmux -u new-session -A -s 'main'"), host(tmux = true).initialCommands())

    @Test fun noOverrideTmuxWithWd() =
        assertEquals(listOf("tmux -u new-session -A -s 'main' -c '/home/pi'"), host(wd = "/home/pi", tmux = true).initialCommands())

    // === workingDir（「在文件屏当前目录打开终端」）：override 强制 cd，优先于 defWd ===
    @Test fun overrideNoTmux() =
        assertEquals(listOf("cd '/srv/app'"), host().initialCommands(workingDir = "/srv/app"))

    @Test fun overrideNoTmuxBeatsDefWd() =
        assertEquals(listOf("cd '/srv/app'"), host(wd = "/home/pi").initialCommands(workingDir = "/srv/app"))

    @Test fun overrideTmuxNoDefWd() =
        assertEquals(
            listOf("tmux -u new-session -A -s 'main'", "cd '/srv/app'"),
            host(tmux = true).initialCommands(workingDir = "/srv/app"),
        )

    @Test fun overrideTmuxWithDefWd() =
        assertEquals(
            listOf("tmux -u new-session -A -s 'main' -c '/home/pi'", "cd '/srv/app'"),
            host(wd = "/home/pi", tmux = true).initialCommands(workingDir = "/srv/app"),
        )

    @Test fun blankOverrideIgnored() = assertEquals(emptyList<String>(), host().initialCommands(workingDir = "   "))

    // === 显式会话名（重连回到上次所在的那个）覆盖默认会话名；主机没开 auto-tmux 也进 ===
    @Test fun tmuxSessionOverridesDefault() =
        assertEquals(listOf("tmux -u new-session -A -s 'proj'"), host(tmux = true).initialCommands(tmuxSession = "proj"))

    @Test fun explicitTmuxSessionDrivesTmuxOnNonTmuxHost() =
        assertEquals(listOf("tmux -u new-session -A -s 'cc'"), host(tmux = false).initialCommands(tmuxSession = "cc"))

    // 非 tmux 主机不产生 tmux 命令（见 noOverridePlainCdWorkingDir 等）。

    @Test fun resolveTmuxSessionPrefersExplicitThenAutoTmux() {
        assertEquals("proj", host(tmux = false).resolveTmuxSession("proj"))
        assertEquals("main", host(tmux = true).resolveTmuxSession())
        assertEquals(null, host(tmux = false).resolveTmuxSession())
    }
}
