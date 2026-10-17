package com.ccmonitor.mobile.ui.rotation

import android.net.Uri
import android.provider.OpenableColumns
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.wire.RotHintKind
import com.ccmonitor.mobile.core.claude.link.wire.RotNote
import com.ccmonitor.mobile.core.claude.link.wire.RotStrategyItem
import com.ccmonitor.mobile.core.claude.link.wire.RotationViewReply
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.ui.chat.newsession.AccountBadge
import com.ccmonitor.mobile.ui.chat.newsession.PickRow
import com.ccmonitor.mobile.ui.chat.newsession.SheetHead
import com.ccmonitor.mobile.ui.common.CopyDetailButton

// ─────────────────────────────── 第一行

/** 策略下拉（满宽 48 dp）＋「⋯」＋ 下面的说明（默认两行，点 ⌄ 展开）。会话里选中的 ＝ 它用的；设置页选中的 ＝ 新会话用的。 */
@Composable
internal fun FirstRow(
    ui: RotationUi,
    board: RotationBoard,
    sid: String?,
    titles: Map<String, String>,
) {
    var picking by remember { mutableStateOf(false) }
    var more by remember { mutableStateOf(false) }
    val list = ui.list?.strategies.orEmpty()
    val current =
        if (sid == null) {
            ui.list?.forNew.orEmpty()
        } else {
            ui.view
                ?.strategy
                ?.id
                .orEmpty()
        }
    val item = list.firstOrNull { it.id == current }
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Surface(
            onClick = { picking = true },
            shape = RoundedCornerShape(6.dp),
            color = MaterialTheme.colorScheme.surfaceContainer,
            border = BorderStroke(1.dp, MaterialTheme.colorScheme.outline),
            modifier = Modifier.weight(1f).heightIn(min = 48.dp).testTag(RotTags.PICK),
        ) {
            Row(Modifier.padding(horizontal = 12.dp, vertical = 12.dp), verticalAlignment = Alignment.CenterVertically) {
                Row(Modifier.weight(1f), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(item?.id ?: copyText("rot.none.label"), fontWeight = FontWeight.SemiBold, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f, fill = false))
                    if (sid == null && item?.forNew == true) Tag(copyText("rot.tag.forNew"))
                }
                Text(CARET, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        Surface(onClick = { more = true }, shape = RoundedCornerShape(6.dp), color = MaterialTheme.colorScheme.surface, modifier = Modifier.size(48.dp).testTag(RotTags.MORE)) {
            Row(horizontalArrangement = Arrangement.Center, verticalAlignment = Alignment.CenterVertically) { Text(MORE_GLYPH, style = MaterialTheme.typography.titleMedium) }
        }
    }
    About(ui.view, board, sid != null)
    if (picking) {
        PickSheet(list, current, sid != null, onPick = { id ->
            picking = false
            if (id != current.ifEmpty { null }) board.pick(id)
        }) { picking = false }
    }
    if (more) MoreSheet(item, sid != null, titles, board) { more = false }
}

/** 说明：选中那份的开头注释（默认两行，⌄ 展开）；不轮换 ⇒「只用开会话的号」＋ 号牌（会话里）。 */
@Composable
private fun About(
    view: RotationViewReply?,
    board: RotationBoard,
    inSession: Boolean,
) {
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    val st = view?.strategy
    val boardUi by board.state.collectAsState()
    if (st == null) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Text(copyText("rot.none.desc"), color = muted, style = MaterialTheme.typography.bodyMedium)
            if (inSession && view != null) {
                AccountBadge(boardUi.plate(view.start)?.text, boardUi.plate(view.start)?.color, 18)
                Text(board.label(view.start), style = MaterialTheme.typography.bodyMedium)
            }
        }
        return
    }
    if (st.description.isBlank()) return
    var open by rememberSaveable(st.id) { mutableStateOf(false) }
    Column(Modifier.fillMaxWidth().combinedClickableOnce { open = !open }) {
        Text(
            st.description
                .split("\n\n")
                .filter(String::isNotBlank)
                .joinToString("\n\n"),
            style = MaterialTheme.typography.bodyMedium,
            color = muted,
            maxLines = if (open) Int.MAX_VALUE else 2,
            overflow = TextOverflow.Ellipsis,
        )
        if (!open) Text(CARET, color = muted)
    }
}

@OptIn(ExperimentalFoundationApi::class)
private fun Modifier.combinedClickableOnce(onClick: () -> Unit): Modifier = this.combinedClickable(onClick = onClick)

/** 下拉抽屉：「不轮换」＋ 每份（文件名 · 说明第一行；出错的写「这份策略出错」；会话里新会话用的那份带小标）；选中的打勾。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PickSheet(
    list: List<RotStrategyItem>,
    current: String,
    inSession: Boolean,
    onPick: (String?) -> Unit,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss) {
        SheetHead(copyText("rot.top.pick"), null)
        PickRow(copyText("rot.none.label"), "rot-pick-none", second = copyText("rot.none.desc"), selected = current.isEmpty(), onClick = { onPick(null) })
        list.forEach { st ->
            PickRow(
                st.id,
                "rot-pick-${st.id}",
                second = if (st.error != null) copyText("rot.top.unreadable") else st.summary.ifEmpty { null },
                selected = st.id == current,
                onClick = { onPick(st.id) },
                trail = { if (inSession && st.forNew) Tag(copyText("rot.tag.forNew")) },
            )
        }
        SheetEnd()
    }
}

/** 「⋯」抽屉：复制 · 改名… ·（会话里）设为新会话用这份 · 删除（有会话在用 ⇒ 灰，右边「{n} 个会话在用」，长按列出会话标题）· 从文件添加…。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun MoreSheet(
    item: RotStrategyItem?,
    inSession: Boolean,
    titles: Map<String, String>,
    board: RotationBoard,
    onDismiss: () -> Unit,
) {
    val context = LocalContext.current
    val pickFile =
        rememberLauncherForActivityResult(ActivityResultContracts.OpenDocument()) { uri ->
            onDismiss()
            uri?.let { readPicked(context, it) }?.let { (name, text) -> board.add(name, text) }
        }
    var users by remember { mutableStateOf(false) }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        item?.let { st ->
            SheetHead(st.id, null)
            PickRow(copyText("rot.more.copy"), "rot-more-copy", onClick = {
                onDismiss()
                board.copy(st.id)
            })
            PickRow(copyText("rot.more.rename"), "rot-more-rename", onClick = {
                onDismiss()
                board.rename(st.id, st.rev)
            })
            if (inSession) {
                PickRow(copyText("rot.more.forNew"), "rot-more-fornew", enabled = !st.forNew, onClick = {
                    if (!st.forNew) {
                        onDismiss()
                        board.forNew(st.id)
                    }
                })
            }
            val busy = st.inUse.size
            PickRow(
                copyText("rot.more.delete"),
                "rot-more-delete",
                enabled = busy == 0,
                onClick = {
                    if (busy == 0) {
                        onDismiss()
                        board.remove(st.id)
                    }
                },
                onLongClick = { if (busy > 0) users = !users },
                trail = { if (busy > 0) Text(copyText("rot.more.inUse", "n" to busy), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) },
            )
            if (users) {
                Text(
                    st.inUse.joinToString(copyText("kit.text.sep")) { titles[it]?.ifEmpty { null } ?: it },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 20.dp),
                )
            }
            HorizontalDivider(Modifier.padding(vertical = 4.dp))
        }
        PickRow(copyText("rot.more.add"), "rot-more-add", onClick = { pickFile.launch(arrayOf("*/*")) })
        SheetEnd()
    }
}

/** 从手机上选的那一份读成（名字，原文）；读不了 ⇒ `null`。 */
private fun readPicked(
    context: android.content.Context,
    uri: Uri,
): Pair<String, String>? =
    runCatching {
        val name =
            context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c -> if (c.moveToFirst()) c.getString(0) else null }
        val text = context.contentResolver.openInputStream(uri)?.use { it.readBytes().decodeToString() } ?: return null
        (name ?: uri.lastPathSegment.orEmpty()) to text
    }.getOrNull()

@Composable
internal fun Tag(text: String) {
    Surface(shape = RoundedCornerShape(4.dp), border = BorderStroke(1.dp, MaterialTheme.colorScheme.primary), color = MaterialTheme.colorScheme.surface) {
        Text(text, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary, modifier = Modifier.padding(horizontal = 6.dp, vertical = 1.dp))
    }
}

@Composable
internal fun SheetEnd() {
    Row(Modifier.navigationBarsPadding().padding(bottom = 12.dp)) {}
}

// ─────────────────────────────── 提示

/** 提示：一条一块，字是那台写好的；颜色与按钮照 `hint.k`（同桌面那张表）。手机没有悬停：设时区直接把「设成 {zone}」写在按钮上。 */
@Composable
internal fun Hints(
    ui: RotationUi,
    view: RotationViewReply,
    board: RotationBoard,
    onOpenFile: (String) -> Unit,
) {
    view.hints.forEach { n -> Hint(n, view, ui, board, onOpenFile) }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Hint(
    n: RotNote,
    view: RotationViewReply,
    ui: RotationUi,
    board: RotationBoard,
    onOpenFile: (String) -> Unit,
) {
    val k = n.hint.k
    val text = hintText(n, view.now)
    NoteBox(toneOf(n.tone), Modifier.testTag("rot-hint-${k.name.lowercase()}")) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
            if (n.tone == Tone.NEED) NeedsTag()
            Text(text, style = MaterialTheme.typography.bodyMedium)
        }
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
            when (k) {
                RotHintKind.STRATEGY_ERROR -> {
                    n.hint.path?.let { p -> OutlinedButton(onClick = { onOpenFile(p) }) { Text(copyText("rot.hint.code")) } }
                    CopyDetailButton(text, n.detail)
                }
                RotHintKind.TZ_UNKNOWN -> n.hint.zone?.let { z -> OutlinedButton(onClick = { board.setTz(z) }) { Text(copyText("rot.hint.setTzTip", "zone" to z)) } }
                RotHintKind.NEWER -> {
                    SecondaryButton(onClick = board::keepMine) { Text(copyText("rot.hint.keepMine")) }
                    OutlinedButton(onClick = board::loadNew) { Text(copyText("rot.hint.loadNew")) }
                }
                RotHintKind.OLD_SCRIPTS -> OutlinedButton(onClick = board::toggleOld) { Text(copyText("rot.hint.which")) }
                else -> Unit
            }
        }
    }
    if (k == RotHintKind.OLD_SCRIPTS && ui.oldOpen) {
        ModalBottomSheet(onDismissRequest = board::toggleOld) {
            SheetHead(copyText("rot.hint.which"), null)
            n.hint.files.orEmpty().forEach { f ->
                PickRow(f.name, "rot-old-${f.name}", second = f.path, mono = true, onClick = {
                    board.toggleOld()
                    onOpenFile(f.path)
                })
            }
            SheetEnd()
        }
    }
}

@Composable
private fun NeedsTag() {
    val ink = toneInk(NoteTone.WARN)
    Surface(shape = RoundedCornerShape(4.dp), border = BorderStroke(1.dp, ink), color = MaterialTheme.colorScheme.surface) {
        Text(copyText("needs.bar.unknown"), style = MaterialTheme.typography.labelSmall, color = ink, modifier = Modifier.padding(horizontal = 6.dp, vertical = 1.dp))
    }
}

// ─────────────────────────────── 确认 · 名字

/** 等人点头的那一件：换策略丢掉没存的改动（默认焦点［取消］）· 删除策略（撤不回）· 问名字。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun RotAsks(
    ui: RotationUi,
    board: RotationBoard,
) {
    when (val ask = ui.ask) {
        null -> Unit
        is RotAsk.Discard -> Confirm(copyText("rot.ask.discardTitle"), copyText("rot.ask.discardBody"), copyText("rot.ask.discardGo"), board)
        is RotAsk.Delete -> Confirm(copyText("rot.ask.deleteTitle", "name" to ask.id), copyText("rot.ask.deleteBody"), copyText("rot.ask.deleteGo"), board)
        is RotAsk.Name -> AskName(ask, board)
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun Confirm(
    title: String,
    body: String,
    go: String,
    board: RotationBoard,
) {
    val cancel = remember { FocusRequester() }
    ModalBottomSheet(onDismissRequest = { board.answer(false) }, modifier = Modifier.testTag("rot-confirm")) {
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            Text(body, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                OutlinedButton(onClick = { board.answer(false) }, modifier = Modifier.focusRequester(cancel)) { Text(copyText("kit.dialog.cancel")) }
                Button(onClick = { board.answer(true) }) { Text(go) }
            }
        }
        SheetEnd()
    }
    LaunchedEffect(Unit) { runCatching { cancel.requestFocus() } }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AskName(
    ask: RotAsk.Name,
    board: RotationBoard,
) {
    var name by remember(ask) { mutableStateOf(ask.initial) }
    ModalBottomSheet(onDismissRequest = { board.named("") }, modifier = Modifier.testTag("rot-ask-name")) {
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(ask.title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            OutlinedTextField(name, { name = it }, label = { Text(copyText("rot.ask.name")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                SecondaryButton(onClick = { board.named("") }) { Text(copyText("kit.dialog.cancel")) }
                Button(onClick = { board.named(name) }, enabled = name.isNotBlank()) { Text(copyText("rot.ask.go")) }
            }
        }
        SheetEnd()
    }
}

internal const val CARET = "⌄"
private const val MORE_GLYPH = "⋯"

/** 变淡（事实上用不了的号牌 · 灰掉的那一项）。 */
internal fun Modifier.dim(on: Boolean): Modifier = alpha(if (on) DIM else 1f)

private const val DIM = 0.45f
