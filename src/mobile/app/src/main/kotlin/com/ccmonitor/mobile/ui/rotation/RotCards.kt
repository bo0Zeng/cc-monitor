package com.ccmonitor.mobile.ui.rotation

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Rect
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.boundsInRoot
import androidx.compose.ui.layout.onGloballyPositioned
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.RotEdit
import com.ccmonitor.mobile.core.claude.link.wire.Fact
import com.ccmonitor.mobile.core.claude.link.wire.RotationViewReply
import com.ccmonitor.mobile.core.claude.link.wire.WinFact
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.ui.chat.newsession.AccountBadge
import com.ccmonitor.mobile.ui.chat.newsession.PickRow
import com.ccmonitor.mobile.ui.chat.newsession.SheetHead

/** 号牌这一块：小标题 ＋ 号牌（390 宽一排三个、360 宽一排两个）＋ 末尾「＋ 加号」＋ 点开的那个号。 */
@Composable
internal fun Cards(
    ui: RotationUi,
    view: RotationViewReply,
    board: RotationBoard,
) {
    RotHeader(copyText("rot.h.cards"), copyText("mobRot.cards.hint"))
    val listed = view.accounts.take(view.listed)
    val rows = view.board.rows
    val rects = remember { mutableStateMapOf<Int, Rect>() }
    var drag by remember { mutableStateOf<Pair<Int, Offset>?>(null) }
    var adding by remember { mutableStateOf(false) }
    val haptic = LocalHapticFeedback.current
    val target = drag?.let { (_, at) -> rects.entries.firstOrNull { it.value.contains(at) }?.key }
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val cols = if (maxWidth >= WIDE) 3 else 2
        val w = (maxWidth - GAP * (cols - 1)) / cols
        FlowRow(horizontalArrangement = Arrangement.spacedBy(GAP), verticalArrangement = Arrangement.spacedBy(GAP)) {
            listed.forEachIndexed { i, f ->
                Card(
                    i = i,
                    f = f,
                    hover = rows.getOrNull(i)?.hover.orEmpty(),
                    ui = ui,
                    board = board,
                    inUse = board.inSession && f.account == view.current,
                    lifted = drag?.first == i,
                    landing = target == i && drag?.first != i,
                    modifier =
                        Modifier
                            .width(w)
                            .onGloballyPositioned { rects[i] = it.boundsInRoot() }
                            .cardDrag(i, rects, onMove = { d ->
                                if (drag == null && d != null) haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                                drag = d
                            }) { from, at ->
                                drag = null
                                val to = rects.entries.firstOrNull { it.value.contains(at) }?.key ?: return@cardDrag
                                if (to != from) board.edit(RotEdit.move(from, to))
                            },
                )
            }
            AddCard(Modifier.width(w), enabled = view.accounts.size > view.listed) { adding = true }
        }
    }
    ui.open?.let { a -> AccountPane(ui, view, board, a) }
    if (adding) AddSheet(view, ui, board) { adding = false }
}

/** 长按（先震一下）再拖：拖的时候号牌浮起；松在哪张号牌上 ⇒ 排到它前头。 */
private fun Modifier.cardDrag(
    i: Int,
    rects: Map<Int, Rect>,
    onMove: (Pair<Int, Offset>?) -> Unit,
    onDrop: (Int, Offset) -> Unit,
): Modifier =
    pointerInput(i) {
        var at = Offset.Zero
        detectDragGesturesAfterLongPress(
            onDragStart = { off ->
                at = (rects[i]?.topLeft ?: Offset.Zero) + off
                onMove(i to at)
            },
            onDrag = { ch, d ->
                ch.consume()
                at += d
                onMove(i to at)
            },
            onDragEnd = { onDrop(i, at) },
            onDragCancel = { onMove(null) },
        )
    }

@Composable
private fun Card(
    i: Int,
    f: Fact,
    hover: String,
    ui: RotationUi,
    board: RotationBoard,
    inUse: Boolean,
    lifted: Boolean,
    landing: Boolean,
    modifier: Modifier,
) {
    val accent = MaterialTheme.colorScheme.primary
    val open = ui.open == f.account
    val border =
        when {
            open || landing -> BorderStroke(if (landing) 2.dp else 1.dp, accent)
            else -> BorderStroke(1.dp, MaterialTheme.colorScheme.outlineVariant)
        }
    Box(modifier.graphicsLayer { shadowElevation = if (lifted) LIFT else 0f }) {
        Surface(
            shape = RoundedCornerShape(6.dp),
            color = MaterialTheme.colorScheme.surfaceContainer,
            border = border,
            modifier =
                Modifier
                    .fillMaxWidth()
                    .height(44.dp)
                    .clickable { board.open(if (open) null else f.account) }
                    .semantics { contentDescription = board.label(f.account) + copyText("kit.text.sep") + hover }
                    .testTag("${RotTags.CARD}-${f.account}"),
        ) {
            Row(Modifier.padding(horizontal = 8.dp).dim(f.blocked != null), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text((i + 1).toString(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                AccountBadge(f.account, ui.accounts[f.account]?.color, 18)
                Text(board.label(f.account), maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                Meters(f)
            }
        }
        if (inUse) {
            Box(
                Modifier
                    .align(Alignment.TopEnd)
                    .padding(2.dp)
                    .size(7.dp)
                    .background(LocalAppTokens.current.success, CircleShape),
            )
        }
    }
}

/** 号牌上的两条小计量（上 5h、下 7d）。 */
@Composable
private fun Meters(f: Fact) {
    Column(verticalArrangement = Arrangement.spacedBy(3.dp)) {
        Meter(copyText("rot.card.w5h"), f.w5h)
        Meter(copyText("rot.card.w7d"), f.w7d)
    }
}

@Composable
private fun Meter(
    win: String,
    w: WinFact,
) {
    val track = MaterialTheme.colorScheme.outlineVariant
    val fill = if (w.used >= FULL) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurface
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(3.dp)) {
        Text(win, style = MaterialTheme.typography.labelSmall.copy(fontSize = MaterialTheme.typography.labelSmall.fontSize * 0.75f), color = MaterialTheme.colorScheme.onSurfaceVariant)
        Box(Modifier.width(18.dp).height(3.dp).background(track, RoundedCornerShape(2.dp))) {
            Box(Modifier.fillMaxWidth((w.used / FULL).coerceIn(0.0, 1.0).toFloat()).height(3.dp).background(fill, RoundedCornerShape(2.dp)))
        }
    }
}

@Composable
private fun AddCard(
    modifier: Modifier,
    enabled: Boolean,
    onClick: () -> Unit,
) {
    Box(
        modifier
            .height(44.dp)
            .border(1.dp, MaterialTheme.colorScheme.outlineVariant, RoundedCornerShape(6.dp))
            .clickable(enabled = enabled, onClick = onClick)
            .dim(!enabled),
        contentAlignment = Alignment.Center,
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
            PhIcon(Ph.Plus, contentDescription = null, size = Touch.iconInline, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(copyText("rot.card.add"), color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/** 「＋ 加号」：号列表外的那几个（那台给的，`accounts` 里 `listed` 之后的）。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun AddSheet(
    view: RotationViewReply,
    ui: RotationUi,
    board: RotationBoard,
    onDismiss: () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss) {
        SheetHead(copyText("rot.card.add"), null)
        view.accounts.drop(view.listed).forEach { f ->
            PickRow(board.label(f.account), "rot-add-${f.account}", onClick = {
                onDismiss()
                board.edit(RotEdit.add(f.account))
            }, lead = { AccountBadge(f.account, ui.accounts[f.account]?.color, 20) })
        }
        SheetEnd()
    }
}

/** 点开的那个号：名 · 此刻用量与重置 ·［立即用］（会话里，不是在用的那个）·［移出］· 被拦那一句 · `scope: each` 那几格。 */
@Composable
private fun AccountPane(
    ui: RotationUi,
    view: RotationViewReply,
    board: RotationBoard,
    account: String,
) {
    val i = view.accounts.indexOfFirst { it.account == account }.takeIf { it in 0 until view.listed } ?: return
    val f = view.accounts[i]
    val row = view.board.rows.getOrNull(i)
    Surface(shape = RoundedCornerShape(8.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth().testTag("rot-pane-$account")) {
        Column(Modifier.padding(12.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                AccountBadge(account, ui.accounts[account]?.color, 20)
                Text(board.label(account), fontWeight = FontWeight.SemiBold)
            }
            Text(row?.line.orEmpty(), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Row(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalAlignment = Alignment.CenterVertically) {
                if (board.inSession && account != view.current) {
                    OutlinedButton(onClick = { board.useNow(account) }, enabled = f.blocked == null) { Text(copyText("rot.acct.useNow", "account" to board.label(account))) }
                }
                SecondaryButton(onClick = { board.edit(RotEdit.remove(i)) }) { Text(copyText("rot.acct.remove")) }
            }
            f.blocked?.let { b -> NoteBox(NoteTone.WARN) { Text(b.text, style = MaterialTheme.typography.bodySmall) } }
            ui.refused?.takeIf { it.first == account }?.let { (_, t) -> Text(t, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
            row?.cells?.forEach { CellRows(ui, board, it, i) }
        }
    }
}

private val WIDE = 340.dp
private val GAP = 8.dp
private const val FULL = 100.0
private const val LIFT = 12f
