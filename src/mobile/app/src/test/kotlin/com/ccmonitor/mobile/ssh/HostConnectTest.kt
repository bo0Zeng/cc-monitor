package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.data.db.Host
import org.junit.Assert.assertEquals
import org.junit.Test

class HostConnectTest {
    // tmux 创建序列里的移动端 setup 段（status off + 唯一的滚动模式触摸设置，new-session 后、attach 前，
    // 无条件施加）。逐字锁死；下方拼接断言仍是全串 assertEquals。
    // 只对进的那个会话（`-t '=<名>:'`），不用 `-g` 改整台机器；不设 `terminal-overrides`。
    private fun setup(session: String) = "tmux set -t '=$session:' status off 2>/dev/null; tmux set -t '=$session:' mouse on 2>/dev/null"

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
            host(os = "windows").initialCommands(LaunchSpec(workingDir = """C:\a b""")),
        )

    @Test fun posixDefaultStillTmux() =
        assertEquals(listOf("tmux new-session -d -s 'main' 2>/dev/null; ${setup("main")}; tmux attach -t 'main'"), host(tmux = true, os = null).initialCommands())

    // === 无 workingDir：tmux 会话名带引号 'main' ===
    @Test fun noOverrideNoTmuxNoWd() = assertEquals(emptyList<String>(), host().initialCommands())

    @Test fun noOverridePlainCdWorkingDir() =
        assertEquals(listOf("cd '/home/pi'"), host(wd = "/home/pi").initialCommands())

    @Test fun noOverrideTmuxNoWd() =
        assertEquals(listOf("tmux new-session -d -s 'main' 2>/dev/null; ${setup("main")}; tmux attach -t 'main'"), host(tmux = true).initialCommands())

    @Test fun noOverrideTmuxWithWd() =
        assertEquals(listOf("tmux new-session -d -s 'main' -c '/home/pi' 2>/dev/null; ${setup("main")}; tmux attach -t 'main'"), host(wd = "/home/pi", tmux = true).initialCommands())

    // === workingDir（「在 SFTP 目录打开终端」）：override 强制 cd，优先于 defWd ===
    @Test fun overrideNoTmux() =
        assertEquals(listOf("cd '/srv/app'"), host().initialCommands(LaunchSpec(workingDir = "/srv/app")))

    @Test fun overrideNoTmuxBeatsDefWd() =
        assertEquals(listOf("cd '/srv/app'"), host(wd = "/home/pi").initialCommands(LaunchSpec(workingDir = "/srv/app")))

    @Test fun overrideTmuxNoDefWd() =
        assertEquals(
            listOf("tmux new-session -d -s 'main' 2>/dev/null; ${setup("main")}; tmux attach -t 'main'", "cd '/srv/app'"),
            host(tmux = true).initialCommands(LaunchSpec(workingDir = "/srv/app")),
        )

    @Test fun overrideTmuxWithDefWd() =
        assertEquals(
            listOf("tmux new-session -d -s 'main' -c '/home/pi' 2>/dev/null; ${setup("main")}; tmux attach -t 'main'", "cd '/srv/app'"),
            host(wd = "/home/pi", tmux = true).initialCommands(LaunchSpec(workingDir = "/srv/app")),
        )

    @Test fun blankOverrideIgnored() = assertEquals(emptyList<String>(), host().initialCommands(LaunchSpec(workingDir = "   ")))

    // === LaunchSpec.tmuxSession 覆盖默认会话名 ===
    @Test fun tmuxSessionOverridesDefault() =
        assertEquals(listOf("tmux new-session -d -s 'proj' 2>/dev/null; ${setup("proj")}; tmux attach -t 'proj'"), host(tmux = true).initialCommands(LaunchSpec(tmuxSession = "proj")))

    // === LaunchSpec.command 末尾追加（cd/tmux 之后）===
    @Test fun commandAppendedAfterCd() =
        assertEquals(
            listOf("cd '/srv/app'", "claude"),
            host().initialCommands(LaunchSpec(workingDir = "/srv/app", command = "claude")),
        )

    @Test fun commandAloneNoTmuxNoWd() =
        assertEquals(listOf("htop"), host().initialCommands(LaunchSpec(command = "htop")))

    @Test fun commandAppendedAfterTmux() =
        assertEquals(
            listOf("tmux new-session -d -s 'main' 2>/dev/null; ${setup("main")}; tmux attach -t 'main'", "cc"),
            host(tmux = true).initialCommands(LaunchSpec(command = "cc")),
        )

    @Test fun blankCommandIgnored() =
        assertEquals(listOf("cd '/home/pi'"), host(wd = "/home/pi").initialCommands(LaunchSpec(command = "  ")))

    // === launcher 的 tmuxSession 驱动 tmux（即使 host 未配 auto-tmux），如 cct ===
    @Test fun launcherTmuxSessionDrivesTmuxOnNonTmuxHost() =
        assertEquals(
            listOf("tmux new-session -d -s 'cc' 2>/dev/null; ${setup("cc")}; tmux attach -t 'cc'", "claude"),
            host(tmux = false).initialCommands(LaunchSpec(tmuxSession = "cc", command = "claude")),
        )

    @Test fun launcherCcNoTmuxJustCommand() =
        assertEquals(listOf("claude"), host(tmux = false).initialCommands(LaunchSpec(command = "claude")))

    // === 移动端 setup（含唯一的滚动模式触摸设置）无条件拼进 tmux 创建序列（new-session 后、attach 前）：
    // 首连确定性施加，避开 exec 通道竞速。非 tmux 主机不产生 tmux 命令（见 noOverridePlainCdWorkingDir 等）===

    @Test fun resolveTmuxSessionPrefersSpecThenAutoTmux() {
        assertEquals("proj", host(tmux = false).resolveTmuxSession(LaunchSpec(tmuxSession = "proj")))
        assertEquals("main", host(tmux = true).resolveTmuxSession(LaunchSpec()))
        assertEquals(null, host(tmux = false).resolveTmuxSession(LaunchSpec()))
    }
}
