package com.ccmonitor.mobile.ui.chat

import android.content.Intent
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.material3.rememberDrawerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.service.SshKeepAliveService
import com.ccmonitor.mobile.ui.common.HostLink
import com.ccmonitor.mobile.ui.common.chatHolder
import com.ccmonitor.mobile.ui.common.rememberHostLink
import com.ccmonitor.mobile.ui.drawer.ChatDrawer
import com.ccmonitor.mobile.ui.drawer.DRAWER_OPEN_DESCRIPTION
import com.ccmonitor.mobile.ui.drawer.DRAWER_OPEN_GLYPH
import com.ccmonitor.mobile.ui.drawer.DrawerAction
import kotlinx.coroutines.launch
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf
import java.util.UUID

/**
 * 生产的聊天屏：一条会话的完整接线。起会话、送字、正文全走那台核心的常驻流（[HostBackends]），见 [ChatSession]。
 * 对话身份是 [chatKey]，不是屏幕实例：对话活在应用级 `ChatController`，用稳定的 key 取，进屏才不会新建对话。
 *
 * 本文件不写 `onDispose`：它在每次离屏时都跑，包括进设置再回来；返回上一屏不算离开对话。
 * `release` 由导航层在「返回 = 离开这个对话」时调。
 */
@Composable
fun ChatRoute(
    hostId: String,
    /** 已有会话的 sid；新建会话是这张草稿的票（[newConversationId]）。 */
    sessionId: String,
    /** 抽屉里点下去的事，去哪由导航层定；不给默认值，免得某个调用方静默焊死整个抽屉。 */
    onDrawerAction: (DrawerAction) -> Unit,
    /** 新建会话（发第一条时请那台起）；`false` ＝ 从列表点进来的那一条。 */
    isNew: Boolean = false,
    modifier: Modifier = Modifier,
) {
    // 先把连接建起来；持有挂在对话上，由 `ChatController.sessionFor(hostId = …)` 负责。
    val link by rememberHostLink(hostId, chatHolder(chatKey(hostId, sessionId)), retainWhileComposed = false)
    KeepAliveWhileLinked(link)
    val host = rememberChatHost(hostId) ?: return
    val target = if (isNew) ChatTarget.Draft(ticket = sessionId, cwd = host.cwd) else ChatTarget.Existing(sessionId)
    val vm: ChatViewModel =
        org.koin.androidx.compose
            .koinViewModel(key = chatKey(hostId, sessionId)) { parametersOf(ChatOpen(hostId, chatKey(hostId, sessionId), target, host.machine)) }
    val state by vm.state.collectAsStateWithLifecycle()

    val drawerState = rememberDrawerState(DrawerValue.Closed)
    val scope = rememberCoroutineScope()
    ChatDrawer(hostId, link is HostLink.Ready, drawerState, onDrawerAction) {
        Scaffold(
            modifier = modifier,
            topBar = { ChatTopBar(onOpenDrawer = { scope.launch { drawerState.open() } }) },
        ) { inner ->
            ChatScreen(
                state = state,
                onSend = vm::send,
                onRetrySend = vm::retry,
                onStop = vm::stop,
                onLoadOlder = vm::loadOlder,
                modifier = Modifier.fillMaxSize().padding(inner),
            )
        }
    }
}

/** 开一条对话要的那几样（DI 按类型取这一件，不按位置取几个裸串）。 */
data class ChatOpen(
    val hostId: String,
    val key: String,
    val target: ChatTarget,
    val machine: String,
)

/** 这台在手机上的名字 ＋ 新建会话起在哪（这台的默认工作目录，没填 ⇒ `~`）；还没读到 ⇒ `null`。 */
private data class ChatHost(
    val machine: String,
    val cwd: String,
)

@Composable
private fun rememberChatHost(hostId: String): ChatHost? {
    val hosts = koinInject<HostRepository>()
    val got by produceState<ChatHost?>(null, hostId) {
        val h = runCatching { hosts.get(hostId) }.getOrNull()
        value = ChatHost(machine = h?.label ?: hostId, cwd = h?.defaultWorkingDir?.trim()?.ifEmpty { null } ?: HOME_DIR)
    }
    return got
}

/** 新建会话的出厂目录（那台按它的家目录读）。 */
private const val HOME_DIR = "~"

/**
 * 聊天屏的顶栏：左上 ☰ 拉出抽屉。
 *
 * 右上 ⋯ 不画：它只放对这条对话本身做的事，那几项还没有。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChatTopBar(onOpenDrawer: () -> Unit) {
    TopAppBar(
        navigationIcon = {
            IconButton(
                onClick = onOpenDrawer,
                modifier = Modifier.semantics { contentDescription = DRAWER_OPEN_DESCRIPTION }.testTag(ChatTestTags.DRAWER_OPEN),
            ) {
                Text(DRAWER_OPEN_GLYPH, style = MaterialTheme.typography.titleLarge)
            }
        },
        title = { Text(CHAT_TITLE) },
    )
}

/** 落地屏的标题。不写主机名，界面上不出现「连接」那套说法。 */
internal const val CHAT_TITLE = "Claude"

/**
 * 连上主机就起前台保活服务。
 *
 * 服务按「有活动会话或有在飞轮次」存活，在飞轮次由 `ChatController.hasInFlightTurn` 持有；
 * 不在这里起，只走聊天屏的用户就收不到「离开 app 后回复完成」的推送。
 * 服务没会话也没在飞轮次时会自停，所以这里只管起、不管停。
 */
@Composable
private fun KeepAliveWhileLinked(link: HostLink) {
    val context = LocalContext.current
    LaunchedEffect(link is HostLink.Ready) {
        if (link is HostLink.Ready) {
            val app = context.applicationContext
            ContextCompat.startForegroundService(app, Intent(app, SshKeepAliveService::class.java))
        }
    }
}

/**
 * 对话的稳定标识。
 *
 * 一个 Claude 对话由「哪台主机 + 哪个对话编号」共同确定 —— 只用后者的话，
 * 两台主机上碰巧同号的对话会被当成同一个。
 */
fun chatKey(
    hostId: String,
    sessionId: String,
): String = "$hostId/$sessionId"

/**
 * 新建会话那张草稿的票：界面上的 [chatKey] 用它，起会话也带它（`session-new.ticket`：期限到了同一张再问，那台不起第二个）。
 * 会话起好之后的 sid 是那台报到时说的，不是它。
 */
fun newConversationId(): String = UUID.randomUUID().toString()
