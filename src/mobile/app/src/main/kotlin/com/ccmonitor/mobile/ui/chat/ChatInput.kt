package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilledIconButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButtonDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.OutlinedTextFieldDefaults
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.TextRange
import androidx.compose.ui.text.input.TextFieldValue
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.wire.CommandScope
import com.ccmonitor.mobile.core.claude.link.wire.NewSessionPick
import com.ccmonitor.mobile.core.claude.link.wire.SessionCommand
import com.ccmonitor.mobile.core.claude.link.wire.SessionCommands
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch

/** 输入那一圈要的几样动作（聊天屏往会话里做的事都经 [ChatSession]）。 */
data class ChatInputActions(
    val send: (String) -> Unit = {},
    val stop: () -> Unit = {},
    val switchModel: (NewSessionPick) -> Unit = {},
    val switchPermission: (NewSessionPick) -> Unit = {},
    val loadPicks: () -> Unit = {},
    val loadCommands: () -> Unit = {},
    val say: (String) -> Unit = {},
)

/**
 * 输入区（稿 `能用门槛` 第 4–6 节）：chip 上方那一行 · 输入框 · 下面一排［+］· 模型 · 权限档 ……… 发送 / 停止。
 * 新建会话（还没 sid）只有输入框与发送：草稿里的那几颗 chip 归新建会话那一屏。
 */
@Composable
internal fun ChatInputArea(
    state: ChatUiState,
    actions: ChatInputActions,
) {
    // `rememberSaveable`：转屏会重建 Activity，`remember` 的草稿会丢光。
    var draft by rememberSaveable(stateSaver = TextFieldValue.Saver) { mutableStateOf(TextFieldValue("")) }
    var sheet by remember { mutableStateOf<InputSheet?>(null) }
    val open = state.sid != null
    val busy = state.model.pending != null || state.permission.pending != null
    val tokens = LocalAppTokens.current
    // 输入区是骨架（`--bg-2`），输入框底 `--field-bg`、边 `--border-medium`、聚焦 `--accent`；➤ / ■ 是这一屏唯一的主按钮。
    Surface(color = tokens.bg2) {
        Column(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.sm, vertical = tokens.spacing.xs)) {
            state.note?.let { NoteLine(it) }
            OutlinedTextField(
                value = draft,
                onValueChange = { draft = it },
                modifier = Modifier.fillMaxWidth().testTag(ChatTestTags.INPUT),
                placeholder = { Text(copyText("chat.input.placeholder"), color = MaterialTheme.colorScheme.onSurfaceVariant) },
                singleLine = false,
                maxLines = INPUT_MAX_LINES,
                textStyle = MaterialTheme.typography.bodyLarge,
                shape = RoundedCornerShape(tokens.radius.l),
                colors =
                    OutlinedTextFieldDefaults.colors(
                        focusedContainerColor = tokens.fieldBg,
                        unfocusedContainerColor = tokens.fieldBg,
                        focusedBorderColor = tokens.accent,
                        cursorColor = tokens.accent,
                    ),
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
                horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
            ) {
                if (open) {
                    val plus = copyText("mobPlus.sheet.title")
                    TextButton(
                        onClick = {
                            actions.loadCommands()
                            sheet = InputSheet.Plus
                        },
                        modifier = Modifier.semantics { contentDescription = plus }.testTag(ChatTestTags.PLUS),
                    ) { PhIcon(Ph.Plus, contentDescription = null) }
                    SwitchChipView(state.model, copyText("session.model.title"), enabled = !busy, tag = ChatTestTags.MODEL_CHIP) {
                        actions.loadPicks()
                        sheet = InputSheet.Models
                    }
                    SwitchChipView(state.permission, copyText("session.permission.title"), enabled = !busy, tag = ChatTestTags.PERMISSION_MODE) {
                        actions.loadPicks()
                        sheet = InputSheet.Modes
                    }
                }
                Spacer(Modifier.weight(1f))
                SendOrStop(state.running, draft.text, actions) { draft = TextFieldValue("") }
            }
        }
    }
    InputSheets(sheet, state, actions, { sheet = it }) { text -> draft = TextFieldValue(text, TextRange(text.length)) }
}

/** 此刻升起的那一个抽屉 / 确认框（[set] 换成别的或 `null` 收起；[fill] 把字填进输入框）。 */
@Composable
private fun InputSheets(
    sheet: InputSheet?,
    state: ChatUiState,
    actions: ChatInputActions,
    set: (InputSheet?) -> Unit,
    fill: (String) -> Unit,
) {
    when (sheet) {
        InputSheet.Models -> PickSheet(copyText("session.model.title"), state.picks, { it.models }, state.model.value, { set(null) }) { actions.switchModel(it) }
        InputSheet.Modes -> PickSheet(copyText("session.permission.title"), state.picks, { it.modes }, state.permission.value, { set(null) }) { actions.switchPermission(it) }
        InputSheet.Plus, InputSheet.Commands, InputSheet.Skills ->
            PlusSheet(
                at = sheet,
                view = state.commands,
                go = set,
                close = { set(null) },
                fill = { text ->
                    set(null)
                    fill(text)
                },
                say = { text ->
                    set(null)
                    actions.say(text)
                },
                confirmClear = { set(InputSheet.Clear(it)) },
            )
        is InputSheet.Clear ->
            ClearDialog(
                onCancel = { set(null) },
                onClear = {
                    set(null)
                    actions.say(sheet.text)
                },
            )
        null -> Unit
    }
}

/** 此刻升起的是哪一个（同一刻只一个）。 */
internal sealed interface InputSheet {
    data object Models : InputSheet

    data object Modes : InputSheet

    data object Plus : InputSheet

    data object Commands : InputSheet

    data object Skills : InputSheet

    /** 清空会话的确认框：确认了送 [text]。 */
    data class Clear(
        val text: String,
    ) : InputSheet
}

/** chip 上方那一行：那台写好的那一句（换的结局没成 · 动作没送到）。 */
@Composable
private fun NoteLine(text: String) {
    Text(
        text,
        modifier = Modifier.fillMaxWidth().padding(vertical = LocalAppTokens.current.spacing.xs).testTag(ChatTestTags.NOTE),
        style = MaterialTheme.typography.bodySmall,
        color = LocalAppTokens.current.warn,
    )
}

/** 一颗 chip：读回过 ⇒ 读回的字；没读回过 ⇒ 这颗是什么；正在换 ⇒ 原位转圈、写换成的那一项。 */
@Composable
private fun SwitchChipView(
    chip: SwitchChip,
    label: String,
    enabled: Boolean,
    tag: String,
    onClick: () -> Unit,
) {
    AssistChip(
        onClick = onClick,
        enabled = enabled,
        modifier = Modifier.testTag(tag),
        label = { Text(chipText(chip, label), maxLines = 1, overflow = TextOverflow.Ellipsis) },
        leadingIcon =
            if (chip.pending != null) {
                { CircularProgressIndicator(Modifier.size(14.dp).testTag(ChatTestTags.CHIP_SPINNER), strokeWidth = 2.dp) }
            } else {
                null
            },
        trailingIcon = { PhIcon(Ph.CaretDown, contentDescription = null, size = Touch.iconInline) },
    )
}

/** chip 上写什么（纯函数，判据读）：正在换 ⇒ 换成的那一项；读回过 ⇒ 读回的字；都没有 ⇒ 这颗是什么。 */
internal fun chipText(
    chip: SwitchChip,
    label: String,
): String = chip.pending?.text ?: chip.text ?: label

@Composable
private fun SendOrStop(
    running: Boolean,
    draft: String,
    actions: ChatInputActions,
    clear: () -> Unit,
) {
    // 运行中且框里没字 ⇒ 停止；二选一，不并存。输入框永远不锁。
    if (running && draft.isBlank()) {
        SendButton(Ph.Stop, copyText("mobChat.input.stop"), enabled = true, onClick = actions.stop, modifier = Modifier.testTag(ChatTestTags.STOP))
    } else {
        SendButton(
            Ph.PaperPlaneRight,
            copyText("mobChat.input.send"),
            enabled = draft.isNotBlank(),
            onClick = {
                actions.send(draft)
                clear()
            },
            modifier = Modifier.testTag(ChatTestTags.SEND),
        )
    }
}

/** 发送 / 停止：主按钮（`--accent-strong` 底 ＋ 白图标），命中区 48 dp；没字时灰掉（0.45）。无障碍名照文案表。 */
@Composable
private fun SendButton(
    icon: Ph,
    label: String,
    enabled: Boolean,
    onClick: () -> Unit,
    modifier: Modifier = Modifier,
) {
    FilledIconButton(
        onClick = onClick,
        enabled = enabled,
        modifier = modifier.size(Touch.target).semantics { contentDescription = label },
        shape = RoundedCornerShape(LocalAppTokens.current.radius.m),
        colors =
            IconButtonDefaults.filledIconButtonColors(
                disabledContainerColor = MaterialTheme.colorScheme.primary.copy(alpha = LocalAppTokens.current.disabledAlpha),
                disabledContentColor = MaterialTheme.colorScheme.onPrimary.copy(alpha = LocalAppTokens.current.disabledAlpha),
            ),
    ) {
        PhIcon(icon, contentDescription = null)
    }
}

/** 模型 · 权限档的抽屉：那台给的那几项，上一次换成了的那一项勾上。点一项 ⇒ 收起、chip 原位转圈。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PickSheet(
    title: String,
    picks: Picks?,
    of: (Picks) -> List<NewSessionPick>,
    current: String?,
    close: () -> Unit,
    pick: (NewSessionPick) -> Unit,
) {
    ModalBottomSheet(onDismissRequest = close, modifier = Modifier.testTag(ChatTestTags.PICK_SHEET)) {
        Column(Modifier.navigationBarsPadding().padding(bottom = LocalAppTokens.current.spacing.lg)) {
            SheetTitle(title)
            when {
                picks == null -> SheetLine(copyText("newSession.facts.loading"))
                picks.error != null -> SheetLine(picks.error)
                else ->
                    of(picks).forEach { p ->
                        SheetRow(p.text, end = if (p.value == current) CHECK_GLYPH else null) {
                            close()
                            pick(p)
                        }
                    }
            }
        }
    }
}

/**
 * ［+］面板：命令 › · Skill ›（那台给这条会话的那一份）；分隔线下两个一次性动作：压缩上下文（不确认）· 清空会话（先确认）。
 * 两个动作只在那份清单里有那条内置命令时画；先不先确认照那一条的 `confirm`（那台判的）。点命令 · Skill 只填进输入框。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun PlusSheet(
    at: InputSheet,
    view: CommandsView,
    go: (InputSheet) -> Unit,
    close: () -> Unit,
    fill: (String) -> Unit,
    say: (String) -> Unit,
    confirmClear: (String) -> Unit,
) {
    ModalBottomSheet(onDismissRequest = close, modifier = Modifier.testTag(ChatTestTags.PLUS_SHEET)) {
        Column(Modifier.navigationBarsPadding().padding(bottom = LocalAppTokens.current.spacing.lg)) {
            val list = (view as? CommandsView.Ready)?.list
            when {
                at != InputSheet.Plus && list != null -> CommandList(at, list, back = { go(InputSheet.Plus) }, fill = fill)
                else -> {
                    SheetTitle(copyText("mobPlus.sheet.title"))
                    when (view) {
                        CommandsView.Idle, CommandsView.Loading -> SheetLine(copyText("newSession.facts.loading"))
                        is CommandsView.Failed -> SheetLine(view.why)
                        is CommandsView.Ready -> PlusRows(view.list, go, say, confirmClear)
                    }
                }
            }
        }
    }
}

@Composable
private fun PlusRows(
    list: SessionCommands,
    go: (InputSheet) -> Unit,
    say: (String) -> Unit,
    confirmClear: (String) -> Unit,
) {
    SheetRow(copyText("mobPlus.row.commands"), end = "${list.commands.size}", next = true, enabled = list.commands.isNotEmpty(), tag = ChatTestTags.PLUS_COMMANDS) { go(InputSheet.Commands) }
    SheetRow(copyText("mobPlus.row.skills"), end = "${list.skills.size}", next = true, enabled = list.skills.isNotEmpty(), tag = ChatTestTags.PLUS_SKILLS) { go(InputSheet.Skills) }
    val acts = plusActions(list)
    if (acts.isNotEmpty()) HorizontalDivider(Modifier.padding(vertical = LocalAppTokens.current.spacing.xs))
    acts.forEach { a ->
        val label = if (a.kind == PlusAction.Kind.COMPACT) copyText("mobPlus.row.compact") else copyText("mobPlus.row.clear")
        SheetRow(label) { if (a.confirm) confirmClear(a.say) else say(a.say) }
    }
}

/** 命令 › · Skill › 那一页：命令按那台写的「住哪」分组；点一行只填进输入框。 */
@Composable
private fun CommandList(
    at: InputSheet,
    list: SessionCommands,
    back: () -> Unit,
    fill: (String) -> Unit,
) {
    val skills = at == InputSheet.Skills
    Row(verticalAlignment = Alignment.CenterVertically) {
        TextButton(onClick = back) { Text(copyText("mobKit.act.back")) }
        Text(
            if (skills) copyText("mobPlus.row.skills") else copyText("mobPlus.row.commands"),
            style = MaterialTheme.typography.titleMedium,
        )
        Spacer(Modifier.width(LocalAppTokens.current.spacing.sm))
        Text("${if (skills) list.skills.size else list.commands.size}", color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    LazyColumn(Modifier.heightIn(max = LIST_MAX_HEIGHT.dp).testTag(ChatTestTags.COMMAND_LIST)) {
        if (skills) {
            items(list.skills, key = { "skill:" + it.name }) { k -> SheetRow(k.name, sub = k.text) { fill(k.say) } }
        } else {
            groupsOf(list.commands).forEach { (head, rows) ->
                item(key = "head:$head") { SheetHead(head) }
                items(rows, key = { "cmd:" + it.scope + it.name }) { c -> SheetRow(SLASH + c.name, sub = c.text) { fill(commandDraft(c)) } }
            }
        }
        items(list.problems) { SheetLine(it) }
    }
}

/** 点一条命令填进输入框的字（那台说的：`/name ` 带一个空格，停在那里等补参数或按 ➤）。 */
internal fun commandDraft(c: SessionCommand): String = "$SLASH${c.name} "

/** 命令按那台写的「住哪」那一格分组，组序照清单里第一次出现的序（那台排好了：内置 · 本项目 · 用户级）。 */
internal fun groupsOf(commands: List<SessionCommand>): List<Pair<String, List<SessionCommand>>> = commands.groupBy { it.scopeText }.toList()

/** ［+］面板分隔线下那两个一次性动作。 */
internal data class PlusAction(
    val kind: Kind,
    /** 经 `session-say` 说的那一句。 */
    val say: String,
    /** 先确认（那台给那条命令标的）。 */
    val confirm: Boolean,
) {
    enum class Kind { COMPACT, CLEAR }
}

/**
 * 那份清单里有没有压缩 · 清空那两条内置命令：有 ⇒ 画那一行、说它的 `/name`、照它的 `confirm` 先不先确认；没有 ⇒ 不画。
 * 〔后端缺口〕那台注册表里有「请求压缩的那一句」却不上线，「清空的那一句」也没有：这里只好按内置命令名认这两条，那台补上两格后改读它们。
 */
internal fun plusActions(list: SessionCommands): List<PlusAction> =
    listOf(PlusAction.Kind.COMPACT to BUILTIN_COMPACT, PlusAction.Kind.CLEAR to BUILTIN_CLEAR).mapNotNull { (kind, name) ->
        list.commands.firstOrNull { it.scope == CommandScope.BUILTIN && it.name == name }?.let { PlusAction(kind, SLASH + it.name, it.confirm) }
    }

/** 清空会话的确认框：默认焦点在「取消」；「清空」用主按钮（原对话还在会话列表里，不是撤不回的事）。 */
@Composable
private fun ClearDialog(
    onCancel: () -> Unit,
    onClear: () -> Unit,
) {
    val cancel = remember { FocusRequester() }
    AlertDialog(
        onDismissRequest = onCancel,
        modifier = Modifier.testTag(ChatTestTags.CLEAR_DIALOG),
        title = { Text(copyText("session.clear.title")) },
        text = { Text(copyText("session.clear.body")) },
        dismissButton = { TextButton(onClick = onCancel, modifier = Modifier.focusRequester(cancel)) { Text(copyText("kit.dialog.cancel")) } },
        confirmButton = { Button(onClick = onClear, modifier = Modifier.testTag(ChatTestTags.CLEAR_OK)) { Text(copyText("session.clear.ok")) } },
    )
    LaunchedEffect(Unit) { runCatching { cancel.requestFocus() } }
}

@Composable
private fun SheetTitle(text: String) {
    Text(text, modifier = Modifier.padding(horizontal = LocalAppTokens.current.spacing.lg, vertical = LocalAppTokens.current.spacing.sm), style = MaterialTheme.typography.titleMedium)
}

@Composable
private fun SheetHead(text: String) {
    Text(
        text,
        modifier = Modifier.padding(horizontal = LocalAppTokens.current.spacing.lg, vertical = LocalAppTokens.current.spacing.xs),
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@Composable
private fun SheetLine(text: String) {
    Text(
        text,
        modifier = Modifier.padding(horizontal = LocalAppTokens.current.spacing.lg, vertical = LocalAppTokens.current.spacing.sm),
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/** 一行（命中区 48 dp 起）：主字 · 第二行灰字 · 行尾（条数 › / 勾）。 */
@Composable
private fun SheetRow(
    text: String,
    sub: String? = null,
    end: String? = null,
    /** 点了进下一层 ⇒ 行尾画一颗 caret。 */
    next: Boolean = false,
    enabled: Boolean = true,
    tag: String? = null,
    onClick: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        modifier =
            Modifier
                .fillMaxWidth()
                .heightIn(min = 48.dp)
                .clickable(enabled = enabled, onClick = onClick)
                .let { if (tag != null) it.testTag(tag) else it }
                .padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(text, color = if (enabled) MaterialTheme.colorScheme.onSurface else tokens.textFaint)
            sub?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis) }
        }
        end?.let { Text(it, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        if (next) PhIcon(Ph.CaretRight, contentDescription = null, size = Touch.iconInline, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

private const val INPUT_MAX_LINES = 4
private const val LIST_MAX_HEIGHT = 480
private const val SLASH = "/"
private const val CHECK_GLYPH = "✓"

/** 那一家内置的压缩 · 清空两条命令名（见 [plusActions] 的缺口注）。 */
private const val BUILTIN_COMPACT = "compact"
private const val BUILTIN_CLEAR = "clear"
