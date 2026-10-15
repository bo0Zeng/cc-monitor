package com.ccmonitor.mobile.ui.session.keys

import android.view.ViewConfiguration
import androidx.activity.compose.BackHandler
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.gestures.detectDragGestures
import androidx.compose.foundation.gestures.detectDragGesturesAfterLongPress
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.rememberScrollState
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableFloatStateOf
import androidx.compose.runtime.mutableStateMapOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.graphics.graphicsLayer
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.layout.onSizeChanged
import androidx.compose.ui.unit.dp
import androidx.compose.ui.zIndex
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.data.db.TermKey
import com.ccmonitor.mobile.core.data.db.cap
import com.ccmonitor.mobile.core.data.db.resetPreview
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.AppSnackbarController
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.icon.BackButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.icon.PrimaryIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.ui.nav.settingsGroupScreenInsets
import kotlinx.coroutines.launch
import org.koin.androidx.compose.koinViewModel

/** 编辑按键页上叠着的那一个框 / 抽屉（一次只一个）。 */
private sealed interface Overlay {
    data object NewGroup : Overlay

    data class Rename(
        val group: KeyGroup,
    ) : Overlay

    data object Reset : Overlay

    data object Unconverted : Overlay

    data object AddModifier : Overlay

    data class Total(
        val double: Boolean,
    ) : Overlay
}

/**
 * 编辑按键：全屏，← 回终端。顶上一排组 tab（最右 ＋ 新建组，长按拖动排先后）；组名那一行 ⋯（改名 · 左移 · 右移 · 删除组）；
 * 每颗一行（拖动柄 · 键帽 · 单击做什么 · 设了的双击 / 长按）；最底下一节两个总时长；底部「＋ 修饰键」「＋ 按键」。
 * 顶栏 ⋯ 只有「恢复默认按键」。删一颗 · 删一组直接删、给 撤销；恢复默认撤不回 ⇒ 确认框。
 */
@Composable
fun KeysEditorScreen(onBack: () -> Unit) {
    val vm = koinViewModel<KeysEditorViewModel>()
    val state by vm.state.collectAsState()
    var tab by remember { mutableStateOf<String?>(null) }
    var editing by remember { mutableStateOf<KeyDraft?>(null) }
    var overlay by remember { mutableStateOf<Overlay?>(null) }
    val scope = rememberCoroutineScope()
    val snackbar = LocalAppSnackbar.current
    val current = state.groups.firstOrNull { it.id == tab } ?: state.groups.firstOrNull { it.id == state.currentGroup } ?: state.groups.firstOrNull()
    val draft = editing
    if (draft != null) {
        BackHandler { editing = null }
        EditingKey(draft, state, vm.ops, onSaved = { tab = it }, onLeave = { editing = null }, onDraft = { editing = it })
        return
    }
    Column(Modifier.fillMaxSize().background(MaterialTheme.colorScheme.background).settingsGroupScreenInsets()) {
        EditorTopBar(
            title = copyText("mobKeys.more.edit"),
            onBack = onBack,
            menu = { close -> ResetMenuItem(!resetPreview(state.groups, state.keys).pristine, close) { overlay = Overlay.Reset } },
        )
        if (state.loaded && state.groups.isEmpty()) {
            EmptyGroups { overlay = Overlay.NewGroup }
            return@Column
        }
        current ?: return@Column
        GroupTabs(state.groups, current.id, onSelect = { tab = it }, onNew = { overlay = Overlay.NewGroup }, onMove = { id, to -> scope.launch { vm.ops.moveGroup(id, to) } })
        if (state.unconverted.isNotEmpty()) MigrateBanner(state.unconverted.size) { overlay = Overlay.Unconverted }
        val keys = state.keysOf(current.id)
        GroupHeader(
            group = current,
            count = keys.size,
            index = state.groups.indexOf(current),
            last = state.groups.lastIndex,
            onRename = { overlay = Overlay.Rename(current) },
            onMove = { to -> scope.launch { vm.ops.moveGroup(current.id, to) } },
            onDelete = { scope.launch { deleteGroupWithUndo(vm.ops, current, snackbar) } },
        )
        KeyList(
            keys = keys,
            state = state,
            onOpen = { editing = KeyDraft.of(it) },
            onMove = { from, to -> scope.launch { vm.ops.moveKey(current.id, from, to) } },
            onTotal = { overlay = Overlay.Total(it) },
            modifier = Modifier.weight(1f),
        )
        BottomActions(
            onModifier = { overlay = Overlay.AddModifier },
            onKey = { editing = KeyDraft(id = null, groupId = current.id) },
        )
    }
    EditorOverlay(overlay, state, current, vm.ops, onDone = { overlay = null }, onCreated = { tab = it })
}

/** 编辑一颗那一页接到读写上：存了回到那一组；删了直接删、给 撤销；还原默认就地换成默认那样。 */
@Composable
private fun EditingKey(
    draft: KeyDraft,
    state: KeysState,
    ops: KeyOps,
    onSaved: (String) -> Unit,
    onLeave: () -> Unit,
    onDraft: (KeyDraft) -> Unit,
) {
    val scope = rememberCoroutineScope()
    val snackbar = LocalAppSnackbar.current
    KeyEditPage(
        draft = draft,
        state = state,
        onBack = onLeave,
        onSave = { d ->
            scope.launch {
                ops.save(d)
                onSaved(d.groupId)
                onLeave()
            }
        },
        onDelete = { k ->
            scope.launch {
                ops.deleteKey(k)
                onLeave()
            }
            snackbar.showUndo(copyText("mobKeys.done.deleteKey", "cap" to k.cap(state::groupName))) { ops.undoDeleteKey(k) }
        },
        onRevert = { k -> scope.launch { onDraft(KeyDraft.of(ops.revert(k))) } },
    )
}

/** 删一组（连组里的按键）直接删，给 撤销；撤销把组和按键照原样放回原位。 */
private suspend fun deleteGroupWithUndo(
    ops: KeyOps,
    g: KeyGroup,
    snackbar: AppSnackbarController,
) {
    val gone = ops.deleteGroup(g)
    snackbar.showUndo(copyText("mobKeys.done.deleteGroup", "name" to g.name, "n" to gone.size)) { ops.undoDeleteGroup(g, gone) }
}

/** 叠在编辑页上的那一个框 / 抽屉。 */
@Composable
private fun EditorOverlay(
    overlay: Overlay?,
    state: KeysState,
    current: KeyGroup?,
    ops: KeyOps,
    onDone: () -> Unit,
    onCreated: (String) -> Unit,
) {
    val scope = rememberCoroutineScope()
    when (overlay) {
        null -> {}
        Overlay.NewGroup ->
            GroupNameDialog(copyText("mobKeys.group.new"), copyText("mobKeys.group.create"), "", state.groups, null, onDone) { name ->
                scope.launch { onCreated(ops.createGroup(name)) }
                onDone()
            }
        is Overlay.Rename ->
            GroupNameDialog(copyText("mobKeys.group.rename"), copyText("mobKeys.edit.save"), overlay.group.name, state.groups, overlay.group.id, onDone) { name ->
                scope.launch { ops.renameGroup(overlay.group, name) }
                onDone()
            }
        Overlay.Reset ->
            ResetDialog(resetLines(resetPreview(state.groups, state.keys)), onDone) {
                scope.launch { ops.restoreDefaults() }
                onDone()
            }
        Overlay.Unconverted -> UnconvertedSheet(state.unconverted, state::groupName, onDone)
        Overlay.AddModifier ->
            ModifierPickSheet(onPick = { m ->
                current?.let { g -> scope.launch { ops.addModifier(g.id, m) } }
                onDone()
            }, onDismiss = onDone)
        is Overlay.Total -> {
            val own = if (overlay.double) state.doubleTapMs else state.longPressMs
            val system = if (overlay.double) ViewConfiguration.getDoubleTapTimeout() else ViewConfiguration.getLongPressTimeout()
            DurationSheet(
                title = if (overlay.double) copyText("mobKeys.dur.double") else copyText("mobKeys.dur.long"),
                sub = null,
                single = false,
                own = own,
                following = system,
                system = system,
                range = if (overlay.double) SettingsRepository.DOUBLE_TAP_MS_RANGE else SettingsRepository.LONG_PRESS_MS_RANGE,
                onSave = { ms ->
                    scope.launch { if (overlay.double) ops.setDoubleTapMs(ms) else ops.setLongPressMs(ms) }
                    onDone()
                },
                onDismiss = onDone,
            )
        }
    }
}

/** 二级页顶栏：← · 标题 ·（[extra]）· ⋯ 菜单（[menu] 为 null ⇒ 不画）。 */
@Composable
internal fun EditorTopBar(
    title: String,
    onBack: () -> Unit,
    sub: String? = null,
    menu: (@Composable (close: () -> Unit) -> Unit)? = null,
    action: @Composable () -> Unit = {},
) {
    val tokens = LocalAppTokens.current
    Row(Modifier.fillMaxWidth().height(Touch.topBar).padding(end = tokens.spacing.sm), verticalAlignment = Alignment.CenterVertically) {
        BackButton(onBack)
        Text(title, style = MaterialTheme.typography.titleLarge)
        sub?.let {
            Spacer(Modifier.width(tokens.spacing.xs))
            HintLine(it)
        }
        Spacer(Modifier.weight(1f))
        action()
        if (menu != null) {
            var open by remember { mutableStateOf(false) }
            Box {
                PhIconButton(Ph.DotsThree, copyText("mobKeys.tool.more"), onClick = { open = true }, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                DropdownMenu(expanded = open, onDismissRequest = { open = false }, containerColor = tokens.card) { menu { open = false } }
            }
        }
    }
}

/** 组都删光了：图标 ＋ 一句 ＋ 一颗次按钮（C16）。 */
@Composable
private fun EmptyGroups(onNew: () -> Unit) {
    val tokens = LocalAppTokens.current
    Column(Modifier.fillMaxSize(), horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.Center) {
        PhIcon(Ph.Rows, contentDescription = null, size = Touch.iconEmpty, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.height(tokens.spacing.sm))
        Text(copyText("mobKeys.empty.none"), style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        HintLine(copyText("mobKeys.empty.noneHint"))
        Spacer(Modifier.height(tokens.spacing.sm))
        OutlinedButton(onClick = onNew) {
            PhIcon(Ph.Plus, contentDescription = null, size = Touch.iconInline)
            Spacer(Modifier.width(tokens.spacing.xs))
            Text(copyText("mobKeys.group.new"))
        }
    }
}

/** 一排组 tab（C5）：当前那个 accent 下沿；长按拖动排先后（组的先后只有一份）；最右 ＋ 新建组。 */
@Composable
private fun GroupTabs(
    groups: List<KeyGroup>,
    current: String,
    onSelect: (String) -> Unit,
    onNew: () -> Unit,
    onMove: (String, Int) -> Unit,
) {
    val tokens = LocalAppTokens.current
    val widths = remember { mutableStateMapOf<String, Int>() }
    var dragging by remember { mutableStateOf<String?>(null) }
    var dx by remember { mutableFloatStateOf(0f) }
    Row(Modifier.fillMaxWidth().background(tokens.bg2), verticalAlignment = Alignment.CenterVertically) {
        Row(Modifier.weight(1f).horizontalScroll(rememberScrollState())) {
            groups.forEachIndexed { i, g ->
                val on = g.id == current
                Column(
                    Modifier
                        .zIndex(if (dragging == g.id) 1f else 0f)
                        .graphicsLayer { translationX = if (dragging == g.id) dx else 0f }
                        .onSizeChanged { widths[g.id] = it.width }
                        .pointerInput(g.id, groups) {
                            detectDragGesturesAfterLongPress(
                                onDragStart = {
                                    dragging = g.id
                                    dx = 0f
                                },
                                onDragEnd = {
                                    onMove(g.id, dropIndex(groups.map { widths[it.id] ?: 0 }, i, dx))
                                    dragging = null
                                },
                                onDragCancel = { dragging = null },
                            ) { c, d ->
                                c.consume()
                                dx += d.x
                            }
                        }.clickable { onSelect(g.id) }
                        .heightIn(min = Touch.rowSingle)
                        .padding(horizontal = tokens.spacing.md),
                    verticalArrangement = Arrangement.Center,
                    horizontalAlignment = Alignment.CenterHorizontally,
                ) {
                    Text(
                        g.name,
                        style = MaterialTheme.typography.bodyMedium,
                        color = if (on) MaterialTheme.colorScheme.onSurface else MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(vertical = tokens.spacing.s),
                    )
                    Box(Modifier.height(2.dp).width(24.dp).background(if (on) tokens.accent else MaterialTheme.colorScheme.background.copy(alpha = 0f)))
                }
            }
        }
        PhIconButton(Ph.Plus, copyText("mobKeys.group.new"), onClick = onNew, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** 拖了 [offset] 像素之后落在第几格（各格宽 [sizes]，从第 [from] 格起拖）。 */
internal fun dropIndex(
    sizes: List<Int>,
    from: Int,
    offset: Float,
): Int {
    var i = from
    var left = offset
    while (left > 0 && i < sizes.lastIndex && left > sizes[i + 1] / 2f) {
        left -= sizes[i + 1]
        i++
    }
    while (left < 0 && i > 0 && -left > sizes[i - 1] / 2f) {
        left += sizes[i - 1]
        i--
    }
    return i
}

/** 升级那一次：「{n} 颗未能转换」［查看］。不带 ×，那几颗都改过或删掉之后自己消失（C15）。 */
@Composable
private fun MigrateBanner(
    n: Int,
    onView: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier.fillMaxWidth().background(tokens.card).padding(start = tokens.spacing.lg, end = tokens.spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(Modifier.weight(1f)) { NoteLine(copyText("mobKeys.migrate.bar", "n" to n), warn = true) }
        SecondaryButton(onClick = onView) { Text(copyText("mobKeys.migrate.view")) }
    }
}

/** 组名那一行：「常用 · 9 颗」，行尾 ⋯（改名 · 左移 · 右移 · 删除组）。 */
@Composable
private fun GroupHeader(
    group: KeyGroup,
    count: Int,
    index: Int,
    last: Int,
    onRename: () -> Unit,
    onMove: (Int) -> Unit,
    onDelete: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    var open by remember { mutableStateOf(false) }
    Row(Modifier.fillMaxWidth().padding(start = tokens.spacing.lg), verticalAlignment = Alignment.CenterVertically) {
        HintLine(group.name + " · " + copyText("mobKeys.group.count", "n" to count), Modifier.weight(1f))
        Box {
            PhIconButton(Ph.DotsThree, copyText("mobKeys.tool.more"), onClick = { open = true }, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            DropdownMenu(expanded = open, onDismissRequest = { open = false }, containerColor = tokens.card) {
                MenuItem(Ph.PencilSimple, copyText("mobKeys.group.rename")) {
                    open = false
                    onRename()
                }
                MenuItem(Ph.CaretLeft, copyText("mobKeys.group.moveLeft"), enabled = index > 0) {
                    open = false
                    onMove(index - 1)
                }
                MenuItem(Ph.CaretRight, copyText("mobKeys.group.moveRight"), enabled = index < last) {
                    open = false
                    onMove(index + 1)
                }
                MenuItem(Ph.Trash, copyText("mobKeys.group.delete"), danger = true) {
                    open = false
                    onDelete()
                }
            }
        }
    }
}

/** 顶栏 ⋯ 里唯一那一项；一处都没改过时灰掉。 */
@Composable
private fun ResetMenuItem(
    enabled: Boolean,
    close: () -> Unit,
    onClick: () -> Unit,
) {
    DropdownMenuItem(
        text = { Text(copyText("mobKeys.reset.menu")) },
        enabled = enabled,
        onClick = {
            close()
            onClick()
        },
    )
}

@Composable
private fun MenuItem(
    icon: Ph,
    text: String,
    enabled: Boolean = true,
    danger: Boolean = false,
    onClick: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val color = if (danger) tokens.errorText else MaterialTheme.colorScheme.onSurface
    DropdownMenuItem(
        text = { Text(text, color = if (enabled) color else tokens.textFaint) },
        leadingIcon = { PhIcon(icon, contentDescription = null, size = Touch.iconInline, tint = if (enabled) color else tokens.textFaint) },
        enabled = enabled,
        onClick = onClick,
    )
}

/** 一组的按键（拖动柄拖排先后）＋ 最底下一节「时长」。 */
@Composable
private fun KeyList(
    keys: List<TermKey>,
    state: KeysState,
    onOpen: (TermKey) -> Unit,
    onMove: (Int, Int) -> Unit,
    onTotal: (Boolean) -> Unit,
    modifier: Modifier = Modifier,
) {
    val heights = remember { mutableStateMapOf<String, Int>() }
    var dragging by remember { mutableStateOf<String?>(null) }
    var dy by remember { mutableFloatStateOf(0f) }
    LazyColumn(modifier.fillMaxWidth()) {
        itemsIndexed(keys, key = { _, k -> k.id }) { i, k ->
            KeyRow(
                k = k,
                state = state,
                onOpen = { onOpen(k) },
                handle =
                    Modifier.pointerInput(k.id, keys) {
                        detectDragGestures(
                            onDragStart = {
                                dragging = k.id
                                dy = 0f
                            },
                            onDragEnd = {
                                onMove(i, dropIndex(keys.map { heights[it.id] ?: 0 }, i, dy))
                                dragging = null
                            },
                            onDragCancel = { dragging = null },
                        ) { c, d ->
                            c.consume()
                            dy += d.y
                        }
                    },
                modifier =
                    Modifier
                        .zIndex(if (dragging == k.id) 1f else 0f)
                        .graphicsLayer { translationY = if (dragging == k.id) dy else 0f }
                        .onSizeChanged { heights[k.id] = it.height },
            )
        }
        item(key = "durations") { TotalDurations(state, onTotal) }
    }
}

/** 一颗一行：拖动柄 · 键帽 · 单击做什么 · 第二行写设了的双击 / 长按（没设的不写）。 */
@Composable
private fun KeyRow(
    k: TermKey,
    state: KeysState,
    onOpen: () -> Unit,
    handle: Modifier,
    modifier: Modifier = Modifier,
) {
    val tokens = LocalAppTokens.current
    Row(
        modifier
            .fillMaxWidth()
            .background(MaterialTheme.colorScheme.background)
            .heightIn(min = Touch.rowDouble)
            .clickable(onClick = onOpen)
            .padding(end = tokens.spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(handle.heightIn(min = Touch.target).width(Touch.target), contentAlignment = Alignment.Center) {
            PhIcon(Ph.DotsSixVertical, contentDescription = null, size = Touch.iconInline, tint = tokens.textFaint)
        }
        MiniCap(k.cap(state::groupName))
        Spacer(Modifier.width(tokens.spacing.sm))
        Column(Modifier.weight(1f)) {
            Text(firstLine(k, state::groupName), style = MaterialTheme.typography.bodyMedium)
            secondLine(k, state::groupName)?.let { HintLine(it) }
        }
        PhIcon(Ph.CaretRight, contentDescription = null, size = Touch.iconInline, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
    HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
}

/** 列表最底下一节：双击判定 · 长按，各写「300 ms · 跟系统」。 */
@Composable
private fun TotalDurations(
    state: KeysState,
    onTotal: (Boolean) -> Unit,
) {
    val tokens = LocalAppTokens.current
    Column(Modifier.fillMaxWidth().padding(top = tokens.spacing.md)) {
        SectionLabel(copyText("mobKeys.dur.title"), Modifier.padding(horizontal = tokens.spacing.lg))
        ValueRow(
            label = { TipLabel(copyText("mobKeys.dur.double"), copyText("mobKeys.dur.tip"), MaterialTheme.typography.bodyMedium, onClick = { onTotal(true) }) },
            value = durationLine(state.doubleTapMs, state.doubleTapMs ?: ViewConfiguration.getDoubleTapTimeout(), single = false),
            onClick = { onTotal(true) },
        )
        ValueRow(
            label = { Text(copyText("mobKeys.dur.long"), style = MaterialTheme.typography.bodyMedium) },
            value = durationLine(state.longPressMs, state.longPressMs ?: ViewConfiguration.getLongPressTimeout(), single = false),
            onClick = { onTotal(false) },
        )
    }
}

/** 底部：次按钮「＋ 修饰键」，主按钮「＋ 按键」。 */
@Composable
private fun BottomActions(
    onModifier: () -> Unit,
    onKey: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier.fillMaxWidth().background(tokens.bg2).padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.sm),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm, Alignment.End),
    ) {
        OutlinedButton(onClick = onModifier) {
            PhIcon(Ph.Plus, contentDescription = null, size = Touch.iconInline)
            Spacer(Modifier.width(tokens.spacing.xs))
            Text(copyText("mobKeys.add.modifier"))
        }
        PrimaryIconButton(Ph.Plus, copyText("mobKeys.add.key"), onClick = onKey)
    }
}

/** 新建组 · 改名：一个框「组名」；重名就在框下写「已有组「vim」」，主按钮灰掉。 */
@Composable
private fun GroupNameDialog(
    title: String,
    ok: String,
    initial: String,
    groups: List<KeyGroup>,
    self: String?,
    onDismiss: () -> Unit,
    onOk: (String) -> Unit,
) {
    var text by remember { mutableStateOf(initial) }
    val check = checkGroupName(text, groups, self)
    val focus = remember { FocusRequester() }
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = LocalAppTokens.current.card,
        title = { Text(title) },
        text = {
            Column {
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    singleLine = true,
                    label = { Text(copyText("mobKeys.group.nameField")) },
                    isError = check is NameCheck.Dup,
                    modifier = Modifier.fillMaxWidth().focusRequester(focus),
                )
                (check as? NameCheck.Dup)?.let { NoteLine(it.message, warn = false, Modifier.padding(top = LocalAppTokens.current.spacing.xs)) }
            }
            LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
        },
        confirmButton = { Button(onClick = { (check as? NameCheck.Ok)?.let { onOk(it.name) } }, enabled = check is NameCheck.Ok) { Text(ok) } },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) } },
    )
}

/** 恢复默认按键：撤不回 ⇒ 确认框，列清单；焦点默认在「取消」（危险钮不抢焦点）。 */
@Composable
private fun ResetDialog(
    lines: ResetLines,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val focus = remember { FocusRequester() }
    AlertDialog(
        onDismissRequest = onDismiss,
        containerColor = tokens.card,
        title = { Text(copyText("mobKeys.reset.title")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
                lines.changed?.let { ResetLine(copyText("mobKeys.reset.changed"), it) }
                lines.deleted?.let { ResetLine(copyText("mobKeys.reset.deleted"), it) }
            }
            LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
        },
        confirmButton = {
            Button(onClick = onConfirm, colors = ButtonDefaults.buttonColors(containerColor = tokens.errorStrong)) { Text(copyText("mobKeys.reset.confirm")) }
        },
        dismissButton = { SecondaryButton(onClick = onDismiss, modifier = Modifier.focusRequester(focus)) { Text(copyText("kit.dialog.cancel")) } },
    )
}

@Composable
private fun ResetLine(
    head: String,
    body: String,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(LocalAppTokens.current.spacing.sm)) {
        HintLine(head)
        Text(body, style = MaterialTheme.typography.bodyMedium)
    }
}
