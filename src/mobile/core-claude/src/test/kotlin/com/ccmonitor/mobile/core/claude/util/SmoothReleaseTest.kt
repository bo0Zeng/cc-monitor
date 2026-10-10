package com.ccmonitor.mobile.core.claude.util

import com.ccmonitor.mobile.core.claude.bridge.BridgeFrame
import com.ccmonitor.mobile.core.claude.bridge.ReplayVectors
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * [SmoothRelease]：流式文字平滑放出，速率不得低于天然到达速率。
 *
 * 关键一条用 `long-reply` golden 的真 `t_ns` 驱动，不拿手编数字凑。
 */
class SmoothReleaseTest {
    /** 聊天面的窗口：16 字符 / 100ms。 */
    private fun chat() = SmoothRelease(chunkChars = 16, intervalMs = 100)

    @Test
    fun releasesInChunksNotAllAtOnce() {
        val r = chat()
        r.offer("a".repeat(100))
        val first = r.tick()
        assertTrue("一次不该全吐完", first.length < 100)
        assertTrue("也不该一次只吐 1 个字符", first.length >= 16)
    }

    /**
     * 速率不得低于 176 字符/秒（天然速率：2575 字符 / 31 块 / 0.472 秒 ≈ 176）。
     *
     * 低于天然速率的话输出永远追不上输入，每块比上块欠得更多：点了「停止」，屏幕还在吐字。
     *
     * 这里用 `long-reply` golden 的真实到达节奏跑一遍，断言放完不落后于收完。
     */
    @Test
    fun keepsUpWithTheRealArrivalRateOfLongReply() {
        val dir = File("../bridge/vectors").takeIf { it.isDirectory } ?: File("bridge/vectors")
        val rows = ReplayVectors.parse(File(dir, "long-reply.frames.ndjson").readLines().asSequence())
        val deltas = rows.filter { it.frame is BridgeFrame.TextDelta }
        assertTrue("前提：long-reply 里有正文 delta", deltas.size > 20)

        val totalChars = deltas.sumOf { (it.frame as BridgeFrame.TextDelta).x.length }
        val spanMs = (deltas.last().tNs - deltas.first().tNs) / 1_000_000
        val naturalRate = totalChars * 1000.0 / spanMs
        assertTrue("前提：天然速率≈176 字符/秒（算得 $naturalRate）", naturalRate > 100)

        // 按真实节奏喂，按 100ms 一拍放
        val r = chat()
        var clock = 0L
        var released = 0
        var fed = 0
        for (d in deltas) {
            val at = (d.tNs - deltas.first().tNs) / 1_000_000
            while (clock < at) {
                clock += 100
                released += r.tick().length
            }
            val delta = d.frame as BridgeFrame.TextDelta
            r.offer(delta.x)
            fed += delta.x.length
        }
        // 收完之后再放同样长的时间，必须追平
        val tail = spanMs
        var t = 0L
        while (t < tail) {
            t += 100
            released += r.tick().length
        }

        assertEquals("放完的必须等于收到的——落后就是速率下限被破了", fed, released)
        assertEquals("不该还有积压", 0, r.backlog)
    }

    /**
     * 显式钉住速率下限，且这条必须真的能红。
     *
     * 注意：积压必须压在追赶阈值以下，逼它走 `base` 那条路径。积压超过阈值的话第一拍就整包吐光，
     * 测的是追赶分支，把 `MIN_CHARS_PER_SECOND` 调低也照样绿。
     */
    @Test
    fun minimumRateHoldsAndTheFloorPathIsActuallyExercised() {
        val r = SmoothRelease(chunkChars = 1, intervalMs = 100)
        val floorPerTickAtLeast = SmoothRelease.MIN_CHARS_PER_SECOND / 10 // 100ms 一拍 ⇒ 10 拍 1 秒

        // 只放略少于追赶阈值的量，确保不触发整包吐光那条捷径
        r.offer("x".repeat(SmoothRelease.MIN_CHARS_PER_SECOND - 1))
        val firstTick = r.tick().length
        assertTrue(
            "这一拍必须走 base 路径（若整包吐光说明测试又空了）：$firstTick",
            firstTick < SmoothRelease.MIN_CHARS_PER_SECOND - 1,
        )
        assertTrue(
            "单拍放出必须 ≥ $floorPerTickAtLeast（= ${SmoothRelease.MIN_CHARS_PER_SECOND}/秒 ÷ 10 拍），实际 $firstTick",
            firstTick >= floorPerTickAtLeast,
        )
    }

    /**
     * 整除截断会让实际速率低于下限：176×100/1000 = 17.6，截断成 17 就是 170/秒。
     *
     * 下限是「不得低于 176」，所以每一步取整只能往「够」的方向走。
     * 这条直接按秒对账，截断一旦回来就红。
     */
    @Test
    fun steadyStateRateIsNeverBelowTheFloorDueToIntegerTruncation() {
        val r = SmoothRelease(chunkChars = 1, intervalMs = 100)
        // 每拍前补一点，让积压始终存在但不越过追赶阈值 ⇒ 稳态走 base 路径
        var out = 0
        repeat(10) {
            r.offer("y".repeat(SmoothRelease.MIN_CHARS_PER_SECOND / 5))
            out += r.tick().length
        }
        assertTrue(
            "稳态 1 秒（10 拍）放出 $out 字符，低于下限 ${SmoothRelease.MIN_CHARS_PER_SECOND} —— 取整方向错了",
            out >= SmoothRelease.MIN_CHARS_PER_SECOND,
        )
    }

    /**
     * 持续落后才追平，而不是「这一刻积压多」就放弃平滑。
     *
     * 前 10 拍（= 1 秒）照常平滑地放；第 11 拍才一次追平。
     * 若换成看瞬时快照，第一拍就会全吐，这条会红。
     */
    @Test
    fun catchUpHappensOnlyAfterBeingBehindForAFullSecond() {
        val r = chat()
        r.offer("x".repeat(5000))

        val first = r.tick().length
        assertTrue("第一拍必须还在平滑地放，实际 $first", first < 100)

        var total = first
        repeat(9) { total += r.tick().length }
        assertTrue("1 秒内不该追平（仍有大量积压）", r.backlog > 0)

        val catchUp = r.tick().length
        assertTrue("连续落后 1 秒之后应一次追平，实际 $catchUp", catchUp > 1000)
        assertEquals("追平后不该还有积压", 0, r.backlog)
    }

    // ---- 与权威全文覆盖的接缝 ----------------------------------------------------

    /**
     * `at` 全文覆盖时保留已放出的部分，只把剩下的排队。
     *
     * 清空重放的表现是：已经读过的文字倒回去重新冒一遍。
     */
    @Test
    fun authoritativeOverrideKeepsWhatWasAlreadyShown() {
        val r = chat()
        r.offer("已经读过的")
        val shown = r.tick()
        assertEquals("已经读过的", shown)

        r.replaceAll("已经读过的后面还有很多")
        assertEquals("已放出的不许倒回去", "已经读过的", r.released.toString())
        assertEquals("只有新增的部分排队", "后面还有很多".length, r.backlog)
    }

    /**
     * 权威全文与已放出的对不上（delta 丢过/乱序）时，以权威为准整体重来。
     *
     * 权威全文无条件覆盖：宁可闪一下，也不能显示一段权威说不存在的文字。
     */
    @Test
    fun divergentAuthoritativeTextRestartsFromScratch() {
        val r = chat()
        r.offer("丢包拼出来的错东西")
        r.tick()
        r.replaceAll("权威说的是这个")

        assertEquals("对不上就整体重来", "", r.released.toString())
        assertEquals("权威说的是这个".length, r.backlog)
        r.flush()
        assertEquals("权威说的是这个", r.released.toString())
    }

    /** 用户点停止 / turn 已 `res` 收口 → 一次性吐完，不留尾巴。 */
    @Test
    fun flushEmptiesTheBacklogAtOnce() {
        val r = chat()
        r.offer("a".repeat(500))
        r.flush()
        assertEquals(0, r.backlog)
        assertEquals(500, r.released.length)
    }

    /** 空的时候 tick 不许崩、不许返回 null 之类。 */
    @Test
    fun tickOnEmptyIsHarmless() {
        val r = chat()
        assertEquals("", r.tick())
        assertEquals("", r.flush())
        assertFalse(r.backlog > 0)
    }
}
