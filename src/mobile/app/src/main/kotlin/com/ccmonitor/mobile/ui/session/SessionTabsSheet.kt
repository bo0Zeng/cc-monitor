package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import org.koin.compose.koinInject

/**
 * 会话 tab 面板（[ModalBottomSheet]），由终端 / SFTP 工具条左端的按钮唤出；工具条在 `imePadding()` 内，键盘弹起也够得着。
 * 列出全部已开 tab（类型符 ▷/📁、可读标签、当前高亮、× 关闭）；点击经 [onSwitchTab] 切换
 * （导航在 AppNavHost：launchSingleTop + popUpTo，点当前 tab 直接短路）。
 * 底部「＋」二选：[onNewTerminalTab] 为当前主机强制新开终端 tab（forceNew=true）；
 * [onNewSftpTab] 打开或聚焦当前主机的 SFTP tab（forceNew=false，每主机至多一个）。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SessionTabsSheet(
    currentKey: String?,
    onSwitchTab: (key: String) -> Unit,
    onNewTerminalTab: () -> Unit,
    onNewSftpTab: () -> Unit,
    onDismiss: () -> Unit,
) {
    val tabManager = koinInject<SessionTabManager>()
    val hostRepo = koinInject<HostRepository>()
    val sessions by tabManager.sessions.collectAsState()
    val hosts by hostRepo.observeAll().collectAsState(initial = emptyList())
    // 可读标签兜底链，加上 forceNew 同名副本的渲染期编号（·2/·3）；元数据与去重键不掺渲染修饰。
    // 编号对含类型符的完整显示文本做：forceNew 的副本类型符相同，仍会编号；
    // 同主机一个终端加一个 SFTP，类型符已能区分，不编号。
    val labels =
        remember(sessions, hosts) {
            numberDuplicates(
                sessions.map { s ->
                    val symbol = if (s.target is SessionTabManager.TabTarget.Sftp) "📁" else "▷"
                    "$symbol ${tabBaseLabel(s, hosts.firstOrNull { it.id == s.hostId }?.label ?: s.hostId)}"
                },
            )
        }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(bottom = 24.dp)) {
            Text("会话 tab (${sessions.size})", style = MaterialTheme.typography.titleMedium)
            if (sessions.isEmpty()) {
                Text(
                    "没有打开的 tab",
                    style = monoSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(vertical = 12.dp),
                )
            } else {
                LazyColumn(Modifier.fillMaxWidth().heightIn(max = 480.dp).padding(top = 8.dp)) {
                    itemsIndexed(sessions, key = { _, s -> s.key }) { i, s ->
                        SessionTabRowItem(
                            text = labels[i],
                            selected = s.key == currentKey,
                            onClick = { onSwitchTab(s.key) },
                            onClose = { tabManager.close(s.key) },
                        )
                    }
                }
            }
            Row(
                Modifier.fillMaxWidth().padding(top = 8.dp),
                horizontalArrangement = Arrangement.spacedBy(8.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                TextButton(onClick = onNewTerminalTab) { Text("＋ 终端 tab") }
                TextButton(onClick = onNewSftpTab) { Text("＋ 文件 tab") }
            }
        }
    }
}

/** 单行 tab 条目——点主体切换，点 × 关闭（内层 clickable 优先消费，故 × 不触发切换）；当前 tab 高亮。 */
@Composable
private fun SessionTabRowItem(
    text: String,
    selected: Boolean,
    onClick: () -> Unit,
    onClose: () -> Unit,
) {
    val bg = if (selected) MaterialTheme.colorScheme.primaryContainer else Color.Transparent
    val fg = if (selected) MaterialTheme.colorScheme.onPrimaryContainer else MaterialTheme.colorScheme.onSurface
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(8.dp))
            .background(bg)
            .clickable(onClick = onClick)
            .padding(start = 12.dp, end = 4.dp, top = 10.dp, bottom = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            text,
            color = fg,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
            style = MaterialTheme.typography.bodyMedium,
            modifier = Modifier.weight(1f),
        )
        Box(
            Modifier
                .clip(RoundedCornerShape(50))
                .clickable(onClick = onClose)
                .padding(horizontal = 10.dp, vertical = 2.dp),
        ) {
            Text("×", color = fg, style = MaterialTheme.typography.labelLarge)
        }
    }
}

/**
 * tab 的基础可读标签。后缀兜底链：[SessionTabManager.OpenSession.label]
 * （resume 会话标题，建 tab 时同步传入）?: tmux 会话名（attach tab）?: `cc-<sid8>`
 * （resume tab 无 label 时由 sid 推导实际 tmux 名）?: 无后缀，即纯主机名。
 * 有后缀时格式 `主机名 · 后缀`；类型符（▷/📁）由渲染处另加，不入本函数。
 */
internal fun tabBaseLabel(
    s: SessionTabManager.OpenSession,
    hostLabel: String,
): String {
    val term = s.target as? SessionTabManager.TabTarget.Terminal
    val suffix = s.label ?: term?.tmuxSession ?: term?.resumeSessionId?.let(ClaudeInvocation::resumeSessionName)
    return if (suffix != null) "$hostLabel · $suffix" else hostLabel
}

/**
 * 渲染期给重复标签编号：同名标签第 2、3 次出现补 ` ·2`/` ·3`，首个不编号，
 * 让 forceNew 产生的同名副本在面板里可区分。只是渲染修饰，不进 tab 元数据与去重键。
 */
internal fun numberDuplicates(labels: List<String>): List<String> {
    val seen = HashMap<String, Int>()
    return labels.map { l ->
        val n = (seen[l] ?: 0) + 1
        seen[l] = n
        if (n > 1) "$l ·$n" else l
    }
}
