package com.ccmonitor.mobile.core.ssh

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.IOException

/**
 * `aggregateRaceError` 的类型保真：host key 变更必须以 [HostKeyChangedException] 穿过竞速层，
 * 上层的 `catch (e: HostKeyChangedException)` 才能进恢复页。
 */
class RaceConnectAggregateTest {
    private fun err(e: Throwable) = listOf("h:22" to e)

    /** host key 变更原样上抛，不包成 IOException：它是唯一可恢复的连接失败，包起来用户只会看到「所有地址连接失败」。 */
    @Test
    fun hostKeyChangedSurvivesAggregationSoTheRecoveryPageStaysReachable() {
        val original = HostKeyChangedException("h", 22, "ssh-ed25519", "SHA256:old", "SHA256:new")
        val out = aggregateRaceError(err(original)) { it }
        assertTrue("必须原样上抛，实际是 ${out::class.simpleName}", out is HostKeyChangedException)
        assertEquals("身份信息不许丢", "SHA256:old", (out as HostKeyChangedException).expectedFingerprint)
    }

    /** 多个候选里只要有一个是 host key 变更，就优先上抛它。 */
    @Test
    fun hostKeyChangedWinsOverOtherFailuresWhenRacing() {
        val hk = HostKeyChangedException("h", 22, "ssh-ed25519")
        val out =
            aggregateRaceError(
                listOf("a:22" to IOException("超时"), "b:22" to hk, "c:22" to IOException("拒绝")),
            ) { it }
        assertTrue(out is HostKeyChangedException)
    }

    /** 其余失败仍然聚合：不能泛化成「单个错误原样上抛」，那会改变所有单地址失败的异常类型。 */
    @Test
    fun otherFailuresAreStillAggregatedIntoIoException() {
        val out = aggregateRaceError(err(IllegalStateException("连接已关闭"))) { it }
        assertTrue("非 host-key 失败仍应聚合成 IOException，实际 ${out::class.simpleName}", out is IOException)
        assertTrue("聚合信息要带上真因摘要", out.message!!.contains("连接已关闭"))
        assertTrue("真因要挂在 suppressed 上不丢", out.suppressed.any { it is IllegalStateException })
    }

    /** 空错误列表的兜底不变。 */
    @Test
    fun emptyErrorsStillYieldsAnIoException() {
        assertTrue(aggregateRaceError(emptyList<Pair<String, Throwable>>()) { it } is IOException)
    }
}
