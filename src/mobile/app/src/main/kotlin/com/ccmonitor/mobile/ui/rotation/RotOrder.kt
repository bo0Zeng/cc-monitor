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
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
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
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.wire.gen.RotGroup
import com.ccmonitor.mobile.core.wire.gen.RotLine
import com.ccmonitor.mobile.core.wire.gen.RotationViewReply
import com.ccmonitor.mobile.ui.chat.newsession.AccountBadge

/*
 * 会话的顺序（桌面 `rot-order.ts` 同一形）：那台排好的 `lineup` 一号一行（第一格在用；「此刻用不了」·「先不用」各成一组、拿不起）；
 * 行：序号 · 号牌 · 号名＋记号（名下策略写的那一句）· 两条量 · 右格（那台写好的此刻怎样）· 勾（去掉 ＝ 本会话先不用）。
 * 长按拿起（`RotDrag.kt` 那张表，同桌面）：跟手 · 别的行让位 · 悬停第一格写「松手换到」· 放下交一次 move、回包才定（拒 ⇒ 滑回；
 * 没回包 ⇒ 留在放下处标「没确认」）。拿着 · 等回包时那台推来的新顺序先不换上（手上正在改的不被盖掉）。离线不拿起。
 * 点一行从底下拉出那个号：只看 ＋［立即用］·［编辑策略］。都只这个会话、不改策略。
 */

/** 状态机此刻的样子（重组照它摆）：每行让开多少 · 拿着第几行 · 悬停第一格时拿的是谁。 */
private data class Pose(
    val shifts: List<Float> = emptyList(),
    val lifted: Int = -1,
    val first: String? = null,
)

/** 手上那一行：状态机 ＋ 量好的行 ＋ 拿起那一刻的那一份 ＋ 没确认的那一个号（重组照这几格画）。 */
private class Hand(
    private val drag: OrderDrag,
    private val board: RotationBoard,
) {
    val rects = mutableStateMapOf<Int, Rect>()
    var pose by mutableStateOf(Pose())
    var held by mutableStateOf<List<RotLine>?>(null)
    var unsure by mutableStateOf<String?>(null)
    private var lineup: List<RotLine> = emptyList()
    private var moved = 0f

    private fun sync() {
        pose = Pose(lineup.indices.map(drag::offset), if (drag.phase == DragPhase.LIFTED) drag.from else -1, drag.account.takeIf { drag.first })
    }

    /** 长按拿起第 [i] 行（拿不起 ⇒ `false`）。 */
    fun lift(
        rows: List<RotLine>,
        i: Int,
    ): Boolean {
        val geo = rows.mapIndexed { k, l -> DragRow(l.account, l.movable, rects[k]?.top ?: 0f, rects[k]?.height ?: 0f) }
        if (!drag.lift(geo, i)) return false
        lineup = rows
        held = rows
        unsure = null
        moved = 0f
        sync()
        return true
    }

    /** 跟手（拿着才认，认了 ⇒ `true`）。 */
    fun follow(dy: Float): Boolean {
        if (drag.phase != DragPhase.LIFTED) return false
        moved += dy
        drag.move(moved)
        sync()
        return true
    }

    /** 松手：交了 ⇒ 回包才定（成 · 拒 ⇒ 回静止照那台画；说不清 ⇒ 留在放下处标「没确认」）。 */
    fun release() {
        val end = drag.release()
        sync()
        if (end !is DragEnd.Drop) {
            held = null
            return
        }
        board.drop(end.account, end.to) { how ->
            if (how == MoveEnd.UNSURE) {
                unsure = end.account
            } else {
                drag.settle()
                held = null
            }
            sync()
        }
    }

    fun cancel() {
        drag.cancel()
        held = null
        sync()
    }
}

/** 顺序这一块：小标题（手动排过 ·［照策略排］）＋ 被拦下那一句 ＋ 一号一行 ＋ 点开的那个号（底下拉出）。 */
@Composable
internal fun Order(
    ui: RotationUi,
    view: RotationViewReply,
    board: RotationBoard,
    offline: Boolean,
    onEditStrategy: (String?) -> Unit,
) {
    RotHeader(copyText("rot.h.order"), if (view.manual) copyText("rot.order.manual") else copyText("rot.h.orderHint")) {
        if (view.manual) SecondaryButton(onClick = board::clearTemp) { Text(copyText("rot.order.reset")) }
    }
    ui.refused?.let { (_, t) -> Text(t, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
    val down by rememberUpdatedState(offline)
    val hand = remember { Hand(OrderDrag { !down }, board) }
    val haptic = LocalHapticFeedback.current
    // 拿着 · 等回包 ⇒ 画拿起那一刻的那一份；静止了照那台此刻的。
    val lineup = hand.held ?: view.lineup
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        var group: RotGroup? = null
        lineup.forEachIndexed { i, line ->
            val g = if (line.group == RotGroup.USE) RotGroup.NEXT else line.group
            if (g != group) {
                group = g
                GroupHead(g)
            }
            if (i == 0) hand.pose.first?.let { DropFirst(board.label(it)) }
            OrderRow(
                i,
                line,
                view,
                board,
                lifted = hand.pose.lifted == i,
                unconfirmed = hand.unsure == line.account,
                shift = hand.pose.shifts.getOrElse(i) { 0f },
                modifier =
                    Modifier
                        .onGloballyPositioned { hand.rects[i] = it.boundsInRoot() }
                        .pointerInput(i, line.account, offline) {
                            detectDragGesturesAfterLongPress(
                                onDragStart = { if (hand.lift(lineup, i)) haptic.performHapticFeedback(HapticFeedbackType.LongPress) },
                                onDrag = { ch, d -> if (hand.follow(d.y)) ch.consume() },
                                onDragEnd = hand::release,
                                onDragCancel = hand::cancel,
                            )
                        },
            )
        }
    }
    ui.open?.takeIf { a -> lineup.any { it.account == a } }?.let { OrderSheet(view, board, it, onEditStrategy) }
}

/** 悬停第一格：空槽上那一句「松手换到」＋ 拿着的那个号。 */
@Composable
private fun DropFirst(name: String) {
    Row(Modifier.testTag(RotTags.FIRST), horizontalArrangement = Arrangement.spacedBy(4.dp)) {
        Text(copyText("rot.order.dropFirst"), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary)
        Text(name, style = MaterialTheme.typography.labelSmall, fontWeight = FontWeight.SemiBold)
    }
}

/** 一组的小标题（可挪那一组不写：顺序的标题就是它）。 */
@Composable
private fun GroupHead(g: RotGroup) {
    val text =
        when (g) {
            RotGroup.OUT -> copyText("rot.order.groupOut")
            RotGroup.OFF -> copyText("rot.order.groupOff")
            RotGroup.USE, RotGroup.NEXT -> return
        }
    Text(text, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(top = 6.dp))
}

@Composable
private fun OrderRow(
    i: Int,
    line: RotLine,
    view: RotationViewReply,
    board: RotationBoard,
    lifted: Boolean,
    unconfirmed: Boolean,
    shift: Float,
    modifier: Modifier,
) {
    val account = line.account
    val f = view.accounts.firstOrNull { it.account == account }
    val marks = marksOf(view, account)
    val use = line.group == RotGroup.USE
    val said = listOfNotNull(line.stay) + marks.map { it.tip }
    Surface(
        shape = RoundedCornerShape(6.dp),
        color = MaterialTheme.colorScheme.surfaceContainer,
        border = if (use) BorderStroke(1.dp, MaterialTheme.colorScheme.primary) else null,
        modifier =
            modifier
                .fillMaxWidth()
                .heightIn(min = if (use) USE_ROW else ROW)
                .graphicsLayer {
                    translationY = shift
                    shadowElevation = if (lifted) LIFT else 0f
                    scaleX = if (lifted) LIFT_SCALE else 1f
                    scaleY = if (lifted) LIFT_SCALE else 1f
                }.dashedFrame(marks.any { it.dashed } || unconfirmed, MaterialTheme.colorScheme.outline)
                .clickable { board.open(account) }
                .semantics { if (said.isNotEmpty()) stateDescription = said.joinToString(copyText("kit.text.sep")) }
                .testTag("${RotTags.ROW}-$account"),
    ) {
        Row(Modifier.padding(start = 8.dp, end = 4.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            Row(Modifier.weight(1f).dim(!line.movable && !use), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text((i + 1).toString(), style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                f?.let { AccountBadge(it.plate.text, it.plate.color, 18) }
                Column(Modifier.weight(1f)) {
                    NameWithMarks(board.label(account), marks, bold = use)
                    line.why?.let { Text(it, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
                f?.let { Meters(it) }
            }
            StateCell(line.state, unconfirmed)
            Checkbox(
                checked = line.group != RotGroup.OFF,
                onCheckedChange = { on -> board.off(account, !on) },
                modifier = Modifier.semantics { contentDescription = board.label(account) },
            )
        }
    }
}

/** 右格：那台写好的此刻怎样；放下没回包 ⇒「没确认」。 */
@Composable
private fun StateCell(
    state: String,
    unconfirmed: Boolean,
) {
    val word = copyText("rot.order.unconfirmed")
    Text(if (unconfirmed) word else state, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
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
    val line = view.lineup.firstOrNull { it.account == account }
    val row = rowOf(view, account)
    val muted = MaterialTheme.colorScheme.onSurfaceVariant
    ModalBottomSheet(onDismissRequest = { board.open(null) }, modifier = Modifier.testTag("rot-sheet-$account")) {
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                f?.let { AccountBadge(it.plate.text, it.plate.color, 20) }
                Text(board.label(account), fontWeight = FontWeight.SemiBold)
                Marks(marksOf(view, account))
            }
            (line?.why ?: line?.stay)?.let { Text(it, style = MaterialTheme.typography.bodyMedium) }
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
                    }) { Text(copyText("rot.order.useNow")) }
                }
            }
        }
        SheetEnd()
    }
}

private const val LIFT = 12f
private const val LIFT_SCALE = 1.02f
private val ROW = 48.dp
private val USE_ROW = 56.dp
