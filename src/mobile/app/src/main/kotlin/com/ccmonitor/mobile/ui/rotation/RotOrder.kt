package com.ccmonitor.mobile.ui.rotation

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Checkbox
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
import androidx.compose.ui.semantics.stateDescription
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.wire.RotationViewReply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.ui.chat.newsession.AccountBadge

/*
 * 会话的顺序（桌面 `rot-board.ts` 那一块同一处）：第一格 ＝ 在用的号，往下照那台排好的 `lineup`；每行一个勾（去掉 ＝ 本会话先不用）·
 * 号名旁的记号；长按拖一行改本会话先后（松在第一格 ＝ 现在用）。
 * 点一行从底下拉出那个号：只看（此刻 · 用量与重置 · 它在这份策略里的规则）＋［立即用］·［编辑策略］（开设置页里这个号的编辑小窗）。都只这个会话、不改策略。
 */

/** 这个号此刻怎样：策略交的理由；没有 ⇒ 用不了的原因（那台写好的字）。 */
private fun nowOf(
    view: RotationViewReply,
    account: String,
): String {
    val why = view.decision?.why?.firstOrNull { it.account == account }
    val blocked = view.accounts.firstOrNull { it.account == account }?.blocked
    return why?.why ?: blocked?.text.orEmpty()
}

/** 顺序这一块：小标题 ＋ 被拦下那一句 ＋ 一号一行 ＋ 点开的那个号（底下拉出）。 */
@Composable
internal fun Order(
    ui: RotationUi,
    view: RotationViewReply,
    board: RotationBoard,
    onEditStrategy: (String?) -> Unit,
) {
    RotHeader(copyText("rot.h.order"), copyText("rot.h.orderHint"))
    ui.refused?.let { (_, t) -> Text(t, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
    val rects = remember { mutableStateMapOf<Int, Rect>() }
    var drag by remember { mutableStateOf<Pair<Int, Offset>?>(null) }
    val haptic = LocalHapticFeedback.current
    val target = drag?.let { (_, at) -> rects.entries.firstOrNull { it.value.contains(at) }?.key }
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        view.lineup.forEachIndexed { i, account ->
            OrderRow(
                i,
                account,
                view,
                board,
                lifted = drag?.first == i,
                landing = target == i && drag?.first != i,
                modifier =
                    Modifier
                        .onGloballyPositioned { rects[i] = it.boundsInRoot() }
                        .rowDrag(i, rects, onMove = { d ->
                            if (drag == null && d != null) haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                            drag = d
                        }) { from, at ->
                            drag = null
                            val to = rects.entries.firstOrNull { it.value.contains(at) }?.key ?: return@rowDrag
                            board.drop(account, from, to)
                        },
            )
        }
    }
    ui.open?.takeIf { it in view.lineup }?.let { OrderSheet(view, board, it, onEditStrategy) }
}

/** 长按（先震一下）再拖：拖的时候那一行浮起；松在哪一行上 ⇒ 落到那一格。 */
private fun Modifier.rowDrag(
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
private fun OrderRow(
    i: Int,
    account: String,
    view: RotationViewReply,
    board: RotationBoard,
    lifted: Boolean,
    landing: Boolean,
    modifier: Modifier,
) {
    val f = view.accounts.firstOrNull { it.account == account }
    val off = account in view.off
    val marks = marksOf(view, account)
    val dashed = marks.any { it.dashed }
    Surface(
        shape = RoundedCornerShape(6.dp),
        color = MaterialTheme.colorScheme.surfaceContainer,
        border = if (landing) BorderStroke(2.dp, MaterialTheme.colorScheme.primary) else null,
        modifier =
            modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .graphicsLayer { shadowElevation = if (lifted) LIFT else 0f }
                .dashedFrame(dashed && !landing, MaterialTheme.colorScheme.outline)
                .clickable { board.open(account) }
                .semantics { if (marks.isNotEmpty()) stateDescription = marks.joinToString(copyText("kit.text.sep")) { it.tip } }
                .testTag("${RotTags.ROW}-$account"),
    ) {
        Row(Modifier.padding(end = 8.dp).dim(off || f?.blocked != null), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Checkbox(
                checked = !off,
                onCheckedChange = { on -> board.off(account, !on) },
                modifier = Modifier.semantics { contentDescription = copyText("rot.order.use", "account" to board.label(account)) },
            )
            Text((i + 1).toString(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            f?.let { AccountBadge(it.plate.text, it.plate.color, 18) }
            Column(Modifier.weight(1f)) {
                NameWithMarks(board.label(account), marks, bold = i == 0)
                val now = nowOf(view, account)
                if (now.isNotEmpty()) Text(now, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            f?.let { Meters(it) }
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun OrderSheet(
    view: RotationViewReply,
    board: RotationBoard,
    account: String,
    onEditStrategy: (String?) -> Unit,
) {
    val f = view.accounts.firstOrNull { it.account == account }
    val row = rowOf(view, account)
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    ModalBottomSheet(onDismissRequest = { board.open(null) }, modifier = Modifier.testTag("rot-sheet-$account")) {
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                f?.let { AccountBadge(it.plate.text, it.plate.color, 20) }
                Text(board.label(account), fontWeight = FontWeight.SemiBold)
                Marks(marksOf(view, account))
            }
            nowOf(view, account).takeIf(String::isNotEmpty)?.let { Text(copyText("rot.order.now", "why" to it), style = MaterialTheme.typography.bodyMedium) }
            row?.let { Text(it.line, style = MaterialTheme.typography.bodySmall, color = muted) }
            row?.rules?.forEach { Text(it, style = MaterialTheme.typography.bodySmall, color = muted) }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(8.dp, Alignment.End)) {
                SecondaryButton(onClick = {
                    board.open(null)
                    onEditStrategy(account)
                }) { Text(copyText("rot.top.edit")) }
                if (account != view.current) {
                    OutlinedButton(onClick = {
                        board.open(null)
                        board.useNow(account)
                    }) { Text(copyText("rot.acct.useNow", "account" to board.label(account))) }
                }
            }
        }
        SheetEnd()
    }
}

private const val LIFT = 12f
