package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.remote.ExecResult
import com.ccmonitor.mobile.core.remote.RemoteExecutor
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class OneShotAndGateTest {
    private fun read(
        exit: Int?,
        out: String,
        err: String,
    ) = OneShot.read(Triple(exit, out, err))

    @Test
    fun `成功是 stdout 一行 JSON、stderr 0 字节`() {
        assertEquals(OneShotOutcome.Ok(mapOf("pid" to 42.0)), read(0, "{\"pid\":42}\n", ""))
    }

    @Test
    fun `失败读 stderr 那一行信封，message 与 detail 原样`() {
        val got = read(2, "", "{\"code\":\"bad_args\",\"message\":\"参数不对\",\"detail\":\"时刻：…\\n码：bad_args\"}\n")
        assertEquals(OneShotOutcome.Failed(CoreFailure("bad_args", "参数不对", "时刻：…\n码：bad_args")), got)
    }

    @Test
    fun `127 与 126 是那台没装后端`() {
        assertEquals(OneShotOutcome.Absent, read(127, "", "sh: 1: /home/u/.cc-monitor/bin/ccm: not found\n"))
        assertEquals(OneShotOutcome.Absent, read(126, "", "permission denied\n"))
    }

    @Test
    fun `对不上协议的都原样带回，不装成成功或没装`() {
        // 失败却没有信封
        assertTrue(read(2, "", "panic at …\n") is OneShotOutcome.Unreadable)
        // 信封多于一行
        assertTrue(read(2, "", "{\"code\":\"a\",\"message\":\"b\"}\n{\"code\":\"c\",\"message\":\"d\"}\n") is OneShotOutcome.Unreadable)
        // 成功却往 stderr 写了字
        assertTrue(read(0, "{}", "warn\n") is OneShotOutcome.Unreadable)
        // 成功却不是 JSON 对象
        assertTrue(read(0, "ok\n", "") is OneShotOutcome.Unreadable)
        // 退出码没回（连接断 · 被杀）不当成 0
        assertTrue(read(null, "{}", "") is OneShotOutcome.Unreadable)
    }

    @Test
    fun `命令只拼成部署落点上那一份 ccm，常量词以外的不收`() =
        runTest {
            var seen = ""
            OneShot(
                RemoteExecutor {
                    seen = it
                    ExecResult(0, "{}", "")
                },
            ).run("--backend-probe")
            assertEquals("\"\$HOME\"/.cc-monitor/bin/ccm -- --backend-probe", seen)
            val bad = runCatching { BackendBin.command("--x; rm -rf ~") }
            assertTrue(bad.isFailure)
        }

    @Test
    fun `门槛是同一个 BUILD_ID：同 ⇒ 接；不同或说不出 ⇒ 要换`() {
        assertEquals(GateVerdict.Same, BackendGate.verdict(OneShotOutcome.Ok(mapOf("buildId" to "p9x-a")), ours = "p9x-a"))
        // 比它新也不接：不比大小
        assertEquals(GateVerdict.Differs("p9z-b"), BackendGate.verdict(OneShotOutcome.Ok(mapOf("buildId" to "p9z-b")), ours = "p9x-a"))
        assertEquals(GateVerdict.Differs(null), BackendGate.verdict(OneShotOutcome.Ok(emptyMap()), ours = "p9x-a"))
        assertEquals(GateVerdict.Absent, BackendGate.verdict(OneShotOutcome.Absent))
    }

    @Test
    fun `内嵌的身份就是本仓后端 lib_rs 里那一个`() {
        val lib = File(Fixtures.repoRoot, "src/backend/lib.rs").readText()
        val want = Regex("(?m)^pub const BUILD_ID: &str = \"([^\"]+)\";").find(lib)!!.groupValues[1]
        assertEquals(want, EmbeddedBuild.ID)
    }
}
