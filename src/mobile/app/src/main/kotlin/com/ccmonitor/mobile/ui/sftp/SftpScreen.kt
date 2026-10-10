package com.ccmonitor.mobile.ui.sftp

import android.net.Uri
import android.widget.Toast
import androidx.activity.compose.ManagedActivityResultLauncher
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExtendedFloatingActionButton
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.data.db.Bookmark
import com.ccmonitor.mobile.core.data.repo.BookmarkRepository
import com.ccmonitor.mobile.core.ssh.SftpEntry
import com.ccmonitor.mobile.core.ssh.SshConnectionManager
import com.ccmonitor.mobile.core.ui.copy.SizeFormat
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ssh.dialectFor
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf
import java.io.File
import java.util.UUID

/** 当前打开的对话框（长按菜单/重命名/删除/新建目录/预览）。 */
private sealed interface SftpDialog {
    data class Menu(
        val entry: SftpEntry,
    ) : SftpDialog

    data class Rename(
        val entry: SftpEntry,
    ) : SftpDialog

    data class Delete(
        val entry: SftpEntry,
    ) : SftpDialog

    data object Mkdir : SftpDialog

    data class Preview(
        val name: String,
        val content: String,
    ) : SftpDialog

    /** 小文件编辑（content=可编辑全量文本，已过 sftpReadTextForEdit 防护）。 */
    data class Edit(
        val name: String,
        val path: String,
        val content: String,
    ) : SftpDialog

    /** 覆写前确认（text=待写的编辑结果）。 */
    data class ConfirmSave(
        val name: String,
        val path: String,
        val text: String,
    ) : SftpDialog
}

/**
 * SFTP 文件管理：浏览、上传下载、书签、在此打开终端、重命名/删除/新建目录、预览与小文件编辑、面包屑、排序。
 * 与终端共享 [SshConnectionManager] 里同一条主机连接，无则即时连。
 */
@Composable
fun SftpScreen(
    hostId: String,
    onOpenTerminal: (String) -> Unit = {},
    onShowTabs: () -> Unit = {}, // 工具条按钮唤出 tab 面板（宿主在 HostedSession）
) = SftpContent(hostId, onOpenTerminal, onShowTabs)

@Composable
private fun SftpContent(
    hostId: String,
    onOpenTerminal: (String) -> Unit,
    onShowTabs: () -> Unit,
) {
    // 连接编排与浏览状态在 [SftpViewModel]；这里只渲染并处理 Android 侧（URI/文件/Toast/瞬态对话框）。
    val vm = koinViewModel<SftpViewModel> { parametersOf(hostId) }
    val context = LocalContext.current

    val ui by vm.ui.collectAsState()
    val path by vm.path.collectAsState()
    val entries by vm.entries.collectAsState()
    val busy by vm.busy.collectAsState()
    val sortBySize by vm.sortBySize.collectAsState()
    val hostOs by vm.hostOs.collectAsState() // 远端 OS（「终端」按钮 sftpCwdToShell 归一）

    var dialog by remember { mutableStateOf<SftpDialog?>(null) } // 对话框是瞬态 UI，留在 Composable

    // VM 的一次性提示 → Toast。
    LaunchedEffect(vm) { vm.events.collect { Toast.makeText(context, it, Toast.LENGTH_LONG).show() } }

    val actions = sftpEntryActions(vm) { dialog = it }
    val uploadLauncher = rememberSftpUploadLauncher(vm)
    val sortedEntries = rememberSortedEntries(entries, sortBySize)

    Scaffold(
        floatingActionButton = {
            if (ui is SftpUi.Ready) {
                ExtendedFloatingActionButton(text = { Text("上传") }, icon = {}, onClick = { uploadLauncher.launch("*/*") })
            }
        },
    ) { inner ->
        Box(Modifier.fillMaxSize().padding(inner).statusBarsPadding()) {
            when (val s = ui) {
                is SftpUi.Connecting -> Box(Modifier.fillMaxSize(), Alignment.Center) { CircularProgressIndicator() }
                is SftpUi.Error ->
                    Box(Modifier.fillMaxSize().padding(24.dp), Alignment.Center) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            Text("SFTP 错误: ${s.message}", color = MaterialTheme.colorScheme.error)
                            TextButton(onClick = { vm.connect() }) { Text("重试") }
                        }
                    }
                is SftpUi.Ready ->
                    SftpReadyContent(
                        vm = vm,
                        hostId = hostId,
                        path = path,
                        busy = busy,
                        entries = sortedEntries,
                        sortBySize = sortBySize,
                        hostOs = hostOs,
                        onOpenTerminal = onOpenTerminal,
                        onShowTabs = onShowTabs,
                        onDialog = { dialog = it },
                        actions = actions,
                    )
            }
        }
    }

    SftpDialogHost(dialog = dialog, vm = vm, actions = actions, onDialog = { dialog = it })
}

/** 条目级动作（下载/预览/编辑载入）。 */
private class SftpEntryActions(
    val download: (SftpEntry) -> Unit,
    val preview: (SftpEntry) -> Unit,
    val startEdit: (SftpEntry) -> Unit,
)

/** [SftpEntryActions] 工厂：自持 context/scope；[onDialog] 弹预览/编辑对话框。 */
@Composable
private fun sftpEntryActions(
    vm: SftpViewModel,
    onDialog: (SftpDialog) -> Unit,
): SftpEntryActions {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    // remember(vm) 给出稳定实例；否则每次重组新建 SftpEntryActions（含 3 个 lambda），
    // 作为 unstable 参数削弱 SftpReadyContent/SftpDialogHost 的 skippable。
    return remember(vm) {
        SftpEntryActions(
            download = { e -> vm.download(e, context.getExternalFilesDir(null)) },
            preview = { e ->
                scope.launch { vm.readPreview(e.path).getOrNull()?.let { onDialog(SftpDialog.Preview(e.name, it)) } }
            },
            // sftpReadTextForEdit 对过大/二进制返回 null，不进编辑，防写坏。
            startEdit = { e ->
                scope.launch {
                    vm.readForEdit(e.path).onSuccess { content ->
                        if (content == null) {
                            Toast.makeText(context, "文件过大或非文本，不可编辑", Toast.LENGTH_LONG).show()
                        } else {
                            onDialog(SftpDialog.Edit(e.name, e.path, content))
                        }
                    }
                }
            },
        )
    }
}

/** 系统文件选择器 → 临时文件 → VM 上传。 */
@Composable
private fun rememberSftpUploadLauncher(vm: SftpViewModel): ManagedActivityResultLauncher<String, Uri?> {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    return rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        scope.launch {
            // Android 侧：从 URI 备好临时文件（需 contentResolver/cacheDir），再交 VM 上传（上传后删临时文件）。
            val name = queryDisplayName(context, uri) ?: "upload.bin"
            val tmp = File.createTempFile("up_", "_$name", context.cacheDir)
            runCatching {
                withContext(Dispatchers.IO) {
                    context.contentResolver.openInputStream(uri)!!.use { input -> tmp.outputStream().use { input.copyTo(it) } }
                }
            }.onSuccess { vm.upload(tmp, name) }
                .onFailure {
                    tmp.delete()
                    Toast.makeText(context, "上传失败: ${it.message}", Toast.LENGTH_LONG).show()
                }
        }
    }
}

/** 列表排序视图：大小排序时目录仍在前；名称排序沿用 sftpList 的顺序。 */
@Composable
private fun rememberSortedEntries(
    entries: List<SftpEntry>,
    sortBySize: Boolean,
): List<SftpEntry> =
    remember(entries, sortBySize) {
        if (sortBySize) {
            entries.sortedWith(compareByDescending<SftpEntry> { it.isDir }.thenByDescending { it.size })
        } else {
            entries // sftpList 已按目录在前+名称排
        }
    }

/** Ready 主体：路径头/工具条/书签条/条目列表。 */
@Composable
private fun SftpReadyContent(
    vm: SftpViewModel,
    hostId: String,
    path: String,
    busy: Boolean,
    entries: List<SftpEntry>,
    sortBySize: Boolean,
    hostOs: String?,
    onOpenTerminal: (String) -> Unit,
    onShowTabs: () -> Unit,
    onDialog: (SftpDialog) -> Unit,
    actions: SftpEntryActions,
) {
    // 本机目录书签：星标与横条共用一份订阅。
    val bookmarkRepo = koinInject<BookmarkRepository>()
    val snackbar = LocalAppSnackbar.current // 操作反馈 + 撤销
    val bookmarks by bookmarkRepo.observeForHost(hostId).collectAsState(initial = emptyList())
    val scope = rememberCoroutineScope()
    val tokens = LocalAppTokens.current
    Column(Modifier.fillMaxSize()) {
        val bookmarked = bookmarks.firstOrNull { it.path == path }
        SftpPathHeader(
            path = path,
            bookmarked = bookmarked,
            onNavigate = { vm.navigateTo(it) },
            onToggleBookmark = { toggleBookmark(scope, bookmarkRepo, hostId, path, bookmarked) },
        )
        // 工具条：tab 面板 / 新建目录 / 排序 / 在此打开终端。
        Row(
            Modifier.fillMaxWidth().padding(horizontal = tokens.spacing.sm),
            horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            TextButton(onClick = onShowTabs) { Text("▤ tab") } // ▤ 是允许的符号，不是 emoji
            TextButton(onClick = { onDialog(SftpDialog.Mkdir) }) { Text("＋ 新建目录") }
            TextButton(onClick = { vm.toggleSort() }) { Text(if (sortBySize) "排序: 大小" else "排序: 名称") }
            // SFTP 路径归一成 shell cd 路径（Windows /C:/x→C:/x；POSIX 原样）。
            TextButton(onClick = { onOpenTerminal(dialectFor(hostOs).sftpCwdToShell(path)) }) { Text("终端") }
        }
        // 书签横条（点跳转 / 长按删）
        if (bookmarks.isNotEmpty()) {
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = tokens.spacing.md, vertical = tokens.spacing.sm),
                horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                bookmarks.forEach { bm ->
                    // 书签删除是本地、可逆的，所以直接做并给撤销，不加确认框。
                    BookmarkChip(
                        bm = bm,
                        onJump = { vm.navigateTo(bm.path) },
                        onDelete = {
                            scope.launch {
                                bookmarkRepo.delete(bm)
                                snackbar.showUndo("已删除书签「${bm.label.ifBlank { bm.path }}」") {
                                    bookmarkRepo.save(bm) // 调用方自己持有被删的那条，控件不替谁记东西
                                }
                            }
                        },
                    )
                }
            }
        }
        if (busy) LinearProgressIndicator(Modifier.fillMaxWidth()) // 操作/加载进度（mkdir/rename/delete/preview/上传/切目录）
        LazyColumn(Modifier.fillMaxSize()) {
            items(entries, key = { it.path }) { e ->
                SftpRow(
                    e,
                    onClick = { if (e.isDir) vm.navigateTo(e.path) else actions.download(e) },
                    onLongClick = { onDialog(SftpDialog.Menu(e)) }, // 长按操作菜单
                )
            }
        }
    }
}

/** 面包屑路径头（段可点跳祖先）+ 上级 + 书签星标。 */
@Composable
private fun SftpPathHeader(
    path: String,
    bookmarked: Bookmark?,
    onNavigate: (String) -> Unit,
    onToggleBookmark: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier.fillMaxWidth().background(tokens.surfaceRaised).padding(horizontal = tokens.spacing.md, vertical = tokens.spacing.sm),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        TextButton(onClick = { onNavigate(parentOf(path)) }) { Text("⬆") }
        Row(Modifier.weight(1f).horizontalScroll(rememberScrollState()), verticalAlignment = Alignment.CenterVertically) {
            buildCrumbs(path).forEachIndexed { i, (label, full) ->
                if (i > 0) Text("/", style = monoSmall, color = tokens.textFaint)
                Text(
                    label,
                    style = monoSmall,
                    color = if (full == path) MaterialTheme.colorScheme.onSurface else tokens.textFaint,
                    modifier = Modifier.clickable { onNavigate(full) }.padding(horizontal = 2.dp, vertical = 4.dp),
                )
            }
        }
        TextButton(onClick = onToggleBookmark) { Text(if (bookmarked != null) "★" else "☆") }
    }
}

/** 书签星标切换：已存在→删；不存在→存当前路径。 */
private fun toggleBookmark(
    scope: CoroutineScope,
    repo: BookmarkRepository,
    hostId: String,
    path: String,
    bookmarked: Bookmark?,
) {
    scope.launch {
        if (bookmarked != null) {
            repo.delete(bookmarked)
        } else {
            repo.save(Bookmark(id = "bm-" + UUID.randomUUID(), hostId = hostId, path = path, label = bookmarkLabel(path)))
        }
    }
}

/** 对话框分派。 */
@Composable
private fun SftpDialogHost(
    dialog: SftpDialog?,
    vm: SftpViewModel,
    actions: SftpEntryActions,
    onDialog: (SftpDialog?) -> Unit,
) {
    val scope = rememberCoroutineScope()
    when (val d = dialog) {
        null -> {}
        is SftpDialog.Menu -> EntryMenuDialog(d.entry, actions, onDialog)
        is SftpDialog.Rename ->
            InputDialog(
                title = "重命名",
                initial = d.entry.name,
                confirm = "重命名",
                onConfirm = { name ->
                    vm.rename(d.entry.path, name)
                    onDialog(null)
                },
                onDismiss = { onDialog(null) },
            )
        is SftpDialog.Mkdir ->
            InputDialog(
                title = "新建目录",
                initial = "",
                confirm = "创建",
                onConfirm = { name ->
                    vm.mkdir(name)
                    onDialog(null)
                },
                onDismiss = { onDialog(null) },
            )
        is SftpDialog.Delete -> DeleteEntryDialog(d.entry, onConfirm = { vm.delete(d.entry) }, onDialog = onDialog)
        is SftpDialog.Preview -> PreviewDialog(d.name, d.content, onDismiss = { onDialog(null) })
        is SftpDialog.Edit -> EditFileDialog(d, onDialog)
        is SftpDialog.ConfirmSave ->
            ConfirmSaveDialog(
                d = d,
                // 成功才关对话框；失败保留 ConfirmSave，可「返回编辑」保住已改全文。
                onConfirm = { scope.launch { vm.saveText(d.path, d.text).onSuccess { onDialog(null) } } },
                onDialog = onDialog,
            )
    }
}

/** 长按菜单接线：先关菜单再执行动作；重命名/删除转下一对话框状态。 */
@Composable
private fun EntryMenuDialog(
    entry: SftpEntry,
    actions: SftpEntryActions,
    onDialog: (SftpDialog?) -> Unit,
) {
    ActionMenuDialog(
        entry = entry,
        onPreview = {
            onDialog(null)
            actions.preview(entry)
        },
        onEdit = {
            onDialog(null)
            actions.startEdit(entry)
        },
        onDownload = {
            onDialog(null)
            actions.download(entry)
        },
        onRename = { onDialog(SftpDialog.Rename(entry)) },
        onDelete = { onDialog(SftpDialog.Delete(entry)) },
        onDismiss = { onDialog(null) },
    )
}

/** 删除二次确认。 */
@Composable
private fun DeleteEntryDialog(
    entry: SftpEntry,
    onConfirm: () -> Unit,
    onDialog: (SftpDialog?) -> Unit,
) {
    AlertDialog(
        onDismissRequest = { onDialog(null) },
        title = { Text("删除") },
        text = { Text("确定删除${if (entry.isDir) "目录" else "文件"} “${entry.name}”？${if (entry.isDir) "（仅限空目录）" else ""}") },
        confirmButton = {
            TextButton(onClick = {
                onConfirm()
                onDialog(null)
            }) {
                Text("删除", color = MaterialTheme.colorScheme.error)
            }
        },
        dismissButton = { TextButton(onClick = { onDialog(null) }) { Text("取消") } },
    )
}

/** 只读预览。 */
@Composable
private fun PreviewDialog(
    name: String,
    content: String,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        text = {
            Text(
                content.ifEmpty { "（空文件）" },
                style = monoSmall,
                modifier = Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState()),
            )
        },
        confirmButton = { TextButton(onClick = onDismiss) { Text("关闭") } },
    )
}

/** 可编辑文本（live 状态）。保存先进覆盖前确认，不丢改动。 */
@Composable
private fun EditFileDialog(
    d: SftpDialog.Edit,
    onDialog: (SftpDialog?) -> Unit,
) {
    var text by remember(d) { mutableStateOf(d.content) }
    AlertDialog(
        onDismissRequest = { onDialog(null) },
        title = { Text("编辑 ${d.name}", maxLines = 1, overflow = TextOverflow.Ellipsis) },
        text = {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                textStyle = monoSmall,
                modifier = Modifier.fillMaxWidth().heightIn(min = 200.dp, max = 420.dp),
            )
        },
        confirmButton = { TextButton(onClick = { onDialog(SftpDialog.ConfirmSave(d.name, d.path, text)) }) { Text("保存") } },
        dismissButton = { TextButton(onClick = { onDialog(null) }) { Text("取消") } },
    )
}

/** 覆盖前确认：[onConfirm] 成功才关；失败保留本对话框，可「返回编辑」。 */
@Composable
private fun ConfirmSaveDialog(
    d: SftpDialog.ConfirmSave,
    onConfirm: () -> Unit,
    onDialog: (SftpDialog?) -> Unit,
) {
    AlertDialog(
        onDismissRequest = { onDialog(null) },
        title = { Text("覆盖远端文件？") },
        text = {
            // 写入按字节，所以同时显示 UTF-8 字节数（CJK/emoji 时与字符数差异大）。
            val bytes = d.text.toByteArray(Charsets.UTF_8).size
            Text(
                "将以 UTF-8 覆写 “${d.name}”（${d.text.length} 字符 / $bytes 字节）。此操作不可撤销。",
                style = monoSmall,
            )
        },
        confirmButton = { TextButton(onClick = onConfirm) { Text("确认覆盖", color = MaterialTheme.colorScheme.error) } },
        // 返回编辑（保留改动）。
        dismissButton = { TextButton(onClick = { onDialog(SftpDialog.Edit(d.name, d.path, d.text)) }) { Text("返回编辑") } },
    )
}

/** 长按操作菜单（文件：预览/下载/重命名/删除；目录：重命名/删除）。 */
@Composable
private fun ActionMenuDialog(
    entry: SftpEntry,
    onPreview: () -> Unit,
    onEdit: () -> Unit,
    onDownload: () -> Unit,
    onRename: () -> Unit,
    onDelete: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(entry.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        text = {
            Column {
                if (!entry.isDir) {
                    TextButton(onClick = onPreview) { Text("预览") }
                    TextButton(onClick = onEdit) { Text("编辑") } // 小文件编辑
                    TextButton(onClick = onDownload) { Text("下载") }
                }
                TextButton(onClick = onRename) { Text("重命名") }
                TextButton(onClick = onDelete) { Text("删除", color = MaterialTheme.colorScheme.error) }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

/** 单输入框对话框（重命名/新建目录复用）。空输入禁用确认。 */
@Composable
private fun InputDialog(
    title: String,
    initial: String,
    confirm: String,
    onConfirm: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    var value by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { OutlinedTextField(value, { value = it }, singleLine = true, modifier = Modifier.fillMaxWidth()) },
        confirmButton = {
            TextButton(onClick = { onConfirm(value.trim()) }, enabled = value.trim().isNotEmpty() && !value.contains('/')) {
                Text(confirm)
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun BookmarkChip(
    bm: Bookmark,
    onJump: () -> Unit,
    onDelete: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Text(
        bm.label,
        style = monoSmall,
        color = MaterialTheme.colorScheme.onSurface,
        maxLines = 1,
        modifier =
            Modifier
                .background(tokens.surfaceRaised, RoundedCornerShape(50))
                .combinedClickable(onClick = onJump, onLongClick = onDelete) // 点跳转，长按删
                .padding(horizontal = tokens.spacing.md, vertical = tokens.spacing.sm),
    )
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun SftpRow(
    e: SftpEntry,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier
            .fillMaxWidth()
            .combinedClickable(onClick = onClick, onLongClick = onLongClick) // 长按出菜单
            .padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.md),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(if (e.isDir) "📁" else "📄")
        Text(e.name, modifier = Modifier.weight(1f), maxLines = 1, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurface)
        if (!e.isDir) Text(SizeFormat.text(e.size), style = monoSmall, color = tokens.textFaint)
    }
}

private fun queryDisplayName(
    context: android.content.Context,
    uri: android.net.Uri,
): String? =
    runCatching {
        context.contentResolver.query(uri, null, null, null, null)?.use { c ->
            val idx = c.getColumnIndex(android.provider.OpenableColumns.DISPLAY_NAME)
            if (idx >= 0 && c.moveToFirst()) c.getString(idx) else null
        }
    }.getOrNull()
