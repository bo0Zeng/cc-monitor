package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens

/**
 * ⋯ 里那几项：会话里找 · 大纲 · 无人答时自动答 · 结束会话…（照 `can.end`：不能 ⇒ 灰着、第二行写那台那一句）。
 * [close] 收起 ⋯；[title] 是这条会话的名字（确认框里列它）。字只取骨架那几档；结局的字都是那台写好的。
 */
@Composable
fun ChatMoreItems(
    more: ChatMore,
    title: String,
    close: () -> Unit,
) {
    DropdownMenuItem(text = { Text(copyText("sessionFind.box.placeholder")) }, onClick = {
        close()
        more.openFind()
    })
    DropdownMenuItem(text = { Text(copyText("userInputPanel.outline.label")) }, onClick = {
        close()
        more.openOutline()
    })
    if (more.autoShown()) {
        DropdownMenuItem(text = { Text(copyText("tabMenu.autoAnswer.label")) }, onClick = {
            close()
            more.openAuto()
        })
    }
    val end = more.endEntry() ?: return
    val kill = copyText("tabMenu.kill.label")
    val why = (end as? EndEntry.Blocked)?.why
    val tokens = LocalAppTokens.current
    DropdownMenuItem(
        text = {
            Column {
                Text(kill, color = if (why == null) tokens.errorText else tokens.textFaint)
                why?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = tokens.textFaint) }
            }
        },
        enabled = why == null,
        onClick = {
            close()
            more.askEnd(title)
        },
    )
}

/** ⋯ 点开的那几面（底部抽屉 · 确认框）与没做成时那一句；挂在顶栏下。 */
@Composable
fun ChatMorePanes(more: ChatMore) {
    val ui by more.state.collectAsStateWithLifecycle()
    ui.note?.let { MoreNote(it, more::clearNote) }
    ui.find?.let { FindSheet(it, more) }
    ui.outline?.let { OutlineSheet(it, more) }
    ui.auto?.let { AutoSheet(it, more) }
    ui.ending?.let { EndDialog(it, more) }
}

@Composable
private fun MoreNote(
    text: String,
    onClose: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.xs), verticalAlignment = Alignment.CenterVertically) {
        Text(text, Modifier.weight(1f), style = MaterialTheme.typography.bodySmall, color = tokens.errorText)
        SecondaryButton(onClick = onClose) { Text(copyText("kit.drawer.close")) }
    }
}

/** 会话里找：框里敲、按回车问那台；命中照那台的先后（头一行是那台写好的时刻），点一条跳过去。没问成 ⇒ 那台那一句。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun FindSheet(
    ui: FindUi,
    more: ChatMore,
) {
    val tokens = LocalAppTokens.current
    var q by remember { mutableStateOf(ui.query) }
    val busy = copyText("findStrip.status.busy")
    ModalBottomSheet(onDismissRequest = more::closeFind) {
        Column(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg).padding(bottom = tokens.spacing.lg), verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
            OutlinedTextField(
                value = q,
                onValueChange = { q = it },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                placeholder = { Text(copyText("sessionFind.box.placeholder")) },
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { more.find(q) }),
            )
            (if (ui.busy) busy else ui.failed)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = tokens.textFaint) }
            LazyColumn(Modifier.heightIn(max = 420.dp)) {
                items(ui.hits) { h ->
                    Column(Modifier.fillMaxWidth().clickable { more.pickHit(h) }.padding(vertical = tokens.spacing.xs)) {
                        if (h.tsText.isNotEmpty()) Text(h.tsText, style = MaterialTheme.typography.bodySmall, color = tokens.textFaint)
                        Text(
                            buildAnnotatedString {
                                append(h.before)
                                withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { append(h.matched) }
                                append(h.after)
                            },
                            maxLines = 2,
                            overflow = TextOverflow.Ellipsis,
                        )
                    }
                }
            }
        }
    }
}

/** 大纲：人说过的话（那台给的摘要，对话序），点一句跳过去。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun OutlineSheet(
    ui: OutlineUi,
    more: ChatMore,
) {
    val tokens = LocalAppTokens.current
    val busy = copyText("findStrip.status.busy")
    val hint = copyText("userInputPanel.outline.hint")
    ModalBottomSheet(onDismissRequest = more::closeOutline) {
        Column(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg).padding(bottom = tokens.spacing.lg), verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
            Text(copyText("userInputPanel.outline.label"), style = MaterialTheme.typography.titleMedium)
            Text(if (ui.busy) busy else ui.failed ?: hint, style = MaterialTheme.typography.bodySmall, color = tokens.textFaint)
            LazyColumn(Modifier.heightIn(max = 420.dp)) {
                items(ui.rows, key = { it.uuid }) { r ->
                    Text(
                        r.excerpt,
                        Modifier.fillMaxWidth().clickable { more.pickSaid(r) }.padding(vertical = tokens.spacing.sm),
                        maxLines = 2,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
}

/** 无人答时自动答：几档照那台列（字也是那台写的），勾在此刻那一档；点一档就交。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AutoSheet(
    ui: AutoUi,
    more: ChatMore,
) {
    val tokens = LocalAppTokens.current
    val busy = copyText("findStrip.status.busy")
    ModalBottomSheet(onDismissRequest = more::closeAuto) {
        Column(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg).padding(bottom = tokens.spacing.lg)) {
            Text(copyText("tabMenu.autoAnswer.label"), style = MaterialTheme.typography.titleMedium)
            Text(copyText("tabMenu.autoAnswer.tip"), style = MaterialTheme.typography.bodySmall, color = tokens.textFaint)
            if (ui.busy) Text(busy, style = MaterialTheme.typography.bodySmall, color = tokens.textFaint)
            ui.choices.forEach { c ->
                Row(Modifier.fillMaxWidth().clickable { more.setAuto(c.min) }, verticalAlignment = Alignment.CenterVertically) {
                    RadioButton(selected = c.min == ui.now, onClick = { more.setAuto(c.min) })
                    Text(c.text)
                }
            }
        }
    }
}

/** 确认框里一行：标签（中断 · 保留）＋ 那几样。 */
@Composable
private fun EndRow(
    label: String,
    items: String,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(LocalAppTokens.current.spacing.sm)) {
        Text(label, style = MaterialTheme.typography.bodySmall, color = LocalAppTokens.current.textFaint)
        Text(items, style = MaterialTheme.typography.bodySmall)
    }
}

/** 结束会话那一个确认框（字同桌面 `end-confirm`）：列名字 · 会中断什么 · 会保留什么。 */
@Composable
private fun EndDialog(
    ask: EndAsk,
    more: ChatMore,
) {
    AlertDialog(
        onDismissRequest = more::cancelEnd,
        title = { Text(copyText("tabMenu.kill.label")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(LocalAppTokens.current.spacing.xs)) {
                ask.titles.forEach { Text(it, maxLines = 2, overflow = TextOverflow.Ellipsis) }
                EndRow(copyText("kit.interrupts.cut"), copyText("tabBatch.stop.cuts"))
                EndRow(copyText("kit.interrupts.keep"), copyText("tabSessionActions.kill.keeps"))
            }
        },
        confirmButton = { SecondaryButton(onClick = more::confirmEnd, danger = true) { Text(copyText("tabBatch.verb.stop")) } },
        dismissButton = { SecondaryButton(onClick = more::cancelEnd) { Text(copyText("kit.dialog.cancel")) } },
    )
}
