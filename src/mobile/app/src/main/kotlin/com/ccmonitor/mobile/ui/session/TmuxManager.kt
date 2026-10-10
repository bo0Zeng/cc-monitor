package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ssh.SessionBackend
import com.ccmonitor.mobile.ssh.TmuxBackend
import com.ccmonitor.mobile.ssh.TmuxCommands
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.launch

/**
 * 跑一条会退出的命令，收集 stdout 到完成。字节累积后一次性解码（多字节 UTF-8 不被 chunk 边界劈坏）。
 * :app 内经 exec 通道跑一次性命令的唯一入口（切模型、抓屏也走它）。
 */
internal suspend fun runOnce(
    channel: RemoteCommandChannel,
    cmd: String,
): String {
    val buf = java.io.ByteArrayOutputStream()
    channel.exec(cmd).collect { buf.write(it) }
    return buf.toString("UTF-8")
}

/** 列 tmux 会话，解析见 [parseTmuxList]。没有 server 时 stderr 被吞掉，得到空列表。 */
suspend fun listTmux(
    channel: RemoteCommandChannel,
    backend: SessionBackend = TmuxBackend,
): List<TmuxSession> = parseTmuxList(runOnce(channel, backend.listCommand()))

/**
 * tmux 会话管理弹窗：列出、附着、结束、新建，不用手输命令。
 * list/kill/new-detached 走 [channel]；附着要进终端，所以经 [onAttach]/[onNewAttach] 交给调用方。
 * 状态与 scope 都在本函数，子块只收参数。
 */
@Composable
fun TmuxManagerDialog(
    channel: RemoteCommandChannel,
    onAttach: (String) -> Unit,
    onNewAttach: (String) -> Unit,
    onRead: (cwd: String) -> Unit, // 不附着，只读该会话的 Claude 输出；传入的是已解析出的 cwd
    onDismiss: () -> Unit,
    backend: SessionBackend = TmuxBackend, // 命令族由会话后端给出
) {
    val scope = rememberCoroutineScope()
    var sessions by remember { mutableStateOf<List<TmuxSession>>(emptyList()) }
    var newName by remember { mutableStateOf("") }
    var loading by remember { mutableStateOf(true) }
    var refreshTick by remember { mutableStateOf(0) }
    // 待确认要结束的会话（null = 没有）
    var pendingKill by remember { mutableStateOf<TmuxSession?>(null) }
    val snackbar = LocalAppSnackbar.current // 结束会话后的结果反馈

    androidx.compose.runtime.LaunchedEffect(refreshTick) {
        loading = true
        sessions = runCatching { listTmux(channel, backend) }.getOrDefault(emptyList())
        loading = false
    }

    fun refresh() {
        refreshTick++
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("tmux 会话") },
        text = {
            TmuxManagerBody(
                channel = channel,
                backend = backend,
                loading = loading,
                sessions = sessions,
                newName = newName,
                onNameChange = { newName = it },
                onRead = onRead,
                onAttach = onAttach,
                onNewAttach = onNewAttach,
                onKillRequested = { pendingKill = it },
                onCreated = {
                    newName = ""
                    refresh()
                },
                scope = scope,
            )
        },
        confirmButton = { TextButton(onClick = { refresh() }) { Text("刷新") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("关闭") } },
    )

    KillConfirmDialog(
        target = pendingKill,
        onDismiss = { pendingKill = null },
        onConfirm = { t ->
            pendingKill = null
            scope.launch {
                // 结果必须可见；结束会话不可逆，所以不给撤销
                snackbar.show(killSessionReportingResult(channel, backend, t))
                refresh()
            }
        },
    )
}

/** 弹窗主体：会话列表 + 新建行。 */
@Composable
private fun TmuxManagerBody(
    channel: RemoteCommandChannel,
    backend: SessionBackend,
    loading: Boolean,
    sessions: List<TmuxSession>,
    newName: String,
    onNameChange: (String) -> Unit,
    onRead: (cwd: String) -> Unit,
    onAttach: (String) -> Unit,
    onNewAttach: (String) -> Unit,
    onKillRequested: (TmuxSession) -> Unit,
    onCreated: () -> Unit,
    scope: kotlinx.coroutines.CoroutineScope,
) {
    val tokens = LocalAppTokens.current
    Column(Modifier.fillMaxWidth().heightIn(max = 420.dp).verticalScroll(rememberScrollState())) {
        TmuxSessionList(
            channel = channel,
            loading = loading,
            sessions = sessions,
            onRead = onRead,
            onAttach = onAttach,
            onKillRequested = onKillRequested,
            backend = backend,
        )
        HorizontalDivider(color = tokens.surfaceHigh, modifier = Modifier.padding(vertical = tokens.spacing.sm))
        TmuxNewSessionRow(
            newName = newName,
            onNameChange = onNameChange,
            onNewDetached = { n ->
                scope.launch {
                    runCatching { runOnce(channel, backend.newDetachedCommand(n)) }
                    onCreated()
                }
            },
            onNewDetachedCc = { n ->
                scope.launch {
                    runCatching { runOnce(channel, backend.newDetachedRunningCommand(n, "claude")) }
                    onCreated()
                }
            },
            onNewAttach = onNewAttach,
        )
    }
}

/**
 * 结束会话并把结果说出来。只刷新列表的话，失败和成功看起来一样（那条还在，像是没点中）。
 *
 * @return 上屏的一句话，成功与失败不同。
 */
private suspend fun killSessionReportingResult(
    channel: RemoteCommandChannel,
    backend: SessionBackend,
    target: TmuxSession,
): String =
    runCatching { runOnce(channel, backend.killCommand(target.name)) }
        .fold(
            onSuccess = { out ->
                // 传输层没抛不等于命令成功：要看到成功标记才算，命令没跑起来时 stdout 同样是空的。
                // 标记与判定都在 `TmuxCommands`，与命令同处一地。
                if (TmuxCommands.tmuxKillSucceeded(out)) {
                    "已结束「${target.name}」"
                } else {
                    val detail = out.replace(TmuxCommands.TMUX_KILL_OK_MARKER, "").trim().ifBlank { "远端没有回应" }
                    "没能结束「${target.name}」：$detail"
                }
            },
            onFailure = { "没能结束「${target.name}」：${it.message ?: "未知原因"}" },
        )

/** 结束会话不可逆，里面可能正跑着活，所以二次确认；文案写清后果（会中断什么、不能撤销），而不是问「确定吗」。 */
@Composable
private fun KillConfirmDialog(
    target: TmuxSession?,
    onDismiss: () -> Unit,
    onConfirm: (TmuxSession) -> Unit,
) {
    target ?: return
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("结束「${target.name}」？") },
        text = {
            Text(
                "里面正在跑的东西会被中断，" +
                    (target.command.takeIf { it.isNotBlank() }?.let { "当前是：$it。" } ?: "") +
                    "这一步不能撤销。",
            )
        },
        confirmButton = { TextButton(onClick = { onConfirm(target) }) { Text("结束") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

/**
 * 会话列表段（加载 / 空态 / 各行）及行上的 exec 动作（读 cwd）。
 * 行上点「杀」只经 [onKillRequested] 交给父级，由父级做二次确认与结果反馈。
 */
@Composable
private fun TmuxSessionList(
    channel: RemoteCommandChannel,
    loading: Boolean,
    sessions: List<TmuxSession>,
    onRead: (cwd: String) -> Unit,
    onAttach: (String) -> Unit,
    onKillRequested: (TmuxSession) -> Unit,
    backend: SessionBackend,
) {
    val tokens = LocalAppTokens.current
    val scope = rememberCoroutineScope()
    when {
        loading -> Text("加载中…", color = tokens.textFaint)
        sessions.isEmpty() -> Text("无 tmux 会话。下方新建一个。", color = tokens.textFaint)
        else ->
            sessions.forEach { s ->
                TmuxSessionRow(
                    s = s,
                    onRead = {
                        // 解析该会话的工作目录，交给阅读面跟读其 Claude jsonl，不附着终端。
                        scope.launch {
                            val cwd = runCatching { runOnce(channel, backend.sessionCwdCommand(s.name)).trim() }.getOrDefault("")
                            if (cwd.isNotBlank()) onRead(cwd)
                        }
                    },
                    onAttach = { onAttach(s.name) },
                    onKill = { onKillRequested(s) },
                )
            }
    }
}

/** 单个 tmux 会话行：名称、运行中命令，以及读 / 附着 / 杀。 */
@Composable
private fun TmuxSessionRow(
    s: TmuxSession,
    onRead: () -> Unit,
    onAttach: () -> Unit,
    onKill: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier.fillMaxWidth().padding(vertical = tokens.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(s.name, maxLines = 1, overflow = TextOverflow.Ellipsis)
            // 显示活动 pane 在跑什么（免得盲杀正在干活的会话）、窗口数、附着态。
            Text(
                buildString {
                    append("${s.windows} 窗口")
                    if (s.command.isNotBlank()) append(" · 运行: ${s.command}")
                    if (s.attached) append(" · 已附着")
                },
                style = monoSmall,
                color = tokens.textFaint,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        TextButton(onClick = onRead) { Text("读") }
        TextButton(onClick = onAttach) { Text("附着") }
        TextButton(onClick = onKill) { Text("杀", color = androidx.compose.material3.MaterialTheme.colorScheme.error) }
    }
    HorizontalDivider(color = androidx.compose.material3.MaterialTheme.colorScheme.outlineVariant)
}

/** 底部新建行：输入名，新建 / 后台 cc / 新建并附着。 */
@Composable
private fun TmuxNewSessionRow(
    newName: String,
    onNameChange: (String) -> Unit,
    onNewDetached: (String) -> Unit,
    onNewDetachedCc: (String) -> Unit,
    onNewAttach: (String) -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm), verticalAlignment = Alignment.CenterVertically) {
        OutlinedTextField(
            value = newName,
            onValueChange = onNameChange,
            label = { Text("新会话名") },
            singleLine = true,
            modifier = Modifier.weight(1f),
        )
        TextButton(enabled = newName.isNotBlank(), onClick = { onNewDetached(newName.trim()) }) { Text("新建") }
        // 后台起一个跑着 claude 的 detached 会话，不进入，之后可附着回来。
        TextButton(enabled = newName.isNotBlank(), onClick = { onNewDetachedCc(newName.trim()) }) { Text("后台 cc") }
        TextButton(enabled = newName.isNotBlank(), onClick = { onNewAttach(newName.trim()) }) { Text("新建并附着") }
    }
}
