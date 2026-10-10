package com.ccmonitor.mobile.ui.common

import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.ssh.HostConnector
import kotlinx.coroutines.CancellationException
import org.koin.compose.koinInject

/**
 * 界面的建连门：确保一台主机是通的，并回答「现在能用吗，不能的话为什么」。
 *
 * `commandChannel(hostId)` 在无活连接时抛 `ConnectionDeadException`，所以对话列表和对话屏
 * 必须先经过这里。建连走 `HostConnector.connect`（密码/身份解析、跳板链、纪元守门都在里面），
 * 持有走 `HostConnector.retain` / `release`。本文件不碰任何连接细节。
 */
sealed interface HostLink {
    /** 还在建。UI 显示「正在接通…」而不是空白。 */
    data object Connecting : HostLink

    /** 可以用了。 */
    data object Ready : HostLink

    /**
     * 接不上。[why] 是人可读的原因：失败要说出来，不许静默。
     *
     * 它与「连上了但那台机器上没有对话」是两件事，UI 必须分得出来。
     */
    data class Failed(
        val why: String,
    ) : HostLink
}

/**
 * 确保这台主机是通的，并把过程状态交出去。
 *
 * @param holder 持有者标识，传给 `HostConnector.retain`。聊天面必须用 `"chat-<sid>"`（见 [chatHolder]）；
 *   列表这种看一眼就走的屏用自己的标识。两者都必须 retain，否则终端 tab 一关持有者归零，
 *   连接被断开，连带杀掉正在跑的管道。
 * @param retainWhileComposed `true` = 屏幕在就持有、离屏即释放（对话列表）。
 *   `false` = 本函数不管持有，由更长命的东西负责（聊天面：持有挂在 `ChatController` 的对话上，
 *   下行不随屏幕生死）。
 * @param attempt 再试一次：值一变就整条重跑，含建连。接不上主机时界面唯一的动作是「重试」，
 *   它必须真的重连，而不只是重挂下行。默认 0。
 */
@Composable
fun rememberHostLink(
    hostId: String,
    holder: String,
    retainWhileComposed: Boolean = true,
    attempt: Int = 0,
): State<HostLink> {
    val connector = koinInject<HostConnector>()
    val hosts = koinInject<HostRepository>()
    val state = remember(hostId) { mutableStateOf<HostLink>(HostLink.Connecting) }

    // `retainWhileComposed = true` 时离屏即释放。`release` 是引用计数（持有者集空了才断），
    // 一个永不移除的 token 会让这台主机的连接再也断不掉。
    //
    // 用 `DisposableEffect` 而不是下面协程的 `finally`：协程建完连就正常返回，`finally` 会当场放手。
    // 持有的寿命跟组合走，不跟协程走。
    //
    // key 只取 `(hostId, holder)`，不含 `attempt`：重试时 release 一个马上又要 retain 的持有者，
    // 持有者集可能归零，正要重试的连接先被杀一次。`holders` 是 Set、`retain` 幂等，不进 key 也不漏释放。
    //
    // 注意：`false` 那条路（聊天面）绝不走这里。它的 holder 与 `ChatController` 铸的是同一个串，
    // 在这里 release 会把 `ChatController` 的持有拿掉，离屏当场断连。
    if (retainWhileComposed) {
        DisposableEffect(hostId, holder) {
            connector.retain(hostId, holder)
            onDispose { connector.release(hostId, holder) }
        }
    }

    LaunchedEffect(hostId, holder, retainWhileComposed, attempt) {
        state.value = HostLink.Connecting
        // 已经连上了就别再连：重复 connect 会重建连接、打断正在跑的东西。
        if (connector.connection(hostId)?.isConnected == true) {
            if (retainWhileComposed) connector.retain(hostId, holder)
            state.value = HostLink.Ready
            return@LaunchedEffect
        }
        // retain 必须在 connect 之前：它返回的纪元交给 connect 守门；否则在 retain 与建连之间
        // 一次强制断开会让我们装进一个无人持有的孤儿连接。
        val epoch = connector.retain(hostId, holder)
        val host = runCatching { hosts.get(hostId) }.getOrNull()
        if (host == null) {
            state.value = HostLink.Failed("找不到这台主机")
            return@LaunchedEffect
        }
        state.value =
            try {
                connector.connect(host, expectedGen = epoch)
                HostLink.Ready
            } catch (e: CancellationException) {
                throw e // 取消不是失败，原样放行
            } catch (
                @Suppress("TooGenericExceptionCaught") e: Exception,
            ) {
                // 接不上要说出来，别退化成一片空白、看起来像「没有对话」
                HostLink.Failed(e.message?.takeIf { it.isNotBlank() } ?: e.javaClass.simpleName)
            }
    }
    return state
}

/** 对话列表屏的持有者标识。 */
fun conversationsHolder(hostId: String): String = "conversations-$hostId"

/** 聊天面的持有者标识。前缀 `chat-` 是约定，别改。 */
fun chatHolder(sessionId: String): String = "chat-$sessionId"
