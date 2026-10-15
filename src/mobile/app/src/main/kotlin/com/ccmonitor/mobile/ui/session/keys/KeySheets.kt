package com.ccmonitor.mobile.ui.session.keys

import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.AppAction
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.KeyPress
import com.ccmonitor.mobile.core.data.db.TermKey
import com.ccmonitor.mobile.core.data.db.appActionName
import com.ccmonitor.mobile.core.data.db.cap
import com.ccmonitor.mobile.core.data.db.modName
import com.ccmonitor.mobile.core.data.db.specialName
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.icon.IconTextButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch

/** 抽屉外框：`--card` 底，标题一行（主标题 ＋ 一小段灰字；[title] 为 null ⇒ 不画这一行），内容随后。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun KeysSheet(
    title: String?,
    sub: String?,
    onDismiss: () -> Unit,
    trailing: @Composable () -> Unit = {},
    content: @Composable () -> Unit,
) {
    val tokens = LocalAppTokens.current
    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true),
        containerColor = tokens.card,
        scrimColor = MaterialTheme.colorScheme.scrim,
        tonalElevation = 0.dp,
    ) {
        Column(Modifier.fillMaxWidth().padding(bottom = tokens.spacing.lg)) {
            if (title != null) {
                Row(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.sm), verticalAlignment = Alignment.CenterVertically) {
                    Text(title, style = MaterialTheme.typography.titleMedium)
                    sub?.let {
                        Spacer(Modifier.width(tokens.spacing.xs))
                        HintLine(it)
                    }
                    Spacer(Modifier.weight(1f))
                    trailing()
                }
            }
            content()
        }
    }
}

/** 抽屉里的一行：图标 ＋ 字 ＋ 行尾灰字（可空）＋ ›（有下一层时）。 */
@Composable
fun SheetRow(
    icon: Ph?,
    text: String,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
    trailing: String? = null,
    more: Boolean = false,
    checked: Boolean = false,
    enabled: Boolean = true,
) {
    val tokens = LocalAppTokens.current
    val fg = if (enabled) MaterialTheme.colorScheme.onSurface else tokens.textFaint
    Row(
        modifier
            .fillMaxWidth()
            .heightIn(min = Touch.rowSingle)
            .clickable(enabled = enabled, onClick = onClick)
            .padding(horizontal = tokens.spacing.lg),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.md),
    ) {
        icon?.let { PhIcon(it, contentDescription = null, tint = if (enabled) MaterialTheme.colorScheme.onSurfaceVariant else tokens.textFaint) }
        Text(text, style = MaterialTheme.typography.bodyMedium, color = fg, modifier = Modifier.weight(1f))
        trailing?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        if (checked) PhIcon(Ph.Check, contentDescription = null, size = Touch.iconInline, tint = tokens.accent)
        if (more) PhIcon(Ph.CaretRight, contentDescription = null, size = Touch.iconInline, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** App 动作的图标。 */
internal fun actionIcon(a: AppAction): Ph =
    when (a) {
        AppAction.NEXT_GROUP -> Ph.ArrowsLeftRight
        AppAction.GOTO_GROUP -> Ph.Rows
        AppAction.EDIT_KEYS -> Ph.PencilSimple
        AppAction.PASTE -> Ph.Clipboard
        AppAction.SHOW_TABS -> Ph.Tabs
        AppAction.COMMAND_PALETTE -> Ph.Command
        AppAction.TMUX_SESSIONS -> Ph.TerminalWindow
        AppAction.SHOW_KEYBOARD -> Ph.Keyboard
        AppAction.CAPTURE -> Ph.Selection
        AppAction.PUSH_KEY -> Ph.Key
    }

/** 选 App 动作：一列动作；「切到某一组 ›」进第二层列出全部组（存的是组的编号）。要 tmux 的那几项行尾写「需 tmux」。 */
@Composable
internal fun ActionPickSheet(
    trigger: String,
    groups: List<KeyGroup>,
    onPick: (KeyBinding.App) -> Unit,
    onDismiss: () -> Unit,
) {
    var pickGroup by remember { mutableStateOf(false) }
    KeysSheet(if (pickGroup) appActionName(AppAction.GOTO_GROUP) else copyText("mobKeys.bind.action"), trigger, onDismiss) {
        Column(Modifier.verticalScroll(rememberScrollState())) {
            if (pickGroup) {
                groups.forEach { g -> SheetRow(Ph.Rows, g.name, onClick = { onPick(KeyBinding.App(AppAction.GOTO_GROUP, g.id)) }) }
            } else {
                AppAction.entries.forEachIndexed { i, a ->
                    if (i > 0) HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                    SheetRow(
                        actionIcon(a),
                        appActionName(a),
                        onClick = { if (a == AppAction.GOTO_GROUP) pickGroup = true else onPick(KeyBinding.App(a)) },
                        trailing = if (a in AppAction.NEEDS_TMUX) copyText("mobKeys.action.needTmux") else null,
                        more = a == AppAction.GOTO_GROUP,
                        enabled = a != AppAction.GOTO_GROUP || groups.isNotEmpty(),
                    )
                }
            }
        }
    }
}

/** 选一组（所在组）。 */
@Composable
internal fun GroupPickSheet(
    groups: List<KeyGroup>,
    current: String,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    KeysSheet(copyText("mobKeys.edit.group"), null, onDismiss) {
        Column(Modifier.verticalScroll(rememberScrollState())) {
            groups.forEach { g -> SheetRow(null, g.name, onClick = { onPick(g.id) }, checked = g.id == current) }
        }
    }
}

/** 选一颗修饰键（编辑页「＋ 修饰键」）。 */
@Composable
internal fun ModifierPickSheet(
    onPick: (KeyMod) -> Unit,
    onDismiss: () -> Unit,
) {
    KeysSheet(copyText("mobKeys.add.modifier"), null, onDismiss) {
        KeyMod.entries.forEach { m -> SheetRow(Ph.Keyboard, modName(m), onClick = { onPick(m) }) }
    }
}

/**
 * 录入抽屉：显示框 · 修饰键一排 · 特殊键一排（F1–F12 收在最后一颗里）· 一行灰字 · 软键盘。
 * 软键盘的输入经一个看不见的框收进来：框里恒留一个占位字，删掉它 ＝ 软键盘退格（删最后一下），多出来的字 ＝ 打进来的。
 */
@Composable
internal fun RecorderSheet(
    trigger: String,
    initial: List<KeyPress>,
    onDone: (List<KeyPress>) -> Unit,
    onDismiss: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    var rec by remember { mutableStateOf(Recorder(presses = initial)) }
    val sentinel = " "
    var field by remember { mutableStateOf(TextFieldValue(sentinel, TextRange(sentinel.length))) }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    KeysSheet(
        copyText("mobKeys.rec.title"),
        trigger,
        onDismiss,
        trailing = {
            Row(verticalAlignment = Alignment.CenterVertically) {
                IconTextButton(Ph.ArrowCounterClockwise, copyText("mobKeys.rec.undo"), onClick = { rec = rec.undo() }, enabled = rec.presses.isNotEmpty())
                SecondaryButton(onClick = { rec = rec.clear() }, enabled = rec.presses.isNotEmpty() || rec.armed.isNotEmpty()) { Text(copyText("mobKeys.rec.clear")) }
                Button(onClick = { onDone(rec.presses) }) { Text(copyText("mobKeys.rec.done")) }
            }
        },
    ) {
        Column(Modifier.padding(horizontal = tokens.spacing.lg).imePadding(), verticalArrangement = Arrangement.spacedBy(tokens.spacing.s)) {
            Box(
                Modifier
                    .fillMaxWidth()
                    .heightIn(min = Touch.rowSingle)
                    .border(1.dp, tokens.accent, RoundedCornerShape(tokens.radius.m))
                    .clickable { runCatching { focus.requestFocus() } }
                    .padding(horizontal = tokens.spacing.sm),
                contentAlignment = Alignment.CenterStart,
            ) {
                PressesLine(rec.presses)
                BasicTextField(
                    value = field,
                    onValueChange = { v ->
                        val t = v.text
                        rec =
                            when {
                                t.isEmpty() -> rec.undo()
                                t.startsWith(sentinel) && t.length > sentinel.length -> rec.type(t.substring(sentinel.length))
                                else -> rec
                            }
                        field = TextFieldValue(sentinel, TextRange(sentinel.length))
                    },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Ascii, autoCorrectEnabled = false),
                    textStyle = MaterialTheme.typography.bodySmall.copy(color = Color.Transparent),
                    cursorBrush = SolidColor(Color.Transparent),
                    modifier = Modifier.size(1.dp).focusRequester(focus),
                )
            }
            rec.note?.let { NoteLine(it.text, warn = it is RecNote.Same) }
            RecorderKeys(rec) { rec = it }
            HintLine(copyText("mobKeys.rec.ime"))
        }
    }
}

/** 录入抽屉里那两排（就是内置清单）：修饰键点亮后带给下一下；特殊键点一下录一颗，F1–F12 收在最后一颗里。 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun RecorderKeys(
    rec: Recorder,
    onRec: (Recorder) -> Unit,
) {
    val tokens = LocalAppTokens.current
    var showF by remember { mutableStateOf(false) }
    SectionLabel(copyText("mobKeys.rec.mods") + " · " + copyText("mobKeys.rec.modsTip"))
    FlowRow(horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
        KeyMod.entries.forEach { m -> MiniCap(modName(m), active = m in rec.armed, onClick = { onRec(rec.toggle(m)) }) }
    }
    SectionLabel(copyText("mobKeys.rec.specials"))
    FlowRow(horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs), verticalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
        RECORDER_SPECIALS.forEach { k -> MiniCap(specialName(k), onClick = { onRec(rec.special(k)) }) }
        MiniCap(F_RANGE, active = showF, onClick = { showF = !showF })
        if (showF) RECORDER_FKEYS.forEach { k -> MiniCap(specialName(k), onClick = { onRec(rec.special(k)) }) }
    }
}

private const val F_RANGE = "F1–F12"

/**
 * 时长抽屉：分段「跟系统 | 另设」（单颗那份是「跟总的 | 另设」）；跟着时写当前生效的数，另设时一个数字框（ms）。
 * 越界标红、写范围、「保存」灰掉，不悄悄夹回去。
 */
@Composable
internal fun DurationSheet(
    title: String,
    sub: String?,
    single: Boolean,
    own: Int?,
    following: Int,
    system: Int,
    range: IntRange,
    onSave: (Int?) -> Unit,
    onDismiss: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    var custom by remember { mutableStateOf(own != null) }
    var text by remember { mutableStateOf((own ?: following).toString()) }
    val value = text.toIntOrNull()
    val ok = !custom || (value != null && value in range)
    KeysSheet(title, sub, onDismiss) {
        Column(Modifier.padding(horizontal = tokens.spacing.lg), verticalArrangement = Arrangement.spacedBy(tokens.spacing.s)) {
            val follow = if (single) copyText("mobKeys.dur.followAll") else copyText("mobKeys.dur.followSys")
            Segments(listOf(false to follow, true to copyText("mobKeys.dur.custom")), custom, { custom = it })
            if (custom) {
                OutlinedTextField(
                    value = text,
                    onValueChange = { s -> text = s.filter { it.isDigit() }.take(MS_DIGITS) },
                    isError = !ok,
                    singleLine = true,
                    suffix = { Text(MS) },
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                    modifier = Modifier.fillMaxWidth(),
                )
                if (!ok) NoteLine(copyText("mobKeys.dur.range", "min" to range.first, "max" to range.last), warn = false)
            } else if (single) {
                HintLine("$following $MS")
            } else {
                HintLine(copyText("mobKeys.dur.sysNow", "ms" to system))
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs, Alignment.End)) {
                SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) }
                Button(onClick = { onSave(if (custom) value else null) }, enabled = ok) { Text(copyText("mobKeys.edit.save")) }
            }
        }
    }
}

private const val MS = "ms"
private const val MS_DIGITS = 5

/** 升级那一次没转过来的那几颗：键帽 · 当初填的原文（照写）· 哪里没转过来；［关闭］只关抽屉。 */
@Composable
internal fun UnconvertedSheet(
    keys: List<TermKey>,
    groupName: (String) -> String?,
    onDismiss: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    KeysSheet(copyText("mobKeys.migrate.title"), copyText("mobKeys.group.count", "n" to keys.size), onDismiss) {
        Column(Modifier.verticalScroll(rememberScrollState())) {
            keys.forEach { k ->
                k.unconverted.orEmpty().forEach { n ->
                    Row(
                        Modifier.fillMaxWidth().heightIn(min = Touch.rowDouble).padding(horizontal = tokens.spacing.lg),
                        verticalAlignment = Alignment.CenterVertically,
                        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.md),
                    ) {
                        MiniCap(k.cap(groupName))
                        Column(Modifier.weight(1f)) {
                            Text(n.original, style = MaterialTheme.typography.bodyMedium)
                            HintLine(issueLine(n))
                        }
                    }
                    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                }
            }
            Row(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.sm), horizontalArrangement = Arrangement.End) {
                SecondaryButton(onClick = onDismiss) { Text(copyText("mobKeys.migrate.ok")) }
            }
        }
    }
}
