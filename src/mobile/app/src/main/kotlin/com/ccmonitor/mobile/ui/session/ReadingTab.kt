package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.ui.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.chat.ChatController
import com.ccmonitor.mobile.ui.chat.ChatOpen
import com.ccmonitor.mobile.ui.chat.ChatTranscript
import com.ccmonitor.mobile.ui.chat.existingChat
import com.ccmonitor.mobile.ui.chat.rememberChatHost
import com.ccmonitor.mobile.ui.chat.sessionOf
import com.ccmonitor.mobile.ui.overview.HistoryItem
import com.ccmonitor.mobile.ui.overview.conversationsViewModel
import org.koin.compose.koinInject

/**
 * 阅读子页读哪一条：钉住的（历史页点的）＞ resume 出来的那条 ＞ 这个目录最近的一条 ＞（不知道目录）这台最近的一条。
 * 「最近」是核心排的先后（`history-list` 按最后活动），这里只按 sid · 目录挑，不判。这个目录没有会话 ⇒ `null`。
 */
internal fun readingSid(
    rows: List<HistoryItem>,
    pinned: String?,
    resumed: String?,
    cwd: String?,
): String? =
    pinned ?: resumed ?: if (cwd == null) rows.firstOrNull()?.sessionId else rows.firstOrNull { it.projectPath == cwd }?.sessionId

/** 阅读子页打开一条会话：与聊天屏点进来的是同一个 key（[existingChat]），取到的是同一条 `ChatSession`。 */
internal fun readingChat(
    hostId: String,
    sid: String,
    machine: String,
): ChatOpen = existingChat(hostId, sid, machine)

/**
 * 终端屏的「阅读」子页：同聊天屏一套 —— 清单读 `history-list`，正文是聊天屏那条 `ChatSession`
 *（`history-tail` / `history-read` ＋ 流上的 `line` 帧 ⇒ `RecordUnits`），画的是同一个 [ChatTranscript]。哪一家都一样。
 */
@Composable
internal fun ReadingTab(
    hostId: String,
    /** 终端这边的目录（可被 tmux 会话「读」重定向）。 */
    cwd: String?,
    /** 历史页点的那一条。 */
    pinnedSid: String?,
    /** resume 出来的那条（resume 标签）。 */
    resumedSid: String?,
    onResume: (sessionId: String, cwd: String?, title: String?, command: String?) -> Unit,
    onShowHistory: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val host = rememberChatHost(hostId) ?: return
    val list = conversationsViewModel(hostId, host.machine)
    val state by list.state.collectAsStateWithLifecycle()
    val rows = state.recent
    // 下拉里换一条：只在这一次钉住 / 目录下有效。
    var picked by rememberSaveable(pinnedSid, cwd) { mutableStateOf<String?>(null) }
    val sid = picked ?: readingSid(rows, pinnedSid, resumedSid, cwd)
    val current = rows.firstOrNull { it.sessionId == sid }
    Column(modifier.fillMaxSize()) {
        ReadingToolbar(
            rows = rows,
            current = current,
            onPick = { picked = it },
            // 正在跑的那条（会话帧里有它）不再起 --resume：两个进程会写同一份记录。
            onResume = current?.takeIf { it.status == null }?.let { c -> { onResume(c.sessionId, c.projectPath, c.label, null) } },
            onShowHistory = onShowHistory,
        )
        Box(Modifier.fillMaxWidth().weight(1f)) {
            val problem = state.linkProblem ?: state.listProblem
            when {
                sid != null -> ReadingTranscript(hostId, sid, host.machine)
                problem != null -> Centered { Text(problem.text, color = MaterialTheme.colorScheme.error) }
                state.connecting || state.listLoading -> Centered { CircularProgressIndicator() }
                else -> Centered { Text(copyText("history.list.empty"), color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
        }
    }
}

/** 一条会话的正文：聊天屏那一条 `ChatSession`（同一个 key），画同一个 [ChatTranscript]。 */
@Composable
private fun ReadingTranscript(
    hostId: String,
    sid: String,
    machine: String,
) {
    val controller = koinInject<ChatController>()
    val backends = koinInject<HostBackends>()
    // 不经 ViewModel 持有：聊天屏离开时会 release 这一条，下次进来要能重新取到一条活的。
    val session = remember(hostId, sid) { controller.sessionOf(readingChat(hostId, sid, machine)) { h -> backends.of(h).also { it.ensure() } } }
    LaunchedEffect(session) { session.open() }
    val state by session.state.collectAsStateWithLifecycle()
    ChatTranscript(state = state, modifier = Modifier.fillMaxSize(), onLoadOlder = session::loadOlder, onRetrySend = session::retry)
}

/** 顶部工具条：换一条（这台的清单）· resume · 历史。 */
@Composable
private fun ReadingToolbar(
    rows: List<HistoryItem>,
    current: HistoryItem?,
    onPick: (String) -> Unit,
    /** `null` ＝ 这一条续不了（没选 · 正在跑）。 */
    onResume: (() -> Unit)?,
    onShowHistory: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        var menuOpen by remember { mutableStateOf(false) }
        Box(Modifier.weight(1f)) {
            TextButton(onClick = { menuOpen = true }) {
                Text("${current?.label ?: ""} ▾", style = monoSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                rows.forEach { r ->
                    DropdownMenuItem(
                        text = { Text(r.label, style = monoSmall, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                        onClick = {
                            onPick(r.sessionId)
                            menuOpen = false
                        },
                    )
                }
            }
        }
        TextButton(onClick = { onResume?.invoke() }, enabled = onResume != null) { Text("▶ resume", style = monoSmall) }
        TextButton(onClick = onShowHistory) { Text("历史", style = monoSmall) }
    }
}

@Composable
private fun Centered(content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize().padding(24.dp), Alignment.Center) { content() }
}
