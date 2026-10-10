package com.ccmonitor.mobile.ui.identity

import android.widget.Toast
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
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
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.isKeystoreBacked
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
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
    val tokens = LocalAppTokens.current
    val context = LocalContext.current

    // 身份生成/导入反馈：成功/失败弹提示，Working 时盖 loading。
    val op by vm.op.collectAsState()
    LaunchedEffect(op) {
        when (val o = op) {
            is IdentityOp.Done -> {
                Toast.makeText(context, o.message, Toast.LENGTH_SHORT).show()
                vm.ackOp()
            }
            is IdentityOp.Failed -> {
                Toast.makeText(context, "失败: ${o.message}", Toast.LENGTH_LONG).show()
                vm.ackOp()
            }
            else -> {}
        }
    }

    // inset 垫在最外层 Box 上：返回键/标题让开状态栏，右下的 FAB 让开手势条。
    Box(Modifier.fillMaxSize().settingsGroupScreenInsets()) {
        Column(Modifier.fillMaxSize()) {
            // 返回键与 `ButtonSettingsScreen` 同一写法。
            TextButton(onClick = onBack, modifier = Modifier.padding(start = 8.dp, top = 8.dp)) {
                Text("← 返回")
            }
            Text(
                text = IDENTITY_SCREEN_TITLE,
                style = MaterialTheme.typography.titleLarge,
                modifier = Modifier.padding(start = 16.dp, end = 16.dp, bottom = 16.dp),
            )
            if (identities.isEmpty()) {
                Box(Modifier.fillMaxSize(), Alignment.Center) {
                    // 文案说差别、不说算法名（下面的选择框同理）。
                    Text("还没有钥匙。点 + 新配一把，或者把别处已有的搬进来。", color = tokens.textFaint)
                }
            } else {
                LazyColumn(Modifier.fillMaxSize()) {
                    items(identities, key = { it.id }) { identity ->
                        IdentityRow(
                            identity = identity,
                            onCopy = { scope.launch { clipboard.copyPlainText(identity.publicKeyOpenSsh) } },
                            onDelete = { vm.delete(identity) },
                        )
                        HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
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

@Composable
private fun BoxScope.AddIdentityButton(vm: IdentityViewModel) {
    var showChooser by remember { mutableStateOf(false) }
    var showGenerate by remember { mutableStateOf(false) }
    var showImport by remember { mutableStateOf(false) }
    var showGenerateEd25519 by remember { mutableStateOf(false) }
    FloatingActionButton(
        onClick = { showChooser = true },
        modifier = Modifier.align(Alignment.BottomEnd).padding(24.dp),
    ) {
        Text("+", style = MaterialTheme.typography.titleLarge)
    }
    if (showChooser) {
        IdentityChooserDialog(
            onDismiss = { showChooser = false },
            onGenerate = {
                showChooser = false
                showGenerate = true
            },
            onGenerateEd25519 = {
                showChooser = false
                showGenerateEd25519 = true
            },
            onImport = {
                showChooser = false
                showImport = true
            },
        )
    }
    if (showGenerate) {
        NewIdentityDialog(
            // 说差别不说算法名：硬件那一档的差别是私钥出不了这台手机。
            title = "新配一把，装在这台手机的保险箱里",
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
            title = "新配一把软件的钥匙（能导出来备份）",
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
    val tokens = LocalAppTokens.current
    var showDeleteConfirm by remember { mutableStateOf(false) } // 删除二次确认（删身份有级联后果）
    if (showDeleteConfirm) {
        AlertDialog(
            onDismissRequest = { showDeleteConfirm = false },
            title = { Text("删除身份") },
            text = { Text(identityDeleteWarning(identity.label, identity.isKeystoreBacked())) },
            confirmButton = {
                TextButton(onClick = {
                    showDeleteConfirm = false
                    onDelete()
                }) { Text("删除", color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { showDeleteConfirm = false }) { Text("取消") } },
        )
    }
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 16.dp, vertical = 12.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(identity.label, style = MaterialTheme.typography.bodyMedium)
            Text(identity.keyType, style = MaterialTheme.typography.labelSmall, color = tokens.textFaint)
        }
        Text(
            text = identity.fingerprintSha256,
            style = monoSmall,
            color = tokens.textFaint,
            maxLines = 1,
            overflow = TextOverflow.Ellipsis,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = onCopy) { Text("复制公钥") }
            TextButton(onClick = { showDeleteConfirm = true }) { Text("删除", color = MaterialTheme.colorScheme.error) }
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
    buildString {
        append("删除身份「$label」？引用它的主机将无法连接。")
        if (keystoreBacked) append("这是硬件密钥（Android Keystore/TEE），私钥将被不可恢复地销毁。")
        append("此操作不可撤销。")
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
                label = { Text("名称") },
                singleLine = true,
            )
        },
        confirmButton = { TextButton(onClick = { onCreate(label) }) { Text("生成") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

@Composable
private fun IdentityChooserDialog(
    onDismiss: () -> Unit,
    onGenerate: () -> Unit,
    onGenerateEd25519: () -> Unit,
    onImport: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        // 上屏叫「钥匙」不叫「身份」：「身份」会与账号身份撞（同 `IDENTITY_SCREEN_TITLE`）。
        title = { Text("添一把钥匙") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(4.dp)) {
                // 说差别不说算法名：硬件那一档私钥出不了这台手机，软件那一档能导出来备份。
                TextButton(onClick = onGenerate) { Text("新配一把，装在这台手机的保险箱里（更安全）") }
                TextButton(onClick = onGenerateEd25519) { Text("新配一把软件的（能导出来备份）") }
                // 格式判别交给解析，不让用户判。
                TextButton(onClick = onImport) { Text("把别处已有的一把搬进来") }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
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
        title = { Text("把别处的一把钥匙搬进来") },
        text = {
            Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                OutlinedTextField(
                    value = label,
                    onValueChange = { label = it },
                    label = { Text("给它起个名字") },
                    singleLine = true,
                )
                OutlinedTextField(
                    value = pem,
                    onValueChange = { pem = it },
                    label = { Text("粘贴那段私钥") },
                    minLines = 4,
                    maxLines = 8,
                )
                // 注意：带口令的私钥解不开。不说的话失败会表现成「贴进去了但进不去那台机器」。
                Text(
                    "带口令保护的那种今天还认不了 —— 先在原来那台机器上把口令去掉。",
                    style = MaterialTheme.typography.bodySmall,
                )
            }
        },
        confirmButton = {
            TextButton(onClick = { onImport(label, pem) }, enabled = pem.isNotBlank()) { Text("搬进来") }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}
