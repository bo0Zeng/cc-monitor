package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
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
 * 终端管理弹窗：列出、附着、结束、新建，不用手输命令。
 * 列 · 结束 · 新建 · 后台起 cc 都是那台核心的命令（[terminals] 带着它们，见 `TerminalOps`）；附着要进终端，经 [onAttach] 交给调用方
 * （新 tab 敲那台渲出的那一行）。状态与 scope 都在本函数，子块只收参数。
 * 名单照那台列（tmux 与托管两种都在）；新建那一排是在那台 tmux 里建，[noTmux] ＝ 那台没 tmux 时那一句 ⇒ 那一排灰着、上面写它。
 */
@Composable
fun TerminalManagerDialog(
    terminals: TerminalOps,
    noTmux: String?,
    onAttach: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val scope = rememberCoroutineScope()
    var sessions by remember { mutableStateOf<List<TerminalsList.Row>>(emptyList()) }
    var newName by remember { mutableStateOf("") }
    var loading by remember { mutableStateOf(true) }
    // 名单读不出时那台那一句（不画成「无终端」，［刷新］再问）；读到了 ⇒ null。
    var failed by remember { mutableStateOf<String?>(null) }
    var refreshTick by remember { mutableStateOf(0) }
    // 待确认要结束的会话（null = 没有）
    var pendingKill by remember { mutableStateOf<TerminalsList.Row?>(null) }
    val snackbar = LocalAppSnackbar.current // 结束会话后的结果反馈

    androidx.compose.runtime.LaunchedEffect(refreshTick) {
        loading = true
        terminals.list().fold(
            onSuccess = {
                sessions = it
                failed = null
            },
            onFailure = { failed = it.message.orEmpty() },
        )
        loading = false
    }

    fun refresh() {
        refreshTick++
    }

    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(copyText("mobTerms.dialog.title")) },
        text = {
            TerminalManagerBody(
                terminals = terminals,
                noTmux = noTmux,
                loading = loading,
                failed = failed,
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
        confirmButton = { SecondaryButton(onClick = { refresh() }) { Text(copyText("mobTerms.dialog.refresh")) } },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("mobKit.act.close")) } },
    )

    KillConfirmDialog(
        target = pendingKill,
        onDismiss = { pendingKill = null },
        onConfirm = { t ->
            pendingKill = null
            scope.launch {
                // 结果必须可见；结束会话不可逆，所以不给撤销
                snackbar.show(terminals.end(t))
                refresh()
            }
        },
    )
}

/** 弹窗主体：会话列表 + 新建行。 */
@Composable
private fun TerminalManagerBody(
    terminals: TerminalOps,
    noTmux: String?,
    loading: Boolean,
    failed: String?,
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
        TerminalList(
            loading = loading,
            failed = failed,
            sessions = sessions,
            onAttach = onAttach,
            onKillRequested = onKillRequested,
        )
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant, modifier = Modifier.padding(vertical = tokens.spacing.sm))
        TmuxNewSessionRow(
            noTmux = noTmux,
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
                    val refused = terminals.create(n)
                    if (refused != null) onFailed(refused) else onAttach(n)
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
        title = { Text(copyText("mobTerms.kill.title", "name" to target.name)) },
        text = {
            Text(
                target.title.takeIf { it.isNotBlank() }?.let { copyText("mobTerms.kill.bodyCmd", "command" to it) }
                    ?: copyText("mobTerms.kill.body"),
            )
        },
        confirmButton = { SecondaryButton(onClick = { onConfirm(target) }) { Text(copyText("mobTerms.kill.ok")) } },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) } },
    )
}

/**
 * 会话列表段（加载 / 读不出 / 空态 / 各行）。读不出 ⇒ 那台那一句（［刷新］在弹窗底下），不画成「无终端」。
 * 行上点「结束」只经 [onKillRequested] 交给父级，由父级做二次确认与结果反馈。
 * 托管终端谁在输入照接入协议（`host/managed/attach.rs`）：终端 tab 接上即持有，只看时回车接管，别的端接上即交出；
 * 手机不另调 `terminal-take-input` · `terminal-release-input`（聊天走队，不靠持有输入）。
 */
@Composable
private fun TerminalList(
    loading: Boolean,
    failed: String?,
    sessions: List<TerminalsList.Row>,
    onAttach: (String) -> Unit,
    onKillRequested: (TerminalsList.Row) -> Unit,
) {
    when {
        loading -> Text(copyText("kit.state.reading"), color = MaterialTheme.colorScheme.onSurfaceVariant)
        failed != null -> Text(failed, color = MaterialTheme.colorScheme.error)
        sessions.isEmpty() -> Text(copyText("mobTerms.list.empty"), color = MaterialTheme.colorScheme.onSurfaceVariant)
        else ->
            sessions.forEach { s ->
                TerminalRow(
                    s = s,
                    onAttach = { onAttach(s.name) },
                    onKill = { onKillRequested(s) },
                )
            }
    }
}

/** 单个终端行：名称、运行中命令（tmux 那一形带「tmux」）、托管那一形谁在输入（那台那一句），以及附着 / 结束。 */
@Composable
private fun TerminalRow(
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
                listOfNotNull(
                    copyText("mobTerms.row.tmux").takeIf { s.tmux },
                    s.title.takeIf { it.isNotBlank() }?.let { copyText("mobTerms.row.running", "command" to it) },
                    copyText("mobTerms.row.attached").takeIf { s.attached },
                    s.holder,
                ).joinToString(copyText("kit.text.sep")),
                style = monoSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        SecondaryButton(onClick = onAttach) { Text(copyText("mobTerms.row.attach")) }
        SecondaryButton(danger = true, onClick = onKill) { Text(copyText("mobTerms.row.kill")) }
    }
    HorizontalDivider(color = androidx.compose.material3.MaterialTheme.colorScheme.outlineVariant)
}

/** 底部新建那一段：名字框一行，下面新建 / 后台 agent / 新建并附着；那台没 tmux（[noTmux] 那一句）⇒ 上面写它、框与按钮灰着。 */
@Composable
private fun TmuxNewSessionRow(
    noTmux: String?,
    newName: String,
    onNameChange: (String) -> Unit,
    onNewDetached: (String) -> Unit,
    onNewDetachedCc: (String) -> Unit,
    onNewAttach: (String) -> Unit,
) {
    val tokens = LocalAppTokens.current
    val can = noTmux == null && newName.isNotBlank()
    noTmux?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    // 名字框独占一行：与三颗按钮同挤一行时窄屏上按钮先量，框只剩一个字宽（标签竖排、敲不进字）。按钮一排靠右，放不下换行。
    OutlinedTextField(
        value = newName,
        onValueChange = onNameChange,
        label = { Text(copyText("mobTerms.new.name")) },
        singleLine = true,
        enabled = noTmux == null,
        modifier = Modifier.fillMaxWidth(),
    )
    FlowRow(
        Modifier.fillMaxWidth().padding(top = tokens.spacing.sm),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm, Alignment.End),
        verticalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
    ) {
        SecondaryButton(enabled = can, onClick = { onNewDetached(newName.trim()) }) { Text(copyText("mobTerms.new.detached")) }
        // 后台起一个跑着 agent 的 tmux 会话，不进入，之后可打开。
        SecondaryButton(enabled = can, onClick = { onNewDetachedCc(newName.trim()) }) { Text(copyText("mobTerms.new.detachedCc")) }
        SecondaryButton(enabled = can, onClick = { onNewAttach(newName.trim()) }) { Text(copyText("mobTerms.new.attach")) }
    }
}
