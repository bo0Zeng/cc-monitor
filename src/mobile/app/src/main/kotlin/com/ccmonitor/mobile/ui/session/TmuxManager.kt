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
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.copy.withRaw
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
        title = { Text(copyText("mobTmux.dialog.title")) },
        text = {
            TmuxManagerBody(
                channel = channel,
                backend = backend,
                loading = loading,
                sessions = sessions,
                newName = newName,
                onNameChange = { newName = it },
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
        confirmButton = { TextButton(onClick = { refresh() }) { Text(copyText("mobTmux.dialog.refresh")) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(copyText("mobKit.act.close")) } },
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
    onAttach: (String) -> Unit,
    onNewAttach: (String) -> Unit,
    onKillRequested: (TmuxSession) -> Unit,
    onCreated: () -> Unit,
    scope: kotlinx.coroutines.CoroutineScope,
) {
    val tokens = LocalAppTokens.current
    Column(Modifier.fillMaxWidth().heightIn(max = 420.dp).verticalScroll(rememberScrollState())) {
        TmuxSessionList(
            loading = loading,
            sessions = sessions,
            onAttach = onAttach,
            onKillRequested = onKillRequested,
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
                    copyText("mobTmux.kill.done", "name" to target.name)
                } else {
                    val detail = out.replace(TmuxCommands.TMUX_KILL_OK_MARKER, "").trim().ifBlank { copyText("mobTmux.kill.noReply") }
                    withRaw(copyText("mobTmux.kill.failed", "name" to target.name), detail)
                }
            },
            onFailure = { withRaw(copyText("mobTmux.kill.failed", "name" to target.name), it.message) },
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
        title = { Text(copyText("mobTmux.kill.title", "name" to target.name)) },
        text = {
            Text(
                target.command.takeIf { it.isNotBlank() }?.let { copyText("mobTmux.kill.bodyCmd", "command" to it) }
                    ?: copyText("mobTmux.kill.body"),
            )
        },
        confirmButton = { TextButton(onClick = { onConfirm(target) }) { Text(copyText("mobTmux.kill.ok")) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) } },
    )
}

/**
 * 会话列表段（加载 / 空态 / 各行）。
 * 行上点「杀」只经 [onKillRequested] 交给父级，由父级做二次确认与结果反馈。
 */
@Composable
private fun TmuxSessionList(
    loading: Boolean,
    sessions: List<TmuxSession>,
    onAttach: (String) -> Unit,
    onKillRequested: (TmuxSession) -> Unit,
) {
    val tokens = LocalAppTokens.current
    when {
        loading -> Text(copyText("mobTmux.list.loading"), color = tokens.textFaint)
        sessions.isEmpty() -> Text(copyText("mobTmux.list.empty"), color = tokens.textFaint)
        else ->
            sessions.forEach { s ->
                TmuxSessionRow(
                    s = s,
                    onAttach = { onAttach(s.name) },
                    onKill = { onKillRequested(s) },
                )
            }
    }
}

/** 单个 tmux 会话行：名称、运行中命令，以及附着 / 杀。 */
@Composable
private fun TmuxSessionRow(
    s: TmuxSession,
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
                    append(copyText("mobTmux.row.windows", "n" to s.windows))
                    if (s.command.isNotBlank()) append(copyText("mobTmux.row.running", "command" to s.command))
                    if (s.attached) append(copyText("mobTmux.row.attached"))
                },
                style = monoSmall,
                color = tokens.textFaint,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        TextButton(onClick = onAttach) { Text(copyText("mobTmux.row.attach")) }
        TextButton(onClick = onKill) { Text(copyText("mobTmux.row.kill"), color = androidx.compose.material3.MaterialTheme.colorScheme.error) }
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
            label = { Text(copyText("mobTmux.new.name")) },
            singleLine = true,
            modifier = Modifier.weight(1f),
        )
        TextButton(enabled = newName.isNotBlank(), onClick = { onNewDetached(newName.trim()) }) { Text(copyText("mobTmux.new.detached")) }
        // 后台起一个跑着 claude 的 detached 会话，不进入，之后可附着回来。
        TextButton(enabled = newName.isNotBlank(), onClick = { onNewDetachedCc(newName.trim()) }) { Text(copyText("mobTmux.new.detachedCc")) }
        TextButton(enabled = newName.isNotBlank(), onClick = { onNewAttach(newName.trim()) }) { Text(copyText("mobTmux.new.attach")) }
    }
}
