@file:Suppress("MatchingDeclarationName") // 文件按「历史记录面板」主题聚合 Sheet + 分组/行渲染 + 组键/相对时间纯函数

package com.ccmonitor.mobile.ui.claude

import android.widget.Toast
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.clickable
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
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.catalog.ClaudeSessionCatalog
import com.ccmonitor.mobile.core.claude.catalog.HistorySession
import com.ccmonitor.mobile.core.claude.catalog.ProjectHistory
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.transport.ClaudePaths
import com.ccmonitor.mobile.core.remote.ConnectionDeadException
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.flowOn
import java.io.IOException

/** 长按菜单里的一个 launcher 命令选项：显示的 label 与实际 command。 */
data class ResumeLauncherOption(
    val label: String,
    val command: String,
)

/**
 * 「历史记录」面板：全项目的历史会话按项目分组。
 * - 分组默认全部折叠，点组头 `▸/▾` 展开或收起。
 * - 点任意行是阅读（[onRead]，不设守卫，活会话或无 cwd 也可读）：关面板、切到阅读 tab、钉住该 JSONL。
 * - 长按是选命令 resume（[onResume]）。可 resume 的条件是 `!live && sid 合法`，cwd 不必需（resume 时远端按 sid
 *   定位 jsonl 读 cwd）。菜单列主机各 launcher（[launchers]）、「claude（默认）」与「自定义命令…」；
 *   不可 resume 时 Toast 说明，不弹菜单。
 *
 * 数据：复用当前连接的 [channel] 与 [claudeDir]，不新开连接，走 [ClaudeSessionCatalog.historyByProject] 的渐进 Flow：
 * 骨架（sid8 标题、编码目录兜底分组）先出，标题探针分片到货后标题与真实 cwd 逐步落位。上游 `flowOn(IO)`，解析不占主线程。
 * 「无历史」与「未连接」两种空态靠 catch [ConnectionDeadException] 区分。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ClaudeHistorySheet(
    channel: RemoteCommandChannel,
    claudeDir: String?,
    launchers: List<ResumeLauncherOption>,
    lastCustomCommand: String,
    // 点击即阅读：cwd 可为 null（阅读不需要 cwd）；title 供阅读器的钉住项标题。
    onRead: (path: String, cwd: String?, sessionId: String, title: String) -> Unit,
    // 长按菜单选定后 resume。cwd 是批量探针解出的 fallbackCwd（可 null，远端按 sid 兜底）；
    // command 是选定命令（null 即「claude（默认）」，走默认候选链）。参数顺序 title 在前、command 收尾。
    onResume: (cwd: String?, sessionId: String, title: String, command: String?) -> Unit,
    onCustomCommandUsed: (String) -> Unit, // 记住本次自定义命令，下次预填
    onDismiss: () -> Unit,
) {
    val catalog = remember(channel, claudeDir) { ClaudeSessionCatalog(channel, ClaudePaths.resolveClaudeDir(claudeDir, null)) }
    var groups by remember { mutableStateOf<List<ProjectHistory>>(emptyList()) }
    var loading by remember { mutableStateOf(true) }
    var error by remember { mutableStateOf<String?>(null) }
    var connectionDead by remember { mutableStateOf(false) } // 断连单独归类，显「未连接」
    LaunchedEffect(catalog) {
        loading = true
        error = null
        connectionDead = false
        // historyByProject 内部的解析和探针失败都已降级，抛到这里的只有 jsonl 列表的 exec 通道错误：
        // 连接未建或已断抛 [ConnectionDeadException]（IOException 子类，须排在 IOException 之前），归「未连接」；
        // 其余 sshj 传输错误是 IOException 族，归「加载失败」。IllegalStateException 子句兜其他来源。
        // 注意：CancellationException 继承 IllegalStateException，必须最先重抛，离屏取消不置错误态。
        try {
            catalog.historyByProject().flowOn(Dispatchers.IO).collect {
                groups = it
                loading = false // 骨架首版即出列表（标题随后逐片刷新）
            }
        } catch (e: CancellationException) {
            throw e
        } catch (_: ConnectionDeadException) {
            connectionDead = true // 断连显「未连接」空态，不显误导性的「加载失败」
            loading = false
        } catch (e: IOException) {
            error = e.message ?: e.toString()
            loading = false
        } catch (e: IllegalStateException) {
            error = e.message ?: e.toString()
            loading = false
        }
    }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(bottom = 24.dp)) {
            Text("历史记录", style = MaterialTheme.typography.titleMedium)
            Text(
                "按项目分组 · 点击查看 · 长按选择命令续跑",
                style = monoSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            when {
                // 断连显「未连接」，不显误导性的「无历史」。
                connectionDead ->
                    Text("未连接——无法读取历史记录", color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(vertical = 16.dp))
                error != null ->
                    Text("加载失败: $error", color = MaterialTheme.colorScheme.error, modifier = Modifier.padding(vertical = 16.dp))
                loading ->
                    Row(Modifier.padding(vertical = 16.dp), verticalAlignment = Alignment.CenterVertically) {
                        CircularProgressIndicator(Modifier.size(18.dp))
                        Text("  加载中…", style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                groups.isEmpty() ->
                    // 走到这里必是连接活着且 historyByProject 成功返回空，确实没有 jsonl。
                    Text(
                        "无历史会话（该主机 Claude 数据目录下没有 .jsonl）",
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(vertical = 16.dp),
                    )
                else -> HistoryGroupList(groups, launchers, lastCustomCommand, onRead, onResume, onCustomCommandUsed)
            }
        }
    }
}

/**
 * 分组列表：LazyColumn（全史可达数百行）。分组默认全部折叠，组头可点展开或收起；会话行只在展开时发射。
 * 组键见 [historyGroupKey]（真实 cwd 优先、编码目录名兜底，前缀隔离防撞）。渐进加载中 cwd 落位会换键，
 * 该组回到折叠态；默认全折叠下影响很小。
 */
@Composable
private fun HistoryGroupList(
    groups: List<ProjectHistory>,
    launchers: List<ResumeLauncherOption>,
    lastCustomCommand: String,
    onRead: (path: String, cwd: String?, sessionId: String, title: String) -> Unit,
    onResume: (cwd: String?, sessionId: String, title: String, command: String?) -> Unit,
    onCustomCommandUsed: (String) -> Unit,
) {
    val nowSec = remember { System.currentTimeMillis() / 1000 } // 打开面板时刻——相对时间基准（够用，无需滴答刷新）
    var expandedKeys by remember { mutableStateOf(emptySet<String>()) } // 默认全部折叠
    LazyColumn(Modifier.fillMaxWidth().heightIn(max = 560.dp).padding(top = 8.dp)) {
        groups.forEach { g ->
            val key = historyGroupKey(g.cwd, g.sessions.firstOrNull()?.path)
            val expanded = key in expandedKeys
            item(key = "hdr-$key") {
                HistoryGroupHeader(
                    g = g,
                    expanded = expanded,
                    onToggle = { expandedKeys = if (expanded) expandedKeys - key else expandedKeys + key },
                )
            }
            if (expanded) {
                items(g.sessions, key = { it.path }) { s ->
                    HistorySessionRow(s, nowSec, launchers, lastCustomCommand, onRead, onResume, onCustomCommandUsed)
                }
            }
        }
    }
}

/** 可点组头：`▸/▾` 折叠指示、📁 项目名、会话数；副行是真实 cwd（未解析时给提示）。 */
@Composable
private fun HistoryGroupHeader(
    g: ProjectHistory,
    expanded: Boolean,
    onToggle: () -> Unit,
) {
    Column(Modifier.fillMaxWidth().clickable(onClick = onToggle)) {
        Row(
            Modifier.fillMaxWidth().padding(top = 12.dp, bottom = 4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                "${if (expanded) "▾" else "▸"} 📁 ${g.label}",
                style = MaterialTheme.typography.titleSmall,
                color = MaterialTheme.colorScheme.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f),
            )
            Text("${g.sessions.size} 个会话", style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        // 全路径小字（同名尾段的项目靠它区分）；cwd 未知的组显提示，resume 不受阻，远端按 sid 定位。
        Text(
            g.cwd ?: "（工作目录未解析——点击仍可阅读，续跑由远端定位）",
            style = monoSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant, modifier = Modifier.padding(top = 4.dp))
    }
}

/**
 * 单条历史会话行：点击即阅读（不设守卫），长按弹命令菜单（有守卫）。
 * 守卫是 `resumable = !live && sid 合法`，cwd 为 null 不算障碍（resume 时远端按 sid 查找）；
 * 不可 resume 时长按 Toast 说明原因（活会话防双写、sid 非法防注入），不弹菜单。
 * 活会话行显 `●`（primary 色点）加「活动中」；不可 resume 的行标灰，仍可点击阅读。
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun HistorySessionRow(
    s: HistorySession,
    nowSec: Long,
    launchers: List<ResumeLauncherOption>,
    lastCustomCommand: String,
    onRead: (path: String, cwd: String?, sessionId: String, title: String) -> Unit,
    onResume: (cwd: String?, sessionId: String, title: String, command: String?) -> Unit,
    onCustomCommandUsed: (String) -> Unit,
) {
    val context = LocalContext.current
    var menuOpen by remember { mutableStateOf(false) }
    var customOpen by remember { mutableStateOf(false) }
    val resumable = !s.live && ClaudeInvocation.isValidSessionId(s.sessionId)
    Box {
        Column(
            Modifier
                .fillMaxWidth()
                .combinedClickable(
                    onClick = { onRead(s.path, s.cwd, s.sessionId, s.title) }, // 点击=阅读，无守卫（活会话也可读）
                    onLongClick = {
                        if (resumable) {
                            menuOpen = true
                        } else {
                            Toast.makeText(context, resumeBlockReason(s), Toast.LENGTH_SHORT).show()
                        }
                    },
                ).padding(vertical = 8.dp),
        ) {
            HistorySessionRowTexts(s, nowSec, resumable)
        }
        ResumeCommandMenu(
            expanded = menuOpen,
            launchers = launchers,
            onPick = { command ->
                menuOpen = false
                onResume(s.cwd, s.sessionId, s.title, command)
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
                onResume(s.cwd, s.sessionId, s.title, command)
            },
            onDismiss = { customOpen = false },
        )
    }
}

/** 不可 resume 的行被长按时 Toast 的原因。 */
private fun resumeBlockReason(s: HistorySession): String =
    if (s.live) "该会话正在运行中（已是活动会话）——点击即可阅读" else "会话 ID 非法，无法 resume"

/** 行文本（标题、相对时间·sid8）：活会话 `●`(primary) 加「活动中」；不可 resume 的行标灰（仍可点读）。 */
@Composable
private fun HistorySessionRowTexts(
    s: HistorySession,
    nowSec: Long,
    resumable: Boolean,
) {
    val titleColor = if (resumable) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant
    if (s.live) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Text("●", color = MaterialTheme.colorScheme.primary)
            Text(" 活动中 · ${s.title}", color = titleColor, maxLines = 1, overflow = TextOverflow.Ellipsis)
        }
    } else {
        Text(s.title, color = titleColor, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
    val sub = listOfNotNull(relativeTimeLabel(s.mtimeEpochSec, nowSec), s.sessionId.take(8)).joinToString(" · ")
    Text(sub, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
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

/**
 * 分组折叠状态的组键：真实 cwd 优先，编码目录名（首会话 path 倒数第二段）兜底。
 * 沿用 catalog 的 `cwd:`/`enc:` 前缀隔离：cwd 是 JSONL 里可写的文本，构造的 cwd 不能与编码兜底组撞键；
 * 组键同时用作 LazyColumn item key，撞键即崩。
 */
internal fun historyGroupKey(
    cwd: String?,
    firstSessionPath: String?,
): String = cwd?.let { "cwd:$it" } ?: "enc:${firstSessionPath?.let(ClaudeSessionCatalog::encodedProjectDir).orEmpty()}"

/**
 * 相对时间标签。null（stat 不可用）返回 null，UI 隐藏；未来时间（远端时钟偏差）当「刚刚」；
 * 30 天及以上显绝对日期（默认时区）。
 */
internal fun relativeTimeLabel(
    epochSec: Long?,
    nowEpochSec: Long,
): String? {
    if (epochSec == null) return null
    val d = nowEpochSec - epochSec
    return when {
        d < 60 -> "刚刚"
        d < 3_600 -> "${d / 60} 分钟前"
        d < 86_400 -> "${d / 3_600} 小时前"
        d < 30L * 86_400 -> "${d / 86_400} 天前"
        else -> java.text.SimpleDateFormat("yyyy-MM-dd", java.util.Locale.ROOT).format(java.util.Date(epochSec * 1000))
    }
}
