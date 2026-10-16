package com.ccmonitor.mobile.core.remote

import java.io.IOException

/**
 * 一次远端命令的完整结果。
 *
 * [stderrTail] 只保尾部若干字节（错误定位够用，防大 stderr 占内存）。
 */
data class ExecResult(
    /** `null` = 远端**没回**退出码（连接中断、命令被杀）。**不要**把它当成 0。 */
    val exitStatus: Int?,
    val stdout: String,
    val stderrTail: String,
) {
    val ok: Boolean get() = exitStatus == 0

    /**
     * 失败即抛，异常带可读上下文。
     *
     * @param what 这条命令在做什么（进异常消息，别只上抛「通道关闭了」）。
     */
    fun orThrow(what: String): ExecResult {
        if (!ok) throw RemoteExecException("$what 失败（退出码 $exitStatus）：${stderrTail.takeLast(STDERR_IN_MESSAGE)}")
        return this
    }

    private companion object {
        const val STDERR_IN_MESSAGE = 500
    }
}

/** [ExecResult.orThrow] 抛的异常。是 IOException 子类，`catch (IOException)` 照样接得住。 */
class RemoteExecException(
    message: String,
) : IOException(message)

/**
 * 一次性远端命令执行（带退出码与 stderr 尾巴）。
 *
 * 实现方必须守的两条：
 * 1. 无条件排空 stderr：stderr 与 stdout 共用同一个通道窗口，不读 stderr 会耗尽窗口、把 stdout 一起拖停。
 * 2. 绝不分配 PTY：PTY 的 ONLCR 会污染字节流。
 */
fun interface RemoteExecutor {
    /**
     * 跑一条命令并等它结束。
     *
     * @return 永远返回结果对象；**命令失败不抛异常**（要抛请接 [ExecResult.orThrow]）——
     *   调用方常常需要区分「失败了但可以保守降级」与「失败了必须告诉用户」。
     *   连接不可用等**传输层**故障仍然抛（[ConnectionDeadException]）。
     */
    suspend fun execCapture(command: String): ExecResult
}

/**
 * 底层连接不可用（连接对象不存在 / 未连上）时抛这个异常。是 IOException 子类，只 `catch (IOException)` 的地方行为不变；
 * 要区分「连接死了」与「其他传输/命令错」的消费方单独 catch 它。
 */
class ConnectionDeadException(
    message: String,
) : IOException(message)
