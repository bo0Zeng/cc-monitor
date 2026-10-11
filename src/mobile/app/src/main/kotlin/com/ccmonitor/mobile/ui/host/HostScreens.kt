package com.ccmonitor.mobile.ui.host

import android.widget.Toast
import androidx.compose.foundation.ExperimentalFoundationApi
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
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.selection.selectable
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.FloatingActionButton
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Switch
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
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.Endpoint
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.Launcher
import com.ccmonitor.mobile.core.data.db.parseAddressLine
import com.ccmonitor.mobile.core.ui.feedback.clearOfNotice
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ssh.dialectFor
import com.ccmonitor.mobile.ui.nav.settingsGroupScreenInsets
import kotlinx.coroutines.launch
import org.koin.androidx.compose.koinViewModel
import java.util.UUID

/**
 * 服务器列表那一屏的标题。没有机器时冷启动落在这一屏，它是新用户看到的第一屏。
 * 「SSH 主机」在界面上一律叫「服务器」。
 */
val HOSTS_SCREEN_TITLE: String get() = copyText("mobHosts.page.title")

/**
 * 钥匙（SSH 密钥）那一屏的标题，以及通往它那颗按钮的字。
 * 叫「钥匙」不叫「身份」，免得与账号身份撞名。
 */
val IDENTITY_SCREEN_TITLE: String get() = copyText("mobIdentity.page.title")

/**
 * 服务器列表。[onConnect] 进这台上空的新对话（`AppNavHost.freshLanding`，哪一家都一样）。
 */
@Composable
fun HostListScreen(
    onConnect: (String) -> Unit,
    onAddHost: () -> Unit,
    onIdentities: () -> Unit,
    onFiles: (String) -> Unit = {},
    onEdit: (String) -> Unit = {},
    onLaunch: (String, String) -> Unit = { _, _ -> }, // (hostId, launcherId) 一键启动
    onSessions: () -> Unit = {}, // 会话总览（多 tab）
    // 「新建会话 · 选主机」页复用本屏：true 时显示对应的标题与提示。连接语义（forceNew）由调用方接，
    // 本屏不感知；返回键取消由导航自然提供。
    newSessionMode: Boolean = false,
    title: String = HOSTS_SCREEN_TITLE,
) {
    val vm = koinViewModel<HostViewModel>()
    val hosts by vm.hosts.collectAsState()
    val launchersByHost by vm.launchersByHost.collectAsState()
    val tokens = LocalAppTokens.current

    // 按 groupId 分组：命名组按名排序在前，未分组的放最后。
    val groups = remember(hosts) { groupHosts(hosts) }
    val showHeaders = groups.any { it.first != null }

    // `MainActivity` 开着 `enableEdgeToEdge()`，不垫 inset 标题会压在状态栏下、FAB 压在手势条上。
    // 垫在最外层 Box 上，FAB 的 `BottomEnd` 也跟着让开。
    Box(Modifier.fillMaxSize().settingsGroupScreenInsets()) {
        Column(Modifier.fillMaxSize()) {
            HostListHeader(vm, title, newSessionMode, onIdentities, onSessions)
            if (hosts.isEmpty()) {
                Box(Modifier.fillMaxSize(), Alignment.Center) {
                    Text(copyText("mobHosts.list.empty"), color = tokens.textFaint)
                }
            } else {
                LazyColumn(Modifier.fillMaxSize()) {
                    groups.forEach { (group, groupHosts) ->
                        if (showHeaders) {
                            item(key = "hdr-${group ?: "__none__"}") {
                                Text(
                                    group ?: copyText("mobHosts.list.ungrouped"),
                                    style = MaterialTheme.typography.labelMedium,
                                    color = tokens.textFaint,
                                    modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp),
                                )
                            }
                        }
                        items(groupHosts, key = { it.id }) { host ->
                            HostRow(
                                host,
                                launchers = launchersByHost[host.id].orEmpty(),
                                // 种类跟着 hostId 一起交出去，见本屏头注。
                                onClick = { onConnect(host.id) },
                                onLaunch = { launcherId -> onLaunch(host.id, launcherId) },
                                onFiles = { onFiles(host.id) },
                                onEdit = { onEdit(host.id) },
                                onDelete = { vm.deleteHost(host) },
                            )
                            HorizontalDivider(color = MaterialTheme.colorScheme.outlineVariant)
                        }
                    }
                }
            }
        }
        FloatingActionButton(
            onClick = onAddHost,
            modifier = Modifier.align(Alignment.BottomEnd).clearOfNotice().padding(24.dp),
        ) { Text("+", style = MaterialTheme.typography.titleLarge) }
    }
}

/** 列表头部：标题、动作行，以及新建会话模式下的提示。 */
@Composable
private fun HostListHeader(
    vm: HostViewModel,
    title: String,
    newSessionMode: Boolean,
    onIdentities: () -> Unit,
    onSessions: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().padding(16.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(title, style = MaterialTheme.typography.titleLarge)
        HostListActions(vm, onIdentities, onSessions)
    }
    // 新建会话模式的提示：点主机即强制新建独立 tab（不复用已开会话），返回键取消。
    if (newSessionMode) {
        Text(
            copyText("mobHosts.newSession.hint"),
            style = MaterialTheme.typography.bodySmall,
            color = LocalAppTokens.current.textFaint,
            modifier = Modifier.fillMaxWidth().padding(horizontal = 16.dp),
        )
    }
}

/** 导入按钮与对话框，状态自管。 */
@Composable
private fun ImportSshConfigButton(vm: HostViewModel) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var show by remember { mutableStateOf(false) }
    TextButton(onClick = { show = true }) { Text(copyText("mobHosts.import.action")) }
    if (show) {
        ImportSshConfigDialog(
            onDismiss = { show = false },
            onImport = { text ->
                scope.launch {
                    val n = vm.importSshConfig(text)
                    val msg = if (n > 0) copyText("mobHosts.import.added", "n" to n) else copyText("mobHosts.import.none")
                    Toast.makeText(context, msg, Toast.LENGTH_SHORT).show()
                    show = false
                }
            },
        )
    }
}

/** 粘贴 ssh config 文本导入主机。 */
@Composable
private fun ImportSshConfigDialog(
    onDismiss: () -> Unit,
    onImport: (String) -> Unit,
) {
    var text by remember { mutableStateOf("") }
    AlertDialog(
        onDismissRequest = onDismiss,
        // 这条路一次填好地址、端口、用户名、钥匙四个字段，多台一起。
        // 文案不提文件名与协议名；也不说「问那台要」，因为实际要手工把那段字粘过来。
        title = { Text(copyText("mobHosts.import.title")) },
        text = {
            Column {
                Text(
                    copyText("mobHosts.import.hint"),
                    style = MaterialTheme.typography.bodySmall,
                )
                OutlinedTextField(
                    value = text,
                    onValueChange = { text = it },
                    modifier = Modifier.fillMaxWidth().heightIn(min = 140.dp, max = 300.dp).padding(top = 8.dp),
                    placeholder = { Text("Host myserver\n  HostName 1.2.3.4\n  Port 22\n  User pi") },
                )
            }
        },
        confirmButton = { TextButton(onClick = { onImport(text) }, enabled = text.isNotBlank()) { Text(copyText("mobHosts.import.ok")) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(copyText("kit.dialog.cancel")) } },
    )
}

/** 按 groupId 分组：命名组按名排序在前，未分组的放最后。 */
private fun groupHosts(hosts: List<Host>): List<Pair<String?, List<Host>>> {
    val byGroup = hosts.groupBy { it.groupId?.trim()?.ifBlank { null } }
    return buildList {
        byGroup.keys
            .filterNotNull()
            .sorted()
            .forEach { add(it to byGroup.getValue(it)) }
        byGroup[null]?.let { add(null to it) }
    }
}

/** 主机列表头部动作（导入 ssh config / 钥匙）。设置从抽屉底行 ⚙ 进，这里不另给。 */
@Composable
private fun HostListActions(
    vm: HostViewModel,
    onIdentities: () -> Unit,
    onSessions: () -> Unit,
) {
    Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
        TextButton(onClick = onSessions) { Text(copyText("mobHosts.list.sessions")) } // 多 tab 会话总览
        ImportSshConfigButton(vm)
        TextButton(onClick = onIdentities) { Text(IDENTITY_SCREEN_TITLE) }
    }
}

/** 主机删除确认文案：点明不可撤销，且不动远端数据。 */
internal fun hostDeleteWarning(label: String): String =
    copyText("mobHosts.delete.body", "label" to label)

@OptIn(ExperimentalFoundationApi::class) // combinedClickable：点行连接，长按删除，与 SftpScreen 的「点跳转，长按删」一致
@Composable
private fun HostRow(
    host: Host,
    launchers: List<Launcher>,
    onClick: () -> Unit,
    onLaunch: (String) -> Unit,
    onFiles: () -> Unit,
    onEdit: () -> Unit,
    onDelete: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    var showDeleteConfirm by remember { mutableStateOf(false) } // 删除二次确认
    if (showDeleteConfirm) {
        AlertDialog(
            onDismissRequest = { showDeleteConfirm = false },
            title = { Text(copyText("mobHosts.delete.title")) },
            text = { Text(hostDeleteWarning(host.label)) },
            confirmButton = {
                TextButton(onClick = {
                    showDeleteConfirm = false
                    onDelete()
                }) { Text(copyText("mobHosts.delete.ok"), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { showDeleteConfirm = false }) { Text(copyText("kit.dialog.cancel")) } },
        )
    }
    Column(
        Modifier
            .fillMaxWidth()
            // 点行按默认方式连接（无 launcher）；长按弹删除确认。删除不放动作栏，免得与 编辑/文件 并排误触。
            .combinedClickable(onClick = onClick, onLongClick = { showDeleteConfirm = true })
            .padding(horizontal = 16.dp, vertical = 14.dp),
    ) {
        Row(
            Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.spacedBy(4.dp),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Column(Modifier.weight(1f).padding(end = 8.dp)) {
                Text(host.label, style = MaterialTheme.typography.bodyMedium)
                // 主地址；有额外地址时标出数量
                val extraCount = host.extraAddresses?.lineSequence()?.count { it.isNotBlank() } ?: 0
                Text(
                    "${host.username}@${host.host}:${host.port}" + if (extraCount > 0) "  +$extraCount" else "",
                    style = monoSmall,
                    color = tokens.textFaint,
                )
            }
            TextButton(onClick = onEdit) { Text(copyText("mobHosts.row.edit")) }
            TextButton(onClick = onFiles) { Text(copyText("mobHosts.row.files")) }
            // 删除由长按主机行触发（见上方 combinedClickable）。
        }
        // 启动配置 chips（cc/cct/项目）：点 chip 即带 launcher 连接（一键 cd、tmux、跑命令）。
        // 能力位门控：launcher 要跑 tmux/claude，`supportsTmux` 为 false（如 Windows/PS）时隐藏。
        if (launchers.isNotEmpty() && dialectFor(host.os).supportsTmux) {
            Row(
                Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(top = 6.dp),
                horizontalArrangement = Arrangement.spacedBy(6.dp),
            ) {
                launchers.forEach { l ->
                    AssistChip(onClick = { onLaunch(l.id) }, label = { Text("▶ ${l.label}", style = monoSmall) })
                }
            }
        }
    }
}

private enum class HostEditLoad { Loading, Loaded, NotFound }

/**
 * 主机编辑器。[hostId] 非空是编辑（载入回显，保留 id 等不可编辑字段），空是新建。
 */
@Composable
fun HostEditorScreen(
    hostId: String?,
    onDone: () -> Unit,
    onEditButtons: () -> Unit = {}, // 进本机按钮编辑器（只对已保存的主机可用）
    // 新加的一台写进库之后调用（加完进这台上空的新对话）。null 则保存后照 onDone 退回。只对新建生效。
    onAdded: ((Host) -> Unit)? = null,
) {
    val vm = koinViewModel<HostViewModel>()
    val saveScope = rememberCoroutineScope()
    val identities by vm.identities.collectAsState()
    val allHosts by vm.hosts.collectAsState() // 跳板机候选（排除自身）
    val tokens = LocalAppTokens.current

    var existing by remember { mutableStateOf<Host?>(null) }
    var label by remember { mutableStateOf("") }
    var host by remember { mutableStateOf("") }
    var port by remember { mutableStateOf("22") }
    var user by remember { mutableStateOf("") }
    var authRef by remember { mutableStateOf<String?>(null) }
    var authIsPassword by remember { mutableStateOf(false) } // 认证方式：钥匙 / 密码
    var isWindows by remember { mutableStateOf(false) } // 远端 OS；Windows 走 PowerShell 方言，没有 tmux/Claude
    var password by remember { mutableStateOf("") } // 明文密码，保存时加密
    var extraAddresses by remember { mutableStateOf("") }
    var workingDir by remember { mutableStateOf("") }
    var useTmux by remember { mutableStateOf(false) }
    var autoReconnect by remember { mutableStateOf(true) }
    var group by remember { mutableStateOf("") }
    // agent 种类存档案本身；选项行遍历 `AgentProfile.ALL`，加一种 agent 这一页不用改。
    var agentProfile by remember { mutableStateOf(AgentProfile.DEFAULT) }
    var proxyJumpHostId by remember { mutableStateOf<String?>(null) } // 经跳板机（另一台 Host 的 id）
    // 新建直接 Loaded；编辑先 Loading，载入后转 Loaded 或 NotFound（防首帧空表，也防查无时静默新建）。
    var load by remember { mutableStateOf(if (hostId == null) HostEditLoad.Loaded else HostEditLoad.Loading) }

    // 编辑模式：载入现有主机回显全部字段。
    LaunchedEffect(hostId) {
        if (hostId != null) {
            val h = vm.getHost(hostId)
            if (h == null) {
                load = HostEditLoad.NotFound // 已被删除或 id 对不上，不落回「新建」路径
            } else {
                existing = h
                label = h.label
                host = h.host
                port = h.port.toString()
                user = h.username
                authRef = h.authRef
                authIsPassword = h.passwordEnc != null
                isWindows = h.os.equals("windows", ignoreCase = true)
                if (h.passwordEnc != null) password = vm.hostPassword(h) ?: "" // 回显解密后的密码
                extraAddresses = h.extraAddresses ?: ""
                workingDir = h.defaultWorkingDir ?: ""
                useTmux = h.sessionManager == "tmux"
                autoReconnect = h.autoReconnect
                group = h.groupId ?: ""
                // 存量值转档案，大小写不敏感（见 `ofStoredNameIgnoringCase` 的头注）。
                agentProfile = AgentProfile.ofStoredNameIgnoringCase(h.agentKind)
                proxyJumpHostId = h.proxyJumpHostId
                load = HostEditLoad.Loaded
            }
        }
    }

    if (load == HostEditLoad.Loading) {
        Box(Modifier.fillMaxSize(), Alignment.Center) { CircularProgressIndicator() }
        return
    }
    if (load == HostEditLoad.NotFound) {
        Box(Modifier.fillMaxSize().padding(24.dp), Alignment.Center) {
            Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(8.dp)) {
                Text(copyText("mobHostEdit.load.missing"), color = MaterialTheme.colorScheme.error)
                TextButton(onClick = onDone) { Text(copyText("mobHostEdit.load.back")) }
            }
        }
        return
    }

    val previewPort = port.toIntOrNull() ?: 22
    val previewEps =
        buildList {
            if (host.isNotBlank()) add(Endpoint(host.trim(), previewPort))
            extraAddresses.lineSequence().forEach { line -> parseAddressLine(line, previewPort)?.let(::add) }
        }.distinctBy { "${it.host}:${it.port}" }

    Column(
        Modifier
            .fillMaxSize()
            // inset 垫在滚动之前：键盘弹起时缩的是可滚视口，滚动范围跟着变。这张表很长，键盘弹起后只看得见一小半。
            .settingsGroupScreenInsets()
            .verticalScroll(rememberScrollState())
            .padding(16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        Text(if (hostId == null) copyText("mobHostEdit.page.newTitle") else copyText("mobHostEdit.page.editTitle"), style = MaterialTheme.typography.titleLarge)
        OutlinedTextField(label, { label = it }, label = { Text(copyText("mobHostEdit.field.name")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(host, { host = it }, label = { Text(copyText("mobHostEdit.field.host")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(port, { v -> port = v.filter { it.isDigit() } }, label = { Text(copyText("mobHostEdit.field.port")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(user, { user = it }, label = { Text(copyText("mobHostEdit.field.user")) }, singleLine = true, modifier = Modifier.fillMaxWidth())

        // 多地址：每行一个 host / host:port / [IPv6]:port，连接时与主地址并发竞速，先连通者胜。
        OutlinedTextField(
            extraAddresses,
            { extraAddresses = it },
            label = { Text(copyText("mobHostEdit.field.extra")) },
            placeholder = { Text("192.0.2.20\nwan.example.com:2222") },
            minLines = 2,
            modifier = Modifier.fillMaxWidth(),
        )
        if (extraAddresses.isNotBlank()) {
            Text(
                copyText("mobHostEdit.field.extraPreview", "n" to previewEps.size, "addresses" to previewEps.joinToString(", ") { "${it.host}:${it.port}" }),
                style = monoSmall,
                color = tokens.textFaint,
            )
        }

        OutlinedTextField(
            workingDir,
            { workingDir = it },
            label = { Text(copyText("mobHostEdit.field.workDir")) },
            placeholder = { Text("/home/pi") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(group, { group = it }, label = { Text(copyText("mobHostEdit.field.group")) }, singleLine = true, modifier = Modifier.fillMaxWidth())
        // agent 种类：Claude Code（默认）或 Codex CLI，决定落点、resume 的载荷与定位走哪套。
        Text(copyText("mobHostEdit.agent.title"), style = MaterialTheme.typography.labelMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
            // 遍历档案，产品名取档案的 `displayName`，不硬写；加一种 agent 这里自然多一项。
            AgentProfile.ALL.forEach { p ->
                ChoiceRow(p.displayName, agentProfile.kind == p.kind) { agentProfile = p }
            }
        }
        // 开关即 useTmux；文案不说「连接」「tmux」。
        SwitchRow(copyText("mobHostEdit.switch.workspace"), useTmux) { useTmux = it }
        SwitchRow(copyText("mobHostEdit.switch.reconnect"), autoReconnect) { autoReconnect = it }

        // 远端系统：POSIX（Linux/macOS，默认）或 Windows（PowerShell，没有 tmux）。
        Text(copyText("mobHostEdit.os.title"), style = MaterialTheme.typography.labelMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
            ChoiceRow("Linux/macOS", !isWindows) { isWindows = false }
            ChoiceRow("Windows", isWindows) { isWindows = true }
        }
        if (isWindows) {
            Text(
                copyText("mobHostEdit.os.windowsHint"),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        // 认证方式：钥匙或密码，二选一。
        Text(copyText("mobHostEdit.auth.title"), style = MaterialTheme.typography.labelMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
            ChoiceRow(copyText("mobHostEdit.auth.key"), !authIsPassword) { authIsPassword = false }
            ChoiceRow(copyText("mobHostEdit.auth.password"), authIsPassword) { authIsPassword = true }
        }
        if (authIsPassword) {
            OutlinedTextField(
                value = password,
                onValueChange = { password = it },
                label = { Text(copyText("mobHostEdit.auth.password")) },
                visualTransformation = PasswordVisualTransformation(),
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Text(
                copyText("mobHostEdit.auth.passwordHint"),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            identities.forEach { id ->
                ChoiceRow(id.label, authRef == id.id, Modifier.fillMaxWidth()) { authRef = id.id }
            }
        }

        ProxyJumpPicker(
            candidates = allHosts.filter { it.id != existing?.id },
            selected = proxyJumpHostId,
            onSelect = { proxyJumpHostId = it },
        )

        // 本机自定义按钮编辑器入口；只对已保存的主机给，因为要 host id 作专属作用域。
        if (existing != null) {
            TextButton(onClick = onEditButtons) { Text(copyText("mobHostEdit.buttons.edit")) }
        }

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = onDone) { Text(copyText("kit.dialog.cancel")) }
            Button(
                onClick = {
                    val base = existing ?: Host(id = "host-" + UUID.randomUUID(), label = "", host = "", port = 22, username = "")
                    val chosen =
                        base.copy(
                            label = label.ifBlank { host },
                            host = host.trim(),
                            port = port.toIntOrNull() ?: 22,
                            username = user.trim(),
                            authRef = if (authIsPassword) null else authRef, // 密码认证与 authRef 互斥
                            extraAddresses = extraAddresses.trim().ifBlank { null },
                            defaultWorkingDir = workingDir.trim().ifBlank { null },
                            sessionManager = if (useTmux) "tmux" else null,
                            autoReconnect = autoReconnect,
                            groupId = group.trim().ifBlank { null },
                            proxyJumpHostId = proxyJumpHostId,
                            os = if (isWindows) "windows" else null,
                            // 存 `AgentKind.name`；缺省档存 null。
                            agentKind = agentProfile.kind.name.takeIf { agentProfile != AgentProfile.DEFAULT },
                        )
                    // 密码模式加密进 passwordEnc；钥匙模式清空 passwordEnc（encryptPassword(_, null) 统一处理）。
                    val saving = vm.saveHost(vm.encryptPassword(chosen, if (authIsPassword) password else null))
                    val added = onAdded?.takeIf { existing == null }
                    if (added == null) onDone() else saveScope.launch { saving.join().also { added(chosen) } }
                },
                // 密码模式必须填密码；钥匙模式只走基本校验。
                enabled = host.isNotBlank() && user.isNotBlank() && (!authIsPassword || password.isNotBlank()),
            ) { Text(copyText("mobHostEdit.page.save")) }
        }
    }
}

/** 选一台已存主机作跳板机（ssh -J），或「无（直连）」。 */
@Composable
private fun ProxyJumpPicker(
    candidates: List<Host>,
    selected: String?,
    onSelect: (String?) -> Unit,
) {
    Text(copyText("mobHostEdit.jump.title"), style = MaterialTheme.typography.labelMedium)
    ChoiceRow(copyText("mobHostEdit.jump.none"), selected == null, Modifier.fillMaxWidth()) { onSelect(null) }
    candidates.forEach { j ->
        ChoiceRow("${j.label} (${j.host})", selected == j.id, Modifier.fillMaxWidth()) { onSelect(j.id) }
    }
}

/** 单选里的一项：整行（圆圈 ＋ 字）都能点，按单选的角色报给无障碍；圆圈只画状态，不自己接点击。 */
@Composable
private fun ChoiceRow(
    label: String,
    selected: Boolean,
    modifier: Modifier = Modifier,
    onSelect: () -> Unit,
) {
    Row(
        modifier.selectable(selected = selected, role = Role.RadioButton, onClick = onSelect),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RadioButton(selected = selected, onClick = null)
        Text(label, style = MaterialTheme.typography.bodyMedium)
    }
}

@Composable
private fun SwitchRow(
    label: String,
    checked: Boolean,
    onChange: (Boolean) -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().clickable { onChange(!checked) },
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(label, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f).padding(end = 8.dp))
        Switch(checked = checked, onCheckedChange = onChange)
    }
}
