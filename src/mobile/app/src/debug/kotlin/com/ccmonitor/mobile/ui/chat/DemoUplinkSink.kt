package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.bridge.SendOutcome
import com.ccmonitor.mobile.core.claude.bridge.SendRequest
import com.ccmonitor.mobile.core.claude.bridge.UplinkSink
import kotlinx.coroutines.delay

/**
 * 假上行（只在 debug 构建里），让发送状态机的每条分支都能在真机上用眼验：
 *
 * | 输入以什么开头 | 结果 |
 * |---|---|
 * | `fail ` | 可重试失败：消息留在对话里，带「重试」 |
 * | `nope ` | 不可重试失败：不给骗人的重试键 |
 * | 其余 | 延迟后成功 |
 *
 * 同一条第二次尝试一律成功（`attempts` 记着），「重试不产生第二条」也能当场看见。
 */
class DemoUplinkSink : UplinkSink {
    /** 不回显：本地那条就是最终形态。 */
    override val echoesBack: Boolean = false

    private val attempts = mutableMapOf<String, Int>()

    override suspend fun send(request: SendRequest): SendOutcome {
        delay(SEND_LATENCY_MS)
        val n = (attempts[request.localId] ?: 0) + 1
        attempts[request.localId] = n
        // 第二次尝试一律成功 —— 让「重试真的能救回来」看得见
        if (n >= 2) return SendOutcome.Accepted
        return when {
            request.text.startsWith("fail ") -> SendOutcome.Rejected("演示：可重试失败", retryable = true)
            request.text.startsWith("nope ") -> SendOutcome.Rejected("演示：不可重试", retryable = false)
            else -> SendOutcome.Accepted
        }
    }

    /**
     * 假中断：投得出去，但永远等不到远端确认。重放屏没有真远端，不会有 `res.terminal_reason=aborted_streaming`，
     * 正好把「正在让远端停下…」这个中间态摆出来看；它最容易被错写成「已经停下」。
     */
    override suspend fun interrupt(): SendOutcome {
        delay(SEND_LATENCY_MS)
        return SendOutcome.Accepted
    }

    private companion object {
        const val SEND_LATENCY_MS = 800L
    }
}
