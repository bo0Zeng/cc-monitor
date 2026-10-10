package com.ccmonitor.mobile.core.claude.util

/**
 * 缓释器：把突发到达的大块文字摊成均匀冒出来的小块。
 *
 * 一条长回复的 delta 平均八十来个字符一块、块间隔约半秒，直接上屏就是卡一下、蹦一大段。
 *
 * 速率下限 176 字符/秒，即这种回复的天然速率。放得比它慢，输出永远追不上输入，越播越落后：
 * 点了停止，屏幕还在吐字，比不做缓释更糟。
 *
 * 不持有时钟：只回答「此刻该放出多少字符」，`delay` 由调用方做。两个消费方（聊天面 16/100、
 * 阅读面 90/800）各配自己的窗口。
 */
class SmoothRelease(
    /** 每次放出的字符数上限。聊天面 16（细腻），阅读面 90（省重组）。 */
    private val chunkChars: Int,
    /** 两次放出的间隔（毫秒）。聊天面 100，阅读面 800。 */
    private val intervalMs: Long,
    /**
     * 速率下限（字符/秒）。持续落后时一次性吐完积压：宁可牺牲一次动画的顺滑，也不让文字流持续欠账。
     */
    private val minCharsPerSecond: Int = MIN_CHARS_PER_SECOND,
) {
    private val pending = StringBuilder()

    /** 已放出的全文。 */
    val released: StringBuilder = StringBuilder()

    val backlog: Int get() = pending.length

    fun offer(text: String) {
        pending.append(text)
    }

    /**
     * 全文覆盖（`at` / `tt` 帧的语义）。保留已放出的部分、剩下的重新排队；
     * 不是清空重放，否则读过的文字会倒回去重新冒一遍。
     */
    fun replaceAll(fullText: String) {
        val shown = released.length
        if (fullText.length >= shown && fullText.startsWith(released)) {
            pending.setLength(0)
            pending.append(fullText, shown, fullText.length)
        } else {
            // 权威全文与已放出的对不上（delta 丢过 / 乱序），以权威为准，整体重来
            released.setLength(0)
            pending.setLength(0)
            pending.append(fullText)
        }
    }

    /**
     * 放出下一批。
     *
     * @return 本次新放出的字符（可能为空串）。
     */
    fun tick(): String {
        if (pending.isEmpty()) return ""
        val n = minOf(takeSize(), pending.length)
        val out = pending.substring(0, n)
        pending.delete(0, n)
        released.append(out)
        return out
    }

    /** 一次性吐完（用户点了停止、或 turn 已 `res` 收口时）。 */
    fun flush(): String {
        val out = pending.toString()
        pending.setLength(0)
        released.append(out)
        return out
    }

    /** 连续多少拍清不完积压。追赶看的是持续落后，不是某一刻的快照。 */
    private var behindTicks = 0

    private fun takeSize(): Int {
        // 每个 interval 至少要放这么多，才够 minCharsPerSecond。必须向上取整：
        // 176×100/1000 = 17.6，截断成 17 就只有 170 字符/秒，低于下限。
        val floorPerTick =
            ((minCharsPerSecond * intervalMs + MILLIS_PER_SECOND - 1) / MILLIS_PER_SECOND)
                .toInt()
                .coerceAtLeast(1)
        val base = maxOf(chunkChars, floorPerTick)

        // 追赶看连续落后多久，不看此刻积压多少：突发本来就是缓释器要处理的，一个正常块或一条全文帧
        // 一到就越过任何积压阈值，按积压判就等于在最该干活的时候放弃。
        // 连着 CATCH_UP_AFTER_MS 毫秒都清不完，才算真落后。
        if (pending.length <= base) {
            behindTicks = 0
            return base
        }
        behindTicks++
        val ticksToTolerate = (CATCH_UP_AFTER_MS / intervalMs).coerceAtLeast(1)
        if (behindTicks > ticksToTolerate) {
            behindTicks = 0
            return pending.length
        }
        return base
    }

    companion object {
        /** 长回复的天然速率（约 2575 字符 / 31 块 / 0.472 秒）。 */
        const val MIN_CHARS_PER_SECOND = 176

        private const val MILLIS_PER_SECOND = 1000L

        /** 落后多久就放弃平滑、一次追平。1 秒是开始觉得卡的量级。 */
        private const val CATCH_UP_AFTER_MS = 1000L
    }
}
