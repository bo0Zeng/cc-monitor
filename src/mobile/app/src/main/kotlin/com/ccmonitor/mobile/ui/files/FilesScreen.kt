package com.ccmonitor.mobile.ui.files

import android.content.Context
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
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
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.FilesCommitUpload
import com.ccmonitor.mobile.core.claude.link.FilesLs
import com.ccmonitor.mobile.core.claude.link.RemotePath
import com.ccmonitor.mobile.core.copy.SizeFormat
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.Bookmark
import com.ccmonitor.mobile.core.data.repo.BookmarkRepository
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.feedback.clearOfNotice
import com.ccmonitor.mobile.core.ui.icon.IconTextButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ui.common.CopyDetailButton
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.launch
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf
import java.io.IOException
import java.util.UUID

/** 当前打开的对话框。 */
private sealed interface FilesDialog {
    data class Menu(
        val entry: FilesLs.Entry,
    ) : FilesDialog

    data class Rename(
        val entry: FilesLs.Entry,
    ) : FilesDialog

    data class Delete(
        val entry: FilesLs.Entry,
    ) : FilesDialog

    data class Chmod(
        val entry: FilesLs.Entry,
        val now: String?,
    ) : FilesDialog

    data object Mkdir : FilesDialog

    data object Create : FilesDialog

    data class Preview(
        val name: String,
        val content: String,
    ) : FilesDialog

    /** 编辑（[sha256] 是读时那一份的摘要，存回时交回去）。 */
    data class Edit(
        val entry: FilesLs.Entry,
        val content: String,
        val sha256: String,
    ) : FilesDialog

    /** 覆盖前确认（[text] 是改完的全文）。 */
    data class ConfirmSave(
        val edit: Edit,
        val text: String,
    ) : FilesDialog
}

/**
 * 文件屏：浏览 · 上传下载 · 书签 · 在此打开终端 · 改名 / 删除 / 改权限 / 新建 · 预览与编辑 · 面包屑 · 排序。
 * 读写都问那台后端（[FilesViewModel] → [FileOps]）；这里只渲染与 Android 那一侧（选文件 · 存到本机 · 对话框）。
 */
@Composable
fun FilesScreen(
    hostId: String,
    onOpenTerminal: (String) -> Unit = {},
    onShowTabs: () -> Unit = {},
    /** 别处点名要打开的那个路径（那台上的全路径）；没有 ⇒ `null`。一个导航项只开一次（回到这一项不再开）。 */
    openPath: String? = null,
) {
    val vm = koinViewModel<FilesViewModel> { parametersOf(hostId) }
    val snackbar = LocalAppSnackbar.current
    val ui by vm.ui.collectAsState()
    var dialog by remember { mutableStateOf<FilesDialog?>(null) }

    LaunchedEffect(vm) { vm.events.collect { snackbar.show(it) } }
    LaunchedEffect(vm) { vm.opened.collect { (name, text) -> dialog = FilesDialog.Preview(name, text) } }
    var opened by rememberSaveable(openPath) { mutableStateOf(false) }
    LaunchedEffect(openPath) {
        if (openPath != null && !opened) {
            opened = true
            vm.open(openPath)
        }
    }

    val actions = entryActions(vm) { dialog = it }
    val uploadLauncher = rememberUploadLauncher(vm)

    Scaffold(
        floatingActionButton = {
            if (ui is FilesUi.Ready) {
                // 只放字的那一种（带空图标那一种的字不进无障碍树，读屏念不出、冒烟也认不出）。
                ExtendedFloatingActionButton(
                    onClick = { uploadLauncher.launch("*/*") },
                    modifier = Modifier.clearOfNotice(),
                ) { Text(copyText("rsFilewinUpload.label.upload")) }
            }
        },
    ) { inner ->
        Box(Modifier.fillMaxSize().padding(inner).statusBarsPadding()) {
            when (val s = ui) {
                is FilesUi.Connecting -> Box(Modifier.fillMaxSize(), Alignment.Center) { CircularProgressIndicator() }
                is FilesUi.Error ->
                    Box(Modifier.fillMaxSize().padding(24.dp), Alignment.Center) {
                        Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                            ErrorLine(s.problem.text)
                            SecondaryButton(onClick = { vm.connect() }) { Text(copyText("front.act.retry")) }
                            CopyDetailButton(s.problem.text, s.problem.detail)
                        }
                    }
                is FilesUi.Ready -> ReadyContent(vm, hostId, onOpenTerminal, onShowTabs, { dialog = it }, actions)
            }
        }
    }

    DialogHost(dialog = dialog, vm = vm, actions = actions, onDialog = { dialog = it })
    val clash by vm.clash.collectAsState()
    clash?.let { (dir, c) ->
        ConflictSheet(copyText("rsFilewinBatch.verb.upload"), dir, c.said, c.conflicts, FilesCommitUpload.ANSWERS, vm::answer)
    }
}

/** 条目级动作（下载 · 预览 · 编辑 · 改权限要先读的那几件）。 */
private class EntryActions(
    val download: (FilesLs.Entry) -> Unit,
    val preview: (FilesLs.Entry) -> Unit,
    val startEdit: (FilesLs.Entry) -> Unit,
    val startChmod: (FilesLs.Entry) -> Unit,
)

@Composable
private fun entryActions(
    vm: FilesViewModel,
    onDialog: (FilesDialog) -> Unit,
): EntryActions {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    return remember(vm) {
        EntryActions(
            download = { e ->
                vm.download(e, context.getExternalFilesDir(null)) { f ->
                    ParcelFileDescriptor.open(f, ParcelFileDescriptor.MODE_WRITE_ONLY or ParcelFileDescriptor.MODE_CREATE or ParcelFileDescriptor.MODE_TRUNCATE).detachFd()
                }
            },
            preview = { e -> scope.launch { vm.readText(e.path, e.preview).onSuccess { onDialog(FilesDialog.Preview(e.name, it.text)) } } },
            startEdit = { e -> scope.launch { vm.readText(e.path, e.edit).onSuccess { onDialog(FilesDialog.Edit(e, it.text, it.sha256)) } } },
            startChmod = { e -> scope.launch { onDialog(FilesDialog.Chmod(e, vm.modeText(e))) } },
        )
    }
}

/** 系统文件选择器 ⇒ 交 VM 上传（流每次从头开）。 */
@Composable
private fun rememberUploadLauncher(vm: FilesViewModel): ManagedActivityResultLauncher<String, Uri?> {
    val context = LocalContext.current
    return rememberLauncherForActivityResult(ActivityResultContracts.GetContent()) { uri ->
        if (uri == null) return@rememberLauncherForActivityResult
        vm.upload(nameOf(context, uri) ?: uri.lastPathSegment.orEmpty()) {
            context.contentResolver.openFileDescriptor(uri, "r")?.detachFd() ?: throw IOException(uri.toString())
        }
    }
}

@Composable
private fun ReadyContent(
    vm: FilesViewModel,
    hostId: String,
    onOpenTerminal: (String) -> Unit,
    onShowTabs: () -> Unit,
    onDialog: (FilesDialog) -> Unit,
    actions: EntryActions,
) {
    val cwd by vm.cwd.collectAsState()
    val listing by vm.listing.collectAsState()
    val busy by vm.busy.collectAsState()
    val progress by vm.progress.collectAsState()
    val sortBySize by vm.sortBySize.collectAsState()
    val dir = cwd ?: return
    val entries = listing.entries
    val bookmarkRepo = koinInject<BookmarkRepository>()
    val bookmarks by bookmarkRepo.observeForHost(hostId).collectAsState(initial = emptyList())
    val scope = rememberCoroutineScope()
    val tokens = LocalAppTokens.current
    Column(Modifier.fillMaxSize()) {
        // 书签存那一串：名字不是合法 UTF-8 的目录不给星标。
        val bookmarked = dir.text?.let { t -> bookmarks.firstOrNull { it.path == t } }
        PathHeader(
            dir = dir,
            listing = listing,
            bookmarked = bookmarked,
            onNavigate = vm::navigateTo,
            onToggleBookmark = {
                dir.text?.let { toggleBookmark(scope, bookmarkRepo, hostId, it, listing.crumbs.lastOrNull()?.first ?: dir.shown, bookmarked) }
            },
        )
        Toolbar(dir, sortBySize, vm, onOpenTerminal, onShowTabs, onDialog)
        BookmarkBar(bookmarks, vm, bookmarkRepo)
        val p = progress
        when {
            p != null -> LinearProgressIndicator(progress = { p }, modifier = Modifier.fillMaxWidth())
            busy -> LinearProgressIndicator(Modifier.fillMaxWidth())
        }
        if (listing.truncated) {
            Text(
                copyText("mobFiles.list.truncated", "n" to listing.entries.size, "total" to listing.total),
                style = monoSmall,
                color = tokens.textFaint,
                modifier = Modifier.padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.sm),
            )
        }
        LazyColumn(Modifier.fillMaxSize()) {
            items(entries, key = { it.path.hashCode() }) { e ->
                EntryRow(
                    e,
                    onClick = { if (e.isDir) vm.navigateTo(e.path) else actions.download(e) },
                    onLongClick = { onDialog(FilesDialog.Menu(e)) },
                )
            }
        }
    }
}

/** 工具条：tab 面板 · 新建目录 · 新建空文件 · 排序 · 在此打开终端。 */
@Composable
private fun Toolbar(
    dir: RemotePath,
    sortBySize: Boolean,
    vm: FilesViewModel,
    onOpenTerminal: (String) -> Unit,
    onShowTabs: () -> Unit,
    onDialog: (FilesDialog) -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = tokens.spacing.sm),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PhIconButton(Ph.Tabs, copyText("mobTabs.open.aria"), onClick = onShowTabs, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        IconTextButton(Ph.FolderPlus, copyText("rsFilewinWriteops.label.mkdir"), onClick = { onDialog(FilesDialog.Mkdir) })
        IconTextButton(Ph.File, copyText("rsFilewinChrome.command.newFile"), onClick = { onDialog(FilesDialog.Create) })
        SecondaryButton(onClick = vm::toggleSort) { Text(if (sortBySize) copyText("rsFilewinSource.sort.size") else copyText("rsFilewinSource.sort.name")) }
        SecondaryButton(onClick = { onOpenTerminal(dir.shown) }) { Text(copyText("rsFilewinShell.frame.terminal")) }
    }
}

/** 书签横条（点跳转 · 长按删，删了给撤销）。 */
@Composable
private fun BookmarkBar(
    bookmarks: List<Bookmark>,
    vm: FilesViewModel,
    bookmarkRepo: BookmarkRepository,
) {
    val tokens = LocalAppTokens.current
    val snackbar = LocalAppSnackbar.current
    val scope = rememberCoroutineScope()
    if (bookmarks.isNotEmpty()) {
        Row(
            Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = tokens.spacing.md, vertical = tokens.spacing.sm),
            horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            bookmarks.forEach { bm ->
                // 书签删除是本机的、撤得回：直接做并给撤销，不加确认框。
                BookmarkChip(
                    bm = bm,
                    onJump = { vm.navigateTo(RemotePath.of(bm.path)) },
                    onDelete = {
                        scope.launch {
                            bookmarkRepo.delete(bm)
                            snackbar.showUndo(copyText("mobFiles.bookmark.deleted", "label" to bm.label.ifBlank { bm.path })) { bookmarkRepo.save(bm) }
                        }
                    },
                )
            }
        }
    }
}

/** 面包屑（段可点跳祖先）＋ 上级 ＋ 书签星标。 */
@Composable
private fun PathHeader(
    dir: RemotePath,
    /** 上一级与各段（那台切好的）。 */
    listing: FilesLs.Listing,
    bookmarked: Bookmark?,
    onNavigate: (RemotePath) -> Unit,
    onToggleBookmark: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier.fillMaxWidth().background(tokens.bg2).padding(horizontal = tokens.spacing.md, vertical = tokens.spacing.sm),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PhIconButton(
            Ph.ArrowUp,
            copyText("rsFilewinChrome.nav.up"),
            onClick = { listing.up?.let(onNavigate) },
            enabled = listing.up != null,
            tint = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        // 当前目录总在眼前：只排末尾几段，前面的收成「…」（点它到它那一级；再往上用 ⬆）。
        val all = listing.crumbs
        val shown = if (all.size > CRUMBS_SHOWN) listOf("…" to all[all.size - CRUMBS_SHOWN].second) + all.takeLast(CRUMBS_SHOWN - 1) else all
        Row(Modifier.weight(1f).horizontalScroll(rememberScrollState()), verticalAlignment = Alignment.CenterVertically) {
            shown.forEachIndexed { i, (label, upTo) ->
                if (i > 0 && shown[i - 1].first != "/") Text("/", style = monoSmall, color = tokens.textFaint)
                Text(
                    label,
                    style = monoSmall,
                    color = if (upTo == dir) MaterialTheme.colorScheme.onSurface else tokens.textFaint,
                    modifier = Modifier.clickable { onNavigate(upTo) }.padding(horizontal = 2.dp, vertical = 4.dp),
                )
            }
        }
        // 收藏后图标用 `--accent`；Phosphor 的 fill 不另引，靠颜色分。
        if (dir.text != null) {
            PhIconButton(
                Ph.Star,
                if (bookmarked != null) copyText("rsFilewinBookmarks.label.drop") else copyText("rsFilewinBookmarks.label.add"),
                onClick = onToggleBookmark,
                tint = if (bookmarked != null) tokens.accent else MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
    }
}

private fun toggleBookmark(
    scope: CoroutineScope,
    repo: BookmarkRepository,
    hostId: String,
    path: String,
    label: String,
    bookmarked: Bookmark?,
) {
    scope.launch {
        if (bookmarked != null) {
            repo.delete(bookmarked)
        } else {
            repo.save(Bookmark(id = "bm-" + UUID.randomUUID(), hostId = hostId, path = path, label = label))
        }
    }
}

@Composable
private fun DialogHost(
    dialog: FilesDialog?,
    vm: FilesViewModel,
    actions: EntryActions,
    onDialog: (FilesDialog?) -> Unit,
) {
    val scope = rememberCoroutineScope()
    val close = { onDialog(null) }
    when (val d = dialog) {
        null -> {}
        is FilesDialog.Menu -> EntryMenu(d.entry, actions, onDialog)
        is FilesDialog.Rename ->
            NameDialog(copyText("rsFilewinWriteops.label.rename"), d.entry.name, close) {
                vm.rename(d.entry, it)
                close()
            }
        is FilesDialog.Mkdir ->
            NameDialog(copyText("rsFilewinWriteops.label.mkdir"), "", close) {
                vm.mkdir(it)
                close()
            }
        is FilesDialog.Create ->
            NameDialog(copyText("rsFilewinSelect.menu.newFile"), "", close) {
                vm.create(it)
                close()
            }
        is FilesDialog.Delete -> DeleteDialog(d.entry, close) { vm.delete(d.entry) }
        is FilesDialog.Chmod -> ChmodDialog(d, close) { vm.chmod(d.entry, it) }
        is FilesDialog.Preview -> PreviewDialog(d.name, d.content, close)
        is FilesDialog.Edit -> EditDialog(d, onDialog)
        is FilesDialog.ConfirmSave ->
            ConfirmSaveDialog(
                d = d,
                // 做成才关；没成留着本框，可「返回编辑」保住改动。
                onConfirm = { scope.launch { vm.saveText(d.edit.entry, d.text, d.edit.sha256).onSuccess { close() } } },
                onBack = { onDialog(d.edit.copy(content = d.text)) },
                onDismiss = close,
            )
    }
}

@Composable
private fun EntryMenu(
    entry: FilesLs.Entry,
    actions: EntryActions,
    onDialog: (FilesDialog?) -> Unit,
) {
    val then = { act: (FilesLs.Entry) -> Unit ->
        onDialog(null)
        act(entry)
    }
    AlertDialog(
        onDismissRequest = { onDialog(null) },
        title = { Text(entry.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
        text = {
            Column {
                if (!entry.isDir) {
                    if (entry.preview != null) {
                        SecondaryButton(onClick = { then(actions.preview) }) { Text(copyText("rsFilewinWorkspace.label.preview")) }
                    }
                    SecondaryButton(onClick = { then(actions.startEdit) }) { Text(copyText("rsFilewinEditor.label.edit")) }
                    SecondaryButton(onClick = { then(actions.download) }) { Text(copyText("rsFilewinDownload.label.download")) }
                }
                SecondaryButton(onClick = { onDialog(FilesDialog.Rename(entry)) }) { Text(copyText("rsFilewinWriteops.label.rename")) }
                SecondaryButton(onClick = { then(actions.startChmod) }) { Text(copyText("rsFilewinWriteops.label.chmod")) }
                SecondaryButton(onClick = { onDialog(FilesDialog.Delete(entry)) }, danger = true) {
                    Text(copyText("rsFilewinWriteops.label.delete"))
                }
            }
        },
        confirmButton = {},
        dismissButton = { SecondaryButton(onClick = { onDialog(null) }) { Text(copyText("kit.dialog.cancel")) } },
    )
}

/** 删除先问一次（目录连同里面全部）。 */
@Composable
private fun DeleteDialog(
    entry: FilesLs.Entry,
    onDismiss: () -> Unit,
    onConfirm: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(copyText("rsFilewinWriteops.label.delete")) },
        text = {
            Text(
                if (entry.isDir) {
                    copyText("rsFilewinWriteops.op.rmdir", "path" to entry.path.shown)
                } else {
                    copyText("rsFilewinWriteops.op.rm", "path" to entry.path.shown)
                },
            )
        },
        confirmButton = {
            SecondaryButton(
                onClick = {
                    onConfirm()
                    onDismiss()
                },
                danger = true,
            ) { Text(copyText("rsFilewinWriteops.label.delete")) }
        },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) } },
    )
}

/** 改权限：敲八进制（同桌面）；此刻的权限位写在框下。 */
@Composable
private fun ChmodDialog(
    d: FilesDialog.Chmod,
    onDismiss: () -> Unit,
    onConfirm: (Long) -> Unit,
) {
    var value by remember(d) { mutableStateOf(d.now.orEmpty()) }
    val parsed = FileOps.modeOf(value)
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(copyText("rsFilewinWriteops.heading.chmodOne", "name" to d.entry.name), maxLines = 1, overflow = TextOverflow.Ellipsis) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(value, { value = it }, singleLine = true, textStyle = monoSmall, modifier = Modifier.fillMaxWidth())
                val hint =
                    parsed.exceptionOrNull()?.message
                        ?: d.now?.let { copyText("rsFilewinWriteops.mode.now", "mode" to it) }
                        ?: copyText("rsFilewinWriteops.mode.unreadable")
                Text(hint, style = monoSmall, color = LocalAppTokens.current.textFaint)
            }
        },
        confirmButton = {
            SecondaryButton(onClick = {
                parsed.onSuccess {
                    onConfirm(it)
                    onDismiss()
                }
            }, enabled = parsed.isSuccess) { Text(copyText("rsFilewinWriteops.label.chmod")) }
        },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) } },
    )
}

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
                content.ifEmpty { copyText("mobFiles.preview.empty") },
                style = monoSmall,
                modifier = Modifier.heightIn(max = 420.dp).verticalScroll(rememberScrollState()),
            )
        },
        confirmButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("mobKit.act.close")) } },
    )
}

@Composable
private fun EditDialog(
    d: FilesDialog.Edit,
    onDialog: (FilesDialog?) -> Unit,
) {
    var text by remember(d) { mutableStateOf(d.content) }
    AlertDialog(
        onDismissRequest = { onDialog(null) },
        title = { Text(copyText("mobFiles.edit.title", "name" to d.entry.name), maxLines = 1, overflow = TextOverflow.Ellipsis) },
        text = {
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                textStyle = monoSmall,
                modifier = Modifier.fillMaxWidth().heightIn(min = 200.dp, max = 420.dp),
            )
        },
        confirmButton = { SecondaryButton(onClick = { onDialog(FilesDialog.ConfirmSave(d, text)) }) { Text(copyText("rsFilewinShell.editor.save")) } },
        dismissButton = { SecondaryButton(onClick = { onDialog(null) }) { Text(copyText("kit.dialog.cancel")) } },
    )
}

@Composable
private fun ConfirmSaveDialog(
    d: FilesDialog.ConfirmSave,
    onConfirm: () -> Unit,
    onBack: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(copyText("mobFiles.overwrite.title")) },
        text = {
            // 写入按字节：字数与 UTF-8 字节数都写（中文时差得多）。
            val bytes = d.text.toByteArray(Charsets.UTF_8).size
            Text(copyText("mobFiles.overwrite.body", "name" to d.edit.entry.name, "chars" to d.text.length, "bytes" to bytes), style = monoSmall)
        },
        confirmButton = { SecondaryButton(onClick = onConfirm, danger = true) { Text(copyText("mobFiles.overwrite.ok")) } },
        dismissButton = { SecondaryButton(onClick = onBack) { Text(copyText("mobFiles.overwrite.back")) } },
    )
}

/** 一格名字（改名 · 新建）。空的不许确认；合不合法由那台判（不合 ⇒ 那台那一句）。 */
@Composable
private fun NameDialog(
    title: String,
    initial: String,
    onDismiss: () -> Unit,
    onConfirm: (String) -> Unit,
) {
    var value by remember { mutableStateOf(initial) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = { OutlinedTextField(value, { value = it }, singleLine = true, modifier = Modifier.fillMaxWidth()) },
        confirmButton = { SecondaryButton(onClick = { onConfirm(value.trim()) }, enabled = value.isNotBlank()) { Text(title) } },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) } },
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
                .background(tokens.card, RoundedCornerShape(tokens.radius.m))
                .combinedClickable(onClick = onJump, onLongClick = onDelete)
                .padding(horizontal = tokens.spacing.md, vertical = tokens.spacing.sm),
    )
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun EntryRow(
    e: FilesLs.Entry,
    onClick: () -> Unit,
    onLongClick: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Touch.rowFile)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(horizontal = tokens.spacing.lg, vertical = tokens.spacing.md),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PhIcon(if (e.isDir) Ph.FolderSimple else Ph.File, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Column(Modifier.weight(1f)) {
            Text(e.name, maxLines = 1, overflow = TextOverflow.Ellipsis, color = MaterialTheme.colorScheme.onSurface)
            e.mtime?.let { Text(it, style = monoSmall, color = tokens.textFaint) }
        }
        if (!e.isDir) e.size?.let { Text(SizeFormat.text(it), style = monoSmall, color = tokens.textFaint) }
    }
}

/** 选中那一份的显示名（系统给的；没有 ⇒ `null`）。 */
private fun nameOf(
    context: Context,
    uri: Uri,
): String? =
    runCatching {
        context.contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { c ->
            if (c.moveToFirst()) c.getColumnIndex(OpenableColumns.DISPLAY_NAME).takeIf { it >= 0 }?.let(c::getString) else null
        }
    }.getOrNull()

/** 面包屑至多排几段（含「…」那一段）。 */
private const val CRUMBS_SHOWN = 4
