package com.ccmonitor.mobile.ui.overview

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.ui.common.HostLink
import com.ccmonitor.mobile.ui.common.conversationsHolder
import com.ccmonitor.mobile.ui.common.rememberHostLink
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf

/**
 * 「对话」这一屏的入口：先建 SSH 连接（`HostLink`），通了再在上面接那台的常驻流、读核心的清单。
 */
@Composable
fun ConversationsRoute(
    hostId: String,
    onOpen: (sessionId: String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val link by rememberHostLink(hostId, conversationsHolder(hostId))
    val machine = rememberMachineName(hostId)
    Scaffold(modifier = modifier) { inner ->
        when (val l = link) {
            is HostLink.Failed -> HostLinkFailed(l.why)
            HostLink.Connecting -> Unit
            HostLink.Ready ->
                if (machine != null) {
                    val vm = conversationsViewModel(hostId, machine)
                    val state by vm.state.collectAsStateWithLifecycle()
                    ConversationsScreen(state, machine, onOpen, vm::retry, Modifier.fillMaxSize().padding(inner))
                }
        }
    }
}

/** 这台在手机上的名字（机器表里的 `label`）；还没读到 ⇒ `null`。 */
@Composable
private fun rememberMachineName(hostId: String): String? {
    val hosts = koinInject<HostRepository>()
    val name by produceState<String?>(null, hostId) { value = runCatching { hosts.get(hostId)?.label }.getOrNull() ?: hostId }
    return name
}

/** 一台的清单 VM。「对话」这一屏与抽屉「最近」取同一个（同一个 key）。 */
@Composable
private fun conversationsViewModel(
    hostId: String,
    machine: String,
): ConversationsViewModel = koinViewModel(key = "conversations-$hostId") { parametersOf(hostId, machine) }

/**
 * 抽屉「最近」：与「对话」这一屏同一个来源。
 *
 * @param linkReady 这台的 SSH 连接通了没有。没通 ⇒ `null`。
 * @return `null` ＝ 还没拿到；拿到后是清单最前面的那些行（核心已按最后活动排好）。
 */
@Composable
fun rememberRecent(
    hostId: String,
    linkReady: Boolean,
): List<HistoryItem>? {
    val machine = rememberMachineName(hostId)
    if (!linkReady || machine == null) return null
    val state by conversationsViewModel(hostId, machine).state.collectAsStateWithLifecycle()
    return state.recent.takeIf { !state.listLoading }
}

/** SSH 都接不上时那一句（建连那一层给的原因）。 */
@Composable
private fun HostLinkFailed(why: String) {
    Text(why, modifier = Modifier.fillMaxSize().padding(24.dp))
}
