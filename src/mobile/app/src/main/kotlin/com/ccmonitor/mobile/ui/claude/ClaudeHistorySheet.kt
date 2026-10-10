@file:Suppress("MatchingDeclarationName") // 文件按「历史记录面板」主题聚合 Sheet + 段 / 行渲染

package com.ccmonitor.mobile.ui.claude

import android.widget.Toast
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.ui.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ui.chat.rememberChatHost
import com.ccmonitor.mobile.ui.overview.HistoryItem
import com.ccmonitor.mobile.ui.overview.HistorySection
import com.ccmonitor.mobile.ui.overview.conversationsViewModel

/** 长按菜单里的一个 launcher 命令选项：显示的 label 与实际 command。 */
data class ResumeLauncherOption(
    val label: String,
    val command: String,
)

/**
 * 「历史记录」面板：这台核心的会话清单（`history-list`，与「对话」那一屏同一个清单 VM）。段头 · 标题 · 时刻都是核心写好的，照抄。
 * - 点任意行是阅读（[onRead]）：关面板、切到阅读 tab、钉住那一条。
 * - 长按是选命令 resume（[onResume]）：正在跑的那条（会话帧里有它）不给，那会让两个进程写同一份记录。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ClaudeHistorySheet(
    hostId: String,
    launchers: List<ResumeLauncherOption>,
    lastCustomCommand: String,
    onRead: (sessionId: String) -> Unit,
    // 长按菜单选定后 resume。cwd 是核心给的项目目录（可 null，远端按 sid 兜底）；
    // command 是选定命令（null 即「claude（默认）」，走默认候选链）。参数顺序 title 在前、command 收尾。
    onResume: (cwd: String?, sessionId: String, title: String, command: String?) -> Unit,
    onCustomCommandUsed: (String) -> Unit, // 记住本次自定义命令，下次预填
    onDismiss: () -> Unit,
) {
    val host = rememberChatHost(hostId) ?: return
    val state by conversationsViewModel(hostId, host.machine).state.collectAsStateWithLifecycle()
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(bottom = 24.dp)) {
            Text("历史记录", style = MaterialTheme.typography.titleMedium)
            val problem = state.linkProblem ?: state.listProblem
            when {
                problem != null ->
                    Text(problem.text, color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(vertical = 16.dp))
                state.connecting || state.listLoading ->
                    Row(Modifier.padding(vertical = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                        CircularProgressIndicator(Modifier.size(18.dp))
                    }
                state.sections.isEmpty() ->
                    Text(copyText("history.list.empty"), color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(vertical = 16.dp))
                else -> HistorySectionList(state.sections, launchers, lastCustomCommand, onRead, onResume, onCustomCommandUsed)
            }
        }
    }
}

/** 段列表：段头（核心写的「今天」「昨天」…）＋ 那几行。 */
@Composable
private fun HistorySectionList(
    sections: List<HistorySection>,
    launchers: List<ResumeLauncherOption>,
    lastCustomCommand: String,
    onRead: (sessionId: String) -> Unit,
    onResume: (cwd: String?, sessionId: String, title: String, command: String?) -> Unit,
    onCustomCommandUsed: (String) -> Unit,
) {
    LazyColumn(Modifier.fillMaxWidth().heightIn(max = 560.dp).padding(top = 8.dp)) {
        sections.forEachIndexed { i, sec ->
            sec.title?.let { t ->
                item(key = "sec-$i") {
                    Text(t, style = MaterialTheme.typography.titleSmall, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(top = 12.dp, bottom = 4.dp))
                }
            }
            items(sec.rows, key = { it.sessionId }) { r ->
                HistorySessionRow(r, launchers, lastCustomCommand, onRead, onResume, onCustomCommandUsed)
            }
        }
    }
}

/** 单条历史会话行：点击即阅读，长按弹命令菜单（正在跑的 · sid 不合法的不弹）。 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun HistorySessionRow(
    s: HistoryItem,
    launchers: List<ResumeLauncherOption>,
    lastCustomCommand: String,
    onRead: (sessionId: String) -> Unit,
    onResume: (cwd: String?, sessionId: String, title: String, command: String?) -> Unit,
    onCustomCommandUsed: (String) -> Unit,
) {
    val context = LocalContext.current
    var menuOpen by remember { mutableStateOf(false) }
    var customOpen by remember { mutableStateOf(false) }
    val resumable = s.status == null && ClaudeInvocation.isValidSessionId(s.sessionId)
    Box {
        Column(
            Modifier
                .fillMaxWidth()
                .combinedClickable(
                    onClick = { onRead(s.sessionId) },
                    onLongClick = {
                        if (resumable) {
                            menuOpen = true
                        } else {
                            Toast.makeText(context, s.status?.text ?: "会话 ID 非法，无法 resume", Toast.LENGTH_SHORT).show()
                        }
                    },
                ).padding(vertical = 8.dp),
        ) {
            HistorySessionRowTexts(s, resumable)
        }
        ResumeCommandMenu(
            expanded = menuOpen,
            launchers = launchers,
            onPick = { command ->
                menuOpen = false
                onResume(s.projectPath, s.sessionId, s.label, command)
            },
            onCustom = {
                menuOpen = false
                customOpen = true
            },
            onDismiss = { menuOpen = false },
        )
    }
    if (customOpen) {
        CustomCommandDialog(
            initial = lastCustomCommand,
            onConfirm = { command ->
                customOpen = false
                onCustomCommandUsed(command) // 记住本次输入（只有合法输入才走到这里）
                onResume(s.projectPath, s.sessionId, s.label, command)
            },
            onDismiss = { customOpen = false },
        )
    }
}

/** 行文本：标题（正在跑的前面是会话帧那一句）· 时刻 · 目录，都是核心写好的。 */
@Composable
private fun HistorySessionRowTexts(
    s: HistoryItem,
    resumable: Boolean,
) {
    val titleColor = if (resumable) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant
    val title = s.status?.let { "● ${it.text} · ${s.label}" } ?: s.label
    Text(title, color = titleColor, maxLines = 1, overflow = TextOverflow.Ellipsis)
    val sub = listOfNotNull(s.atText, s.agentTag, s.projectPath).joinToString(" · ")
    if (sub.isNotEmpty()) Text(sub, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
}

/** 长按命令菜单：主机各 launcher（label+command）、「claude（默认）」（command=null 走默认链）、「自定义命令…」。 */
@Composable
private fun ResumeCommandMenu(
    expanded: Boolean,
    launchers: List<ResumeLauncherOption>,
    onPick: (command: String?) -> Unit,
    onCustom: () -> Unit,
    onDismiss: () -> Unit,
) {
    DropdownMenu(expanded = expanded, onDismissRequest = onDismiss) {
        launchers.forEach { option ->
            DropdownMenuItem(
                text = {
                    Column {
                        Text(option.label, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        Text(option.command, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                },
                onClick = { onPick(option.command) },
            )
        }
        DropdownMenuItem(text = { Text("claude（默认）") }, onClick = { onPick(null) })
        DropdownMenuItem(text = { Text("自定义命令…") }, onClick = onCustom)
    }
}

/**
 * 自定义 resume 命令弹窗：输入须原样通过 `ClaudeInvocation.sanitizeLaunchCommand` 才放行，非法就地拒并显示原因，
 * 不静默回退 claude；预填上次输入。确认后在远端跑 `<命令> --resume <sid>`。
 */
@Composable
private fun CustomCommandDialog(
    initial: String,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var text by remember { mutableStateOf(initial) }
    var error by remember { mutableStateOf<String?>(null) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("自定义 resume 命令") },
        text = {
            Column {
                Text("将以「<命令> --resume <会话id>」在远端交互 shell 执行（支持别名/带参形态）：", style = MaterialTheme.typography.bodySmall)
                OutlinedTextField(
                    value = text,
                    onValueChange = {
                        text = it
                        error = null
                    },
                    singleLine = true,
                    placeholder = { Text("cc / cct / claude --model opus …") },
                    modifier = Modifier.fillMaxWidth().padding(top = 8.dp),
                )
                error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            }
        },
        confirmButton = {
            TextButton(onClick = {
                val t = text.trim()
                if (t.isNotEmpty() && ClaudeInvocation.sanitizeLaunchCommand(t) == t) {
                    onConfirm(t)
                } else {
                    error = "命令为空或含注入元字符（; | & \$ ` > < 换行），已拒绝"
                }
            }) { Text("续跑") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}
