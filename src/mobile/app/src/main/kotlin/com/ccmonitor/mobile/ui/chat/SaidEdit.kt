package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableIntStateOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.cache.ChatCache
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionRewindOptions
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.sheet.ActionSheet
import com.ccmonitor.mobile.core.ui.sheet.SheetAction
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.wire.gen.Rewind
import com.ccmonitor.mobile.core.wire.gen.RewindHow
import com.ccmonitor.mobile.core.wire.gen.RewindOptions
import com.ccmonitor.mobile.core.wire.gen.RewindWhen
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.claude.SaidHooks
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import java.util.UUID

/*
 * 编辑已发的一句（手机）：自己那句（记录里的）长按 ⇒「编辑 · 复制」；还在队里、没送进的那一句长按 ⇒「撤回」（同那一排的撤）。
 * 编辑：气泡变编辑框、原文填好，下面的流淡掉；改字落本地草稿。开框就问会话那台 `session-rewind-options`，发送 ⇒ 底部抽屉照回包摆
 * 「怎么退」几项（缺省先选上）、主 agent 在干活多一行「什么时候退」（缺省等这一轮跑完）；回包没项 ⇒ 只写那一句为什么。
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
            val s = d.sent
            listOf(d.uuid, d.text, s?.localId ?: "", s?.how?.wire ?: "", s?.`when`?.wire ?: "").joinToString(FIELD.toString())
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

/**
 * 一条会话的编辑草稿（[EditDrafts] 那一格，改了就整格写回、[shown] 交给界面那一份 uuid ⇒ 字）与发送那一下问选项（`session-rewind-options`）。
 * 待发里那一句本身由 [ChatSession] 交（同普通那一句），这里只记它的编号与选的怎么退，回执到了删、撤了留字。
 */
internal class SaidEdits(
    private val link: CoreLink,
    private val machine: String,
    private val cache: ChatCache,
    private val scope: CoroutineScope,
    private val sid: () -> String?,
    /** 编辑那几句进待发、交出去（同普通那一句，[ChatSession] 做）。 */
    private val pend: (List<Outgoing>) -> Unit,
    private val shown: (Map<String, String>) -> Unit,
) {
    private var all: List<EditDraft> = emptyList()

    private fun keep() {
        val s = sid() ?: return
        val now = all
        shown(now.associate { it.uuid to it.text })
        scope.launch { cache.saveDraft(EditDrafts.key(s), EditDrafts.encode(now), null, null) }
    }

    /** 开屏读回；确认过、还没回执的那几句照同一个编号带 `rewind` 回到待发（接上那台再交）。 */
    suspend fun load(s: String) {
        all = EditDrafts.decode(cache.draft(EditDrafts.key(s)))
        shown(all.associate { it.uuid to it.text })
        pend(all.mapNotNull { d -> d.sent?.let { outgoingOf(it.localId, d.uuid, d.text, it.how, it.`when`) } })
    }

    /** 编辑框里的字变了。 */
    fun text(
        uuid: String,
        text: String,
    ) {
        all = all.filterNot { it.uuid == uuid } + (all.firstOrNull { it.uuid == uuid }?.copy(text = text) ?: EditDraft(uuid, text))
        keep()
    }

    /** 确认那一下：铸那一句待发的编号、随草稿记下，进待发交出去；还没有会话 ⇒ 不动。 */
    fun send(
        uuid: String,
        text: String,
        how: RewindHow,
        whn: RewindWhen,
    ) {
        sid() ?: return
        val o = outgoingOf("local-${UUID.randomUUID()}", uuid, text, how, whn)
        all = all.filterNot { it.uuid == uuid } + EditDraft(uuid, text, EditDraft.Sent(o.localId, how, whn))
        keep()
        pend(listOf(o))
    }

    /** 交过一次：编辑的那一句回执到了 ⇒ 它的编辑草稿删掉。 */
    fun landed(o: Outgoing) {
        val r = o.rewind ?: return
        if (!o.receipt) return
        all = all.filterNot { it.uuid == r.to }
        keep()
    }

    /** 撤了编辑的那一句：草稿留着字、不再带待发编号（下次开屏不再交）。 */
    fun unsent(localId: String) {
        if (all.none { it.sent?.localId == localId }) return
        all = all.map { if (it.sent?.localId == localId) it.copy(sent = null) else it }
        keep()
    }

    /** 发送那一下：问会话那台，退到 [uuid] 那一句之前此刻给哪几项。 */
    suspend fun options(uuid: String): RewindPick {
        val s = sid() ?: return RewindPick.Asking
        return when (val r = link.call(SessionRewindOptions.COMMAND, SessionRewindOptions.args(s, uuid), INPUT_MS)) {
            is Reply.Ok ->
                SessionRewindOptions.of(r.data)?.let { RewindPick.Got(it) }
                    ?: RewindPick.Failed(unreadableReply(machine, SessionRewindOptions.COMMAND, r.data.toString()).text)
            else -> RewindPick.Failed(r.problem(machine, SessionRewindOptions.COMMAND, link.offline.value)?.text.orEmpty())
        }
    }

    private fun outgoingOf(
        localId: String,
        uuid: String,
        text: String,
        how: RewindHow,
        whn: RewindWhen,
    ) = Outgoing(localId, text, DeliveryState.SENDING, copyText("mobChat.delivery.sending"), rewind = Rewind(uuid, how, whn))

    private companion object {
        const val INPUT_MS = 20_000L
    }
}

/** 聊天屏交给流的那几样：落编辑草稿 · 问选项 · 确认那一下 · 撤回还没送进的那一句（按编号）。 */
data class SaidActions(
    val edit: (uuid: String, text: String) -> Unit,
    val options: suspend (uuid: String) -> RewindPick,
    val send: (uuid: String, text: String, how: RewindHow, whn: RewindWhen) -> Unit,
    val recall: (id: String) -> Unit,
)

/** 选项里缺省那一项（只在能选的里挑；「怎么退」没有能选的 ⇒ `null`：退不了）；「什么时候退」没给 ⇒ 等这一轮跑完。 */
internal fun defaultsOf(o: RewindOptions): Pair<RewindHow, RewindWhen>? {
    val on = o.how.filter { it.enabled }
    val how = (on.firstOrNull { it.default } ?: on.firstOrNull())?.how ?: return null
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
    var asks by remember { mutableIntStateOf(0) }
    // 开框就问；会话忙闲一变 · 点了［重试］再问，抽屉照新回包重摆（界面不自己判在干活）。
    editing?.let { u ->
        LaunchedEffect(u.key, state.running, asks) {
            pick = RewindPick.Asking
            u.sourceUuid?.let { pick = said.options(it) }
        }
    }
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
        RewindSheet(
            pick = pick,
            target = u.text,
            onRetry = { asks++ },
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

/** 退法抽屉底下那几颗：［取消］·［回退并发送］（主按钮）·［重试］（贴在问不到那一句右边）·［关闭］。 */
internal enum class SheetAct { CANCEL, CONFIRM, RETRY, CLOSE }

/** 退法抽屉此刻摆什么：在问 ⇒ 两行骨架（不跳高）；[acts] 先后照摆；[confirm] 主按钮能不能点。 */
internal data class SheetFace(
    val skeleton: Boolean,
    val acts: List<SheetAct>,
    val confirm: Boolean,
)

/** 在问 ⇒ 骨架 ＋［取消］＋ 灰着的主按钮；有能选的 ⇒［取消］＋ 主按钮；一项都不能选 ⇒［关闭］；问不到 ⇒［重试］＋［取消］。 */
internal fun sheetFace(pick: RewindPick): SheetFace =
    when (pick) {
        RewindPick.Asking -> SheetFace(skeleton = true, acts = listOf(SheetAct.CANCEL, SheetAct.CONFIRM), confirm = false)
        is RewindPick.Failed -> SheetFace(skeleton = false, acts = listOf(SheetAct.RETRY, SheetAct.CANCEL), confirm = false)
        is RewindPick.Got ->
            if (defaultsOf(pick.options) == null) {
                SheetFace(skeleton = false, acts = listOf(SheetAct.CLOSE), confirm = false)
            } else {
                SheetFace(skeleton = false, acts = listOf(SheetAct.CANCEL, SheetAct.CONFIRM), confirm = true)
            }
    }

/**
 * 退法抽屉（已有的底部抽屉样式）：标题「回退并发送」＋ 退到哪一句之前（[target] 那句原文，截一行）；照那台交的选项摆「怎么退」·「什么时候退」，
 * 按钮照 [sheetFace]。确认 ⇒ [onConfirm]；［重试］⇒ [onRetry]；［取消］·［关闭］· 下滑 ⇒ [onDismiss]。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun RewindSheet(
    pick: RewindPick,
    target: String,
    onRetry: () -> Unit,
    onConfirm: (RewindHow, RewindWhen) -> Unit,
    onDismiss: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val face = sheetFace(pick)
    val got = (pick as? RewindPick.Got)?.options
    val first = got?.let(::defaultsOf)
    var how by remember(got) { mutableStateOf(first?.first) }
    var whn by remember(got) { mutableStateOf(first?.second) }
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = tokens.card, tonalElevation = 0.dp) {
        Column(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.md), verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
            Column {
                Text(copyText("mobSaidEdit.sheet.title"), style = MaterialTheme.typography.titleMedium)
                // 「退到此句之前：」＋ 那句原文（截一行）：两截各摆，不拼进一句。
                Row(horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
                    val faint = MaterialTheme.colorScheme.onSurfaceVariant
                    Text(copyText("mobSaidEdit.sheet.to"), style = MaterialTheme.typography.bodySmall, color = faint, maxLines = 1)
                    Text(target.lineSequence().firstOrNull().orEmpty(), style = MaterialTheme.typography.bodySmall, color = faint, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
            when (pick) {
                RewindPick.Asking -> Skeleton()
                is RewindPick.Failed ->
                    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
                        ErrorLine(pick.why, Modifier.weight(1f))
                        SecondaryButton(onClick = onRetry) { Text(copyText("front.act.retry")) }
                    }
                is RewindPick.Got -> {
                    val o = pick.options
                    if (o.how.isNotEmpty()) Text(copyText("mobSaidEdit.pick.how"), style = MaterialTheme.typography.labelMedium)
                    o.how.forEach { r -> Choice(r.text, first != null && how == r.how, enabled = first != null && r.enabled, why = r.why) { how = r.how } }
                    if (first != null && o.`when`.isNotEmpty()) {
                        Text(copyText("mobSaidEdit.pick.when"), style = MaterialTheme.typography.labelMedium)
                        o.`when`.forEach { r -> Choice(r.text, whn == r.`when`) { whn = r.`when` } }
                    }
                    o.said?.let { Text(it, style = MaterialTheme.typography.bodySmall) }
                }
            }
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm, Alignment.End), verticalAlignment = Alignment.CenterVertically) {
                face.acts.forEach { a ->
                    when (a) {
                        SheetAct.CANCEL -> SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) }
                        SheetAct.CLOSE -> SecondaryButton(onClick = onDismiss) { Text(copyText("mobKit.act.close")) }
                        SheetAct.RETRY -> Unit
                        SheetAct.CONFIRM ->
                            Button(
                                onClick = { if (how != null && whn != null) onConfirm(how!!, whn!!) },
                                enabled = face.confirm,
                                modifier = Modifier.testTag(ChatTestTags.SAID_EDIT_CONFIRM),
                            ) { Text(copyText("mobSaidEdit.pick.confirm")) }
                    }
                }
            }
        }
    }
}

/** 在问时的两行骨架：同选项那一行的高，问到了换上去不跳高。 */
@Composable
private fun Skeleton() {
    val tokens = LocalAppTokens.current
    Text(copyText("mobSaidEdit.pick.loading"), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    repeat(SKELETON_ROWS) {
        Box(Modifier.fillMaxWidth().height(Touch.rowSingle).clip(RoundedCornerShape(tokens.radius.m)).background(MaterialTheme.colorScheme.surfaceVariant))
    }
}

private const val SKELETON_ROWS = 2

@Composable
private fun Choice(
    text: String,
    selected: Boolean,
    enabled: Boolean = true,
    why: String? = null,
    onPick: () -> Unit,
) {
    // 不能选：灰着、点不了；那台那一句为什么缩在项字下面、淡色。
    Row(Modifier.fillMaxWidth().selectable(selected = selected, enabled = enabled, onClick = onPick), verticalAlignment = Alignment.CenterVertically) {
        RadioButton(selected = selected, onClick = onPick, enabled = enabled)
        Column {
            Text(text, color = if (enabled) Color.Unspecified else LocalAppTokens.current.textFaint)
            why?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = LocalAppTokens.current.textFaint) }
        }
    }
}
