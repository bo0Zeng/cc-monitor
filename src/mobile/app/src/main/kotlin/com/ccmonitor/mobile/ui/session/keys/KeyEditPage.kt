package com.ccmonitor.mobile.ui.session.keys

import android.view.ViewConfiguration
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.TermKey
import com.ccmonitor.mobile.core.data.db.autoCap
import com.ccmonitor.mobile.core.data.db.isModifiedDefault
import com.ccmonitor.mobile.core.data.db.modName
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.icon.IconTextButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.ui.nav.settingsGroupScreenInsets
import com.ccmonitor.mobile.ui.session.GestureSpec
import com.ccmonitor.mobile.ui.session.SystemTimings
import com.ccmonitor.mobile.ui.session.gestureSpec
import com.ccmonitor.mobile.ui.session.keyIcon

/** 编辑一颗上叠着的那一个抽屉。 */
private sealed interface Sheet {
    data class Record(
        val t: Trigger,
    ) : Sheet

    data class Action(
        val t: Trigger,
    ) : Sheet

    data object Group : Sheet

    data object Icon : Sheet

    data class Duration(
        val double: Boolean,
    ) : Sheet
}

/**
 * 编辑一颗：键帽（另写字或另选图标，都不填照单击自动写）· 所在组 · 单击 · 双击 · 长按（各一个三段分段按钮）· 更多（这颗的两个时长）·
 * 底部「还原默认」（默认按键改过的才有）·「删除按键」。修饰键只有所在组和三行固定的意思。保存在顶栏右边。
 */
@Composable
internal fun KeyEditPage(
    draft: KeyDraft,
    state: KeysState,
    onBack: () -> Unit,
    onSave: (KeyDraft) -> Unit,
    onDelete: (TermKey) -> Unit,
    onRevert: (TermKey) -> Unit,
) {
    val tokens = LocalAppTokens.current
    var d by remember(draft) { mutableStateOf(draft) }
    var sheet by remember { mutableStateOf<Sheet?>(null) }
    val existing = d.id?.let { id -> state.keys.firstOrNull { it.id == id } }
    val groupName = state.groupName(d.groupId).orEmpty()
    val tapCap = d.tap.binding?.let { autoCap(it, state::groupName) }
    val title = if (d.id == null) copyText("mobKeys.edit.newTitle") else d.label.ifBlank { d.modifier?.let(::modName) ?: tapCap.orEmpty() }
    Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).settingsGroupScreenInsets()) {
        EditorTopBar(title = title, sub = groupName, onBack = onBack, action = {
            Button(onClick = { onSave(d) }, enabled = d.canSave) { Text(copyText("mobKeys.edit.save")) }
        })
        Column(
            Modifier.weight(1f).verticalScroll(rememberScrollState()).padding(horizontal = tokens.spacing.lg),
            verticalArrangement = Arrangement.spacedBy(tokens.spacing.s),
        ) {
            EditBody(d, state, tapCap, groupName, onDraft = { d = it }, onSheet = { sheet = it })
            Row(Modifier.padding(vertical = tokens.spacing.sm), horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
                if (existing?.isModifiedDefault() == true) {
                    IconTextButton(Ph.ArrowCounterClockwise, copyText("mobKeys.edit.revert"), onClick = { onRevert(existing) })
                }
                existing?.let { IconTextButton(Ph.Trash, copyText("mobKeys.edit.delete"), onClick = { onDelete(it) }, danger = true) }
            }
        }
    }
    EditSheet(sheet, d, state, onDraft = { d = it }, onDone = { sheet = null })
}

/** 编辑一颗的正文：键帽 · 所在组 ·（修饰键的三行意思 ｜ 三个触发 ＋ 更多）。 */
@Composable
private fun EditBody(
    d: KeyDraft,
    state: KeysState,
    tapCap: String?,
    groupName: String,
    onDraft: (KeyDraft) -> Unit,
    onSheet: (Sheet) -> Unit,
) {
    val mod = d.modifier
    if (mod == null) {
        SectionLabel(copyText("mobKeys.edit.cap"))
        // 另写字或另选图标，二选一；都没有 ＝ 照单击自动写（占位字说会写成什么）。
        val icon = keyIcon(d.icon)
        OutlinedTextField(
            value = d.label,
            onValueChange = { onDraft(d.withLabel(it)) },
            singleLine = true,
            placeholder =
                if (icon == null) {
                    { Text(copyText("mobKeys.edit.capAuto", "x" to (tapCap ?: copyText("mobKeys.edit.capFollow")))) }
                } else {
                    null
                },
            leadingIcon = icon?.let { { PhIcon(it, contentDescription = d.icon, size = Touch.iconInline) } },
            trailingIcon = { SecondaryButton(onClick = { onSheet(Sheet.Icon) }) { Text(copyText("mobKeys.edit.capIcon")) } },
            modifier = Modifier.fillMaxWidth(),
        )
    }
    SectionLabel(copyText("mobKeys.edit.group"))
    GroupChip(groupName) { onSheet(Sheet.Group) }
    if (mod != null) {
        ModifierMeaning(mod)
    } else {
        val gone = goneTargets(d, state.groups)
        Trigger.entries.forEach { t ->
            TriggerBlock(
                t = t,
                draft = d,
                state = state,
                gone = t in gone,
                onKind = { k ->
                    val s = d.slot(t).copy(kind = k)
                    onDraft(d.withSlot(t, s))
                    if (k == BindKind.INPUT && s.input.isEmpty()) onSheet(Sheet.Record(t))
                    if (k == BindKind.ACTION && s.action == null) onSheet(Sheet.Action(t))
                },
                onRecord = { onSheet(Sheet.Record(t)) },
                onAction = { onSheet(Sheet.Action(t)) },
            )
        }
        MoreDurations(d, state) { onSheet(Sheet.Duration(it)) }
    }
}

/** 编辑一颗上的抽屉：录入 · 选 App 动作 · 选组 · 选键帽图标 · 这颗的时长。 */
@Composable
private fun EditSheet(
    sheet: Sheet?,
    d: KeyDraft,
    state: KeysState,
    onDraft: (KeyDraft) -> Unit,
    onDone: () -> Unit,
) {
    when (sheet) {
        null -> {}
        is Sheet.Record ->
            RecorderSheet(triggerName(sheet.t), d.slot(sheet.t).input, onDone = { p ->
                onDraft(d.withSlot(sheet.t, d.slot(sheet.t).copy(kind = BindKind.INPUT, input = p)))
                onDone()
            }, onDismiss = onDone)
        is Sheet.Action ->
            ActionPickSheet(triggerName(sheet.t), state.groups, onPick = { a ->
                onDraft(d.withSlot(sheet.t, d.slot(sheet.t).copy(kind = BindKind.ACTION, action = a)))
                onDone()
            }, onDismiss = onDone)
        Sheet.Group ->
            GroupPickSheet(state.groups, d.groupId, onPick = {
                onDraft(d.copy(groupId = it))
                onDone()
            }, onDismiss = onDone)
        Sheet.Icon ->
            IconPickSheet(d.icon, onPick = {
                onDraft(d.withIcon(it))
                onDone()
            }, onDismiss = onDone)
        is Sheet.Duration -> OneKeyDurationSheet(sheet.double, d, state, onDraft, onDone)
    }
}

/** 这一颗另设的时长（双击判定 · 长按）。 */
@Composable
private fun OneKeyDurationSheet(
    double: Boolean,
    d: KeyDraft,
    state: KeysState,
    onDraft: (KeyDraft) -> Unit,
    onDone: () -> Unit,
) {
    val system = if (double) ViewConfiguration.getDoubleTapTimeout() else ViewConfiguration.getLongPressTimeout()
    val total = if (double) state.doubleTapMs else state.longPressMs
    DurationSheet(
        title = if (double) copyText("mobKeys.dur.double") else copyText("mobKeys.dur.long"),
        sub =
            d.label.ifBlank {
                d.tap.binding
                    ?.let { autoCap(it, state::groupName) }
                    .orEmpty()
            },
        single = true,
        own = if (double) d.doubleTapMs else d.longPressMs,
        following = total ?: system,
        system = system,
        range = if (double) SettingsRepository.DOUBLE_TAP_MS_RANGE else SettingsRepository.LONG_PRESS_MS_RANGE,
        onSave = { ms ->
            onDraft(if (double) d.copy(doubleTapMs = ms) else d.copy(longPressMs = ms))
            onDone()
        },
        onDismiss = onDone,
    )
}

internal fun triggerName(t: Trigger): String =
    when (t) {
        Trigger.TAP -> copyText("mobKeys.trigger.tap")
        Trigger.DOUBLE -> copyText("mobKeys.trigger.double")
        Trigger.LONG -> copyText("mobKeys.trigger.long")
    }

/** 所在组：一颗下拉样子的次按钮。 */
@Composable
private fun GroupChip(
    name: String,
    onClick: () -> Unit,
) {
    OutlinedButton(onClick = onClick) {
        Text(name)
        Spacer(Modifier.width(LocalAppTokens.current.spacing.xs))
        PhIcon(Ph.CaretDown, contentDescription = null, size = Touch.iconInline)
    }
}

/** 修饰键：三行固定的意思 ＋ 一行作用范围，不给绑。 */
@Composable
private fun ModifierMeaning(m: KeyMod) {
    val tokens = LocalAppTokens.current
    SectionLabel(copyText("mobKeys.mod.title"))
    listOf(
        Trigger.TAP to modTapLine(m),
        Trigger.DOUBLE to copyText("mobKeys.mod.double"),
        Trigger.LONG to copyText("mobKeys.mod.long"),
    ).forEach { (t, line) ->
        Row(Modifier.fillMaxWidth().heightIn(min = Touch.rowSingle), verticalAlignment = Alignment.CenterVertically) {
            HintLine(triggerName(t), Modifier.width(tokens.spacing.xxl * 2))
            Text(line, style = MaterialTheme.typography.bodyMedium)
        }
        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
    }
    HintLine(copyText("mobKeys.mod.scope"))
}

/** 一个触发：标签 · 三段 · 下面那一格（录好的 / 选的动作 / 没设时的自然行为）· 绑了双击时的代价 · 目标组删了的那一句。 */
@Composable
private fun TriggerBlock(
    t: Trigger,
    draft: KeyDraft,
    state: KeysState,
    gone: Boolean,
    onKind: (BindKind) -> Unit,
    onRecord: () -> Unit,
    onAction: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val slot = draft.slot(t)
    Column(Modifier.padding(top = tokens.spacing.sm), verticalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
        TipLabel(
            triggerName(t),
            if (t == Trigger.DOUBLE) copyText("mobKeys.double.tip") else null,
            MaterialTheme.typography.titleSmall,
        )
        BindSegments(slot.kind, onKind)
        when (slot.kind) {
            BindKind.INPUT ->
                Box(
                    Modifier
                        .fillMaxWidth()
                        .heightIn(min = Touch.rowSingle)
                        .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(tokens.radius.m))
                        .clickable(onClick = onRecord)
                        .padding(horizontal = tokens.spacing.sm),
                    contentAlignment = Alignment.CenterStart,
                ) { PressesLine(slot.input) }
            BindKind.ACTION ->
                OutlinedButton(onClick = onAction) {
                    Text(slot.action?.let { bindingName(it, state::groupName) } ?: copyText("mobKeys.bind.action"))
                    Spacer(Modifier.width(tokens.spacing.xs))
                    PhIcon(Ph.CaretDown, contentDescription = null, size = Touch.iconInline)
                }
            BindKind.NONE -> {}
        }
        if (slot.binding == null) HintLine(naturalHint(t, draft.tap.binding))
        if (gone) NoteLine(copyText("mobKeys.err.groupGone"), warn = false)
        if (t == Trigger.DOUBLE && slot.binding != null) {
            NoteLine(doubleCostLine(liveSpec(draft, state).doubleTapMs.toInt()), warn = true)
        }
    }
}

/** 更多（C9 折叠）：这颗的双击判定时长 · 长按时长，写「跟总的 · 300 ms」或「另设 · 220 ms」。 */
@Composable
private fun MoreDurations(
    d: KeyDraft,
    state: KeysState,
    onOpen: (Boolean) -> Unit,
) {
    var open by remember { mutableStateOf(d.doubleTapMs != null || d.longPressMs != null) }
    HorizontalDivider(Modifier.padding(top = LocalAppTokens.current.spacing.sm), color = MaterialTheme.colorScheme.outlineVariant)
    IconTextButton(if (open) Ph.CaretDown else Ph.CaretRight, copyText("mobKeys.edit.more"), onClick = { open = !open })
    if (!open) return
    val spec = liveSpec(d, state)
    val doubleNow = spec.doubleTapMs.toInt()
    val longNow = spec.longPressMs.toInt()
    Column {
        ValueRow(
            label = { TipLabel(copyText("mobKeys.dur.double"), copyText("mobKeys.dur.tip"), MaterialTheme.typography.bodyMedium, onClick = { onOpen(true) }) },
            value = durationLine(d.doubleTapMs, doubleNow, single = true),
            onClick = { onOpen(true) },
            inset = false,
        )
        ValueRow(
            label = { Text(copyText("mobKeys.dur.long"), style = MaterialTheme.typography.bodyMedium) },
            value = durationLine(d.longPressMs, longNow, single = true),
            onClick = { onOpen(false) },
            inset = false,
        )
    }
}

/** 这一颗此刻生效的手势（时长与键条那头同一套取法：这颗另设的 → 总的 → 系统的）。 */
private fun liveSpec(
    d: KeyDraft,
    state: KeysState,
): GestureSpec =
    gestureSpec(
        d.toKey(null, endOf = { 0 }, newId = { "" }),
        state.doubleTapMs,
        state.longPressMs,
        SystemTimings(ViewConfiguration.getDoubleTapTimeout(), ViewConfiguration.getLongPressTimeout(), ViewConfiguration.getKeyRepeatDelay()),
    )
