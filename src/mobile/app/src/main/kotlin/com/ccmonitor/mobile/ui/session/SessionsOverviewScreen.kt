package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.data.repo.HostRepository
import org.koin.compose.koinInject

/**
 * 多 tab 的会话总览屏。列出已开会话（[SessionTabManager]），点进入，× 关闭。
 * 回到具体会话不断连（VM 挂在 SessionTabManager 的 per-key store）；只有这里的 × 才真关，并释放连接引用。
 */
@Composable
fun SessionsOverviewScreen(
    onOpen: (key: String) -> Unit,
    onNewSession: () -> Unit,
    onBack: () -> Unit,
) {
    val tabManager = koinInject<SessionTabManager>()
    val hostRepo = koinInject<HostRepository>()
    val sessions by tabManager.sessions.collectAsState()
    val hosts by hostRepo.observeAll().collectAsState(initial = emptyList())

    // 底部导航条 inset，否则列表末项的「× 关闭」落进系统手势条热区。
    // 本屏不是 Scaffold 宿主（AppNavHost 只有 Surface），也没有键盘输入面，navigationBarsPadding 只叠一层。
    Column(Modifier.fillMaxSize().navigationBarsPadding()) {
        Row(
            Modifier.fillMaxWidth().padding(16.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = onBack) { Text("← 返回") }
                Text("会话 (${sessions.size})", style = MaterialTheme.typography.titleLarge)
            }
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                // 关闭全部：多 tab 保活时的显式收口，断开所有连接，停掉前台服务耗电。
                if (sessions.isNotEmpty()) {
                    TextButton(onClick = { sessions.forEach { tabManager.close(it.key) } }) { Text("关闭全部") }
                }
                TextButton(onClick = onNewSession) { Text("+ 新会话") }
            }
        }
        if (sessions.isEmpty()) {
            Box(Modifier.fillMaxSize(), Alignment.Center) {
                Text("还没有打开的会话。点「+ 新会话」选主机连接。", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        } else {
            LazyColumn(Modifier.fillMaxSize()) {
                items(sessions, key = { it.key }) { s ->
                    val label = hosts.firstOrNull { it.id == s.hostId }?.label ?: s.hostId
                    // 区分 tab 目标：终端 shell 还是 SFTP 文件浏览。
                    val isSftp = s.target is SessionTabManager.TabTarget.Sftp
                    Row(
                        Modifier.fillMaxWidth().clickable { onOpen(s.key) }.padding(horizontal = 16.dp, vertical = 12.dp),
                        horizontalArrangement = Arrangement.SpaceBetween,
                        verticalAlignment = Alignment.CenterVertically,
                    ) {
                        Column(Modifier.weight(1f)) {
                            Text("${if (isSftp) "📁" else "▷"}  $label", style = MaterialTheme.typography.titleMedium)
                            // 终端 tab 副标题：有 label（resume 会话的可读标题）先显 label，其次 tmux 会话名
                            // （attach tab 的名，或 resume tab 由 sid 推导的 cc-<sid8>），都没有则显 cd。
                            val term = s.target as? SessionTabManager.TabTarget.Terminal
                            val tmuxName = term?.tmuxSession ?: term?.resumeSessionId?.let(ClaudeInvocation::resumeSessionName)
                            val sub =
                                when {
                                    isSftp -> "SFTP 文件"
                                    s.label != null -> s.label + (tmuxName?.let { " · tmux: $it" } ?: "")
                                    tmuxName != null -> "tmux: $tmuxName"
                                    else -> term?.cd
                                }
                            sub?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                        }
                        TextButton(onClick = { tabManager.close(s.key) }) { Text("× 关闭") }
                    }
                    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                }
            }
        }
    }
}
