package com.ccmonitor.mobile.core.remote

import kotlinx.coroutines.flow.Flow
import java.io.Closeable

/**
 * 一条双向的长 exec：远端命令的 stdin 写得进、stdout 一直读得出。
 *
 * 常驻流（`ccm -- --resident-attach`）就跑在它上面：先读 hello、再写 attach 那一行，之后请求一行一行写进 stdin，
 * 帧与应答一行一行从 stdout 出来。[RemoteCommandChannel] 只搬 stdout、写不了 stdin，所以另开这一个接口。
 *
 * 实现方守的三条：
 * 1. 不分配 PTY（PTY 的回显与 ONLCR 会把写进去的请求混进读出来的帧）。
 * 2. 无条件排空 stderr（与 stdout 共用通道窗口，不读会把 stdout 一起拖停）；[stderrTail] 留尾部给复制详情。
 * 3. [close] 关通道、杀远端命令、解开阻塞读；之后 [stdout] 结束，[write] 抛 IOException。
 */
interface RemoteDuplex : Closeable {
    /**
     * stdout 的字节块，按到达顺序。**只许收集一次**（底下是一条字节流）。
     * 远端命令退出 ⇒ 正常结束；连接断 ⇒ 抛 IOException。
     */
    val stdout: Flow<ByteArray>

    /** 写进 stdin 并冲出去。通道已关 ⇒ 抛 IOException。 */
    suspend fun write(bytes: ByteArray)

    /** 远端命令的退出码；还没退出 / 远端没回 ⇒ `null`（不许当成 0）。 */
    val exitStatus: Int?

    /** stderr 尾部（至多几 KiB）。 */
    val stderrTail: String
}

/** 开一条 [RemoteDuplex]。无活连接 ⇒ 抛 [ConnectionDeadException]。 */
fun interface RemoteDuplexChannel {
    suspend fun open(command: String): RemoteDuplex
}
