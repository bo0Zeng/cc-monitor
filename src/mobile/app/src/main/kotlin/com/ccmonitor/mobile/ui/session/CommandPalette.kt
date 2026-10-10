@file:Suppress("MatchingDeclarationName") // 本文件按「命令面板」聚合 PaletteAction、过滤与 CommandPalette 多个声明

package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.LocalContentColor
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.ui.theme.monoSmall

/**
 * 命令面板的一个动作。[run] 在点击时执行，并关面板。
 * [filterPaletteActions] 只读 group/label/subtitle。
 */
data class PaletteAction(
    val group: String,
    val label: String,
    val subtitle: String? = null,
    /**
     * `null` 表示这一行只读、点不动。
     *
     * 聊天面的目录里混着只读条目（`tools` 是模型能力，agent/MCP 名字插进草稿也不可调用），
     * 它们要看得见，但不能给能点的假象。用可空 lambda 而不是另加 `enabled`：
     * 不可点的那条就根本没有动作，不会出现两个字段对不上。
     */
    val run: (() -> Unit)?,
)

/**
 * [PaletteRow] 的高度下界 ＝ Material3 的最小触控目标。
 */
val PALETTE_ROW_MIN_HEIGHT = 48.dp

/**
 * 列表区至多占屏高的比例。用比例而不是绝对 dp，高屏上列表才能用满下半屏。
 */
const val PALETTE_LIST_MAX_FRACTION = 0.62f

/**
 * 把一个字段切成段的分隔符。命令名、技能名、服务器名几乎都是这几种拼法
 * （`reload-skills` · `discord:configure` · `photo-capture` · `git status`）。
 */
private val PALETTE_SEGMENT_SEPARATORS = charArrayOf('/', '-', '_', ':', '.', ' ')

/**
 * [field] 的任意一段是不是以 [q] 开头（大小写不敏感）。整串本身算第一段。
 *
 * 不用子串匹配：打 `re` 会命中 `discord:configure`、`photo-capture` 这类尾音节巧合；
 * 段首匹配排除它们，`reload-skills` 照样命中。
 */
private fun matchesSegmentPrefix(
    field: String?,
    q: String,
): Boolean {
    if (field.isNullOrEmpty()) return false
    if (field.startsWith(q, ignoreCase = true)) return true
    for (i in field.indices) {
        if (field[i] in PALETTE_SEGMENT_SEPARATORS &&
            field.startsWith(q, i + 1, ignoreCase = true)
        ) {
            return true
        }
    }
    return false
}

/**
 * 按 [query] 过滤动作：空白返回全部，否则 label/group/subtitle 按段首大小写不敏感匹配。
 * 匹配口径见 [matchesSegmentPrefix]。
 */
fun filterPaletteActions(
    all: List<PaletteAction>,
    query: String,
): List<PaletteAction> {
    val q = query.trim()
    if (q.isEmpty()) return all
    return all.filter { a ->
        matchesSegmentPrefix(a.label, q) ||
            matchesSegmentPrefix(a.group, q) ||
            matchesSegmentPrefix(a.subtitle, q)
    }
}

/**
 * 可搜索命令面板：ModalBottomSheet 里放搜索框与分组动作列表。
 * 点动作即执行 `run()` 并关闭面板。模型、抓屏、自定义命令、tmux 会话的入口都在这里。
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun CommandPalette(
    actions: List<PaletteAction>,
    onDismiss: () -> Unit,
) {
    var query by remember { mutableStateOf("") }
    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp).padding(bottom = 24.dp)) {
            OutlinedTextField(
                value = query,
                onValueChange = { query = it },
                label = { Text("搜索命令") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            val filtered = filterPaletteActions(actions, query)
            Column(
                Modifier
                    .fillMaxWidth()
                    .heightIn(max = (LocalConfiguration.current.screenHeightDp * PALETTE_LIST_MAX_FRACTION).dp)
                    .verticalScroll(rememberScrollState())
                    .padding(top = 8.dp),
            ) {
                if (filtered.isEmpty()) {
                    Text("无匹配命令", style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                } else {
                    filtered.groupBy { it.group }.forEach { (group, items) ->
                        Text(
                            group,
                            style = monoSmall,
                            color = MaterialTheme.colorScheme.primary,
                            modifier = Modifier.padding(top = 12.dp, bottom = 4.dp),
                        )
                        items.forEach { a ->
                            PaletteRow(a, onPick = {
                                a.run?.invoke()
                                onDismiss()
                            })
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun PaletteRow(
    action: PaletteAction,
    onPick: () -> Unit,
) {
    Column(
        Modifier
            .fillMaxWidth()
            // 只读条目呈现但不可点（见 [PaletteAction.run]）
            .clickable(enabled = action.run != null, onClick = onPick)
            // padding 撑不出触控下界（字号一小行就矮），下界显式写成 [PALETTE_ROW_MIN_HEIGHT]。
            .heightIn(min = PALETTE_ROW_MIN_HEIGHT)
            .padding(vertical = 10.dp),
    ) {
        Text(
            action.label,
            // 只读的那条要在视觉上也弱下去，否则「点不动」变成一个只能靠试出来的事实
            color = if (action.run != null) LocalContentColor.current else MaterialTheme.colorScheme.onSurfaceVariant,
        )
        action.subtitle?.let {
            Text(
                it,
                style = monoSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}
