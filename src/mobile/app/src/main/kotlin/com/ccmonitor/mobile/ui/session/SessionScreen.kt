package com.ccmonitor.mobile.ui.session

import android.content.Intent
import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Tab
import androidx.compose.material3.TabRow
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
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
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalSoftwareKeyboardController
import androidx.compose.ui.platform.LocalView
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.data.db.ButtonGroups
import com.ccmonitor.mobile.core.data.db.CustomButton
import com.ccmonitor.mobile.core.data.db.CustomButtonDao
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.ssh.SessionStatus
import com.ccmonitor.mobile.core.ssh.SshConnectEvent
import com.ccmonitor.mobile.core.terminal.AtermTerminal
import com.ccmonitor.mobile.core.terminal.TerminalSession
import com.ccmonitor.mobile.core.ui.feedback.AppSnackbarHostScaffold
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.service.AppForeground
import com.ccmonitor.mobile.service.SshKeepAliveService
import com.ccmonitor.mobile.ssh.SessionBackend
import com.ccmonitor.mobile.ssh.appendAuthorizedKeyCommand
import com.ccmonitor.mobile.ui.claude.ClaudeHistorySheet
import com.ccmonitor.mobile.ui.claude.ClaudeReadingPaneConnected
import com.ccmonitor.mobile.ui.claude.ResumeLauncherOption
import com.ccmonitor.mobile.ui.copyPlainText
import com.ccmonitor.mobile.ui.pastePlainText
import com.ccmonitor.mobile.ui.settings.ButtonSettingsScreen
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf
import kotlin.math.roundToInt

/**
 * 连接指定主机并渲染实时终端。连接编排在 [SessionViewModel]，这里只渲染。
 * 连上后可在终端与 Claude 阅读面之间切换；阅读面走同一连接上的另一条 exec 通道，实时 tail 远端 JSONL。
 */
@Composable
fun SessionScreen(
    hostId: String,
    cd: String? = null,
    launcherId: String? = null,
    tmuxSession: String? = null, // 本 tab 首连即附着的 tmux 会话名；普通 tab 为 null
    resumeSessionId: String? = null, // 本 tab 首连即 resume 的 Claude 会话 id；普通 tab 为 null
    resumeCommand: String? = null, // 历史页长按选定的 resume 命令；null 走默认候选链
    onAttachInNewTab: (name: String) -> Unit = {}, // tmux「附着」：开新终端 tab
    // resume：开新终端 tab。title 是会话可读标题（可 null），cwd 只作兜底（可 null），command 是选定的命令。
    onResumeInNewTab: (cwd: String?, sessionId: String, title: String?, command: String?) -> Unit = { _, _, _, _ -> },
    onShowTabs: () -> Unit = {}, // 唤出 tab 面板
) = SessionContent(hostId, cd, launcherId, tmuxSession, resumeSessionId, resumeCommand, onAttachInNewTab, onResumeInNewTab, onShowTabs)

@Composable
private fun SessionContent(
    hostId: String,
    cd: String?,
    launcherId: String?,
    tmuxSession: String?,
    resumeSessionId: String?,
    resumeCommand: String?,
    onAttachInNewTab: (name: String) -> Unit,
    onResumeInNewTab: (cwd: String?, sessionId: String, title: String?, command: String?) -> Unit,
    onShowTabs: () -> Unit,
) {
    // Koin 参数不能传 null，可空参数一律用空串表示 null。
    val vm =
        koinViewModel<SessionViewModel> {
            parametersOf(hostId, cd ?: "", launcherId ?: "", tmuxSession ?: "", resumeSessionId ?: "", resumeCommand ?: "")
        }
    val state by vm.uiState.collectAsState()
    val context = LocalContext.current

    // 连上后起前台保活服务（要 Context，所以在这里起）；会话归零时服务自停。
    LaunchedEffect(state is SessionUiState.Connected) {
        if (state is SessionUiState.Connected) {
            val app = context.applicationContext
            ContextCompat.startForegroundService(app, Intent(app, SshKeepAliveService::class.java))
        }
    }

    // 一次性提示弹 Toast。
    LaunchedEffect(vm) {
        vm.notices.collect { Toast.makeText(context, it, Toast.LENGTH_LONG).show() }
    }

    // 终端自动重挂：只在本 tab 是活动 tab（只有活动 tab 被组合）且 app 在前台时，若 shell 因连接在后台重建而死
    // （不是用户 exit，判据见 [SessionViewModel.reattachNeeded]），就 retry 重挂回原 tmux 现场。
    // 切走 tab 或进后台不触发；切回来仍需重挂则立刻触发。
    LaunchedEffect(vm) {
        combine(vm.reattachNeeded, AppForeground.foreground) { needed, fg -> needed && fg }
            .distinctUntilChanged()
            .collect { if (it) vm.retry() }
    }

    when (val s = state) {
        is SessionUiState.Connecting -> ConnectingContent(vm, onShowTabs)
        is SessionUiState.NeedIdentity -> NeedIdentityContent(s.identities, onShowTabs) { vm.pickIdentity(it) }
        // 服务器身份变了：唯一一个「拒绝连接但可恢复」的态，单独成屏
        is SessionUiState.HostKeyChanged ->
            HostKeyChangedContent(s, onShowTabs) { vm.trustNewHostKey(s.host, s.port) }
        is SessionUiState.Error -> ErrorContent(s.message, vm, onShowTabs)
        is SessionUiState.Connected ->
            ConnectedContent(
                hostId,
                s,
                vm,
                onReconnect = { vm.retry() },
                onAttachInNewTab = onAttachInNewTab,
                onResumeInNewTab = onResumeInNewTab,
                onShowTabs = onShowTabs,
            )
    }
}

/** 连接失败态：保留连接日志（最需要诊断的时候不清空），可重连。 */
@Composable
private fun ErrorContent(
    message: String,
    vm: SessionViewModel,
    onShowTabs: () -> Unit,
) {
    val log by vm.connectLog.collectAsState()
    Column(
        Modifier.fillMaxSize().statusBarsPadding().padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        SessionStateTabButton(onShowTabs)
        Text("连接失败: $message", color = MaterialTheme.colorScheme.error)
        if (log.isNotEmpty()) ConnectLogView(log, Modifier.fillMaxWidth().weight(1f))
        TextButton(onClick = { vm.retry() }) { Text("重连") }
    }
}

/** 历史页点击要阅读的会话：阅读器据此 tail 该 JSONL，[title] 供下拉与标题。 */
private data class ReadPin(
    val path: String,
    val title: String,
)

/**
 * 非连接态（连接中 / 失败 / 选身份）顶部的 tab 按钮。这些态没有工具条，靠它在慢连、连不上、待选身份时
 * 仍能唤出 tab 面板切换或关闭会话。
 */
@Composable
private fun SessionStateTabButton(onShowTabs: () -> Unit) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Start) {
        TextButton(onClick = onShowTabs) { Text("▤ tab") } // ▤ 是排版符号，不是 emoji
    }
}

/** 主机未绑身份且有多个身份：一步选一个，选中即绑定并连接。 */
@Composable
private fun NeedIdentityContent(
    identities: List<Identity>,
    onShowTabs: () -> Unit,
    onPick: (String) -> Unit,
) {
    Column(
        Modifier.fillMaxSize().statusBarsPadding().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        SessionStateTabButton(onShowTabs)
        Text("选择连接身份", style = MaterialTheme.typography.titleMedium)
        Text("该主机未绑定身份，选一个用于连接（会记住，下次点即连）：", style = MaterialTheme.typography.bodySmall)
        identities.forEach { id ->
            TextButton(onClick = { onPick(id.id) }) { Text(id.label.ifBlank { id.id }) }
        }
    }
}

/**
 * 服务器身份变更的恢复页。
 *
 * 做成一整屏而不是一行红字：这是唯一一个「连接被拒但能做点什么」的失败态，触发它的又常是合法操作
 * （重装系统、重建 VPS、轮换密钥）；删掉服务器重加也没用（`KnownHost` 与 `Host` 没有外键级联）。
 *
 * 首次连接故意静默接受：对不会核对指纹的人，首次弹窗只会训练他闭眼点通过。唯一一次打断留给真正异常的时刻。
 *
 * 正文说「身份」，不说「主机密钥 / 指纹」；详情里给出指纹原文，放在正文之下，供会核对的人用。
 */
@Composable
private fun HostKeyChangedContent(
    state: SessionUiState.HostKeyChanged,
    onShowTabs: () -> Unit,
    onTrustNew: () -> Unit,
) {
    var confirming by remember { mutableStateOf(false) }

    Column(
        Modifier.fillMaxSize().statusBarsPadding().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        SessionStateTabButton(onShowTabs)
        Text("这台服务器的身份变了", style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.error)
        Text(
            "aterm 记得的身份和现在这台对不上。",
            style = MaterialTheme.typography.bodyMedium,
        )
        Text(
            "· 如果刚重装过系统、重建过这台机器，这是正常的\n" +
                "· 如果没有，别继续 —— 可能有人在冒充它",
            style = MaterialTheme.typography.bodySmall,
        )
        // 详情给会核对的人，放在正文之下、小字
        if (state.expectedFingerprint.isNotBlank() || state.offeredFingerprint.isNotBlank()) {
            Text(
                "以前记得的：${state.expectedFingerprint.ifBlank { "（算不出）" }}\n" +
                    "现在这台是：${state.offeredFingerprint.ifBlank { "（算不出）" }}",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        TextButton(onClick = { confirming = true }) { Text("我确认是我自己改的") }
    }

    if (confirming) {
        AlertDialog(
            onDismissRequest = { confirming = false },
            title = { Text("信任新身份？") },
            // 二次确认的文案说清后果，而不是只问「确定吗」。
            text = { Text("aterm 会忘掉旧身份、记住现在这台。如果这台其实不是目标服务器，之后的输入都会发给它。") },
            confirmButton = {
                TextButton(onClick = {
                    confirming = false
                    onTrustNew()
                }) { Text("信任并重连") }
            },
            dismissButton = { TextButton(onClick = { confirming = false }) { Text("取消") } },
        )
    }
}

/** 连接中：转圈，加实时滚动的事件日志，看得出卡在 TCP / 协商 / 认证哪一步。 */
@Composable
private fun ConnectingContent(
    vm: SessionViewModel,
    onShowTabs: () -> Unit,
) {
    val log by vm.connectLog.collectAsState()
    Column(
        Modifier.fillMaxSize().statusBarsPadding().padding(24.dp),
        horizontalAlignment = Alignment.CenterHorizontally,
        verticalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        SessionStateTabButton(onShowTabs)
        CircularProgressIndicator()
        Text("连接中…", style = MaterialTheme.typography.titleMedium)
        ConnectLogView(log, Modifier.fillMaxWidth().weight(1f))
    }
}

/** 连接事件日志的滚动视图，连接中与失败态共用。 */
@Composable
private fun ConnectLogView(
    log: List<SshConnectEvent>,
    modifier: Modifier = Modifier,
) {
    val scroll = rememberScrollState()
    LaunchedEffect(log.size) { scroll.animateScrollTo(scroll.maxValue) } // 新事件到达自动滚到底
    Column(modifier.verticalScroll(scroll), verticalArrangement = Arrangement.spacedBy(2.dp)) {
        log.forEach { ev ->
            Text(
                "${ev.host}:${ev.port}  ${ev.message}",
                color =
                    if (ev.stage == SshConnectEvent.Stage.ERROR) {
                        MaterialTheme.colorScheme.error
                    } else {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    },
                style = MaterialTheme.typography.bodySmall,
                fontFamily = FontFamily.Monospace,
            )
        }
    }
}

/**
 * 终端与 Claude 阅读面切换。两者共用同一连接：终端在 shell 通道，阅读面在另一条 exec 通道。
 * 这里只持跨子树共享的状态（tab、对话框开关、readCwd、抓屏文本）与动作；渲染在 [ConnectedScaffold]
 * 与 [ConnectedDialogs]。
 */
@Composable
private fun ConnectedContent(
    hostId: String,
    s: SessionUiState.Connected,
    vm: SessionViewModel,
    onReconnect: () -> Unit,
    onAttachInNewTab: (name: String) -> Unit, // tmux 附着：开新终端 tab
    // resume：开新终端 tab（cwd 可 null）。
    onResumeInNewTab: (cwd: String?, sessionId: String, title: String?, command: String?) -> Unit,
    onShowTabs: () -> Unit,
) {
    var tab by rememberSaveable { mutableStateOf(0) }
    var showTmux by rememberSaveable { mutableStateOf(false) }
    var showHistory by rememberSaveable { mutableStateOf(false) }
    // 阅读面当前 tail 的工作目录：默认是主机默认目录，从 tmux 会话「读」可改到那个会话的 cwd（不 attach）。
    // 以 s.cwd 为键：重连或 resume 后 cwd 变了就重置；cwd 没变则保留「读」的重定向。
    var readCwd by remember(s.cwd) { mutableStateOf(s.cwd) }
    // 历史页点击钉住的 JSONL，同样以 s.cwd 为键。tmux「读」会清掉它：两条阅读入口互斥，后触发的生效。
    var pinnedRead by remember(s.cwd) { mutableStateOf<ReadPin?>(null) }
    // 抓当前 tmux 会话的可见屏为纯文本，弹窗看并可复制。
    var captureText by remember { mutableStateOf<String?>(null) }
    // 公钥推送：命令面板「推送公钥」触发，选一个身份，把它的 OpenSSH 公钥追加到这台主机的 authorized_keys。
    var showKeyPush by remember { mutableStateOf(false) }
    val onResumeSession = rememberResumeAction(onResumeInNewTab)
    ConnectedScaffold(
        hostId = hostId,
        s = s,
        vm = vm,
        tab = tab,
        onTab = { tab = it },
        readCwd = readCwd,
        pinnedRead = pinnedRead,
        onReconnect = onReconnect,
        onShowTmux = { showTmux = true },
        onShowHistory = { showHistory = true },
        onPushKey = { showKeyPush = true },
        onCaptured = { captureText = it },
        onResumeSession = onResumeSession,
        onShowTabs = onShowTabs,
    )
    ConnectedDialogs(
        vm = vm,
        showTmux = showTmux,
        onDismissTmux = { showTmux = false },
        onAttachInNewTab = onAttachInNewTab,
        onReadSession = { cwd ->
            // 不 attach 终端，只把阅读面重定向到该会话的工作目录，并清掉钉住的 JSONL。
            pinnedRead = null
            readCwd = cwd
            tab = 1
            showTmux = false
        },
        showHistory = showHistory,
        onDismissHistory = { showHistory = false },
        // 历史页点击：钉住该 JSONL，cwd 已解出则一并重定向，切到阅读 tab。
        onReadHistory = { path, cwd, _, title ->
            pinnedRead = ReadPin(path, title)
            cwd?.let { readCwd = it }
            tab = 1
            showHistory = false
        },
        onResumeSession = onResumeSession,
        captureText = captureText,
        onDismissCapture = { captureText = null },
        showKeyPush = showKeyPush,
        onDismissKeyPush = { showKeyPush = false },
    )
}

/** 已连接的主界面：TabRow、掉线条幅、终端与阅读两个 tab。 */
@Composable
private fun ConnectedScaffold(
    hostId: String,
    s: SessionUiState.Connected,
    vm: SessionViewModel,
    tab: Int,
    onTab: (Int) -> Unit,
    readCwd: String?,
    pinnedRead: ReadPin?, // 历史页钉住的阅读目标；null 走常规发现
    onReconnect: () -> Unit,
    onShowTmux: () -> Unit,
    onShowHistory: () -> Unit,
    onPushKey: () -> Unit,
    onCaptured: (String) -> Unit,
    onResumeSession: (sessionId: String, cwd: String?, title: String?, command: String?) -> Unit,
    onShowTabs: () -> Unit,
) {
    // 阅读面、tmux、切模型依赖 tail -F 与 tmux，按远端方言的能力位（supportsClaudeReading / supportsTmux）开关。
    val dialect = s.dialect
    // 只订阅本会话的状态。
    val status by vm.status.collectAsState()
    // shell 流断了（reader EOF 或 keepalive 探到对端已死）：即使连接状态仍是 CONNECTED（静默半开），也就地显示重连。
    val terminalDropped by vm.terminalDropped.collectAsState()
    val dropped = terminalDropped || (status != null && status != SessionStatus.CONNECTED)
    // statusBarsPadding：边到边下 TabRow 不被系统状态栏遮挡（否则 tab 与时钟重叠且点不到）。
    Column(Modifier.fillMaxSize().statusBarsPadding()) {
        // 不支持 Claude 阅读（没有 tail -F，如 Windows / PowerShell）：隐藏 Claude tab，只留终端，不渲染 TabRow。
        if (dialect.supportsClaudeReading) {
            TabRow(selectedTabIndex = tab) {
                Tab(selected = tab == 0, onClick = { onTab(0) }, text = { Text("终端") })
                Tab(selected = tab == 1, onClick = { onTab(1) }, text = { Text("Claude 阅读") })
            }
        }
        if (dropped) DroppedBanner(status, onReconnect)
        Box(Modifier.fillMaxWidth().weight(1f)) {
            SessionTabContent(
                hostId = hostId,
                s = s,
                vm = vm,
                tab = tab,
                readCwd = readCwd,
                pinnedRead = pinnedRead,
                onReconnect = onReconnect,
                onShowTmux = onShowTmux,
                onShowHistory = onShowHistory,
                onPushKey = onPushKey,
                onCaptured = onCaptured,
                onResumeSession = onResumeSession,
                onShowTabs = onShowTabs,
            )
        }
    }
}

/** 掉线条幅与重连按钮。 */
@Composable
private fun DroppedBanner(
    status: SessionStatus?,
    onReconnect: () -> Unit,
) {
    Row(
        Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.errorContainer).padding(horizontal = 12.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            if (status == SessionStatus.RECONNECTING) "重连中…" else "连接已断开",
            color = MaterialTheme.colorScheme.onErrorContainer,
        )
        TextButton(onClick = onReconnect) { Text("重连") }
    }
}

/** 终端与阅读 tab 的分派，含终端动作的接线。 */
@Composable
private fun SessionTabContent(
    hostId: String,
    s: SessionUiState.Connected,
    vm: SessionViewModel,
    tab: Int,
    readCwd: String?,
    pinnedRead: ReadPin?,
    onReconnect: () -> Unit,
    onShowTmux: () -> Unit,
    onShowHistory: () -> Unit,
    onPushKey: () -> Unit,
    onCaptured: (String) -> Unit,
    onResumeSession: (sessionId: String, cwd: String?, title: String?, command: String?) -> Unit,
    onShowTabs: () -> Unit,
) {
    val dialect = s.dialect
    // 当前是否已在 tmux 内（决定 attach 用 switch-client，免得嵌套）。动作 lambda 在点击时经 getter 读最新值。
    val currentTmux by vm.tmuxSession.collectAsState()
    // 动作在 when 外创建：其中的 rememberCoroutineScope 跟随本 Composable 的生命周期，切 tab 不打断在途请求。
    val onSwitchModel = rememberModelSwitchAction(s.session, vm, dialect.sessionBackend) { currentTmux }
    val onCapture = rememberCaptureAction(vm, dialect.sessionBackend, { currentTmux }, onCaptured)
    // 切走终端不杀会话：emulator/TerminalSession 状态留存（reader 线程持续写 emulator），切回原样。
    when {
        !dialect.supportsClaudeReading || tab == 0 -> // 没有阅读能力就只显示终端（防 rememberSaveable 残留 tab=1）
            TerminalTab(
                session = s.session,
                hostId = hostId,
                onShowTmux = onShowTmux,
                onModel = onSwitchModel,
                onCapture = onCapture,
                onPushKey = onPushKey,
                onShowHistory = onShowHistory,
                onShowTabs = onShowTabs,
                supportsTmux = dialect.supportsTmux,
            )
        else -> ClaudeReadingTab(vm, readCwd, pinnedRead, onReconnect, onResumeSession, onShowHistory)
    }
}

/** Claude 阅读 tab。 */
@Composable
private fun ClaudeReadingTab(
    vm: SessionViewModel,
    readCwd: String?,
    pinnedRead: ReadPin?,
    onReconnect: () -> Unit,
    onResumeSession: (sessionId: String, cwd: String?, title: String?, command: String?) -> Unit,
    onShowHistory: () -> Unit,
) {
    val claudeDir by vm.claudeDir.collectAsState() // 解析后的 agent 配置目录，阅读面据此定位 JSONL
    val agentKind by vm.agentKind.collectAsState() // agent 种类（Claude / Codex），阅读面按它选实现
    // 阅读面用该连接的 exec 通道（sshj 多路复用，不影响终端 shell 通道）。
    // 底部补导航条 inset，否则末条消息和「↓ 最新」被系统手势条挡住；上层只垫了顶部，不会叠两层。
    ClaudeReadingPaneConnected(
        channel = vm.channel,
        executor = vm.executor,
        cwd = readCwd, // 可被 tmux 会话「读」重定向
        claudeDir = claudeDir,
        agentKind = agentKind,
        modifier = Modifier.fillMaxSize().navigationBarsPadding(),
        onReconnectSsh = onReconnect, // 连接死了（由阅读面的错误态判出）时整条重连
        pinnedPath = pinnedRead?.path, // 历史页钉住的 JSONL
        pinnedLabel = pinnedRead?.title,
        onResume = onResumeSession, // 「续接」走统一的 resume tab 路径
        onShowHistory = onShowHistory,
    )
}

/**
 * 一键切模型。有 tmux 会话就 send-keys 投到 Claude 所在 pane（不依赖终端前台）；没有就直接发给终端前台。
 * Toast 只说「已发送」：切没切成取决于 Claude 是否在提示态。
 */
@Composable
private fun rememberModelSwitchAction(
    session: TerminalSession,
    vm: SessionViewModel,
    backend: SessionBackend?, // null 表示没有 tmux 后端；有 tmux 会话时一定非 null
    currentTmux: () -> String?,
): (String) -> Unit {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    return { model ->
        val ts = currentTmux()
        if (ts != null && backend != null) {
            // send-keys 是异步 exec，成败都给反馈，掉线时不假报「已发送」。
            scope.launch {
                runCatching { runOnce(vm.channel, backend.sendModelCommand(ts, model)) }
                    .onSuccess { Toast.makeText(context, "已发送 /model $model", Toast.LENGTH_SHORT).show() }
                    .onFailure { Toast.makeText(context, "发送失败: ${it.message}", Toast.LENGTH_SHORT).show() }
            }
        } else {
            session.send("/model $model\n".toByteArray()) // 没有 tmux：直接发给终端前台
            Toast.makeText(context, "已发送 /model $model", Toast.LENGTH_SHORT).show()
        }
    }
}

/** 抓当前 tmux 会话的可见屏为纯文本，交给 [onCaptured]。 */
@Composable
private fun rememberCaptureAction(
    vm: SessionViewModel,
    backend: SessionBackend?, // null 表示没有 tmux 后端；有 tmux 会话时一定非 null
    currentTmux: () -> String?,
    onCaptured: (String) -> Unit,
): () -> Unit {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    return {
        val ts = currentTmux()
        if (ts != null && backend != null) {
            scope.launch {
                runCatching { runOnce(vm.channel, backend.captureCommand(ts)) }
                    .onSuccess { onCaptured(it) }
                    .onFailure { Toast.makeText(context, "抓屏失败: ${it.message}", Toast.LENGTH_SHORT).show() }
            }
        } else {
            Toast.makeText(context, "无 tmux 会话可抓屏", Toast.LENGTH_SHORT).show()
        }
    }
}

/**
 * 唯一的 resume 路径（历史页与阅读器「续接」共用）：开新终端 tab，fresh shell 起 tmux 跑
 * `<cc> --resume <sid>`（远端按 sid 定位权威 cwd）。不往当前前台 pty 注入：前台若正跑 Claude，
 * 命令会被打进它的输入框。sid 先验（防注入）。title 可 null；cwd 只作兜底；command 是历史页长按选定的命令。
 */
@Composable
private fun rememberResumeAction(
    onResumeInNewTab: (cwd: String?, sessionId: String, title: String?, command: String?) -> Unit,
): (sessionId: String, cwd: String?, title: String?, command: String?) -> Unit {
    val context = LocalContext.current
    return { sessionId, resumeCwd, title, command ->
        if (ClaudeInvocation.isValidSessionId(sessionId)) {
            onResumeInNewTab(resumeCwd, sessionId, title, command)
        } else {
            Toast.makeText(context, "会话 ID 非法，无法 resume", Toast.LENGTH_SHORT).show()
        }
    }
}

/** 已连接态的四个对话框（tmux 管理 / 历史面板 / 抓屏 / 公钥推送），显隐状态在上层。 */
@Composable
private fun ConnectedDialogs(
    vm: SessionViewModel,
    showTmux: Boolean,
    onDismissTmux: () -> Unit,
    onAttachInNewTab: (name: String) -> Unit,
    onReadSession: (cwd: String) -> Unit,
    showHistory: Boolean,
    onDismissHistory: () -> Unit,
    // 历史页点击：钉住 JSONL 并切阅读 tab（关面板在上层回调里做）。
    onReadHistory: (path: String, cwd: String?, sessionId: String, title: String) -> Unit,
    onResumeSession: (sessionId: String, cwd: String?, title: String?, command: String?) -> Unit,
    captureText: String?,
    onDismissCapture: () -> Unit,
    showKeyPush: Boolean,
    onDismissKeyPush: () -> Unit,
) {
    // tmux 管理与历史面板用该连接的 exec 通道（sshj 多路复用，不影响终端 shell 通道）。
    val channel = vm.channel
    if (showTmux) {
        TmuxManagerDialog(
            channel = channel,
            onAttach = { name ->
                // 附着是新开终端 tab，由它的 fresh shell 跑 `tmux new -A -s name`；不往当前前台 pty 注入
                // （前台若是 Claude Code 会打进它的输入框）。新 shell 不在 tmux 内，也不会嵌套。
                onAttachInNewTab(name)
                onDismissTmux()
            },
            onNewAttach = { name ->
                // 新建并附着同样开新 tab（`tmux new -A -s name` 无则建、有则附），与附着等价。
                onAttachInNewTab(name)
                onDismissTmux()
            },
            onRead = onReadSession,
            onDismiss = onDismissTmux,
        )
    }
    if (showHistory) {
        // 历史记录面板：复用当前连接与 claudeDir 浏览全部项目的历史。点击阅读；长按选命令 resume
        // （选项来自本主机的 launcher，自定义命令记住上次输入）。
        val claudeDir by vm.claudeDir.collectAsState()
        val hostLaunchers by vm.hostLaunchers.collectAsState()
        val lastCustomCommand by vm.lastResumeCustomCommand.collectAsState()
        ClaudeHistorySheet(
            channel = channel,
            claudeDir = claudeDir,
            launchers =
                hostLaunchers.mapNotNull { l ->
                    l.command
                        ?.trim()
                        ?.takeIf { it.isNotEmpty() }
                        ?.let { ResumeLauncherOption(l.label, it) }
                },
            lastCustomCommand = lastCustomCommand,
            onRead = onReadHistory,
            // 注意参数换序：(cwd, sid) → (sid, cwd)。
            onResume = { cwd, sid, title, command ->
                onDismissHistory()
                onResumeSession(sid, cwd, title, command)
            },
            onCustomCommandUsed = { vm.setLastResumeCustomCommand(it) }, // 记住上次自定义命令
            onDismiss = onDismissHistory,
        )
    }
    CaptureControl(captureText, onDismissCapture)
    val identities by vm.identities.collectAsState()
    PushKeyControl(showKeyPush, identities, channel, onDismissKeyPush)
}

/** 抓屏结果弹窗的宿主，自己持有 context / clipboard / scope 做复制与 Toast。 */
@Composable
private fun CaptureControl(
    text: String?,
    onDismiss: () -> Unit,
) {
    if (text == null) return
    val context = LocalContext.current
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    CaptureDialog(
        text = text,
        onCopy = {
            scope.launch { clipboard.copyPlainText(text) }
            Toast.makeText(context, "已复制 ${text.length} 字符", Toast.LENGTH_SHORT).show()
            onDismiss()
        },
        onDismiss = onDismiss,
    )
}

/** 把推送命令的输出映射为可读的反馈。 */
private fun pushKeyResultMessage(out: String): String =
    when {
        out.contains("ATERM_ADDED") -> "已推送公钥到本机 authorized_keys"
        out.contains("ATERM_ALREADY") -> "公钥已在 authorized_keys（未重复添加）"
        else -> "推送未确认，请检查远端 ~/.ssh/authorized_keys" // 没有标记：命令链在写入前就失败了（权限 / 磁盘等），不假报成功
    }

/** 公钥推送对话框的宿主：选身份，在活连接上 exec，给反馈。 */
@Composable
private fun PushKeyControl(
    show: Boolean,
    identities: List<Identity>,
    channel: RemoteCommandChannel,
    onDismiss: () -> Unit,
) {
    if (!show) return
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    PushKeyDialog(
        identities = identities,
        onDismiss = onDismiss,
        onPick = { id ->
            onDismiss()
            if (id.publicKeyOpenSsh.isBlank()) {
                Toast.makeText(context, "该身份无公钥，无法推送（旧的导入身份可重新导入以派生公钥）", Toast.LENGTH_LONG).show()
            } else {
                // 在已认证的连接上 exec，不必重新认证。输出里带 ATERM_ADDED / ALREADY 标记。
                scope.launch {
                    runCatching { runOnce(channel, appendAuthorizedKeyCommand(id.publicKeyOpenSsh)) }
                        .onSuccess { Toast.makeText(context, pushKeyResultMessage(it), Toast.LENGTH_SHORT).show() }
                        .onFailure { Toast.makeText(context, "推送失败: ${it.message}", Toast.LENGTH_SHORT).show() }
                }
            }
        },
    )
}

/** 选一个身份，把它的 OpenSSH 公钥推到这台主机的 ~/.ssh/authorized_keys（在已连接的会话上执行）。 */
@Composable
private fun PushKeyDialog(
    identities: List<Identity>,
    onDismiss: () -> Unit,
    onPick: (Identity) -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("推送公钥到本机") },
        text = {
            Column {
                if (identities.isEmpty()) {
                    Text("还没有身份，先在身份管理创建或导入。", style = MaterialTheme.typography.bodySmall)
                } else {
                    Text("选一个身份，把它的公钥加到本机 ~/.ssh/authorized_keys（幂等，已存在不重复）：", style = MaterialTheme.typography.bodySmall)
                    identities.forEach { id ->
                        TextButton(onClick = { onPick(id) }) { Text(id.label.ifBlank { id.id }) }
                    }
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text("关闭") } },
    )
}

/** 抓屏结果弹窗：等宽滚动显示屏幕内容，可复制到剪贴板。 */
@Composable
private fun CaptureDialog(
    text: String,
    onCopy: () -> Unit,
    onDismiss: () -> Unit,
) {
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("当前屏幕") },
        text = {
            Text(
                text.ifBlank { "(空)" },
                style = monoSmall,
                modifier = Modifier.fillMaxWidth().heightIn(max = 420.dp).verticalScroll(rememberScrollState()),
            )
        },
        confirmButton = { TextButton(onClick = onCopy) { Text("复制") } },
        dismissButton = { TextButton(onClick = onDismiss) { Text("关闭") } },
    )
}

/** 终端 tab：termlib 渲染，加按钮条与 ExtraKeys。 */
@Composable
private fun TerminalTab(
    session: TerminalSession,
    hostId: String,
    onShowTmux: () -> Unit,
    onModel: (String) -> Unit,
    onCapture: () -> Unit,
    onPushKey: () -> Unit,
    onShowHistory: () -> Unit,
    onShowTabs: () -> Unit,
    supportsTmux: Boolean = true, // 远端是否支持 tmux，决定 tmux 会话 / 切模型 / 抓屏 / 公钥这些项
) {
    // 终端字号（sp），双指捏合缩放，termlib 经 onFontSizeChanged 回报（已 clamp 到 7..28）。
    // 存整数 sp 当作量化死区：微小误触（如 13.026）四舍五入回 13，不落库也不抖。旋转与进程恢复不丢。
    var fontSize by rememberSaveable { mutableStateOf(13) }
    // 拉起软键盘：requestFocus termlib 的 ImeInputView，再显式 show IME。
    val focusRequester = remember { FocusRequester() }
    val keyboardController = LocalSoftwareKeyboardController.current
    val localView = LocalView.current // 拉键盘走 window 层的 WindowInsetsController
    // 点击终端时拉起软键盘。termlib 是自定义 View，不是 Compose TextField，`keyboardController.show()` 对它常不响应，
    // 所以主路走 window 层的 `WindowInsetsController.show(ime())`，requestFocus 与 keyboardController 兜底。
    val showKeyboard: () -> Unit = {
        runCatching { focusRequester.requestFocus() }
        runCatching { ViewCompat.getWindowInsetsController(localView)?.show(WindowInsetsCompat.Type.ime()) }
        keyboardController?.show()
    }
    // navigationBarsPadding 让 ExtraKeysRow 不被系统手势条挡住；imePadding 让键盘弹起时按钮条仍在键盘上方。
    Column(Modifier.fillMaxSize().navigationBarsPadding().imePadding()) {
        // 不用 key() 重建 Terminal：回喂的 initialFontSize 由 termlib 就地改字号，滚动位置、选区与 IME 状态都保得住。
        AtermTerminal(
            emulator = session.emulator,
            modifier = Modifier.fillMaxWidth().weight(1f),
            keyboardEnabled = true,
            showSoftKeyboard = true,
            initialFontSize = fontSize.sp,
            focusRequester = focusRequester,
            // 捏合缩放落定：四舍五入到整 sp、clamp 到 7..28，持久化并回喂 initialFontSize。
            onFontSizeChanged = { fontSize = it.value.roundToInt().coerceIn(7, 28) },
            // 点击终端（termlib 判为 tap、无选区、不是超链接）就唤起软键盘。
            onTerminalTap = showKeyboard,
        )
        TerminalBottomControls(
            session = session,
            hostId = hostId,
            supportsTmux = supportsTmux,
            showKeyboard = showKeyboard,
            onShowTabs = onShowTabs,
            onShowTmux = onShowTmux,
            onModel = onModel,
            onCapture = onCapture,
            onPushKey = onPushKey,
            onShowHistory = onShowHistory,
        )
    }
}

/** 终端底部控件区：数据驱动的按钮条、ExtraKeys、命令面板。 */
@Composable
private fun TerminalBottomControls(
    session: TerminalSession,
    hostId: String,
    supportsTmux: Boolean,
    showKeyboard: () -> Unit,
    onShowTabs: () -> Unit,
    onShowTmux: () -> Unit,
    onModel: (String) -> Unit,
    onCapture: () -> Unit,
    onPushKey: () -> Unit,
    onShowHistory: () -> Unit,
) {
    // 命令面板「自定义」段的来源，与按钮条同源，只取 group='main'：fn / vim 组的单键（Esc、方向、hjkl…）
    // 进面板只是噪声。
    val customButtonDao = koinInject<CustomButtonDao>()
    val customButtons by remember(hostId) {
        customButtonDao.observeGroup(hostId, ButtonGroups.MAIN)
    }.collectAsState(initial = emptyList())
    // 「自定义」段也过 tmux 门控（与按钮条同一条 [visibleOn]），否则 `tmux 会话` / `Claude 历史` 这类内建动作
    // 会在非 tmux 主机上冒出来。
    val paletteButtons = customButtons.filter { it.visibleOn(supportsTmux) }
    var showPalette by rememberSaveable { mutableStateOf(false) }
    val mods by session.modifiers.collectAsState() // 粘性 Ctrl / Alt 的点亮态
    // 粘贴动作，ExtraKeysRow 的粘贴键与内建 `paste` 共用。
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    val onPaste: () -> Unit = {
        scope.launch { clipboard.pastePlainText()?.takeIf { it.isNotEmpty() }?.let { session.send(it.toByteArray()) } }
    }
    // 内建动作：type=builtin 的按钮经它触达这些回调。
    val builtins =
        terminalButtonBuiltins(
            session,
            supportsTmux,
            onShowTabs,
            { showPalette = true },
            onShowTmux,
            onPaste,
            onShowHistory,
            onCapture,
            onPushKey,
            showKeyboard,
        )
    // 近键盘那排显示哪个组，每个 tab 各记各的，默认 fn；switch 按钮经 dispatch 的 onSwitchGroup 改它。
    var activeGroup by rememberSaveable { mutableStateOf(ButtonGroups.FN) }
    // 长按 switch 按钮就地开按钮设置（全屏 Dialog，会话内模态，不走导航，免得穿多层回调）。
    var showButtonSettings by remember { mutableStateOf(false) }
    // 唯一的派发入口，按钮条、键排、命令面板共用，免得哪处漏传 builtins。
    // 任意 switch 按钮点了都切近键盘那排的组。
    val dispatch: (CustomButton) -> Unit = { dispatchButton(it, session::send, builtins, onSwitchGroup = { g -> activeGroup = g }) }
    // 按钮条：`▤ tab` / `命令` / `tmux 会话` 是 main 组的内建种子，与用户按钮一起渲染、可编辑可重排。
    // 能力位门控在渲染侧 [visibleOn]。按钮四类：命令 / 按键码 / 转义 / 内建，由 dispatchButton 按类型派发。
    CustomButtonBar(hostId = hostId, dispatch = dispatch, supportsTmux = supportsTmux)
    ExtraKeysRow(
        hostId = hostId, // 查询范围：全局 + 本机
        activeGroup = activeGroup,
        dispatch = dispatch,
        ctrlActive = mods.ctrl, // Ctrl 粘性按钮的点亮态
        altActive = mods.alt,
        onOpenButtonSettings = { showButtonSettings = true },
    )
    // 命令面板：把模型、抓屏、自定义命令、tmux 会话整合成可搜索的动作。
    if (showPalette) {
        val actions =
            terminalPaletteActions(supportsTmux, paletteButtons, onModel, onCapture, onShowTmux, onPushKey, onShowHistory, dispatch)
        CommandPalette(actions = actions, onDismiss = { showPalette = false })
    }
    // 按钮设置全屏 Dialog（hostId=null 即全局按钮），关掉即回终端。
    if (showButtonSettings) {
        Dialog(
            onDismissRequest = { showButtonSettings = false },
            properties = DialogProperties(usePlatformDefaultWidth = false),
        ) {
            // 注意：Snackbar 不跨 window 绘制。Dialog 是独立 window，activity 根部那个宿主在这里看不见，
            // 所以每个 window 的根部都要自己装一个，否则删按钮组的反馈与「撤销」一个像素都看不到。
            AppSnackbarHostScaffold {
                ButtonSettingsScreen(hostId = null, onBack = { showButtonSettings = false })
            }
        }
    }
}

/**
 * 终端 tab 的内建动作。tmux 类动作（会话 / 抓屏 / 历史 / 公钥）在不支持 tmux 的主机上点了也无害。
 */
private fun terminalButtonBuiltins(
    session: TerminalSession,
    supportsTmux: Boolean,
    onShowTabs: () -> Unit,
    onCommandPalette: () -> Unit,
    onShowTmux: () -> Unit,
    onPaste: () -> Unit,
    onShowHistory: () -> Unit,
    onCapture: () -> Unit,
    onPushKey: () -> Unit,
    onShowKeyboard: () -> Unit,
): ButtonBuiltins =
    // tmux 门控在 ButtonBuiltins.invoke 里（按 TMUX_ONLY_BUILTINS），这里不必逐个包。
    ButtonBuiltins(
        supportsTmux = supportsTmux,
        onShowTabs = onShowTabs,
        onCommandPalette = onCommandPalette,
        onTmuxSessions = onShowTmux,
        onPaste = onPaste,
        onShowHistory = onShowHistory,
        onCapture = onCapture,
        onPushKey = onPushKey,
        onShowKeyboard = onShowKeyboard,
        onToggleCtrl = { session.toggleCtrl() },
        onToggleAlt = { session.toggleAlt() },
    )

/**
 * 命令面板的动作集。依赖 tmux 的项（切模型 / 抓屏 / tmux 会话 / 历史 / 公钥推送）按 `supportsTmux` 开关；
 * 自定义命令总在。
 */
internal fun terminalPaletteActions(
    supportsTmux: Boolean,
    customButtons: List<CustomButton>,
    onModel: (String) -> Unit,
    onCapture: () -> Unit,
    onShowTmux: () -> Unit,
    onPushKey: () -> Unit,
    onHistory: () -> Unit,
    onCustom: (CustomButton) -> Unit,
): List<PaletteAction> =
    buildList {
        if (supportsTmux) {
            MODEL_PRESETS.forEach { m -> add(PaletteAction("模型", m, "/model $m") { onModel(m) }) }
            add(PaletteAction("抓屏", "抓当前屏幕") { onCapture() })
        }
        // 副标题显示组合键（如 Ctrl+C）而不是裸键；动作带整个按钮，由调用方按修饰键编码。
        customButtons.forEach { b -> add(PaletteAction("自定义", b.label, customButtonSubtitle(b)) { onCustom(b) }) }
        if (supportsTmux) {
            add(PaletteAction("会话", "tmux 会话管理") { onShowTmux() })
            // 历史记录：resume 依赖 tmux 与 POSIX，所以也按 supportsTmux 开关。副标题与历史面板一致。
            add(PaletteAction("历史", "Claude 历史记录", "按项目分组 · 点击查看 · 长按选择命令续跑") { onHistory() })
            // 公钥推送用的是 POSIX shell（mkdir / chmod / grep >> authorized_keys），严格说要的是 POSIX 能力而非 tmux；
            // 按操作系统定方言时两者总是同值，所以挂在 supportsTmux 上。
            add(PaletteAction("公钥", "推送公钥到本机 authorized_keys") { onPushKey() })
        }
    }

/** 命令面板里自定义按钮的副标题：有修饰键显示组合键（如 `Ctrl+C`），否则显示原命令。 */
internal fun customButtonSubtitle(b: CustomButton): String =
    if (!b.ctrl && !b.alt && !b.shift) {
        b.command
    } else {
        buildString {
            if (b.ctrl) append("Ctrl+")
            if (b.alt) append("Alt+")
            if (b.shift) append("Shift+")
            append(b.command)
        }
    }

/** 常用模型预设，经命令面板「模型」组发 `/model <preset>`。 */
private val MODEL_PRESETS = listOf("opus", "sonnet", "haiku", "default")
