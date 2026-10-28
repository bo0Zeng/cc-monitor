package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.sheet.ActionSheet
import com.ccmonitor.mobile.core.ui.sheet.SheetAction
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.wire.gen.RewindHow
import com.ccmonitor.mobile.core.wire.gen.RewindOptions
import com.ccmonitor.mobile.core.wire.gen.RewindWhen
import com.ccmonitor.mobile.ui.claude.SaidHooks

/*
 * 编辑已发的一句（手机）：自己那句（记录里的）长按 ⇒「编辑 · 复制」；还在队里、没送进的那一句长按 ⇒「撤回」（同那一排的撤）。
 * 编辑：气泡变编辑框、原文填好，下面的流淡掉；改字落本地草稿。发送 ⇒ 问会话那台 `session-rewind-options`，底部弹层照回包摆
 * 「怎么退」几项（缺省先选上）、主 agent 在干活多一行「什么时候退」（缺省等本轮完）；回包没项 ⇒ 只写那一句为什么。
 * 确认 ⇒ 带 `rewind` 的那一句进待发（同编号重交 · 回执才算出去，[ChatSession.sayEdited]）。选项的字都是那台写好的。
 */

/**
 * 一句编辑的草稿：改到一半的字；确认过、还没回执 ⇒ [sent] 带待发编号与选的怎么退（被杀回来照同一个编号带 `rewind` 再交）。
 * 待发表没有 `rewind` 一格，编辑那一句的待发只落在这里。
 */
data class EditDraft(
    val uuid: String,
    val text: String,
    val sent: Sent? = null,
) {
    data class Sent(
        val localId: String,
        val how: RewindHow,
        val `when`: RewindWhen,
    )
}

/** 一条会话的编辑草稿落在草稿表的哪一格、怎么写成一格字（同输入框草稿那一张表）。 */
internal object EditDrafts {
    private const val FIELD = '\u0001'
    private const val RECORD = '\u0002'

    fun key(sid: String): String = "\u0000edit\u0000$sid"

    fun encode(all: List<EditDraft>): String =
        all.joinToString(RECORD.toString()) { d ->
            listOf(d.uuid, d.text, d.sent?.localId.orEmpty(), d.sent?.how?.wire.orEmpty(), d.sent?.`when`?.wire.orEmpty()).joinToString(FIELD.toString())
        }

    fun decode(s: String?): List<EditDraft> =
        s.orEmpty().split(RECORD).mapNotNull { r ->
            val f = r.split(FIELD)
            if (f.size != FIELDS || f[0].isEmpty()) return@mapNotNull null
            val how = RewindHow.entries.firstOrNull { it.wire == f[3] }
            val whn = RewindWhen.entries.firstOrNull { it.wire == f[4] }
            val sent = if (f[2].isNotEmpty() && how != null && whn != null) EditDraft.Sent(f[2], how, whn) else null
            EditDraft(f[0], f[1], sent)
        }

    private const val FIELDS = 5
}

/** 聊天屏交给流的那几样：落编辑草稿 · 问选项 · 确认那一下 · 撤回还没送进的那一句（按编号）。 */
data class SaidActions(
    val edit: (uuid: String, text: String) -> Unit,
    val options: suspend () -> RewindPick,
    val send: (uuid: String, text: String, how: RewindHow, whn: RewindWhen) -> Unit,
    val recall: (id: String) -> Unit,
)

/** 选项里缺省那一项（「怎么退」没有 ⇒ `null`：那一家不支持）；「什么时候退」没给 ⇒ 等本轮完。 */
internal fun defaultsOf(o: RewindOptions): Pair<RewindHow, RewindWhen>? {
    val how = (o.how.firstOrNull { it.default } ?: o.how.firstOrNull())?.how ?: return null
    val whn = (o.`when`.firstOrNull { it.default } ?: o.`when`.firstOrNull())?.`when` ?: RewindWhen.AFTER_TURN
    return how to whn
}

/**
 * 聊天屏的那一份编辑态（长按出的那一列 · 在编辑哪一句 · 发送那一下的弹层），弹层就画在这里；回给流的那几样钩子（[SaidHooks]）。
 * [said] 没给（新建会话还没起成）⇒ `null`：流里不出长按。
 */
@Composable
internal fun saidHooks(
    state: ChatUiState,
    said: SaidActions?,
    copy: (String) -> Unit,
): SaidHooks? {
    if (said == null || state.sid == null) return null
    var menu by remember { mutableStateOf<RenderUnit.UserText?>(null) }
    var editing by remember { mutableStateOf<RenderUnit.UserText?>(null) }
    var asking by remember { mutableStateOf<Pair<RenderUnit.UserText, String>?>(null) }
    var pick by remember { mutableStateOf<RewindPick>(RewindPick.Asking) }
    menu?.let { u ->
        SaidMenu(
            recorded = u.sourceUuid != null && u.delivery == null,
            onEdit = { editing = u },
            onCopy = { copy(u.text) },
            onRecall = { said.recall(u.key) },
            onDismiss = { menu = null },
        )
    }
    asking?.let { (u, text) ->
        LaunchedEffect(u.key, text) {
            pick = RewindPick.Asking
            pick = said.options()
        }
        RewindSheet(
            pick = pick,
            onConfirm = { how, whn ->
                u.sourceUuid?.let { said.send(it, text, how, whn) }
                asking = null
                editing = null
            },
            onDismiss = { asking = null },
        )
    }
    return SaidHooks(
        editing = editing?.key,
        onLongPress = { menu = it },
        editor = { u ->
            val uuid = u.sourceUuid.orEmpty()
            SaidEditor(
                initial = state.editDrafts[uuid] ?: u.text,
                onText = { said.edit(uuid, it) },
                onCancel = { editing = null },
                onSend = { asking = u to it },
            )
        },
    )
}

/** 长按自己那句弹的那一列：记录里的 ⇒ 编辑 · 复制；还在队里没送进的 ⇒ 撤回。 */
@Composable
internal fun SaidMenu(
    recorded: Boolean,
    onEdit: () -> Unit,
    onCopy: () -> Unit,
    onRecall: () -> Unit,
    onDismiss: () -> Unit,
) {
    val actions =
        if (recorded) {
            listOf(SheetAction(Ph.PencilSimple, copyText("mobSaidEdit.menu.edit"), onClick = onEdit), SheetAction(Ph.Copy, copyText("mobSaidEdit.menu.copy"), onClick = onCopy))
        } else {
            listOf(SheetAction(Ph.ArrowCounterClockwise, copyText("mobChat.outbox.cancel"), onClick = onRecall))
        }
    ActionSheet(title = null, actions = actions, onDismiss = onDismiss)
}

/** 气泡就地变成的编辑框：原文（或草稿）填好；字变了落草稿；发送 ⇒ 弹选项。 */
@Composable
internal fun SaidEditor(
    initial: String,
    onText: (String) -> Unit,
    onCancel: () -> Unit,
    onSend: (String) -> Unit,
) {
    val tokens = LocalAppTokens.current
    var text by remember(initial) { mutableStateOf(initial) }
    Column(Modifier.fillMaxWidth(), verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
        Text(copyText("mobSaidEdit.editor.hint"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        OutlinedTextField(
            value = text,
            onValueChange = {
                text = it
                onText(it)
            },
            modifier = Modifier.fillMaxWidth().testTag(ChatTestTags.SAID_EDIT_BOX),
        )
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm, Alignment.End)) {
            SecondaryButton(onClick = onCancel) { Text(copyText("mobSaidEdit.editor.cancel")) }
            SecondaryButton(onClick = { onSend(text) }, modifier = Modifier.testTag(ChatTestTags.SAID_EDIT_SEND)) { Text(copyText("mobSaidEdit.editor.send")) }
        }
    }
}

/** 发送那一下的选项此刻的样子：在问 · 问到了 · 问不到（那一句）。 */
sealed interface RewindPick {
    data object Asking : RewindPick

    data class Got(
        val options: RewindOptions,
    ) : RewindPick

    data class Failed(
        val why: String,
    ) : RewindPick
}

/** 底部弹层：照那台交的选项摆「怎么退」·「什么时候退」，确认 ⇒ [onConfirm]。没项 ⇒ 只写那一句为什么。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun RewindSheet(
    pick: RewindPick,
    onConfirm: (RewindHow, RewindWhen) -> Unit,
    onDismiss: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = tokens.card, tonalElevation = 0.dp) {
        Column(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.md), verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
            when (pick) {
                RewindPick.Asking -> Text(copyText("mobSaidEdit.pick.loading"))
                is RewindPick.Failed -> Text(pick.why)
                is RewindPick.Got -> Choices(pick.options, onConfirm)
            }
        }
    }
}

@Composable
private fun Choices(
    o: RewindOptions,
    onConfirm: (RewindHow, RewindWhen) -> Unit,
) {
    val first = defaultsOf(o)
    if (first == null) {
        Text(o.said.orEmpty())
        return
    }
    var how by remember(o) { mutableStateOf(first.first) }
    var whn by remember(o) { mutableStateOf(first.second) }
    Text(copyText("mobSaidEdit.pick.how"), style = MaterialTheme.typography.labelMedium)
    o.how.forEach { r -> Choice(r.text, how == r.how) { how = r.how } }
    if (o.`when`.isNotEmpty()) {
        Text(copyText("mobSaidEdit.pick.when"), style = MaterialTheme.typography.labelMedium)
        o.`when`.forEach { r -> Choice(r.text, whn == r.`when`) { whn = r.`when` } }
    }
    o.said?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
    SecondaryButton(onClick = { onConfirm(how, whn) }, modifier = Modifier.testTag(ChatTestTags.SAID_EDIT_CONFIRM)) { Text(copyText("mobSaidEdit.pick.confirm")) }
}

@Composable
private fun Choice(
    text: String,
    selected: Boolean,
    onPick: () -> Unit,
) {
    Row(Modifier.fillMaxWidth().selectable(selected = selected, onClick = onPick), verticalAlignment = Alignment.CenterVertically) {
        RadioButton(selected = selected, onClick = onPick)
        Text(text)
    }
}
