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
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
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
import com.ccmonitor.mobile.core.claude.link.TerminalsList
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.feedback.Notice
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import kotlinx.coroutines.launch

/**
 * tmux 会话管理弹窗：列出、附着、结束、新建，不用手输命令。
 * 列 · 结束 · 新建 · 后台起 cc 都是那台核心的命令（[terminals] 带着它们，见 `TerminalOps`）；附着要进终端，经 [onAttach] 交给调用方
 * （新 tab 敲那台渲出的那一行）。状态与 scope 都在本函数，子块只收参数。
 */
@Composable
fun TmuxManagerDialog(
    terminals: TerminalOps,
    onAttach: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    var sessions by remember { mutableStateOf<List<TerminalsList.Row>>(emptyList()) }
    var newName by remember { mutableStateOf("") }
    var loading by remember { mutableStateOf(true) }
    var refreshTick by remember { mutableStateOf(0) }
    // 待确认要结束的会话（null = 没有）
    var pendingKill by remember { mutableStateOf<TerminalsList.Row?>(null) }
    val snackbar = LocalAppSnackbar.current // 结束会话后的结果反馈

    androidx.compose.runtime.LaunchedEffect(refreshTick) {
        loading = true
        sessions = terminals.list().getOrDefault(emptyList())
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
                terminals = terminals,
                loading = loading,
                sessions = sessions,
                newName = newName,
                onNameChange = { newName = it },
                onAttach = onAttach,
                onKillRequested = { pendingKill = it },
                onCreated = {
                    newName = ""
                    refresh()
                },
                onFailed = { snackbar.show(Notice(it)) },
                scope = scope,
            )
        },
        confirmButton = { SecondaryButton(onClick = { refresh() }) { Text(copyText("mobTmux.dialog.refresh")) } },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("mobKit.act.close")) } },
    )

    KillConfirmDialog(
        target = pendingKill,
        onDismiss = { pendingKill = null },
        onConfirm = { t ->
            pendingKill = null
            scope.launch {
                // 结果必须可见；结束会话不可逆，所以不给撤销
                snackbar.show(terminals.kill(t.name))
                refresh()
            }
        },
    )
}

/** 弹窗主体：会话列表 + 新建行。 */
@Composable
private fun TmuxManagerBody(
    terminals: TerminalOps,
    loading: Boolean,
    sessions: List<TerminalsList.Row>,
    newName: String,
    onNameChange: (String) -> Unit,
    onAttach: (String) -> Unit,
    onKillRequested: (TerminalsList.Row) -> Unit,
    onCreated: () -> Unit,
    onFailed: (String) -> Unit,
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
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant, modifier = Modifier.padding(vertical = tokens.spacing.sm))
        TmuxNewSessionRow(
            newName = newName,
            onNameChange = onNameChange,
            onNewDetached = { n ->
                scope.launch {
                    terminals.create(n)?.let(onFailed)
                    onCreated()
                }
            },
            onNewDetachedCc = { n ->
                scope.launch {
                    terminals.startAgent(n)?.let(onFailed)
                    onCreated()
                }
            },
            // 先请那台建（在了就不动），再像附着已有的一样进去；没建成 ⇒ 那一句、不进。
            onNewAttach = { n ->
                scope.launch {
                    val failed = terminals.create(n)
                    if (failed != null) onFailed(failed) else onAttach(n)
                }
            },
        )
    }
}

/** 结束会话不可逆，里面可能正跑着活，所以二次确认；文案写清后果（会中断什么、不能撤销），而不是问「确定吗」。 */
@Composable
private fun KillConfirmDialog(
    target: TerminalsList.Row?,
    onDismiss: () -> Unit,
    onConfirm: (TerminalsList.Row) -> Unit,
) {
    target ?: return
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(copyText("mobTmux.kill.title", "name" to target.name)) },
        text = {
            Text(
                target.title.takeIf { it.isNotBlank() }?.let { copyText("mobTmux.kill.bodyCmd", "command" to it) }
                    ?: copyText("mobTmux.kill.body"),
            )
        },
        confirmButton = { SecondaryButton(onClick = { onConfirm(target) }) { Text(copyText("mobTmux.kill.ok")) } },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) } },
    )
}

/**
 * 会话列表段（加载 / 空态 / 各行）。
 * 行上点「杀」只经 [onKillRequested] 交给父级，由父级做二次确认与结果反馈。
 */
@Composable
private fun TmuxSessionList(
    loading: Boolean,
    sessions: List<TerminalsList.Row>,
    onAttach: (String) -> Unit,
    onKillRequested: (TerminalsList.Row) -> Unit,
) {
    when {
        loading -> Text(copyText("kit.state.reading"), color = MaterialTheme.colorScheme.onSurfaceVariant)
        sessions.isEmpty() -> Text(copyText("mobTmux.list.empty"), color = MaterialTheme.colorScheme.onSurfaceVariant)
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
    s: TerminalsList.Row,
    onAttach: () -> Unit,
    onKill: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier.fillMaxWidth().heightIn(min = Touch.rowDouble).padding(vertical = tokens.spacing.xs),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(s.name, maxLines = 1, overflow = TextOverflow.Ellipsis)
            // 显示那台写好的标题（免得盲杀正在干活的会话）、附着态。
            Text(
                buildString {
                    if (s.title.isNotBlank()) append(copyText("mobTmux.row.running", "command" to s.title))
                    if (s.attached) append(copyText("mobTmux.row.attached"))
                }.removePrefix(" · "),
                style = monoSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        SecondaryButton(onClick = onAttach) { Text(copyText("mobTmux.row.attach")) }
        SecondaryButton(danger = true, onClick = onKill) { Text(copyText("mobTmux.row.kill")) }
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
        SecondaryButton(enabled = newName.isNotBlank(), onClick = { onNewDetached(newName.trim()) }) { Text(copyText("mobTmux.new.detached")) }
        // 后台起一个跑着 agent 的 tmux 会话，不进入，之后可打开。
        SecondaryButton(enabled = newName.isNotBlank(), onClick = { onNewDetachedCc(newName.trim()) }) { Text(copyText("mobTmux.new.detachedCc")) }
        SecondaryButton(enabled = newName.isNotBlank(), onClick = { onNewAttach(newName.trim()) }) { Text(copyText("mobTmux.new.attach")) }
    }
}
