package com.ccmonitor.mobile.ui.rotation

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.gestures.detectTapGestures
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Popup
import com.ccmonitor.mobile.core.claude.link.RotEdit
import com.ccmonitor.mobile.core.claude.link.wire.RotCell
import com.ccmonitor.mobile.core.claude.link.wire.RotDaySeg
import com.ccmonitor.mobile.core.claude.link.wire.RotFormKind
import com.ccmonitor.mobile.core.claude.link.wire.RotFrom
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.ui.chat.newsession.PickRow
import com.ccmonitor.mobile.ui.chat.newsession.SheetHead

/** 一格 ⇒ 一行（数 · 开关 · 选项：名字在左、控件在右；24 小时条：名字在上、条在下）；值 · 从哪来 · 字都是那台解好的；没收下 ⇒ 下面一行红字。 */
@Composable
internal fun CellRows(
    ui: RotationUi,
    board: RotationBoard,
    cell: RotCell,
    row: Int?,
) {
    val id = cellId(cell, row)
    val set: (Any?) -> Unit = { next -> board.edit(RotEdit.set(cell.key, row, next), id) }
    Column(Modifier.fillMaxWidth().testTag("rot-cell-$id")) {
        when (cell.kind) {
            RotFormKind.DAY_SEGMENTS -> DayCell(board, cell, row, id)
            else ->
                Row(Modifier.fillMaxWidth().heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    CellLabel(cell, Modifier.weight(1f))
                    CellControl(cell, set)
                }
        }
        ui.cellFailed?.takeIf { it.cell == id }?.let { Text(it.text, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
    }
}

internal fun cellId(
    cell: RotCell,
    row: Int?,
): String = cell.key + (cell.window?.let { ".$it" } ?: "") + "@" + (row?.toString() ?: "all")

private val RotCell.inherited: Boolean get() = from == RotFrom.ALL

@Composable
private fun CellControl(
    cell: RotCell,
    set: (Any?) -> Unit,
) {
    when (cell.kind) {
        RotFormKind.TOGGLE -> Switch(checked = cell.v == true, onCheckedChange = { set(it) })
        RotFormKind.PERCENT -> NumberCell(cell, copyText("rot.unit.percent"), set)
        RotFormKind.MINUTES -> NumberCell(cell, copyText("rot.unit.minutes"), set)
        RotFormKind.CHOICE -> ChoiceCell(cell, cell.v as? String, set)
        RotFormKind.DAY_SEGMENTS -> Unit
    }
}

/** 格名：只写 `label`；长按升起一个小浮层写 `help`（手机没有悬停）。跟总体 ⇒ 淡。 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun CellLabel(
    cell: RotCell,
    modifier: Modifier = Modifier,
) {
    var help by remember { mutableStateOf(false) }
    val helpText = cell.help.orEmpty()
    Box(modifier) {
        Text(
            cell.label,
            style = MaterialTheme.typography.bodyMedium,
            color = if (cell.inherited) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
            modifier = Modifier.combinedClickable(onClick = {}, onLongClick = { if (helpText.isNotEmpty()) help = true }),
        )
        if (help) {
            Popup(alignment = Alignment.BottomStart, onDismissRequest = { help = false }) {
                Surface(shape = RoundedCornerShape(6.dp), tonalElevation = 6.dp, shadowElevation = 6.dp, modifier = Modifier.padding(top = 24.dp).width(260.dp)) {
                    Text(helpText, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(10.dp))
                }
            }
        }
    }
}

/** 数格（百分数 · 分钟）：可空的带一颗空值说法的钮（按下 ＝ 空着，数变淡）；点数原位聚焦、弹数字键盘，收键盘或点别处即写。 */
@Composable
private fun NumberCell(
    cell: RotCell,
    unit: String,
    set: (Any?) -> Unit,
) {
    val num = cell.num ?: 0.0
    val isNull = cell.nullable != null && cell.v == null
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        cell.nullable?.let { n ->
            val ink = if (isNull) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
            OutlinedButton(onClick = { set(if (isNull) num else null) }, contentPadding = SMALL_PAD) { Text(n, color = ink, style = MaterialTheme.typography.bodySmall) }
        }
        NumberBox(num, unit, faint = isNull || cell.inherited, 88.dp, cell.label) { set(it) }
    }
}

/** 40 dp 高的数框：失焦 · 完成 ⇒ 交（空着 · 不是数 ⇒ 不交、回到读到的那个）。 */
@Composable
internal fun NumberBox(
    value: Double,
    unit: String,
    faint: Boolean,
    width: Dp,
    label: String,
    onSet: (Double) -> Unit,
) {
    var text by remember(value) { mutableStateOf(numText(value)) }
    var focused by remember { mutableStateOf(false) }
    val focus = LocalFocusManager.current
    val commit = {
        text
            .trim()
            .toDoubleOrNull()
            ?.takeIf { it != value }
            ?.let(onSet) ?: run { text = numText(value) }
    }
    val ink = if (faint) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface
    val line = if (focused) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline
    Row(
        Modifier
            .width(width)
            .height(40.dp)
            .dashedOr(faint, line)
            .padding(horizontal = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        BasicTextField(
            text,
            { text = it },
            singleLine = true,
            textStyle = TextStyle(color = ink, textAlign = TextAlign.End, fontSize = MaterialTheme.typography.bodyMedium.fontSize),
            cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number, imeAction = ImeAction.Done),
            keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
            modifier =
                Modifier.weight(1f).testTag("rot-num-$label").onFocusChanged {
                    if (focused && !it.isFocused) commit()
                    focused = it.isFocused
                },
        )
        Text(" $unit", color = ink, style = MaterialTheme.typography.bodyMedium)
    }
}

/** 虚线（跟总体 · 空着）或实线的框。 */
@Composable
private fun Modifier.dashedOr(
    dashed: Boolean,
    line: androidx.compose.ui.graphics.Color,
): Modifier =
    if (!dashed) {
        border(1.dp, line, RoundedCornerShape(6.dp))
    } else {
        drawBehind {
            drawRoundRect(line, style = Stroke(1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 4f))), cornerRadius = CornerRadius(6.dp.toPx()))
        }
    }

/** 选项：三项以内分段钮；更多 ⇒ 底部抽屉。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChoiceCell(
    cell: RotCell,
    cur: String?,
    set: (Any?) -> Unit,
) {
    val choices = cell.choices.orEmpty()
    if (choices.size <= SEGMENTS) {
        SingleChoiceSegmentedButtonRow {
            choices.forEachIndexed { i, c ->
                SegmentedButton(selected = c.v == cur, onClick = { set(c.v) }, shape = SegmentedButtonDefaults.itemShape(i, choices.size), icon = {}) { Text(c.label) }
            }
        }
        return
    }
    var open by remember { mutableStateOf(false) }
    OutlinedButton(onClick = { open = true }) { Text(choices.firstOrNull { it.v == cur }?.label.orEmpty() + " " + CARET) }
    if (open) {
        ModalBottomSheet(onDismissRequest = { open = false }) {
            SheetHead(cell.label, null)
            choices.forEach { c ->
                PickRow(c.label, "rot-choice-${c.v}", selected = c.v == cur, onClick = {
                    open = false
                    set(c.v)
                })
            }
            SheetEnd()
        }
    }
}

// ─────────────────────────────── 24 小时条

/** 一窗一格：名字在上、条在下（32 dp）；长按（先震一下）再横拖刷一段，点已有的一段改那一段 ⇒ 底部抽屉。 */
@Composable
private fun DayCell(
    board: RotationBoard,
    cell: RotCell,
    row: Int?,
    id: String,
) {
    val segs = cell.segs.orEmpty()
    var paint by remember { mutableStateOf<Pair<Long, Long>?>(null) }
    Column(Modifier.fillMaxWidth().padding(vertical = 4.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        CellLabel(cell)
        DayBar(segs, cell.inherited, onPick = { paint = it }, tag = "rot-day-$id")
        DayAxis()
    }
    paint?.let { (from, to) ->
        PaintSheet(from, to, onDismiss = { paint = null }) { v ->
            paint = null
            // 一整天（0–24）交「相等」，到 24:00 为止交「到 0 点」（那台的写法：`to ≤ from` 跨午夜）。
            board.edit(RotEdit.day(cell.key, row, cell.window.orEmpty(), from % DAY_MIN, to % DAY_MIN, v), id)
        }
    }
}

@Composable
private fun DayBar(
    segs: List<RotDaySeg>,
    follows: Boolean,
    onPick: (Pair<Long, Long>) -> Unit,
    tag: String,
) {
    val segColor = MaterialTheme.colorScheme.surfaceContainerHighest
    val zero = MaterialTheme.colorScheme.errorContainer
    val accent = MaterialTheme.colorScheme.primary
    var ghost by remember { mutableStateOf<Pair<Long, Long>?>(null) }
    val haptic = LocalHapticFeedback.current
    BoxWithConstraints(
        Modifier
            .fillMaxWidth()
            .height(32.dp)
            .dashedOr(follows, MaterialTheme.colorScheme.outline)
            .testTag(tag),
    ) {
        val wPx = constraints.maxWidth.toFloat().coerceAtLeast(1f)
        val full = maxWidth
        segs.forEach { g ->
            Box(
                Modifier
                    .offset(x = full * (g.from.toFloat() / DAY_MIN))
                    .width(full * ((g.to - g.from).toFloat() / DAY_MIN))
                    .fillMaxHeight()
                    .padding(1.dp)
                    .background(if (g.v == 0.0) zero else segColor, RoundedCornerShape(3.dp)),
                contentAlignment = Alignment.CenterStart,
            ) { Text(g.text, style = MaterialTheme.typography.labelSmall, maxLines = 1, modifier = Modifier.padding(horizontal = 4.dp)) }
        }
        ghost?.let { (a, b) ->
            Box(
                Modifier
                    .offset(x = full * (a.toFloat() / DAY_MIN))
                    .width(full * ((b - a).toFloat() / DAY_MIN))
                    .fillMaxHeight()
                    .border(2.dp, accent, RoundedCornerShape(3.dp)),
            )
        }
        Box(
            Modifier.fillMaxWidth().fillMaxHeight().dayGestures(wPx, segs, onGhost = { g ->
                if (ghost == null && g != null) haptic.performHapticFeedback(HapticFeedbackType.LongPress)
                ghost = g
            }, onPick = onPick),
        )
    }
}

/** 点 ⇒ 点中的那一段整段（没有 ⇒ 一整天）；长按再横拖 ⇒ 拖出来的那一截（半小时吸附）。只是几何。 */
private fun Modifier.dayGestures(
    wPx: Float,
    segs: List<RotDaySeg>,
    onGhost: (Pair<Long, Long>?) -> Unit,
    onPick: (Pair<Long, Long>) -> Unit,
): Modifier =
    pointerInput(segs, wPx) {
        detectTapGestures { p ->
            val m = minuteAt(p.x / wPx)
            val hit = segs.firstOrNull { it.from <= m && m < it.to }
            onPick((hit?.from ?: 0L) to (hit?.to ?: DAY_MIN))
        }
    }.pointerInput(segs, wPx) {
        var start = 0L
        var end = 0L
        detectDragGesturesAfterLongPress(
            onDragStart = { p ->
                start = minuteAt(p.x / wPx)
                end = start
                onGhost(start to start)
            },
            onDrag = { ch, _ ->
                ch.consume()
                end = minuteAt(ch.position.x / wPx)
                onGhost(minOf(start, end) to maxOf(start, end))
            },
            onDragEnd = {
                onGhost(null)
                val a = minOf(start, end)
                val b = maxOf(start, end)
                if (b - a >= SNAP_MIN) onPick(a to b)
            },
            onDragCancel = { onGhost(null) },
        )
    }

@Composable
private fun DayAxis() {
    Row(Modifier.fillMaxWidth()) {
        listOf(0L, 6L, 12L, 18L, 24L).forEachIndexed { i, h ->
            Text(
                dayClock(h * 60).take(2),
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                textAlign = if (i == 4) TextAlign.End else TextAlign.Start,
                modifier = Modifier.weight(if (i == 4) 0.5f else 1f),
            )
        }
    }
}

/** 松手升起的那一张：「{from}–{to} · 上限」：0% · 50% · 100% ＋ 数格 ·［删这段］·［取消］·［存下］。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PaintSheet(
    from: Long,
    to: Long,
    onDismiss: () -> Unit,
    onGo: (Double?) -> Unit,
) {
    var v by remember { mutableStateOf(0.0) }
    ModalBottomSheet(onDismissRequest = onDismiss, modifier = Modifier.testTag("rot-paint")) {
        Column(Modifier.padding(horizontal = 20.dp), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Text(copyText("rot.day.pop", "from" to dayClock(from), "to" to dayClock(to)), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
            Row(verticalAlignment = Alignment.CenterVertically) {
                SingleChoiceSegmentedButtonRow(Modifier.weight(1f)) {
                    PRESETS.forEachIndexed { i, p ->
                        SegmentedButton(selected = v == p, onClick = { v = p }, shape = SegmentedButtonDefaults.itemShape(i, PRESETS.size), icon = {}) { Text(copyText("rot.day.v", "v" to numText(p))) }
                    }
                }
                NumberBox(v, copyText("rot.unit.percent"), faint = false, 80.dp, copyText("rot.day.popLabel")) { v = it.coerceIn(0.0, 100.0) }
            }
            Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                SecondaryButton(onClick = { onGo(null) }) { Text(copyText("rot.day.del")) }
                Box(Modifier.weight(1f))
                SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) }
                Button(onClick = { onGo(v) }) { Text(copyText("rot.ask.go")) }
            }
        }
        SheetEnd()
    }
}

private val PRESETS = listOf(0.0, 50.0, 100.0)
private const val SEGMENTS = 3
private val SMALL_PAD =
    androidx.compose.foundation.layout
        .PaddingValues(horizontal = 10.dp, vertical = 4.dp)
