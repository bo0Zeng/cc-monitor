package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.ExecResult
import com.ccmonitor.mobile.core.remote.RemoteExecutor
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** [ResumeOffset]：问远端文件大小、决定从哪续读、清僵尸 tail。 */
class ResumeOffsetTest {
    private fun exec(
        exit: Int?,
        out: String,
        err: String = "",
    ) = RemoteExecutor { ExecResult(exitStatus = exit, stdout = out, stderrTail = err) }

    /** 真 `wc -c < file` 的输出形状：纯数字 + 换行，退出码 0。 */
    @Test
    fun itParsesTheRealShapeOfWcOutput() =
        runTest {
            assertEquals(1234L, ResumeOffset.remoteSize(exec(0, "1234\n"), "/p/s.jsonl"))
            assertEquals("前后空白要吃掉", 7L, ResumeOffset.remoteSize(exec(0, "  7  \n"), "/p/s.jsonl"))
        }

    /**
     * 退出码优先，这是 [RemoteExecutor] 带来的分辨力。
     *
     * `wc` 读不到文件时 stdout 空、stderr 有原因、退出码非 0。只看 stdout 的话
     * 「文件不存在」与「文件真是 0 字节」不可辨，而两者处置相反：前者保守沿用旧 offset，后者从头读。
     */
    @Test
    fun aFailedCommandIsNotTheSameAsAZeroByteFile() =
        runTest {
            assertNull(
                "读不到文件 ⇒ 问不出来（null），不许当成 0",
                ResumeOffset.remoteSize(exec(1, "", "wc: /p/s.jsonl: No such file"), "/p/s.jsonl"),
            )
            assertEquals("真的 0 字节就是 0，不是 null", 0L, ResumeOffset.remoteSize(exec(0, "0\n"), "/p/s.jsonl"))
        }

    /** 退出码 `null`（远端没回，连接断了/命令被杀）不等于成功。 */
    @Test
    fun aMissingExitStatusIsNotSuccess() =
        runTest {
            assertNull("没回退出码就是问不出来", ResumeOffset.remoteSize(exec(null, "1234\n"), "/p/s.jsonl"))
        }

    /** 输出不是数字（远端 shell 吐了别的）⇒ 也是「问不出来」，不许乱解析。 */
    @Test
    fun garbageOutputIsTreatedAsUnknown() =
        runTest {
            assertNull(ResumeOffset.remoteSize(exec(0, "wc: 参数错误\n"), "/p/s.jsonl"))
        }

    /** 路径要 quote 住：含空格/单引号的路径不能把命令劈开。 */
    @Test
    fun thePathIsQuoted() =
        runTest {
            var captured = ""
            val e =
                RemoteExecutor { cmd ->
                    captured = cmd
                    ExecResult(0, "1\n", "")
                }
            ResumeOffset.remoteSize(e, "/p/it's a.jsonl")
            assertTrue("要 POSIX 转义：$captured", captured.contains("""'/p/it'\''s a.jsonl'"""))
            // executor 会排空并保留 stderr，不该用 `2>/dev/null` 扔掉原因
            assertFalse("不该丢 stderr", captured.contains("2>/dev/null"))
        }

    /**
     * 清僵尸必须尽力而为：远端没 `pkill`、或没得杀，都不该挡住挂新 tail。
     *
     * 真正保证回收的是 tail 自带的 `timeout`；这条只是「立刻回收」的优化。
     * 写成会抛的话，一台没装 `pkill` 的服务器上阅读面会直接打不开。
     */
    @Test
    fun cleaningZombiesIsBestEffortAndNeverBlocksTheNewTail() =
        runTest {
            var called = false
            val boom =
                RemoteExecutor { cmd ->
                    called = true
                    assertTrue("要发的是清理命令：$cmd", cmd.startsWith("pkill -f "))
                    throw java.io.IOException("远端没有 pkill")
                }
            ResumeOffset.killStaleTail(boom, "/p/s.jsonl") // 抛出去的话这行就把测试炸了
            assertTrue("前提：它真的发了命令（不是被静默跳过）", called)
        }

    /**
     * 「问不出来」必须保守沿用，绝不复位：误复位会让通知面把整份历史重放一遍。
     */
    @Test
    fun anUnknownSizeNeverCausesAReset() {
        assertEquals("问不出来 ⇒ 原样沿用", 100L, ResumeOffset.resumeFrom(known = 100L, size = null))
        assertEquals("文件变小 ⇒ 被重写了 ⇒ 从头来", 0L, ResumeOffset.resumeFrom(known = 100L, size = 3L))
        assertEquals("文件在长 ⇒ 照常续", 100L, ResumeOffset.resumeFrom(known = 100L, size = 999L))
        assertEquals("恰好相等不算被重写", 100L, ResumeOffset.resumeFrom(known = 100L, size = 100L))
    }
}
