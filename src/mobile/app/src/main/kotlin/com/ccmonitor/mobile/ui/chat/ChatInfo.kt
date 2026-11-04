package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
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
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
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
    val context = s.facts?.usage?.takeIf { it.contextTone == Tone.WARN }?.contextText
    return HeadLine(face.dot, s.facts?.line?.text ?: face.line, context)
}

/** 一次工具调用还没配上结果时那一态：会话事实 `pending[]` 里同 id 那一项；不在里面 ⇒ `null`（不画状态）。 */
fun ChatUiState.waitOf(call: String): PendingCall? = facts?.pending?.firstOrNull { it.id == call }

/** 「会话信息」抽屉此刻的样子：各格是那台写好的那一句；[failed] 是没问成的那几问各自那台那一句。 */
data class InfoUi(
    val busy: Boolean = true,
    val writers: String? = null,
    val context: String? = null,
    val account: String? = null,
    /** 有任务才有（没有任务 ⇒ 不画那一段）。 */
    val tasks: TasksList? = null,
    val failed: List<String> = emptyList(),
)

/**
 * 点顶栏标题出的「会话信息」：只读。三问并着问那台 —— 会话事实（`writersLine` · `usage.contextText`）· `session-account`（`accountLine`）·
 * `tasks-list`（`chip` · `hint` · 每条 `statusText`）；手机只摆，不拼、不数。
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
                    writers = f?.writersLine,
                    context = f?.usage?.contextText,
                    account = a?.accountLine,
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
 * 「会话信息」底部抽屉：顶上是机器条那台的名字与目录（两格各自摆，不拼成一句）；下面各段有才画 —— 几个进程在写（那台那一句，不是红）·
 * 上下文 · 号与额度 · 任务（计数那一格 ＋ 那一句 ＋ 每条名字与状态字）；没问成的写那台那一句。
 */
@Composable
fun ChatInfoPane(
    info: ChatInfo,
    machine: String,
    cwd: String?,
) {
    val ui by info.state.collectAsStateWithLifecycle()
    ui?.let { InfoSheet(it, machine, cwd, info::close) }
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun InfoSheet(
    ui: InfoUi,
    machine: String,
    cwd: String?,
    onClose: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val faint = MaterialTheme.typography.bodySmall
    ModalBottomSheet(onDismissRequest = onClose) {
        Column(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg).padding(bottom = tokens.spacing.lg), verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
            Text(machine, style = MaterialTheme.typography.titleSmall)
            cwd?.let { Text(it, style = monoSmall, color = tokens.textFaint, maxLines = 2, overflow = TextOverflow.Ellipsis) }
            if (ui.busy) Text(copyText("findStrip.status.busy"), style = faint, color = tokens.textFaint)
            ui.writers?.let { Text(it, style = faint) }
            ui.context?.let { Text(it, style = faint) }
            ui.account?.let { Text(it, style = faint) }
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
