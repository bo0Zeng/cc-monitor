package com.ccmonitor.mobile.ssh

import org.junit.Assert.assertEquals
import org.junit.Test

/**
 * 手机自己敲的那两条 tmux 命令（用户在 ssh 里也会这么敲）的逐字节定值。
 *
 * 期望值是手写的规格，刻意不引生产常量。带 `-u`：exec / 登录 shell 里常没有 UTF-8 的 locale，
 * 那时 tmux 客户端把中文这类宽字符画成 `_`（模拟器冒烟撞上过）。不设会话选项（状态栏 · 鼠标）：那是会话级的，
 * 桌面那侧接着的也会跟着变。找会话、附着到 cc-monitor 认得的会话、结束、抓屏、送字都不在这里：走那台核心。
 */
class TmuxGatewayGoldenTest {
    @Test
    fun theTwoUserCommandsAreByteIdenticalToTheGolden() {
        assertEquals("有则接、无则建（无工作目录）变了", "tmux -u new-session -A -s 'main'", TmuxCommands.tmuxNewOrAttach("main"))
        assertEquals(
            "有则接、无则建（带工作目录，含空格要被引住）变了",
            "tmux -u new-session -A -s 'proj' -c '/home/u/my proj'",
            TmuxCommands.tmuxNewOrAttach("proj", "/home/u/my proj"),
        )
        assertEquals("后台新建变了", "tmux -u new-session -d -s 'work'", TmuxCommands.tmuxNewDetachedCommand("work"))
    }

    @Test
    fun theBackendDelegatesToTheGateway() {
        assertEquals(TmuxCommands.tmuxNewOrAttach("main", "/w"), TmuxBackend.enter("main", "/w"))
        assertEquals(TmuxCommands.tmuxNewDetachedCommand("work"), TmuxBackend.newDetachedCommand("work"))
    }
}
