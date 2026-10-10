package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.data.db.ButtonGroups
import com.ccmonitor.mobile.core.data.db.CustomButton
import com.ccmonitor.mobile.core.data.db.CustomButtonDao
import com.ccmonitor.mobile.core.data.db.CustomButtonType
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoBody
import kotlinx.coroutines.launch
import org.koin.compose.koinInject

/**
 * 一键按钮条：渲染该主机的全局与专属按钮，长按删，「+」加。
 * 按钮可带 Ctrl/Alt/Shift 修饰：有修饰发按键码（如 Ctrl+C=0x03），无修饰发命令行（`command\n`）。
 * 在这里加的按钮挂当前 [hostId]。
 * 出线字节统一经 [dispatch]（`TerminalBottomControls` 造的 `dispatchButton(it, send, builtins)`），
 * 与键排、命令面板同一条派发路径。`▤ tab`/`命令`/`tmux 会话` 这些工具条动作也是本条里的 main 组 builtin，
 * 所以要 [supportsTmux] 门控：非 tmux 主机上不渲染 tmux 域内建按钮（见 [visibleOn]）。
 */
@OptIn(ExperimentalFoundationApi::class)
@Composable
fun CustomButtonBar(
    hostId: String,
    dispatch: (CustomButton) -> Unit, // 唯一的派发入口
    supportsTmux: Boolean, // 能力位门控：tmux 域 builtin 在非 tmux 主机上不显示
    modifier: Modifier = Modifier,
) {
    val dao = koinInject<CustomButtonDao>()
    val scope = rememberCoroutineScope()
    // 主栏只渲染 group=main（自建按钮与工具条内建）；fn/vim 等组归 ExtraKeysRow。
    val all by remember(hostId) {
        dao.observeGroup(hostId, ButtonGroups.MAIN)
    }.collectAsState(initial = emptyList())
    // 非 tmux 主机上滤掉 tmux 域 builtin（`tmux 会话`/历史/抓屏/公钥），画出来就是按了没反应的死按钮。
    val buttons = all.filter { it.visibleOn(supportsTmux) }
    val tokens = LocalAppTokens.current
    var showAdd by remember { mutableStateOf(false) }
    var deleteTarget by remember { mutableStateOf<CustomButton?>(null) }

    Row(
        modifier
            .fillMaxWidth()
            .background(tokens.surfaceHigh)
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = tokens.spacing.sm, vertical = tokens.spacing.xs),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
    ) {
        buttons.forEach { b ->
            Chip(
                label = b.label,
                bg = tokens.accentSoft,
                onClick = { dispatch(b) }, // 按 type 派发（命令/按键码/转义/内建）
                onLongClick = { deleteTarget = b },
                // 工具条内建动作（▤ tab/命令/tmux 等 builtin）用 TextButton 式扁平样式；
                // 自建命令按钮用填充 chip，区分「动作」与「命令」。
                flat = CustomButtonType.fromDb(b.type) == CustomButtonType.BUILTIN,
            )
        }
        Chip(label = "＋", bg = tokens.surfaceRaised, onClick = { showAdd = true })
    }

    if (showAdd) {
        // 快速添加复用 [ButtonEditDialog]；hostContext 为本主机，可选全局或本机。
        ButtonEditDialog(
            initial = null,
            hostContext = hostId,
            onDismiss = { showAdd = false },
            fixedGroup = ButtonGroups.MAIN, // 按钮条快速添加固定进 main 组，追加序从 all（即 main）算
            onConfirm = { button ->
                // 追加到 main 组末尾；sortOrder 落 0 会排到工具条内建之前，把 ▤tab/命令 挤到右边。
                val appendOrder = (all.maxOfOrNull { it.sortOrder } ?: -1) + 1
                scope.launch { dao.upsert(button.copy(sortOrder = appendOrder)) }
                showAdd = false
            },
        )
    }
    deleteTarget?.let { target ->
        AlertDialog(
            onDismissRequest = { deleteTarget = null },
            title = { Text("删除按钮") },
            text = { Text("删除「${target.label}」？") },
            confirmButton = {
                TextButton(onClick = {
                    scope.launch { dao.delete(target) }
                    deleteTarget = null
                }) { Text("删除") }
            },
            dismissButton = { TextButton(onClick = { deleteTarget = null }) { Text("取消") } },
        )
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun Chip(
    label: String,
    bg: Color,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)? = null,
    flat: Boolean = false, // 扁平变体：透明底加主色字，即工具条内建动作的 TextButton 样式
) {
    Box(
        modifier =
            Modifier
                .clip(RoundedCornerShape(6.dp))
                .background(if (flat) Color.Transparent else bg)
                .combinedClickable(onClick = onClick, onLongClick = onLongClick)
                .defaultMinSize(minWidth = 44.dp, minHeight = 36.dp)
                .padding(horizontal = 12.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text = label, style = monoBody, color = if (flat) MaterialTheme.colorScheme.primary else Color.Unspecified)
    }
}
