package com.ccmonitor.mobile.ui.files

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.claude.link.CoreLink
import com.ccmonitor.mobile.core.claude.link.FilesFind
import com.ccmonitor.mobile.core.claude.link.FilesGrep
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.core.wire.gen.FileKind
import com.ccmonitor.mobile.core.wire.gen.WirePath
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.launch

/**
 * 文件屏的搜索：按名字（`files-find`，那台的文件清单）· 按内容（`files-grep`），搜都由那台做、起点是当前目录。
 * 结果照回包摆（名字命中：路径 · 那台给的所在目录；内容命中：相对路径 · 行号 · 那一行）；零条 ⇒「无匹配」；没问成 ⇒ 那台那一句。
 */
class FileSearch(
    private val feed: CoreLink,
    private val machine: suspend () -> String,
    private val scope: CoroutineScope,
) {
    /** 一条命中：[Name] 名字命中（[dir] 那台说是目录）· [Line] 内容命中。 */
    sealed interface Row {
        val path: RemotePath

        data class Name(
            override val path: RemotePath,
            val location: String,
            val dir: Boolean,
        ) : Row

        data class Line(
            override val path: RemotePath,
            val rel: String,
            val line: Long,
            val text: String,
        ) : Row
    }

    /** 那一面此刻：按内容搜 · 词 · 在问 · 命中（还没问过 · 没问成 ⇒ `null`）· 那一句（无匹配 · 那台写的没问成那一句）。 */
    data class Ui(
        val content: Boolean = false,
        val query: String = "",
        val busy: Boolean = false,
        val rows: List<Row>? = null,
        val said: String? = null,
    )

    private val _state = MutableStateFlow<Ui?>(null)
    val state: StateFlow<Ui?> = _state.asStateFlow()

    private var asking: Job? = null

    fun open() {
        _state.value = _state.value ?: Ui()
    }

    fun close() {
        asking?.cancel()
        _state.value = null
    }

    /** 换「名字 | 内容」：清掉上一趟的命中（两种命中不混着摆）。 */
    fun mode(content: Boolean) {
        val now = _state.value ?: return
        if (now.content != content) _state.value = Ui(content = content, query = now.query)
    }

    /** 在 [dir] 以下搜 [query]（空着不问）；上一问还没回 ⇒ 撤掉，只认这一问。 */
    fun search(
        dir: RemotePath,
        query: String,
    ) {
        val now = _state.value ?: return
        if (query.isBlank()) return
        asking?.cancel()
        _state.value = Ui(now.content, query, busy = true)
        asking =
            scope.launch {
                val (cmd, args) = if (now.content) FilesGrep.COMMAND to FilesGrep.args(dir, query) else FilesFind.COMMAND to FilesFind.args(dir, query)
                val r = feed.call(cmd, args)
                val rows = if (r is Reply.Ok) rowsOf(now.content, r.data) else null
                val said =
                    when {
                        rows != null -> copyText("history.list.noMatch").takeIf { rows.isEmpty() }
                        r is Reply.Ok -> unreadableReply(machine(), cmd, r.data.toString()).text
                        else -> r.problem(machine(), cmd, feed.offline.value)?.text
                    }
                _state.value = Ui(now.content, query, rows = rows, said = said)
            }
    }

    /** 回包 ⇒ 那几行；解不出 ⇒ `null`。 */
    private fun rowsOf(
        content: Boolean,
        data: Any?,
    ): List<Row>? =
        if (content) {
            FilesGrep.of(data)?.hits?.mapNotNull { h -> RemotePath.of(h.path)?.let { Row.Line(it, shown(h.rel), h.line, shown(h.text)) } }
        } else {
            FilesFind.of(data)?.hits?.mapNotNull { h -> RemotePath.of(h.path)?.let { Row.Name(it, shown(h.location), h.kind == FileKind.DIR) } }
        }

    private fun shown(w: WirePath): String = RemotePath.of(w)?.shown.orEmpty()
}

/**
 * 搜索那一面（底部抽屉）：分段「名字 | 内容」（字同桌面文件窗口）· 框（回车问那台）· 那一句 · 命中；点一条交 [onPick]。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun FileSearchSheet(
    search: FileSearch,
    dir: RemotePath,
    onPick: (FileSearch.Row) -> Unit,
) {
    val ui by search.state.collectAsStateWithLifecycle()
    val s = ui ?: return
    val tokens = LocalAppTokens.current
    var q by remember { mutableStateOf(s.query) }
    ModalBottomSheet(onDismissRequest = search::close) {
        Column(Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.lg).padding(bottom = tokens.spacing.lg), verticalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
            val choices = listOf(false to copyText("acctNew.name.label"), true to copyText("rsFilewinShell.search.byContent"))
            SingleChoiceSegmentedButtonRow(Modifier.fillMaxWidth()) {
                choices.forEachIndexed { i, (v, label) ->
                    SegmentedButton(selected = s.content == v, onClick = { search.mode(v) }, shape = SegmentedButtonDefaults.itemShape(i, choices.size), icon = {}) { Text(label) }
                }
            }
            OutlinedTextField(
                value = q,
                onValueChange = { q = it },
                modifier = Modifier.fillMaxWidth(),
                singleLine = true,
                placeholder = { Text(if (s.content) copyText("rsFilewinShell.search.hintContent") else copyText("rsFilewinShell.search.hint")) },
                keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
                keyboardActions = KeyboardActions(onSearch = { search.search(dir, q) }),
            )
            (if (s.busy) copyText("findStrip.status.busy") else s.said)?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = tokens.textFaint) }
            LazyColumn(Modifier.heightIn(max = 420.dp)) {
                items(s.rows.orEmpty()) { r -> HitRow(r) { onPick(r) } }
            }
        }
    }
}

/** 一条命中：名字命中 ⇒ 路径（末尾总看得见）＋ 所在目录；内容命中 ⇒ 相对路径与行号 ＋ 那一行。 */
@Composable
private fun HitRow(
    r: FileSearch.Row,
    onClick: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Column(Modifier.fillMaxWidth().clickable(onClick = onClick).padding(vertical = tokens.spacing.xs)) {
        when (r) {
            is FileSearch.Row.Name -> {
                Text(r.path.shown, maxLines = 1, overflow = TextOverflow.StartEllipsis)
                Text(r.location, style = monoSmall, color = tokens.textFaint, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            is FileSearch.Row.Line -> {
                Row(horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
                    Text(r.rel, Modifier.weight(1f, fill = false), style = monoSmall, color = tokens.textFaint, maxLines = 1, overflow = TextOverflow.StartEllipsis)
                    Text(r.line.toString(), style = monoSmall, color = tokens.textFaint)
                }
                Text(r.text.trim(), style = monoSmall, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
    }
}
