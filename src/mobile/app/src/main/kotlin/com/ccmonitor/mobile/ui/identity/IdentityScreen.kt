package com.ccmonitor.mobile.ui.identity

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
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
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.isKeystoreBacked
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.feedback.Notice
import com.ccmonitor.mobile.core.ui.feedback.clearOfNotice
import com.ccmonitor.mobile.core.ui.icon.BackButton
import com.ccmonitor.mobile.core.ui.icon.IconTextButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.sheet.ActionSheet
import com.ccmonitor.mobile.core.ui.sheet.SheetAction
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ui.copyPlainText
import com.ccmonitor.mobile.ui.host.IDENTITY_SCREEN_TITLE
import com.ccmonitor.mobile.ui.nav.settingsGroupScreenInsets
import kotlinx.coroutines.launch
import org.koin.androidx.compose.koinViewModel

/** 钥匙（SSH 密钥）那一屏。 */
@Composable
fun IdentityScreen(onBack: () -> Unit = {}) {
    val vm = koinViewModel<IdentityViewModel>()
    val identities by vm.identities.collectAsState()
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    val context = LocalContext.current
    val snackbar = LocalAppSnackbar.current

    // 身份生成/导入反馈：成功/失败弹提示，Working 时盖 loading。
    val op by vm.op.collectAsState()
    LaunchedEffect(op) {
        when (val o = op) {
            is IdentityOp.Done -> {
                Toast.makeText(context, o.message, Toast.LENGTH_SHORT).show()
                vm.ackOp()
            }
            is IdentityOp.Failed -> {
                snackbar.show(Notice(copyText("mobIdentity.op.failed"), o.message))
                vm.ackOp()
            }
            else -> {}
        }
    }

    // inset 垫在最外层 Box 上：返回键/标题让开状态栏，右下的 FAB 让开手势条。
    Box(Modifier.fillMaxSize().settingsGroupScreenInsets()) {
        Column(Modifier.fillMaxSize()) {
            // 顶栏：返回图标 ＋ 标题（「← 返回」字换顶栏图标）。
            Row(
                Modifier.fillMaxWidth().heightIn(min = Touch.topBar).padding(start = 4.dp, end = 16.dp),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                BackButton(onBack)
                Text(IDENTITY_SCREEN_TITLE, style = MaterialTheme.typography.titleMedium)
            }
            if (identities.isEmpty()) {
                Box(Modifier.fillMaxSize(), Alignment.Center) {
                    // 文案说差别、不说算法名（下面的选择框同理）。
                    Text(copyText("mobIdentity.list.empty"), color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            } else {
                // 一把一张卡（`--card`、圆角 8）。
                LazyColumn(
                    Modifier.fillMaxSize(),
                    contentPadding = PaddingValues(horizontal = 12.dp, vertical = 8.dp),
                    verticalArrangement = Arrangement.spacedBy(8.dp),
                ) {
                    items(identities, key = { it.id }) { identity ->
                        IdentityRow(
                            identity = identity,
                            onCopy = { scope.launch { clipboard.copyPlainText(identity.publicKeyOpenSsh) } },
                            onDelete = { vm.delete(identity) },
                        )
                    }
                }
            }
        }
        AddIdentityButton(vm)
        if (op is IdentityOp.Working) {
            Box(Modifier.fillMaxSize(), Alignment.Center) { CircularProgressIndicator() } // 生成/导入进行中
        }
    }
}

/** 右下的悬浮「+」：这一屏唯一的主动作（`--accent-strong`、圆角 10），无障碍名照文案表。 */
@Composable
private fun BoxScope.AddIdentityFab(onClick: () -> Unit) {
    FloatingActionButton(
        onClick = onClick,
        modifier =
            Modifier
                .align(Alignment.BottomEnd)
                .clearOfNotice()
                .padding(24.dp)
                .semantics { contentDescription = copyText("mobIdentity.fab.aria") },
        shape = RoundedCornerShape(LocalAppTokens.current.radius.xl),
    ) {
        PhIcon(Ph.Plus, contentDescription = null)
    }
}

@Composable
private fun BoxScope.AddIdentityButton(vm: IdentityViewModel) {
    var showChooser by remember { mutableStateOf(false) }
    var showGenerate by remember { mutableStateOf(false) }
    var showImport by remember { mutableStateOf(false) }
    var showGenerateEd25519 by remember { mutableStateOf(false) }
    AddIdentityFab(onClick = { showChooser = true })
    if (showChooser) {
        IdentityChooserSheet(
            onDismiss = { showChooser = false },
            onGenerate = { showGenerate = true },
            onGenerateEd25519 = { showGenerateEd25519 = true },
            onImport = { showImport = true },
        )
    }
    if (showGenerate) {
        NewIdentityDialog(
            // 说差别不说算法名：硬件那一档的差别是私钥出不了这台手机。
            title = copyText("mobIdentity.new.hardwareTitle"),
            onDismiss = { showGenerate = false },
            onCreate = { label ->
                vm.generateEcdsa(label)
                showGenerate = false
            },
        )
    }
    if (showGenerateEd25519) {
        NewIdentityDialog(
            // 软件那一档的差别是能导出来备份。
            title = copyText("mobIdentity.new.softwareTitle"),
            onDismiss = { showGenerateEd25519 = false },
            onCreate = { label ->
                vm.generateEd25519(label)
                showGenerateEd25519 = false
            },
        )
    }
    if (showImport) {
        ImportPrivateKeyDialog(
            onDismiss = { showImport = false },
            onImport = { label, pem ->
                vm.importPrivateKey(label, pem.trim().encodeToByteArray())
                showImport = false
            },
        )
    }
}

@Composable
private fun IdentityRow(
    identity: Identity,
    onCopy: () -> Unit,
    onDelete: () -> Unit,
) {
    var showDeleteConfirm by remember { mutableStateOf(false) } // 删除二次确认（删身份有级联后果）
    if (showDeleteConfirm) {
        AlertDialog(
            onDismissRequest = { showDeleteConfirm = false },
            title = { Text(copyText("mobIdentity.delete.title")) },
            text = { Text(identityDeleteWarning(identity.label, identity.isKeystoreBacked())) },
            confirmButton = {
                SecondaryButton(danger = true, onClick = {
                    showDeleteConfirm = false
                    onDelete()
                }) { Text(copyText("mobIdentity.delete.ok")) }
            },
            dismissButton = { SecondaryButton(onClick = { showDeleteConfirm = false }) { Text(copyText("kit.dialog.cancel")) } },
        )
    }
    val tokens = LocalAppTokens.current
    Column(
        Modifier
            .fillMaxWidth()
            .background(tokens.card, RoundedCornerShape(tokens.radius.l))
            .padding(start = 16.dp, end = 16.dp, top = 12.dp, bottom = 4.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(12.dp)) {
            PhIcon(Ph.Key, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            Text(identity.label, style = MaterialTheme.typography.titleSmall)
            Text(identity.keyType, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        Text(
            text = identity.fingerprintSha256,
            style = monoSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(start = Touch.iconBar + 12.dp, top = 4.dp),
        )
        Row(Modifier.padding(start = Touch.iconBar), horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            IconTextButton(Ph.Copy, copyText("mobIdentity.row.copyKey"), onClick = onCopy)
            IconTextButton(Ph.Trash, copyText("mobIdentity.row.delete"), onClick = { showDeleteConfirm = true }, danger = true)
        }
    }
}

/**
 * 身份删除确认文案（纯函数）。必须点明级联：引用它的主机将无法连接；
 * 硬件 Keystore 密钥私钥不可恢复销毁。
 */
internal fun identityDeleteWarning(
    label: String,
    keystoreBacked: Boolean,
): String =
    if (keystoreBacked) {
        copyText("mobIdentity.delete.bodyKeystore", "label" to label)
    } else {
        copyText("mobIdentity.delete.body", "label" to label)
    }

@Composable
private fun NewIdentityDialog(
    title: String,
    onDismiss: () -> Unit,
    onCreate: (String) -> Unit,
) {
    var label by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(title) },
        text = {
            OutlinedTextField(
                value = label,
                onValueChange = { label = it },
                label = { Text(copyText("mobIdentity.new.name")) },
                singleLine = true,
            )
        },
        confirmButton = { SecondaryButton(onClick = { onCreate(label) }) { Text(copyText("mobIdentity.new.create")) } },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) } },
    )
}

/** 添一把钥匙：从底部拉起、三种来源。说差别不说算法名；格式判别交给解析，不让用户判。 */
@Composable
private fun IdentityChooserSheet(
    onDismiss: () -> Unit,
    onGenerate: () -> Unit,
    onGenerateEd25519: () -> Unit,
    onImport: () -> Unit,
) {
    // 上屏叫「钥匙」不叫「身份」：「身份」会与账号身份撞（同 `IDENTITY_SCREEN_TITLE`）。
    ActionSheet(
        title = copyText("mobIdentity.add.title"),
        actions =
            listOf(
                SheetAction(Ph.Fingerprint, copyText("mobIdentity.add.hardware"), onClick = onGenerate),
                SheetAction(Ph.Key, copyText("mobIdentity.add.software"), onClick = onGenerateEd25519),
                SheetAction(Ph.UploadSimple, copyText("mobIdentity.add.import"), onClick = onImport),
            ),
        onDismiss = onDismiss,
    )
}

@Composable
private fun ImportPrivateKeyDialog(
    onDismiss: () -> Unit,
    onImport: (label: String, pem: String) -> Unit,
) {
    var label by remember { mutableStateOf("") }
    var pem by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        // 格式判别交给解析，不让用户判 PEM / OpenSSH。
        title = { Text(copyText("mobIdentity.import.title")) },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    value = label,
                    onValueChange = { label = it },
                    label = { Text(copyText("mobIdentity.import.name")) },
                    singleLine = true,
                )
                OutlinedTextField(
                    value = pem,
                    onValueChange = { pem = it },
                    label = { Text(copyText("mobIdentity.import.pem")) },
                    minLines = 4,
                    maxLines = 8,
                )
                // 注意：带口令的私钥解不开。不说的话失败会表现成「贴进去了但进不去那台机器」。
                Text(
                    copyText("mobIdentity.import.noPassphrase"),
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        },
        confirmButton = {
            SecondaryButton(onClick = { onImport(label, pem) }, enabled = pem.isNotBlank()) { Text(copyText("mobIdentity.import.ok")) }
        },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) } },
    )
}
