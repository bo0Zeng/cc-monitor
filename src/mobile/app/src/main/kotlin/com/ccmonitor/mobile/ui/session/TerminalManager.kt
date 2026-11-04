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

/** 管理窗里的一组：小标题（带个数）与这一组的几行。 */
internal data class TermGroup(
    val head: String,
    val lines: List<TermLine>,
)

/** 管理窗里的一行：名单那一行、能不能结束（`can.end`）、结束不了时那台那一句。 */
internal data class TermLine(
    val row: TerminalsList.Row,
    val endable: Boolean,
    val note: String?,
)

/**
 * 名单 ⇒ 管理窗的排法：带 `session.sid` 的进「会话所在的终端」、别的进「终端会话」（各照那台的次序，空的那组不画）；
 * 每行能不能结束照那台的 `can.end`（[TerminalsList.Row.endRefused]），手机不自己判。
 */
internal fun terminalPlan(rows: List<TerminalsList.Row>): List<TermGroup> {
    val (withSession, plain) = rows.partition { it.sid != null }

    fun lines(rs: List<TerminalsList.Row>) = rs.map { TermLine(it, endable = it.endRefused == null, note = it.endRefused?.takeIf { s -> s.isNotBlank() }) }
    return listOfNotNull(
        withSession.takeIf { it.isNotEmpty() }?.let { TermGroup(copyText("mobTerms.group.session", "n" to it.size), lines(it)) },
        plain.takeIf { it.isNotEmpty() }?.let { TermGroup(copyText("mobTerms.group.plain", "n" to it.size), lines(it)) },
    )
}

/**
 * 终端管理弹窗：列出、附着、结束、新建，不用手输命令。
 * 列 · 结束 · 新建都是那台核心的命令（[terminals] 带着它们，见 `TerminalOps`）；附着要进终端，经 [onAttach] 交给调用方
 * （新 tab 敲那台渲出的那一行）。状态与 scope 都在本函数，子块只收参数。
 * 名单照那台列（tmux 与托管两种都在），照 [terminalPlan] 分两组；新建那一排只交名字，建在托管还是 tmux 里由那台定。
 */
@Composable
fun TerminalManagerDialog(
    terminals: TerminalOps,
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
        title = { Text(copyText("mobDrawer.item.terminal")) },
        text = {
            TerminalManagerBody(
                terminals = terminals,
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
        confirmButton = { SecondaryButton(onClick = { refresh() }) { Text(copyText("accountChip.menu.refresh")) } },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("kit.drawer.close")) } },
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
            newName = newName,
            onNameChange = onNameChange,
            onNewDetached = { n ->
                scope.launch {
                    terminals.create(n)?.let(onFailed)
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
        confirmButton = { SecondaryButton(onClick = { onConfirm(target) }) { Text(copyText("tabBatch.verb.stop")) } },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) } },
    )
}

/**
 * 会话列表段（加载 / 读不出 / 空态 / 两组各行）。读不出 ⇒ 那台那一句（［刷新］在弹窗底下），不画成「无终端」。
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
    val tokens = LocalAppTokens.current
    when {
        loading -> Text(copyText("kit.state.reading"), color = MaterialTheme.colorScheme.onSurfaceVariant)
        failed != null -> Text(failed, color = MaterialTheme.colorScheme.error)
        sessions.isEmpty() -> Text(copyText("mobTerms.list.empty"), color = MaterialTheme.colorScheme.onSurfaceVariant)
        else ->
            terminalPlan(sessions).forEach { g ->
                Text(
                    g.head,
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(top = tokens.spacing.sm),
                )
                g.lines.forEach { l ->
                    TerminalRow(
                        line = l,
                        onAttach = { onAttach(l.row.name) },
                        onKill = { onKillRequested(l.row) },
                    )
                }
            }
    }
}

/** 单个终端行：名称、运行中命令（tmux 那一形带「tmux」）、托管那一形谁在输入、结束不了时那台那一句，以及附着 / 结束（那台许才有）。 */
@Composable
private fun TerminalRow(
    line: TermLine,
    onAttach: () -> Unit,
    onKill: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val s = line.row
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
                    line.note,
                ).joinToString(copyText("kit.text.sep")),
                style = monoSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
        SecondaryButton(onClick = onAttach) { Text(copyText("mobTerms.row.attach")) }
        if (line.endable) SecondaryButton(danger = true, onClick = onKill) { Text(copyText("tabBatch.verb.stop")) }
    }
    HorizontalDivider(color = androidx.compose.material3.MaterialTheme.colorScheme.outlineVariant)
}

/** 底部新建那一段：名字框一行，下面新建 / 新建并附着（建在哪由那台定；建不成 ⇒ 那台那一句）。 */
@Composable
private fun TmuxNewSessionRow(
    newName: String,
    onNameChange: (String) -> Unit,
    onNewDetached: (String) -> Unit,
    onNewAttach: (String) -> Unit,
) {
    val tokens = LocalAppTokens.current
    val can = newName.isNotBlank()
    // 名字框独占一行：与按钮同挤一行时窄屏上按钮先量，框只剩一个字宽（标签竖排、敲不进字）。按钮一排靠右，放不下换行。
    OutlinedTextField(
        value = newName,
        onValueChange = onNameChange,
        label = { Text(copyText("mobTerms.new.name")) },
        singleLine = true,
        modifier = Modifier.fillMaxWidth(),
    )
    FlowRow(
        Modifier.fillMaxWidth().padding(top = tokens.spacing.sm),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm, Alignment.End),
        verticalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
    ) {
        SecondaryButton(enabled = can, onClick = { onNewDetached(newName.trim()) }) { Text(copyText("mobTerms.new.detached")) }
        SecondaryButton(enabled = can, onClick = { onNewAttach(newName.trim()) }) { Text(copyText("mobTerms.new.attach")) }
    }
}
