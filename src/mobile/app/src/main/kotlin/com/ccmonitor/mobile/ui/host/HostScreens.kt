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
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.agent.agentKindOrDefault
import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.data.db.Endpoint
import com.ccmonitor.mobile.core.data.db.Host
import com.ccmonitor.mobile.core.data.db.Launcher
import com.ccmonitor.mobile.core.data.db.parseAddressLine
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
const val HOSTS_SCREEN_TITLE = "服务器"

/**
 * 钥匙（SSH 密钥）那一屏的标题，以及通往它那颗按钮的字。
 * 叫「钥匙」不叫「身份」，免得与账号身份撞名。
 */
const val IDENTITY_SCREEN_TITLE = "钥匙"

/**
 * 服务器列表。[onConnect] 带上这台服务器的 agent 种类，落地判定（`AppNavHost.onConnectDestination`）
 * 靠它把 Codex 服务器送去只读档，而不是 Claude 的聊天屏。
 * 种类直接从行里的 `Host` 读（同步、零往返），不另查库：关着新界面的那条连接路径不进协程
 * （见 `rememberConnectAction` 的头注），为取种类加一次挂起读会改变它的时序。
 */
@Composable
fun HostListScreen(
    onConnect: (String, AgentKind) -> Unit,
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
                    Text("还没有主机。点 + 添加。", color = tokens.textFaint)
                }
            } else {
                LazyColumn(Modifier.fillMaxSize()) {
                    groups.forEach { (group, groupHosts) ->
                        if (showHeaders) {
                            item(key = "hdr-${group ?: "__none__"}") {
                                Text(
                                    group ?: "未分组",
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
                                onClick = { onConnect(host.id, host.agentKindOrDefault()) },
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
            modifier = Modifier.align(Alignment.BottomEnd).padding(24.dp),
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
            "点选一台主机，将强制新建一个独立会话 tab（不复用已打开的会话）；按返回键取消。",
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
    TextButton(onClick = { show = true }) { Text("导入") }
    if (show) {
        ImportSshConfigDialog(
            onDismiss = { show = false },
            onImport = { text ->
                scope.launch {
                    val n = vm.importSshConfig(text)
                    val msg = if (n > 0) "加进来 $n 台服务器" else "这段字里没认出任何一台服务器"
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
        title = { Text("从别处搬一批服务器过来") },
        text = {
            Column {
                Text(
                    "把另一台电脑上记着的那份服务器清单整段贴进来，一次能加好几台。贴进来之后按名字对上已经存过的钥匙。",
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
        confirmButton = { TextButton(onClick = { onImport(text) }, enabled = text.isNotBlank()) { Text("导入") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
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
        TextButton(onClick = onSessions) { Text("会话") } // 多 tab 会话总览
        ImportSshConfigButton(vm)
        TextButton(onClick = onIdentities) { Text(IDENTITY_SCREEN_TITLE) }
    }
}

/** 主机删除确认文案：点明不可撤销，且不动远端数据。 */
internal fun hostDeleteWarning(label: String): String =
    "删除主机「$label」？此操作不可撤销（仅删本地主机配置，不影响远端数据）。"

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
            title = { Text("删除主机") },
            text = { Text(hostDeleteWarning(host.label)) },
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
            TextButton(onClick = onEdit) { Text("编辑") }
            TextButton(onClick = onFiles) { Text("文件") }
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
    var claudeDir by remember { mutableStateOf("") } // 每主机的 agent 配置目录（Claude ~/.claude / Codex ~/.codex）
    // agent 种类存档案本身；选项行遍历 `AgentProfile.ALL`，加一种 agent 这一页不用改。
    var agentProfile by remember { mutableStateOf(AgentProfile.DEFAULT) }
    // 目录框的标签、占位与说明按「这一档走不走账号门」区分。
    val gatedByAccount = agentProfile.requiresAccountGate
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
                claudeDir = h.claudeDir ?: ""
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
                Text("主机不存在或已被删除", color = MaterialTheme.colorScheme.error)
                TextButton(onClick = onDone) { Text("返回") }
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
        Text(if (hostId == null) "新建主机" else "编辑主机", style = MaterialTheme.typography.titleLarge)
        OutlinedTextField(label, { label = it }, label = { Text("名称") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(host, { host = it }, label = { Text("主机 / IP（主地址）") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(port, { v -> port = v.filter { it.isDigit() } }, label = { Text("端口") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        OutlinedTextField(user, { user = it }, label = { Text("用户名") }, singleLine = true, modifier = Modifier.fillMaxWidth())

        // 多地址：每行一个 host / host:port / [IPv6]:port，连接时与主地址并发竞速，先连通者胜。
        OutlinedTextField(
            extraAddresses,
            { extraAddresses = it },
            label = { Text("额外地址（每行一个，可选）") },
            placeholder = { Text("192.0.2.20\nwan.example.com:2222") },
            minLines = 2,
            modifier = Modifier.fillMaxWidth(),
        )
        if (extraAddresses.isNotBlank()) {
            Text(
                "将并发连接 ${previewEps.size} 个地址：" + previewEps.joinToString(", ") { "${it.host}:${it.port}" },
                style = monoSmall,
                color = tokens.textFaint,
            )
        }

        OutlinedTextField(
            workingDir,
            { workingDir = it },
            label = { Text("默认工作目录（可选）") },
            placeholder = { Text("/home/pi") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(group, { group = it }, label = { Text("分组（可选）") }, singleLine = true, modifier = Modifier.fillMaxWidth())
        // agent 种类：Claude Code（默认）或 Codex CLI，决定会话定位、解析、turn-end、用量、resume 走哪套实现。
        Text("Agent 种类", style = MaterialTheme.typography.labelMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
            // 遍历档案，产品名取档案的 `displayName`，不硬写；加一种 agent 这里自然多一项。
            AgentProfile.ALL.forEach { p ->
                RadioButton(selected = agentProfile.kind == p.kind, onClick = { agentProfile = p })
                Text(p.displayName, style = MaterialTheme.typography.bodyMedium)
            }
        }
        // 这一格只给终端屏的「Claude 阅读」找记录用；聊天屏起会话跟随那台核心的默认号，不读它。
        OutlinedTextField(
            claudeDir,
            { claudeDir = it },
            label = { Text(if (!gatedByAccount) "Codex 配置目录（可选）" else "Claude 账号目录") },
            placeholder = { Text(if (!gatedByAccount) "默认 \${CODEX_HOME:-\$HOME/.codex}" else "/home/用户名/.claude") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )

        // 开关即 useTmux；文案不说「连接」「tmux」。
        SwitchRow("打开就直接进那台上的工作区", useTmux) { useTmux = it }
        SwitchRow("断线自动重连", autoReconnect) { autoReconnect = it }

        // 远端系统：POSIX（Linux/macOS，默认）或 Windows（PowerShell，没有 tmux 与 Claude 阅读面）。
        Text("远端系统", style = MaterialTheme.typography.labelMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
            RadioButton(selected = !isWindows, onClick = { isWindows = false })
            Text("Linux/macOS", style = MaterialTheme.typography.bodyMedium)
            RadioButton(selected = isWindows, onClick = { isWindows = true })
            Text("Windows", style = MaterialTheme.typography.bodyMedium)
        }
        if (isWindows) {
            Text(
                "Windows：走 PowerShell、跳过 tmux；tmux/Claude 阅读面/一键 cc 等在 Windows 下不可用（连接/终端/SFTP 可用）。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }

        // 认证方式：钥匙或密码，二选一。
        Text("认证方式", style = MaterialTheme.typography.labelMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(4.dp), verticalAlignment = Alignment.CenterVertically) {
            RadioButton(selected = !authIsPassword, onClick = { authIsPassword = false })
            Text("身份（密钥）", style = MaterialTheme.typography.bodyMedium)
            RadioButton(selected = authIsPassword, onClick = { authIsPassword = true })
            Text("密码", style = MaterialTheme.typography.bodyMedium)
        }
        if (authIsPassword) {
            OutlinedTextField(
                value = password,
                onValueChange = { password = it },
                label = { Text("密码") },
                visualTransformation = PasswordVisualTransformation(),
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            Text(
                "密码经设备 Keystore 加密后落库，不明文存储。",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        } else {
            identities.forEach { id ->
                Row(
                    Modifier.fillMaxWidth().clickable { authRef = id.id },
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    RadioButton(selected = authRef == id.id, onClick = { authRef = id.id })
                    Text(id.label, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }

        ProxyJumpPicker(
            candidates = allHosts.filter { it.id != existing?.id },
            selected = proxyJumpHostId,
            onSelect = { proxyJumpHostId = it },
        )

        // 本机自定义按钮编辑器入口；只对已保存的主机给，因为要 host id 作专属作用域。
        if (existing != null) {
            TextButton(onClick = onEditButtons) { Text("自定义按钮（本机）") }
        }

        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            TextButton(onClick = onDone) { Text("取消") }
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
                            claudeDir = claudeDir.trim().ifBlank { null },
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
            ) { Text("保存") }
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
    Text("经跳板机（可选，ssh -J）", style = MaterialTheme.typography.labelMedium)
    Row(Modifier.fillMaxWidth().clickable { onSelect(null) }, verticalAlignment = Alignment.CenterVertically) {
        RadioButton(selected = selected == null, onClick = { onSelect(null) })
        Text("无（直连）", style = MaterialTheme.typography.bodyMedium)
    }
    candidates.forEach { j ->
        Row(Modifier.fillMaxWidth().clickable { onSelect(j.id) }, verticalAlignment = Alignment.CenterVertically) {
            RadioButton(selected = selected == j.id, onClick = { onSelect(j.id) })
            Text("${j.label} (${j.host})", style = MaterialTheme.typography.bodyMedium)
        }
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
