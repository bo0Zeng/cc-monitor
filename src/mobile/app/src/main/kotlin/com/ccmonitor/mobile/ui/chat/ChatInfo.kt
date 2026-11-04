package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.AssistChip
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.HistoryFacts
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.SessionAccount
import com.ccmonitor.mobile.core.claude.link.SessionTasks
import com.ccmonitor.mobile.core.claude.model.NoticeCard
import com.ccmonitor.mobile.core.claude.model.NoticeLook
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.model.RetryLook
import com.ccmonitor.mobile.core.claude.model.RetryStrip
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.core.wire.gen.Kin
import com.ccmonitor.mobile.core.wire.gen.PendingCall
import com.ccmonitor.mobile.core.wire.gen.SessionDot
import com.ccmonitor.mobile.core.wire.gen.TasksList
import com.ccmonitor.mobile.core.wire.gen.Tone
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.common.StatusDot
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.async
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/** 顶栏第二行：脸上那颗点 ＋ 状态一句 ＋ 上下文到了要注意那一档时那一句（[context]；没到 ⇒ `null`）。 */
data class HeadLine(
    val dot: SessionDot,
    val text: String,
    val context: String?,
)

/**
 * 顶栏第二行照哪几格：点与那一句都照脸（`dot` · `line`，那台写好的；手机不读 `life`、不拿会话事实那一句来比）；
 * 上下文那台判到 `warn` ⇒ 接那台写好的 `contextText`。簿里还没这一条 ⇒ `null`、不画第二行。
 */
fun headLine(s: ChatUiState): HeadLine? {
    val face = s.face ?: return null
    val usage = s.facts?.usage
    val context = if (usage?.contextTone == Tone.WARN) usage.contextText else null
    return HeadLine(face.dot, face.line, context)
}

/** 一次工具调用还没配上结果时那一态：会话事实 `pending[]` 里同 id 那一项；不在里面 ⇒ `null`（不画状态）。 */
fun ChatUiState.waitOf(call: String): PendingCall? = facts?.pending?.firstOrNull { it.id == call }

/** 一条重试细条此刻画什么（照会话事实 `retries[]` 那一串，[RetryLook.strip]）；收起 ⇒ `null`。 */
fun ChatUiState.retryOf(u: RenderUnit.Retry): RetryStrip? = RetryLook.strip(u, facts?.retries.orEmpty())

/** 报错卡（它的 key）下一步那一句：紧跟在一串没接上的重试后面 ⇒ 那一串的整句（[RetryLook.next]）；否则 `null`。 */
fun ChatUiState.errorNextOf(error: String): String? = RetryLook.next(units, error, facts?.retries.orEmpty())

/** 一张收场通知卡此刻画什么（照会话事实 `notices[]` 那一组，[NoticeLook.card]）；并进别的组 · 整组不露 ⇒ `null`。 */
fun ChatUiState.noticeOf(u: RenderUnit.Notice): NoticeCard? = NoticeLook.card(u, units, facts?.notices.orEmpty())

/** 血缘一行：抬头 [head] 是那台写好的那一句（「由「X」启动」·「子会话 N」），后面每条一颗。 */
data class KinLine(
    val head: String,
    val kin: List<Kin>,
)

/** 「会话信息」抽屉此刻的样子：各格是那台写好的那一句；[failed] 是没问成的那几问各自那台那一句。 */
data class InfoUi(
    val busy: Boolean = true,
    /** 这条会话在哪：那台写好的「<目录> · <机器>」（会话事实 `whereLine`）；还不知道 ⇒ `null`。 */
    val where: String? = null,
    val writers: String? = null,
    val context: String? = null,
    val account: String? = null,
    /** 一轮停了、后台命令还在跑 ⇒ 那台写好的那一句（会话事实 `background.text`）；不是这一态 ⇒ `null`。 */
    val background: String? = null,
    /** 由谁启动（会话事实 `startedByText` ＋ `startedBy`：标题 · 点 · 隐藏那一格都是那台给的）；不是谁起的 ⇒ `null`。 */
    val parent: KinLine? = null,
    /** 它起的子会话（会话事实 `childrenText` ＋ `children`，按起的先后）；一个都没有 ⇒ `null`。 */
    val children: KinLine? = null,
    /** 有任务才有（没有任务 ⇒ 不画那一段）。 */
    val tasks: TasksList? = null,
    val failed: List<String> = emptyList(),
)

/**
 * 点顶栏标题出的「会话信息」：只读。三问并着问那台 —— 会话事实（`whereLine` · `writersLine` · `usage.contextText` · `background` ·
 * `startedBy` · `children`）· `session-account`（`accountLine`）· `tasks-list`（`chip` · `hint` · 每条 `statusText`）；手机只摆，不拼、不数。
 */
class ChatInfo(
    private val link: CoreLink,
    private val machine: String,
    private val scope: CoroutineScope,
    private val sid: () -> String?,
) {
    private val _state = MutableStateFlow<InfoUi?>(null)
    val state: StateFlow<InfoUi?> = _state.asStateFlow()

    fun open() {
        val s = sid() ?: return
        _state.value = InfoUi()
        scope.launch {
            val facts = async { link.call(HistoryFacts.COMMAND, HistoryFacts.bySid(s), READ_MS) }
            val account = async { link.call(SessionAccount.COMMAND, SessionAccount.args(s), READ_MS) }
            val tasks = async { link.call(SessionTasks.COMMAND, SessionTasks.args(s), READ_MS) }
            val failed = ArrayList<String>()
            val f = read(facts.await(), HistoryFacts.COMMAND, HistoryFacts::of, failed)
            val a = read(account.await(), SessionAccount.COMMAND, SessionAccount::of, failed)
            val t = read(tasks.await(), SessionTasks.COMMAND, SessionTasks::of, failed)
            if (_state.value == null) return@launch
            _state.value =
                InfoUi(
                    busy = false,
                    where = f?.whereLine,
                    writers = f?.writersLine,
                    context = f?.usage?.contextText,
                    account = a?.accountLine,
                    background = f?.background?.text,
                    parent = f?.startedBy?.let { p -> f.startedByText?.let { KinLine(it, listOf(p)) } },
                    children = f?.children?.takeIf { it.isNotEmpty() }?.let { c -> f.childrenText?.let { KinLine(it, c) } },
                    tasks = t?.takeIf { it.tasks.isNotEmpty() },
                    failed = failed,
                )
        }
    }

    fun close() {
        _state.value = null
    }

    /** 一问的回包 ⇒ 成品；没成 · 解不出 ⇒ 那台那一句记进 [failed]。 */
    private fun <T> read(
        r: Reply,
        cmd: String,
        of: (Any?) -> T?,
        failed: MutableList<String>,
    ): T? {
        if (r is Reply.Ok) return of(r.data) ?: null.also { failed += unreadableReply(machine, cmd, r.data.toString()).text }
        r.problem(machine, cmd, link.offline.value)?.text?.let { failed += it }
        return null
    }

    private companion object {
        const val READ_MS = 15_000L
    }
}

/**
 * 「会话信息」底部抽屉：顶上是这条会话在哪（那台写好的「<目录> · <机器>」；还没问到 ⇒ 机器条那台的名字）；下面各段有才画 ——
 * 血缘（「主会话」一行、「子会话」一行，每条一颗：点 ＋ 那台给的标题 ＋ 隐藏了就带那台那一句，点了切过去）· 几个进程在写（那台那一句，不是红）·
 * 上下文 · 号与额度 · 后台在跑（那台那一句）· 任务（计数那一格 ＋ 那一句 ＋ 每条名字与状态字）；没问成的写那台那一句。
 */
@Composable
fun ChatInfoPane(
    info: ChatInfo,
    machine: String,
    /** 切到那一条会话（同一台）。 */
    onOpen: (String) -> Unit,
) {
    val ui by info.state.collectAsStateWithLifecycle()
    ui?.let {
        InfoSheet(it, machine, info::close) { sid ->
            info.close()
            onOpen(sid)
        }
    }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun InfoSheet(
    ui: InfoUi,
    machine: String,
    onClose: () -> Unit,
    onOpen: (String) -> Unit,
) {
    val tokens = LocalAppTokens.current
    val faint = MaterialTheme.typography.bodySmall
    ModalBottomSheet(onDismissRequest = onClose) {
        Column(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg).padding(bottom = tokens.spacing.lg), verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
            Text(ui.where ?: machine, style = MaterialTheme.typography.titleSmall, maxLines = 3, overflow = TextOverflow.Ellipsis)
            ui.parent?.let { KinRow(it, onOpen) }
            ui.children?.let { KinRow(it, onOpen) }
            if (ui.busy) Text(copyText("findStrip.status.busy"), style = faint, color = tokens.textFaint)
            ui.writers?.let { Text(it, style = faint) }
            ui.context?.let { Text(it, style = faint) }
            ui.account?.let { Text(it, style = faint) }
            ui.background?.let { Text(it, style = monoSmall, color = tokens.textFaint) }
            ui.tasks?.let { t ->
                HorizontalDivider()
                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
                    Text(t.chip, style = MaterialTheme.typography.labelLarge)
                    Text(t.hint, style = faint, color = tokens.textFaint)
                }
                t.tasks.forEach { task ->
                    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
                        Text(task.subject, Modifier.weight(1f), style = faint, maxLines = 2, overflow = TextOverflow.Ellipsis)
                        Text(task.statusText, style = faint, color = tokens.textFaint)
                    }
                }
            }
            ui.failed.forEach { Text(it, style = faint, color = tokens.errorText) }
        }
    }
}

/** 血缘一行：抬头那一句（那台写好的「由「X」启动」·「子会话 N」）＋ 每条一颗（点 ＋ 标题；隐藏了 ⇒ 接那台那一句），点了切过去。 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun KinRow(
    line: KinLine,
    onOpen: (String) -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
        Text(line.head, style = MaterialTheme.typography.labelMedium, color = tokens.textFaint, maxLines = 1, overflow = TextOverflow.Ellipsis)
        FlowRow(Modifier.weight(1f), horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
            line.kin.forEach { k ->
                AssistChip(
                    onClick = { onOpen(k.sid) },
                    label = {
                        Row(horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
                            Text(k.title, Modifier.weight(1f, fill = false), maxLines = 1, overflow = TextOverflow.Ellipsis)
                            k.hidden?.let { h -> Text(h.said, color = tokens.textFaint, maxLines = 1) }
                        }
                    },
                    leadingIcon = { StatusDot(k.face.dot, k.face.name) },
                )
            }
        }
    }
}

/** 顶栏第二行（[HeadLine]）：点 ＋ 状态一句；上下文到了要注意那一档 ⇒ 末尾接琥珀色那一句。 */
@Composable
fun HeadLineRow(h: HeadLine) {
    val tokens = LocalAppTokens.current
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
        StatusDot(h.dot, h.text)
        Text(h.text, Modifier.weight(1f, fill = false), style = MaterialTheme.typography.bodySmall, color = tokens.textFaint, maxLines = 1, overflow = TextOverflow.Ellipsis)
        h.context?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = tokens.warn, maxLines = 1) }
    }
}
