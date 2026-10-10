package com.ccmonitor.mobile.ui.settings

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ColumnScope
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.data.db.ButtonGroups
import com.ccmonitor.mobile.core.data.db.CustomButton
import com.ccmonitor.mobile.core.data.db.CustomButtonDao
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.ui.nav.settingsGroupScreenInsets
import com.ccmonitor.mobile.ui.session.BUILTIN_ACTIONS
import com.ccmonitor.mobile.ui.session.BUTTON_GROUPS
import com.ccmonitor.mobile.ui.session.ButtonEditDialog
import com.ccmonitor.mobile.ui.session.ButtonPurpose
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject

/**
 * 自定义按钮编辑器，组优先两级导航：组列表（进组 / ＋新建组 / 整组删除）→ 组内按钮列表（增删改重排）。
 * 按钮属于哪个组由进入时的组上下文决定。[hostId]=null 是全局按钮；非空是该主机专属。
 * 自带不透明背景：本屏也被 SessionScreen 的全屏 Dialog 承载，没有底色会让终端透出来。
 */
@Composable
fun ButtonSettingsScreen(
    hostId: String?,
    onBack: () -> Unit,
) {
    val vm = koinViewModel<ButtonSettingsViewModel>()
    val buttons by remember(hostId) { vm.flowFor(hostId) }.collectAsState(initial = emptyList())
    var openGroup by rememberSaveable { mutableStateOf<String?>(null) }

    // inset：标题别被状态栏压，组名输入框别被键盘遮。
    Column(
        Modifier
            .fillMaxSize()
            .settingsGroupScreenInsets()
            .background(MaterialTheme.colorScheme.background)
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        val group = openGroup
        if (group == null) {
            GroupListView(hostId, buttons, onBack, vm, onOpen = { openGroup = it })
        } else {
            GroupDetailView(hostId, group, buttons.filter { it.btnGroup == group }, vm, onBack = { openGroup = null })
        }
    }
}

/** 组列表：每组名字+按钮数+出现处；点进组、＋新建组、整组删除、恢复默认。 */
@Composable
private fun ColumnScope.GroupListView(
    hostId: String?,
    buttons: List<CustomButton>,
    onBack: () -> Unit,
    vm: ButtonSettingsViewModel,
    onOpen: (String) -> Unit,
) {
    var newGroupOpen by remember { mutableStateOf(false) }
    val snackbar = LocalAppSnackbar.current // 结果反馈 + 撤销
    // 撤销不能挂 VM：按返回后 `viewModelScope` 已取消，点了撤销什么都不发生。
    // 所以用 Koin single 的 DAO 加 Snackbar 控制器的根 scope。
    val buttonDao = koinInject<CustomButtonDao>()
    var pendingCustomGroupDelete by remember { mutableStateOf<String?>(null) }

    val counts = buttons.groupBy { it.btnGroup }
    val known = BUTTON_GROUPS.map { it.first }
    val groups = (known + counts.keys.filter { it !in known }.sorted()).distinct()

    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        TextButton(onClick = onBack) { Text("← 返回") }
        Text(if (hostId == null) "自定义按钮组" else "本机按钮组", style = MaterialTheme.typography.titleLarge)
    }
    Text("点进某组看/改它的按钮；键排上的 ⇌ 按钮在组间切换。", style = MaterialTheme.typography.bodySmall, color = LocalAppTokens.current.textFaint)
    LazyColumn(Modifier.fillMaxWidth().weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        items(groups, key = { it }) { g ->
            GroupRow(
                g,
                counts[g]?.size ?: 0,
                onOpen = { onOpen(g) },
                onDelete = {
                    // 整组删除分两档。「恢复默认按键」只按固定 id 补回 `DEFAULT_SEED_BUTTONS`，
                    // 所以内建组可撤销（撤销窗口外还有恢复默认兜底），直接做并给撤销；
                    // 自建组撤销窗口一过就找不回来，走确认框说清后果。
                    if (g in known) deleteGroupWithUndo(g, counts, buttonDao, snackbar) else pendingCustomGroupDelete = g
                },
            )
            HorizontalDivider(color = LocalAppTokens.current.surfaceHigh)
        }
    }
    Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Button(onClick = { newGroupOpen = true }) { Text("＋ 新建组") }
        if (hostId == null) {
            TextButton(onClick = {
                vm.restoreDefaults()
                snackbar.show("已补回缺失的默认按键")
            }) { Text("恢复默认按键") }
        }
    }

    if (newGroupOpen) {
        NewGroupDialog(
            onConfirm = {
                newGroupOpen = false
                onOpen(it)
            },
            onDismiss = { newGroupOpen = false },
        )
    }
    CustomGroupDeleteDialog(
        group = pendingCustomGroupDelete,
        count = pendingCustomGroupDelete?.let { counts[it]?.size } ?: 0,
        onDismiss = { pendingCustomGroupDelete = null },
        onConfirm = { g ->
            pendingCustomGroupDelete = null
            deleteGroupWithUndo(g, counts, buttonDao, snackbar)
        },
    )

    // 「恢复默认按键」纯增量、幂等、无损（只补回缺失的默认键），所以直接做并说一声，不配确认框。
    // 确认框只留给真正不可逆的动作，免得人习惯闭眼点确定。
}

/** 组内按钮列表：增/删/改/重排；＋新建加进本组（组由上下文固定）。 */
@Composable
private fun ColumnScope.GroupDetailView(
    hostId: String?,
    group: String,
    buttons: List<CustomButton>,
    vm: ButtonSettingsViewModel,
    onBack: () -> Unit,
) {
    var dialogOpen by remember { mutableStateOf(false) }
    var editing by remember { mutableStateOf<CustomButton?>(null) }
    var deleteTarget by remember { mutableStateOf<CustomButton?>(null) }

    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        TextButton(onClick = onBack) { Text("← 组列表") }
        Text(groupLabel(group), style = MaterialTheme.typography.titleLarge)
    }
    Text(groupWhere(group), style = MaterialTheme.typography.bodySmall, color = LocalAppTokens.current.textFaint)
    LazyColumn(Modifier.fillMaxWidth().weight(1f), verticalArrangement = Arrangement.spacedBy(4.dp)) {
        if (buttons.isEmpty()) {
            item {
                Text("本组还没有按钮 · 点下方「＋」新建", style = MaterialTheme.typography.bodySmall, color = LocalAppTokens.current.textFaint)
            }
        }
        itemsIndexed(buttons, key = { _, b -> b.id }) { i, b ->
            ButtonRow(
                button = b,
                canUp = i > 0,
                canDown = i < buttons.lastIndex,
                onUp = { vm.reorder(moveButton(buttons, i, up = true)) },
                onDown = { vm.reorder(moveButton(buttons, i, up = false)) },
                onEdit = {
                    editing = b
                    dialogOpen = true
                },
                onDelete = { deleteTarget = b },
            )
            HorizontalDivider(color = LocalAppTokens.current.surfaceHigh)
        }
    }
    Button(onClick = {
        editing = null
        dialogOpen = true
    }) { Text("＋ 新建按钮") }

    if (dialogOpen) {
        ButtonEditDialog(
            initial = editing,
            hostContext = hostId,
            fixedGroup = group,
            onDismiss = { dialogOpen = false },
            onConfirm = {
                // 新建追加到本组末（不落 sortOrder 0 与种子平级）；编辑保原 sortOrder。
                val appendOrder = (buttons.maxOfOrNull { b -> b.sortOrder } ?: -1) + 1
                vm.upsert(if (editing == null) it.copy(sortOrder = appendOrder) else it)
                dialogOpen = false
            },
        )
    }
    deleteTarget?.let { t ->
        DeleteButtonDialog(
            target = t,
            onConfirm = {
                vm.delete(t)
                deleteTarget = null
            },
            onDismiss = { deleteTarget = null },
        )
    }
}

/** 组列表单行：名字 + 按钮数 + 出现处 + 删组 + 进组箭头。 */
@Composable
private fun GroupRow(
    group: String,
    count: Int,
    onOpen: () -> Unit,
    onDelete: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier.fillMaxWidth().clickable(onClick = onOpen).padding(vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Column(Modifier.weight(1f)) {
            Text(groupLabel(group), style = MaterialTheme.typography.bodyLarge)
            Text("$count 个按钮 · ${groupWhere(group)}", style = MaterialTheme.typography.bodySmall, color = tokens.textFaint)
        }
        TextButton(onClick = onDelete) { Text("删组") }
        Text("›", style = MaterialTheme.typography.titleLarge, color = tokens.textFaint)
    }
}

/** 新建组：起个名，进空组添按钮（组随首个按钮持久化）。 */
@Composable
private fun NewGroupDialog(
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var name by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("新建按钮组") },
        text = {
            OutlinedTextField(value = name, onValueChange = { name = it }, label = { Text("组名（如：常用 / git）") }, singleLine = true)
        },
        confirmButton = { TextButton(enabled = name.isNotBlank(), onClick = { onConfirm(name.trim()) }) { Text("创建") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

/** 删除按钮二次确认。 */
@Composable
private fun DeleteButtonDialog(
    target: CustomButton,
    onConfirm: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("删除按钮") },
        text = { Text("删除「${target.label}」？") },
        confirmButton = { TextButton(onClick = onConfirm) { Text("删除") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

/** 按钮单行：标签 + 类型徽标 + payload 预览 + 上/下/编辑/删。 */
@Composable
private fun ButtonRow(
    button: CustomButton,
    canUp: Boolean,
    canDown: Boolean,
    onUp: () -> Unit,
    onDown: () -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f)) {
            Text("${button.label}  · ${buttonTypeLabel(button.type)}", style = MaterialTheme.typography.bodyMedium)
            Text(buttonPayloadPreview(button), style = MaterialTheme.typography.bodySmall, color = tokens.textFaint)
        }
        TextButton(onClick = onUp, enabled = canUp) { Text("↑") }
        TextButton(onClick = onDown, enabled = canDown) { Text("↓") }
        TextButton(onClick = onEdit) { Text("编辑") }
        TextButton(onClick = onDelete) { Text("删") }
    }
}

/**
 * 自建组删除的确认框：这一档不可逆。
 *
 * `restoreDefaultButtons` 只按固定 id 补 `DEFAULT_SEED_BUTTONS`，自建组撤销窗口一过就找不回来。
 */
@Composable
private fun CustomGroupDeleteDialog(
    group: String?,
    count: Int,
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
) {
    group ?: return
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("删除「$group」？") },
        text = {
            Text(
                "这一组的 $count 个按钮会被删除。它是自建的组，" +
                    "撤销之后就找不回来了 —— 内建组还能用「恢复默认按键」补回，自建组不能。",
            )
        },
        confirmButton = { TextButton(onClick = { onConfirm(group) }) { Text("删除") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

/**
 * 删一组按钮并给撤销。
 *
 * 注意：撤销的写操作挂 Koin single 的 DAO，不挂 ViewModel。`viewModelScope` 在 nav pop 后就取消了，
 * 撤销会静默不执行。`AppSnackbarController` 的 scope 活在应用根部，DAO 是 single，两端都不随屏死。
 */
private fun deleteGroupWithUndo(
    group: String,
    counts: Map<String, List<CustomButton>>,
    dao: CustomButtonDao,
    snackbar: com.ccmonitor.mobile.core.ui.feedback.AppSnackbarController,
) {
    val removed = counts[group].orEmpty()
    snackbar.showUndoAfter(
        action = { removed.forEach { dao.delete(it) } },
        message = "已删除「${groupLabel(group)}」的 ${removed.size} 个按钮",
        onUndo = { removed.forEach { dao.upsert(it) } },
    )
}

/** 组 id → 中文名（复用 [BUTTON_GROUPS]；自建组显原名）。 */
private fun groupLabel(group: String): String = BUTTON_GROUPS.firstOrNull { it.first == group }?.second ?: group

/** 组的出现处提示。 */
private fun groupWhere(group: String): String =
    if (group == ButtonGroups.MAIN) "工具条（终端顶部动作条）" else "键盘上方键排（⇌ 可切到此组）"

/** type → 用户视角「做什么」徽标（与编辑器同一套人话，隐藏 command/keycode 黑话）。 */
private fun buttonTypeLabel(type: String): String = ButtonPurpose.of(type).label

/** 行内 payload 预览：builtin 显动作名，其余显 command。 */
private fun buttonPayloadPreview(button: CustomButton): String =
    if (button.type == "builtin") {
        "→ " + (BUILTIN_ACTIONS.firstOrNull { it.first == button.command }?.second ?: button.command)
    } else {
        button.command
    }
