package com.ccmonitor.mobile.ui.rotation

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
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
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalFocusManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextStyle
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
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.ui.chat.newsession.PickRow
import com.ccmonitor.mobile.ui.chat.newsession.SheetHead

/**
 * 一格 ⇒ 一块（同桌面 `cellRows`）：开关 · 不带前半句的数：名字在左、控件在右；选项 · 带前半句或空值说法的数 · 分段：名字在上、控件在下。
 * 值 · 从哪来 · 字都是那台解好的；挂在别格下的（[indent]）缩进；总体那一格的说明写在名字下面；没收下 ⇒ 下面一行红字。
 */
@Composable
internal fun CellRows(
    ui: RotationUi,
    board: RotationBoard,
    cell: RotCell,
    row: Int?,
    indent: Boolean = false,
) {
    val id = cellId(cell, row)
    val set: (Any?) -> Unit = { next -> board.edit(RotEdit.set(cell.key, row, next), id) }
    val stacked = cell.kind == RotFormKind.CHOICE || cell.kind == RotFormKind.DAY_SEGMENTS || cell.lead != null || cell.nullable != null
    Column(
        Modifier.fillMaxWidth().padding(start = if (indent) INDENT else 0.dp).testTag("rot-cell-$id"),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        if (stacked) {
            CellLabel(cell)
            when (cell.kind) {
                RotFormKind.DAY_SEGMENTS -> Segments(board, cell, row, id)
                RotFormKind.CHOICE -> ChoiceCell(cell, cell.v as? String, set)
                else -> NumberCell(cell, set)
            }
        } else {
            Row(Modifier.fillMaxWidth().heightIn(min = 48.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                Column(Modifier.weight(1f)) {
                    CellLabel(cell)
                    // 总体那几格的说明写在旁边（同桌面）；号那一行的长按看。
                    if (row == null) cell.help?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
                if (cell.kind == RotFormKind.TOGGLE) Switch(checked = cell.v == true, onCheckedChange = { set(it) }) else NumberCell(cell, set)
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

private fun RotCell.unit(): String = if (kind == RotFormKind.MINUTES) copyText("rot.unit.minutes") else copyText("rot.unit.percent")

/** 格名：只写 `label`；长按升起一个小浮层写 `help`（手机没有悬停）。跟总体 ⇒ 淡。 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun CellLabel(cell: RotCell) {
    var help by remember { mutableStateOf(false) }
    val helpText = cell.help.orEmpty()
    Box {
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

/** 数格（百分数 · 分钟）：前面那半句（声明给的）· 数 · 可空的再带「或 ［空值说法］」（按下 ＝ 空着，数画虚线）。 */
@Composable
private fun NumberCell(
    cell: RotCell,
    set: (Any?) -> Unit,
) {
    val num = cell.num ?: 0.0
    val isNull = cell.nullable != null && cell.v == null
    FlowRow(verticalArrangement = Arrangement.Center, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        cell.lead?.let { Text(it, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.align(Alignment.CenterVertically)) }
        NumberBox(num, cell.unit(), faint = isNull || cell.inherited, 96.dp, cell.label, Modifier.align(Alignment.CenterVertically)) { set(it) }
        cell.nullable?.let { n ->
            Text(copyText("rot.cell.or"), style = MaterialTheme.typography.bodyMedium, modifier = Modifier.align(Alignment.CenterVertically))
            val ink = if (isNull) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant
            OutlinedButton(onClick = { set(if (isNull) num else null) }, contentPadding = SMALL_PAD, modifier = Modifier.align(Alignment.CenterVertically)) {
                Text(n, color = ink, style = MaterialTheme.typography.bodySmall)
            }
        }
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
    modifier: Modifier = Modifier,
    onSet: (Double) -> Unit,
) {
    var text by remember(value) { mutableStateOf(numText(value)) }
    InputBox(text, { text = it }, faint, width, label, KeyboardType.Number, unit, modifier) {
        text
            .trim()
            .toDoubleOrNull()
            ?.takeIf { it != value }
            ?.let(onSet) ?: run { text = numText(value) }
    }
}

/** 一格输入框：失焦 · 完成 ⇒ [commit]。 */
@Composable
private fun InputBox(
    text: String,
    onText: (String) -> Unit,
    faint: Boolean,
    width: Dp,
    label: String,
    keyboard: KeyboardType,
    unit: String?,
    modifier: Modifier = Modifier,
    placeholder: String? = null,
    commit: () -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    val focus = LocalFocusManager.current
    val ink = if (faint) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface
    val line = if (focused) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.outline
    Row(
        modifier
            .width(width)
            .height(40.dp)
            .dashedOr(faint, line)
            .padding(horizontal = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.weight(1f), contentAlignment = Alignment.CenterEnd) {
            if (text.isEmpty() && placeholder != null) Text(placeholder, color = MaterialTheme.colorScheme.onSurfaceVariant, style = MaterialTheme.typography.bodyMedium)
            BasicTextField(
                text,
                onText,
                singleLine = true,
                textStyle = TextStyle(color = ink, textAlign = TextAlign.End, fontSize = MaterialTheme.typography.bodyMedium.fontSize),
                cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                keyboardOptions = KeyboardOptions(keyboardType = keyboard, imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { focus.clearFocus() }),
                modifier =
                    Modifier.fillMaxWidth().testTag("rot-num-$label").semantics { contentDescription = label }.onFocusChanged {
                        if (focused && !it.isFocused) commit()
                        focused = it.isFocused
                    },
            )
        }
        unit?.let { Text(" $it", color = ink, style = MaterialTheme.typography.bodyMedium) }
    }
}

/** 虚线（跟总体 · 空着）或实线的框。 */
@Composable
private fun Modifier.dashedOr(
    dashed: Boolean,
    line: Color,
): Modifier =
    if (!dashed) {
        border(1.dp, line, RoundedCornerShape(6.dp))
    } else {
        drawBehind {
            drawRoundRect(line, style = Stroke(1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 4f))), cornerRadius = CornerRadius(6.dp.toPx()))
        }
    }

/** 选项：三项以内分段钮（满宽）；更多 ⇒ 底部抽屉。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ChoiceCell(
    cell: RotCell,
    cur: String?,
    set: (Any?) -> Unit,
) {
    val choices = cell.choices.orEmpty()
    if (choices.size <= SEGMENTS) {
        SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
            choices.forEachIndexed { i, c ->
                SegmentedButton(
                    selected = c.v == cur,
                    onClick = { set(c.v) },
                    shape = SegmentedButtonDefaults.itemShape(i, choices.size),
                    icon = {},
                    modifier = Modifier.testTag("rot-choice-${cell.key}-${c.v}"),
                ) { Text(c.label, maxLines = 1) }
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

// ─────────────────────────────── 分段

/**
 * 分段一窗（同桌面 `segments`）：后面那窗先是［同第一窗 · 另设］；自己设的 ⇒ 上面一条只画的 24 小时条 ＋ 一段一行（从 · 到 · 上限 · ✕）＋「＋ 加一段」。
 * 改哪一格都交整段（`seg`，带第几段）；新加的那一行三格都填好才交。
 */
@Composable
private fun Segments(
    board: RotationBoard,
    cell: RotCell,
    row: Int?,
    id: String,
) {
    val win = cell.window.orEmpty()
    cell.same?.let { same ->
        val on = cell.from == RotFrom.SAME
        SingleChoiceSegmentedButtonRow {
            listOf(true to same, false to copyText("rot.cell.own")).forEachIndexed { i, (v, label) ->
                SegmentedButton(selected = on == v, onClick = { if (on != v) board.edit(RotEdit.same(cell.key, row, win, v), id) }, shape = SegmentedButtonDefaults.itemShape(i, 2), icon = {}) {
                    Text(label)
                }
            }
        }
    }
    if (!segsShown(cell)) return
    val segs = cell.segs.orEmpty()
    DayBar(segs, cell.inherited, "rot-day-$id")
    var adding by remember(segs) { mutableIntStateOf(0) }
    segs.forEachIndexed { i, g -> SegLine(cell, g, i, onEdit = { e -> board.edit(e, id) }, onDel = { board.edit(RotEdit.seg(cell.key, row, win, i, null), id) }, row = row) }
    repeat(adding) { SegLine(cell, null, null, onEdit = { e -> board.edit(e, id) }, onDel = { adding-- }, row = row) }
    TextButton(onClick = { adding++ }, modifier = Modifier.testTag("rot-seg-add-$id")) {
        PhIcon(Ph.Plus, contentDescription = null, size = Touch.iconInline)
        Text(" " + copyText("rot.seg.add"))
    }
}

/** 一段一行：从几点 · – · 到几点 · 上限 · ✕。给了 [g] ⇒ 第 [at] 段；没给 ⇒ 新加的那一行。 */
@Composable
private fun SegLine(
    cell: RotCell,
    g: RotDaySeg?,
    at: Int?,
    onEdit: (Map<String, Any?>) -> Unit,
    onDel: () -> Unit,
    row: Int?,
) {
    var from by remember(g) { mutableStateOf(g?.let { dayClock(it.from) }.orEmpty()) }
    var to by remember(g) { mutableStateOf(g?.let { dayClock(it.to) }.orEmpty()) }
    var v by remember(g) { mutableStateOf(g?.let { numText(it.v) }.orEmpty()) }
    val commit = {
        val e = segEdit(cell, row, at, from, to, v)
        if (e != null && (g == null || minutesOf(from) != g.from || minutesOf(to) != g.to || v.trim().toDoubleOrNull() != g.v)) onEdit(e)
    }
    val hint = copyText("rot.seg.placeholder")
    Row(Modifier.fillMaxWidth().testTag("rot-seg-${at ?: "new"}"), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
        InputBox(from, { from = it }, false, 72.dp, copyText("rot.seg.from"), KeyboardType.Number, null, placeholder = hint, commit = commit)
        Text(copyText("rot.seg.dash"))
        InputBox(to, { to = it }, false, 72.dp, copyText("rot.seg.to"), KeyboardType.Number, null, placeholder = hint, commit = commit)
        InputBox(v, { v = it }, false, 80.dp, copyText("rot.seg.v"), KeyboardType.Number, copyText("rot.unit.percent"), commit = commit)
        Box(Modifier.weight(1f))
        IconButton(onClick = onDel, modifier = Modifier.semantics { contentDescription = copyText("rot.seg.del") }) {
            PhIcon(Ph.X, contentDescription = null, size = Touch.iconInline, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/** 分段那一条（24 小时）：只画，段与字照回包；0% 的段画成出错底色。 */
@Composable
private fun DayBar(
    segs: List<RotDaySeg>,
    follows: Boolean,
    tag: String,
) {
    val segColor = MaterialTheme.colorScheme.surfaceContainerHighest
    val zero = MaterialTheme.colorScheme.errorContainer
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        BoxWithConstraints(
            Modifier
                .fillMaxWidth()
                .height(28.dp)
                .dashedOr(follows, MaterialTheme.colorScheme.outline)
                .testTag(tag),
        ) {
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
        }
        DayAxis()
    }
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

private const val SEGMENTS = 3
private val INDENT = 16.dp
private val SMALL_PAD =
    androidx.compose.foundation.layout
        .PaddingValues(horizontal = 10.dp, vertical = 4.dp)
