package com.ccmonitor.mobile.core.claude.transport

import com.ccmonitor.mobile.core.remote.RemoteExecutor
import com.ccmonitor.mobile.core.remote.shellQuote

/**
 * tail 续传的 offset 账本，「远端文件多大」只在这里实现。它和 [TailTransport] 服务同一件事：
 * 续传锚点（`tail -c +N` 的那个 N）。
 *
 * 不变量：`offset ≤ 已结算位置 ≤ 真实 EOF`。文件被重写（`/compact` 原地改写）时 `size < offset`，
 * 必须复位从 0 重建，否则会从一个已经不存在的位置往后读。
 */
object ResumeOffset {
    /**
     * 远端文件字节数。
     *
     * @return `null` = 问不出来（文件不在 / 读不了 / 输出不是数字）。调用方必须保守沿用旧 offset，
     *   不要复位：误复位会让通知面把整份历史重放一遍。
     *
     * [RemoteExecutor] 排空并保留 stderr 尾部，所以不需要 `2>/dev/null`，失败原因能进诊断。
     */
    suspend fun remoteSize(
        executor: RemoteExecutor,
        path: String,
    ): Long? {
        val r = executor.execCapture("wc -c < ${shellQuote(path)}")
        // 退出码优先：只看 stdout 的话「文件不存在」与「文件真是 0 字节」不可辨，
        // 而两者的处置相反（前者保守沿用，后者是合法的从头读）。
        if (!r.ok) return null
        return r.stdout.trim().toLongOrNull()
    }

    /**
     * 按「文件是否被重写」决定这次从哪儿续。
     *
     * @param known 我们记着的 offset。
     * @return 这次该用的起始 offset。`size` 问不出来（null）⇒ 原样沿用 [known]，绝不复位。
     */
    fun resumeFrom(
        known: Long,
        size: Long?,
    ): Long = if (size != null && size < known) 0L else known

    /**
     * 重挂 tail 之前先清掉自己上次遗留的那条（见 [TailTransport.killStaleCommand]）。
     *
     * 移动网络每掉线一次就可能在远端漏一条 tail：sshd 没有应用层保活，内核 keepalive 要两个小时才发现。
     * `timeout` 是兜底的绝对上限，这条是立刻回收。
     *
     * 失败不抛、不上报：远端没有 `pkill` 或没有可杀的进程，都不该阻止挂新 tail。
     */
    suspend fun killStaleTail(
        executor: RemoteExecutor,
        path: String,
    ) {
        runCatching { executor.execCapture(TailTransport.killStaleCommand(path)) }
    }
}
