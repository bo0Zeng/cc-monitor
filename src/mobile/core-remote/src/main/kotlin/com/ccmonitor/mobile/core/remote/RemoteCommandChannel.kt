package com.ccmonitor.mobile.core.remote

import kotlinx.coroutines.flow.Flow
import java.io.IOException

/**
 * 远端命令通道（SAM）：通用的流式远端 exec，与具体 agent（Claude/Codex）无关。
 * core-ssh 实现它（见 `SshConnectionManager.commandChannel`），core-claude 消费它，两者互不依赖；
 * core-claude 因此保持纯 JVM。
 */
fun interface RemoteCommandChannel {
    /** 跑一条远端命令，按到达顺序发 stdout 字节块。Flow 取消 → 杀远端命令。 */
    fun exec(command: String): Flow<ByteArray>
}

/**
 * [RemoteCommandChannel.exec] 时底层连接不可用（连接对象不存在 / 未连上）抛这个异常。
 * 它是 IOException 子类，只 `catch (IOException)` 的地方行为不变；
 * 要区分「连接死了」与「其他传输/命令错」的消费方（重试还是重连、「未连接」还是「无历史」）单独 catch 它。
 */
class ConnectionDeadException(
    message: String,
) : IOException(message)
