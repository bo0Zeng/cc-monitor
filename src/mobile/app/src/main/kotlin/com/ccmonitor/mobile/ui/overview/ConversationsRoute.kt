package com.ccmonitor.mobile.ui.overview

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.claude.transport.DaemonLocator
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ssh.commandChannel
import com.ccmonitor.mobile.ui.common.HostLink
import com.ccmonitor.mobile.ui.common.Settled
import com.ccmonitor.mobile.ui.common.conversationsHolder
import com.ccmonitor.mobile.ui.common.ifKnown
import com.ccmonitor.mobile.ui.common.rememberHostLink
import com.ccmonitor.mobile.ui.common.rememberSettled
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf

/**
 * 生产的对话总览屏。
 *
 * 来源路径从 `SettingsRepository.overviewSourcePath()` 读，没设过用 [DEFAULT_SOURCE_PATH]。
 * 采集初值是「还不知道」而不是回退值，见 [overviewSourceOrUnknown]。
 */
@Composable
fun ConversationsRoute(
    hostId: String,
    onOpen: (sessionId: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val manager = koinInject<SshConnectionManager>()
    val settings = koinInject<SettingsRepository>()
    // 三值采集：初值是「还不知道」，不是回退值。
    val source by rememberSettled(settings) { settings.overviewSourcePath() }

    // 先建连，否则 `commandChannel` 没有活连接可用。
    // 建连不进下面的 `when`：它与来源路径读没读到无关，放进去会让未知那一帧白白不建连。
    val link by rememberHostLink(hostId, conversationsHolder(hostId))

    // 开新对话只在抽屉里，这一屏不另给按钮。
    Scaffold(
        modifier = modifier,
    ) { inner ->
        val sourcePath = overviewSourceOrUnknown(source)
        when {
            // 未知 ⇒ 不建 VM、不发查询。屏上就是 VM 的初始那一帧（`loading = true`），不是新加的空白。
            sourcePath == null ->
                SessionOverviewScreen(
                    state = SessionOverviewUiState(loading = true),
                    onOpen = {},
                    modifier = Modifier.fillMaxSize().padding(inner),
                )
            // 接不上与「连上了但没有对话」是两件事，不混成一句「读不到」。
            link is HostLink.Failed -> HostLinkFailed((link as HostLink.Failed).why)
            else ->
                ConversationsList(
                    hostId = hostId,
                    sourcePath = sourcePath,
                    manager = manager,
                    onOpen = onOpen,
                    modifier = Modifier.fillMaxSize().padding(inner),
                )
        }
    }
}

/**
 * 「来源路径还没读到」与「用户没设过」是两件事。
 *
 * | 采集到的 | 出口 |
 * |---|---|
 * | [Settled.Unknown]（还没读到） | `null` = 什么都不做（不建 VM、不发查询） |
 * | [Settled.Known]`(null)`（读到了，用户没设过） | [DEFAULT_SOURCE_PATH] |
 * | [Settled.Known]`(路径)` | 那个路径 |
 *
 * 若拿回退值当初值：VM 的 `init` 构造即发查询，第一帧会用占位名对远端多发一次 exec，
 * 真值到后 key 变了再建第二个 VM，第一个留在 `ViewModelStore` 里不被清。
 */
internal fun overviewSourceOrUnknown(source: Settled<String?>): String? = source.ifKnown { it ?: DEFAULT_SOURCE_PATH }

/**
 * 列表那一支，只在来源路径真的读到后才组合。
 *
 * 抽成独立 composable：`koinViewModel` 的 key 带 [sourcePath]，必须落在「路径已知」那一支里。
 */
@Composable
private fun ConversationsList(
    hostId: String,
    sourcePath: String,
    manager: SshConnectionManager,
    onOpen: (sessionId: String) -> Unit,
    modifier: Modifier,
) {
    val vm = conversationsViewModel(hostId, sourcePath, manager)
    val state by vm.state.collectAsStateWithLifecycle()
    SessionOverviewScreen(
        state = state,
        onOpen = { row -> onOpen(row.sessionId) },
        onReconnect = vm::reconnect,
        modifier = modifier,
        // 历史行走同一条 `chat/{sid}` 路由：活没活着不决定用哪个入口打开。
        onOpenConversation = { row -> onOpen(row.sessionId) },
        onLoadOlder = vm::loadOlderConversations,
    )
}

/**
 * 一台服务器的对话列表 VM。「对话」这一屏与抽屉「最近」取同一个，只有一处读法。
 * 只能在连接已通、来源路径已读到之后调（`commandChannel` 无活连接就抛）。
 */
@Composable
private fun conversationsViewModel(
    hostId: String,
    sourcePath: String,
    manager: SshConnectionManager,
): SessionOverviewViewModel =
    koinViewModel(key = "conversations-$hostId-$sourcePath") {
        parametersOf(manager.commandChannel(hostId), sourcePath)
    }

/**
 * 抽屉「最近」用的那份历史：与「对话」这一屏同一个来源，只收历史那半、不起实时流。
 *
 * @param linkReady 这台的连接通了没有（由调用方的那条连接给）。没通 ⇒ null。
 * @return null = 还没拿到（连接没通、来源路径没读到）；拿到后是历史翻页的现状。
 */
@Composable
fun rememberConversationHistory(
    hostId: String,
    linkReady: Boolean,
): HistoryPaging? {
    val manager = koinInject<SshConnectionManager>()
    val settings = koinInject<SettingsRepository>()
    val source by rememberSettled(settings) { settings.overviewSourcePath() }
    val sourcePath = overviewSourceOrUnknown(source)
    if (!linkReady || sourcePath == null) return null
    val vm = conversationsViewModel(hostId, sourcePath, manager)
    val history by vm.conversationHistory.collectAsStateWithLifecycle()
    return history
}

/**
 * 没配过来源路径时的占位名，也是给 [com.ccmonitor.mobile.core.claude.transport.DaemonLocator] 的信号：
 * `daemonPath` 等于它 ⇒ 用户没填 ⇒ 按候选表定位（远端肯定答复才算数，定位时不执行候选）。
 * 二进制改名的兼容由候选表吸收，不改这个常量。
 *
 * 不用空串：空串会静默查出「没有对话」。不用 `~/…` 路径：[com.ccmonitor.mobile.core.remote.shellQuote] 无条件加单引号，`~` 不展开。
 * 注意：探测发的是裸命令（还不知道对端能力时不带 flag）。若这个名字指向 bash 启动器（如 `ccm`），
 * 裸跑会在远端起一条新会话。换名字前需手动在真机上裸跑一次，确认会话数不变。
 */
const val DEFAULT_SOURCE_PATH = DaemonLocator.UNSET_PLACEHOLDER

/** 接不上那台主机时的说明，与「连上了但没有对话」分开。 */
@Composable
private fun HostLinkFailed(why: String) {
    Text(
        "接不上这台主机：$why",
        modifier = Modifier.fillMaxSize().padding(24.dp),
    )
}
