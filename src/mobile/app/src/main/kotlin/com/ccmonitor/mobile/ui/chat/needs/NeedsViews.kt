package com.ccmonitor.mobile.ui.chat.needs

import androidx.compose.foundation.BorderStroke
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoBody
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.core.wire.gen.NeedsNote
import com.ccmonitor.mobile.core.wire.gen.NeedsQuestion
import com.ccmonitor.mobile.ui.common.CopyDetailButton
import kotlinx.coroutines.delay

/** 供冒烟与 UI 测试定位的 tag（与字解耦）。 */
object NeedsTags {
    const val BAR = "needs-bar"
    const val ANSWER = "needs-answer"
    const val ANSWERED = "needs-answered"
    const val SHEET = "needs-sheet"
    const val DETAIL = "needs-detail"
    const val DETAIL_MORE = "needs-detail-more"
    const val GONE = "needs-gone"
    const val NOTE = "needs-note"
    const val OTHER = "needs-other"
    const val OTHER_INPUT = "needs-other-input"
    const val CLOSE = "needs-close"

    /** 抽屉头上那颗［关闭］（每一态都有；走 dismiss，不算答）。 */
    const val HIDE = "needs-hide"

    /** 头一行底下会滚的那一截。 */
    const val BODY = "needs-body"

    fun choice(id: String) = "needs-choice-$id"

    fun option(id: String) = "needs-option-$id"
}

/** 可点的那一整行至少这么高（适配层触控尺寸：按钮整行 48 dp）。 */
private val TOUCH = 48.dp
private val BAR_SHAPE = RoundedCornerShape(10.dp)
private val ROW_SHAPE = RoundedCornerShape(8.dp)

/** 选项行上下留白（说明折两行时不贴框）。 */
private val OPTION_PAD = 8.dp

/** 本机单调钟，按秒走（「已等」那一格每秒重画，不重问那台）。 */
@Composable
private fun rememberTick(now: () -> Long): Long {
    var t by remember { mutableLongStateOf(now()) }
    LaunchedEffect(Unit) {
        while (true) {
            delay(TICK_MS)
            t = now()
        }
    }
    return t
}

private const val TICK_MS = 1_000L

/**
 * 钉条（乙1）：输入框上方满宽，琥珀边 ＋ 淡底、琥珀点；第一行那台写的 `head` ＋ 已等，第二行等宽的那一段（答不了的那种是那台那一句）；
 * 能答 ⇒ 右侧［答］主按钮升抽屉。永不打断：不闪、不弹、不抢焦点。
 */
@Composable
fun NeedsBarView(
    bar: NeedsBar,
    now: () -> Long,
    onAnswer: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val tick = rememberTick(now)
    val amber = LocalAppTokens.current.warn
    Row(
        modifier =
            modifier
                .fillMaxWidth()
                .padding(horizontal = 8.dp, vertical = 4.dp)
                .border(BorderStroke(1.dp, amber), BAR_SHAPE)
                .background(amber.copy(alpha = AMBER_WASH), BAR_SHAPE)
                .padding(start = 12.dp, end = 8.dp, top = 6.dp, bottom = 6.dp)
                .testTag(NeedsTags.BAR),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.size(8.dp).background(amber, CircleShape))
        Spacer(Modifier.width(10.dp))
        Column(Modifier.weight(1f)) {
            Text(bar.firstLine(tick), style = MaterialTheme.typography.bodyMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
            bar.line2?.let {
                Text(
                    it,
                    style = if (bar.code) monoSmall else MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
        if (bar.canAnswer) {
            Button(
                onClick = onAnswer,
                modifier = Modifier.heightIn(min = TOUCH).testTag(NeedsTags.ANSWER),
                shape = ROW_SHAPE,
            ) { Text(copyText("mobNeeds.act.answer")) }
        }
    }
}

/** 替人答过的那一行（钉条那一格）：灰字一行，不是在等 ⇒ 不用琥珀、没有按钮。 */
@Composable
fun NeedsAnsweredView(
    said: String,
    modifier: Modifier = Modifier,
) {
    Text(
        said,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier =
            modifier
                .fillMaxWidth()
                .padding(horizontal = 20.dp, vertical = 6.dp)
                .testTag(NeedsTags.ANSWERED),
    )
}

/** 琥珀淡底（琥珀边 ＋ 10% 底）。 */
private const val AMBER_WASH = 0.10f

/** 抽屉里能做的几件事（都交给 [NeedsDesk]）。 */
class AskActions(
    val pick: (String) -> Unit,
    val sendNote: (String) -> Unit,
    val cancelNote: () -> Unit,
    val toggle: (String) -> Unit,
    val other: (String) -> Unit,
    val next: () -> Unit,
    val dismiss: () -> Unit,
)

/**
 * 答题抽屉（乙2–乙5 · 别处先答了）：底部抽屉，抽屉外点一下 · 下滑收起、不算答。
 * 头一行那台写的 `head`（提问 ＝ 第几题）＋ 子 agent 标 ＋ 已等；批准 · 批计划的全文进等宽块（可滚）；选项照那台的次序分段（[choiceGroups]）。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AskSheetView(
    sheet: AskSheet,
    now: () -> Long,
    act: AskActions,
) {
    val state = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    ModalBottomSheet(
        onDismissRequest = act.dismiss,
        sheetState = state,
        shape = RoundedCornerShape(topStart = 10.dp, topEnd = 10.dp),
        containerColor = MaterialTheme.colorScheme.surfaceContainer,
        modifier = Modifier.testTag(NeedsTags.SHEET),
    ) {
        // 抽屉最高到屏高的 [SHEET_MAX]：上面总留一截点一下就收起；头一行钉住不随内容滚（［关闭］一直够得着）。
        Column(
            Modifier
                .fillMaxWidth()
                .heightIn(max = LocalConfiguration.current.screenHeightDp.dp * SHEET_MAX)
                .padding(horizontal = 16.dp),
        ) {
            SheetHead(sheet, rememberTick(now), act.dismiss)
            SheetBody(sheet, act, Modifier.weight(1f, fill = false))
        }
    }
}

/** 抽屉最高占屏高的这么多。 */
private const val SHEET_MAX = 0.75f

/** 头一行底下那一截（会滚）：别处先答了那一句 · 全文或题 · 没答成 · 选项。 */
@Composable
private fun SheetBody(
    sheet: AskSheet,
    act: AskActions,
    modifier: Modifier,
) {
    Column(
        modifier
            .fillMaxWidth()
            .verticalScroll(rememberScrollState())
            .padding(top = 8.dp, bottom = 24.dp)
            .testTag(NeedsTags.BODY),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        sheet.gone?.let { GoneLine(it) }
        val q = sheet.question
        if (q == null) {
            if (sheet.ask.detail.isNotEmpty()) DetailBlock(sheet.ask.detail, dim = sheet.gone != null)
        } else {
            QuestionPage(q, sheet, act)
        }
        sheet.failed?.let {
            Text(it.text, color = LocalAppTokens.current.errorText, style = MaterialTheme.typography.bodyMedium)
            CopyDetailButton(it.text, it.detail)
        }
        when {
            sheet.gone != null -> GoneChoices(sheet, act)
            sheet.noteFor != null -> NoteBox(sheet, act)
            else -> Choices(sheet, act)
        }
    }
}

@Composable
private fun SheetHead(
    sheet: AskSheet,
    tick: Long,
    onHide: () -> Unit,
) {
    val title =
        if (sheet.ask.questions.isEmpty()) {
            sheet.head
        } else {
            copyText(
                "mobAsk.head.page",
                "head" to sheet.head,
                "i" to (sheet.page + 1).toString(),
                "n" to
                    sheet.ask.questions.size
                        .toString(),
            )
        }
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Text(title, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        sheet.ask.run?.let { run ->
            Spacer(Modifier.width(8.dp))
            Text(
                run.text,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                style = MaterialTheme.typography.labelSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier =
                    Modifier
                        .weight(1f, fill = false)
                        .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(4.dp))
                        .padding(horizontal = 6.dp, vertical = 2.dp),
            )
        }
        Spacer(Modifier.weight(1f))
        if (sheet.gone == null) {
            sheet.waited(tick)?.let {
                Text(copyText("askBlock.head.since", "waited" to it), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        // 每一态都有的［关闭］（同桌面抽屉右上角那颗）：收起、不算答。
        PhIconButton(
            Ph.X,
            contentDescription = copyText("kit.drawer.close"),
            onClick = onHide,
            modifier = Modifier.testTag(NeedsTags.HIDE),
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

/** 别处先答了那一行（那台那一句，或同桌面那一句）。 */
@Composable
private fun GoneLine(said: String) {
    Text(
        said,
        style = MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.testTag(NeedsTags.GONE),
    )
}

/**
 * 要批的全文：等宽、不截字。排出来超过 [DETAIL_LINES] 行先收着（只显前几行），下面一颗「展开 N 行」，点了全显、不再收（同桌面 Bash 卡）；
 * 块里不自己滚，全交给抽屉那一层滚（不滚中滚）。
 */
@Composable
private fun DetailBlock(
    text: String,
    dim: Boolean,
) {
    var lines by remember(text) { mutableIntStateOf(0) }
    var cut by remember(text) { mutableFloatStateOf(0f) }
    var open by rememberSaveable(text) { mutableStateOf(false) }
    val fold = detailFold(lines, open)
    val clip = if (fold != null && cut > 0f) Modifier.height(with(LocalDensity.current) { cut.toDp() }).clipToBounds() else Modifier
    Box(
        Modifier
            .fillMaxWidth()
            .background(MaterialTheme.colorScheme.surface, ROW_SHAPE)
            .padding(12.dp)
            .testTag(NeedsTags.DETAIL),
    ) {
        Text(
            text,
            style = monoBody,
            color = if (dim) MaterialTheme.colorScheme.onSurfaceVariant else MaterialTheme.colorScheme.onSurface,
            modifier = clip,
            onTextLayout = { r ->
                lines = r.lineCount
                cut = if (r.lineCount > DETAIL_LINES) r.getLineBottom(DETAIL_LINES - 1) else 0f
            },
        )
    }
    fold?.let { n ->
        SecondaryButton(onClick = { open = true }, modifier = Modifier.heightIn(min = TOUCH).testTag(NeedsTags.DETAIL_MORE)) {
            Text(detailExpandLabel(n))
        }
    }
}

/** 要批的全文收着时显几行（按排出来的行算，长命令折行也算）。 */
internal const val DETAIL_LINES = 8

/** 选项说明：没选中的折两行，选中的全显（点了就看得全）。 */
internal const val OPTION_DETAIL_LINES = 2

internal fun optionDetailLines(on: Boolean): Int = if (on) Int.MAX_VALUE else OPTION_DETAIL_LINES

/** 全文排出来 [lines] 行：超过 [DETAIL_LINES] 又没点开 ⇒ 要画「展开 N 行」的 N；不然 `null`（全显）。 */
internal fun detailFold(
    lines: Int,
    open: Boolean,
): Int? = lines.takeIf { !open && it > DETAIL_LINES }

/** 「展开 N 行」：同桌面 Bash 输出那一句（一件事一个说法）。 */
internal fun detailExpandLabel(lines: Int): String = copyText("bash.output.showAll", "n" to lines)

/** 选项那几段：段与段之间多空一截。 */
@Composable
private fun Choices(
    sheet: AskSheet,
    act: AskActions,
) {
    val groups = choiceGroups(sheet.ask)
    groups.forEachIndexed { i, g ->
        if (i > 0) Spacer(Modifier.size(8.dp))
        for (row in g) {
            val pager = sheet.ask.questions.isNotEmpty() && row.look == ChoiceLook.PRIMARY
            if (pager && !sheet.last) {
                // 非末题：主按钮是「下一题」（这一题答了才亮），末题才是那台的「提交答案」。
                ChoiceButton(copyText("mobAsk.act.next"), ChoiceLook.PRIMARY, enabled = sheet.canNext, busy = false, tag = NeedsTags.choice("next"), onClick = act.next)
            } else {
                val enabled = !sheet.locked && (!pager || sheet.allAnswered)
                ChoiceButton(row.label, row.look, enabled = enabled, busy = sheet.sending == row.choice.id, tag = NeedsTags.choice(row.choice.id)) { act.pick(row.choice.id) }
            }
        }
    }
}

/** 别处先答了：按钮照原样全灰，只留［关闭］。 */
@Composable
private fun GoneChoices(
    sheet: AskSheet,
    act: AskActions,
) {
    if (sheet.ask.questions.isEmpty()) {
        for (row in choiceGroups(sheet.ask).flatten().filter { it.look != ChoiceLook.QUIET }) {
            ChoiceButton(row.label, row.look, enabled = false, busy = false, tag = NeedsTags.choice(row.choice.id)) {}
        }
    }
    OutlinedButton(
        onClick = act.dismiss,
        modifier = Modifier.fillMaxWidth().heightIn(min = TOUCH).testTag(NeedsTags.CLOSE),
        shape = ROW_SHAPE,
    ) { Text(copyText("kit.drawer.close")) }
}

/** 要附话的那一项：抽屉里就地换成一格输入框 ＋ 那一项的主按钮 ＋［取消］（不借聊天输入框）。 */
@Composable
private fun NoteBox(
    sheet: AskSheet,
    act: AskActions,
) {
    val c = sheet.ask.choices.firstOrNull { it.id == sheet.noteFor } ?: return
    var text by rememberSaveable(c.id) { mutableStateOf("") }
    OutlinedTextField(
        value = text,
        onValueChange = { text = it },
        modifier = Modifier.fillMaxWidth().testTag(NeedsTags.NOTE),
        label = { Text(c.text) },
        keyboardOptions = KeyboardOptions(imeAction = ImeAction.Send),
        enabled = !sheet.locked,
    )
    val required = c.note == NeedsNote.REQUIRED
    ChoiceButton(c.text, ChoiceLook.PRIMARY, enabled = !sheet.locked && (!required || text.isNotBlank()), busy = sheet.sending == c.id, tag = NeedsTags.choice(c.id)) {
        act.sendNote(text)
    }
    ChoiceButton(copyText("kit.dialog.cancel"), ChoiceLook.QUIET, enabled = !sheet.locked, busy = false, tag = NeedsTags.choice("note-back"), onClick = act.cancelNote)
}

/** 一题一屏：题头（header）· 题目 · 选项（单选圆点 / 多选方块，选中淡底 ＋ 强调色描边）· 末尾「其他…」。 */
@Composable
private fun QuestionPage(
    q: NeedsQuestion,
    sheet: AskSheet,
    act: AskActions,
) {
    val head = if (q.multi) q.header?.let { copyText("askBlock.question.multi", "header" to it) } ?: copyText("askBlock.question.multiBare") else q.header
    head?.let { Text(it, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    Text(q.text, style = MaterialTheme.typography.titleSmall, fontWeight = FontWeight.SemiBold)
    val pick = sheet.picks[q.id]
    for (o in q.options) {
        val on = pick?.options?.contains(o.id) == true
        OptionRow(on = on, multi = q.multi, enabled = !sheet.locked, tag = NeedsTags.option(o.id), onClick = { act.toggle(o.id) }, top = o.detail != null) {
            // 说明在选项字下面、占满整宽（并排会被挤成窄列）；没选中的折两行，选中的全显。
            Column(Modifier.weight(1f)) {
                Text(o.text, style = MaterialTheme.typography.bodyLarge)
                o.detail?.let {
                    Text(
                        it,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = optionDetailLines(on),
                        overflow = TextOverflow.Ellipsis,
                    )
                }
            }
        }
    }
    val other = pick?.text.orEmpty()
    OptionRow(on = other.isNotBlank(), multi = q.multi, enabled = !sheet.locked, tag = NeedsTags.OTHER, onClick = {}) {
        // 每一题一格，换题时跟着换（按题 id 记）。输入框不另画框：选项行那一圈框就是它的框。
        val muted = MaterialTheme.colorScheme.onSurfaceVariant
        BasicTextField(
            value = other,
            onValueChange = act.other,
            singleLine = true,
            enabled = !sheet.locked,
            textStyle = MaterialTheme.typography.bodyLarge.copy(color = MaterialTheme.colorScheme.onSurface),
            cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
            modifier = Modifier.weight(1f).testTag(NeedsTags.OTHER_INPUT),
            decorationBox = { field ->
                Box(contentAlignment = Alignment.CenterStart) {
                    if (other.isEmpty()) Text(copyText("askBlock.question.other"), style = MaterialTheme.typography.bodyLarge, color = muted)
                    field()
                }
            },
        )
    }
}

@Composable
private fun OptionRow(
    on: Boolean,
    multi: Boolean,
    enabled: Boolean,
    tag: String,
    onClick: () -> Unit,
    /** 底下还有说明 ⇒ 圆点 / 方块贴首行（不居中在几行字中间）。 */
    top: Boolean = false,
    content: @Composable androidx.compose.foundation.layout.RowScope.() -> Unit,
) {
    val accent = MaterialTheme.colorScheme.primary
    val tokens = LocalAppTokens.current
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = TOUCH)
            .border(1.dp, if (on) accent else MaterialTheme.colorScheme.outline, ROW_SHAPE)
            .background(if (on) tokens.selectedBg else MaterialTheme.colorScheme.surfaceContainer, ROW_SHAPE)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = 8.dp, vertical = OPTION_PAD)
            .testTag(tag),
        verticalAlignment = if (top) Alignment.Top else Alignment.CenterVertically,
    ) {
        val mark =
            when {
                multi && on -> Ph.CheckSquare
                multi -> Ph.Square
                on -> Ph.RadioButton
                else -> Ph.Circle
            }
        PhIcon(mark, contentDescription = null, tint = if (on) accent else MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.width(8.dp))
        content()
    }
}

/** 一颗满宽、≥ 48 dp 的选项按钮；送出中那一颗原位转圈。 */
@Composable
private fun ChoiceButton(
    label: String,
    look: ChoiceLook,
    enabled: Boolean,
    busy: Boolean,
    tag: String,
    onClick: () -> Unit,
) {
    val m = Modifier.fillMaxWidth().heightIn(min = TOUCH).testTag(tag)
    val inner: @Composable () -> Unit = {
        if (busy) {
            CircularProgressIndicator(Modifier.size(18.dp), strokeWidth = 2.dp)
        } else {
            Text(label, textAlign = if (look == ChoiceLook.PRIMARY || look == ChoiceLook.QUIET) TextAlign.Center else TextAlign.Start, modifier = Modifier.fillMaxWidth())
        }
    }
    when (look) {
        ChoiceLook.PRIMARY -> Button(onClick = onClick, enabled = enabled || busy, modifier = m, shape = ROW_SHAPE) { inner() }
        ChoiceLook.OUTLINE ->
            OutlinedButton(onClick = onClick, enabled = enabled || busy, modifier = m, shape = ROW_SHAPE) { inner() }
        ChoiceLook.DANGER ->
            OutlinedButton(
                onClick = onClick,
                enabled = enabled || busy,
                modifier = m,
                shape = ROW_SHAPE,
                colors = ButtonDefaults.outlinedButtonColors(contentColor = LocalAppTokens.current.errorText),
            ) { inner() }
        ChoiceLook.QUIET -> SecondaryButton(onClick = onClick, modifier = m, enabled = enabled || busy) { inner() }
    }
}
