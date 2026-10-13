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
import com.ccmonitor.mobile.core.claude.link.wire.NewSessionPick
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.ButtonGroups
import com.ccmonitor.mobile.core.data.db.CustomButton
import com.ccmonitor.mobile.core.data.db.CustomButtonDao
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.ssh.SessionStatus
import com.ccmonitor.mobile.core.ssh.SshConnectEvent
import com.ccmonitor.mobile.core.terminal.CcMonitorTerminal
import com.ccmonitor.mobile.core.terminal.TerminalSession
import com.ccmonitor.mobile.core.ui.feedback.AppSnackbarHostScaffold
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.feedback.Notice
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.service.AppForeground
import com.ccmonitor.mobile.service.SshKeepAliveService
import com.ccmonitor.mobile.ssh.appendAuthorizedKeyCommand
import com.ccmonitor.mobile.ui.common.CopyDetailButton
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
 * 终端屏只有终端：看一条会话的正文回聊天屏（`界面.md`「终端」）。
 */
@Composable
fun SessionScreen(
    hostId: String,
    cd: String? = null,
    launcherId: String? = null,
    tmuxSession: String? = null, // 进终端 tab 首连即接回的会话名；普通 tab 为 null
    onAttachInNewTab: (name: String) -> Unit = {}, // tmux「附着」：开新终端 tab
    onShowTabs: () -> Unit = {}, // 唤出 tab 面板
) = SessionContent(hostId, cd, launcherId, tmuxSession, onAttachInNewTab, onShowTabs)

@Composable
private fun SessionContent(
    hostId: String,
    cd: String?,
    launcherId: String?,
    tmuxSession: String?,
    onAttachInNewTab: (name: String) -> Unit,
    onShowTabs: () -> Unit,
) {
    // Koin 参数不能传 null，可空参数一律用空串表示 null。
    val vm =
        koinViewModel<SessionViewModel> {
            parametersOf(hostId, cd ?: "", launcherId ?: "", tmuxSession ?: "")
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
        Text(copyText("mobSession.connect.failed"), color = MaterialTheme.colorScheme.error)
        // 下层给的原话（或这一侧判出的原因）不上屏，进复制详情；连接日志照旧给原文。
        CopyDetailButton(copyText("mobSession.connect.failed"), message)
        if (log.isNotEmpty()) ConnectLogView(log, Modifier.fillMaxWidth().weight(1f))
        TextButton(onClick = { vm.retry() }) { Text(copyText("mobSession.connect.retry")) }
    }
}

/**
 * 非连接态（连接中 / 失败 / 选身份）顶部的 tab 按钮。这些态没有工具条，靠它在慢连、断开、待选身份时
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
        Text(copyText("mobSession.identity.pickTitle"), style = MaterialTheme.typography.titleMedium)
        Text(copyText("mobSession.identity.pickHint"), style = MaterialTheme.typography.bodySmall)
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
        Text(copyText("mobHostKey.page.title"), style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.error)
        Text(
            copyText("hostIdentity.changed.body"),
            style = MaterialTheme.typography.bodyMedium,
        )
        Text(
            copyText("mobHostKey.page.cases"),
            style = MaterialTheme.typography.bodySmall,
        )
        // 详情给会核对的人，放在正文之下、小字
        if (state.expectedFingerprint.isNotBlank() || state.offeredFingerprint.isNotBlank()) {
            Text(
                copyText(
                    "mobHostKey.page.fingerprints",
                    "old" to state.expectedFingerprint.ifBlank { copyText("mobHostKey.page.unknown") },
                    "new" to state.offeredFingerprint.ifBlank { copyText("mobHostKey.page.unknown") },
                ),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        TextButton(onClick = { confirming = true }) { Text(copyText("mobHostKey.page.confirm")) }
    }

    if (confirming) {
        AlertDialog(
            onDismissRequest = { confirming = false },
            title = { Text(copyText("mobHostKey.trust.title")) },
            // 二次确认的文案说清后果，而不是只问「确定吗」。
            text = { Text(copyText("hostIdentity.trust.consequence")) },
            confirmButton = {
                TextButton(onClick = {
                    confirming = false
                    onTrustNew()
                }) { Text(copyText("mobHostKey.trust.ok")) }
            },
            dismissButton = { TextButton(onClick = { confirming = false }) { Text(copyText("kit.dialog.cancel")) } },
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
        Text(copyText("mobSession.connect.progress"), style = MaterialTheme.typography.titleMedium)
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
 * 已连接态：终端 ＋ 几个对话框（tmux 管理 / 抓屏 / 公钥推送）。这里只持对话框开关与抓屏文本；渲染在 [ConnectedScaffold]
 * 与 [ConnectedDialogs]。
 */
@Composable
private fun ConnectedContent(
    hostId: String,
    s: SessionUiState.Connected,
    vm: SessionViewModel,
    onReconnect: () -> Unit,
    onAttachInNewTab: (name: String) -> Unit, // tmux 附着：开新终端 tab
    onShowTabs: () -> Unit,
) {
    var showTmux by rememberSaveable { mutableStateOf(false) }
    // 抓当前 tmux 会话的可见屏为纯文本，弹窗看并可复制。
    var captureText by remember { mutableStateOf<String?>(null) }
    // 公钥推送：命令面板「推送公钥」触发，选一个身份，把它的 OpenSSH 公钥追加到这台主机的 authorized_keys。
    var showKeyPush by remember { mutableStateOf(false) }
    ConnectedScaffold(
        hostId = hostId,
        s = s,
        vm = vm,
        onReconnect = onReconnect,
        onShowTmux = { showTmux = true },
        onPushKey = { showKeyPush = true },
        onCaptured = { captureText = it },
        onShowTabs = onShowTabs,
    )
    ConnectedDialogs(
        vm = vm,
        showTmux = showTmux,
        onDismissTmux = { showTmux = false },
        onAttachInNewTab = onAttachInNewTab,
        captureText = captureText,
        onDismissCapture = { captureText = null },
        showKeyPush = showKeyPush,
        onDismissKeyPush = { showKeyPush = false },
    )
}

/** 已连接的主界面：掉线条幅 ＋ 终端。 */
@Composable
private fun ConnectedScaffold(
    hostId: String,
    s: SessionUiState.Connected,
    vm: SessionViewModel,
    onReconnect: () -> Unit,
    onShowTmux: () -> Unit,
    onPushKey: () -> Unit,
    onCaptured: (String) -> Unit,
    onShowTabs: () -> Unit,
) {
    // 只订阅本会话的状态。
    val status by vm.status.collectAsState()
    // shell 流断了（reader EOF 或 keepalive 探到对端已死）：即使连接状态仍是 CONNECTED（静默半开），也就地显示重连。
    val terminalDropped by vm.terminalDropped.collectAsState()
    val dropped = terminalDropped || (status != null && status != SessionStatus.CONNECTED)
    // statusBarsPadding：边到边下终端不被系统状态栏遮挡。
    Column(Modifier.fillMaxSize().statusBarsPadding()) {
        if (dropped) DroppedBanner(status, onReconnect)
        Box(Modifier.fillMaxWidth().weight(1f)) {
            ConnectedTerminal(hostId, s, vm, onShowTmux, onPushKey, onCaptured, onShowTabs)
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
            if (status == SessionStatus.RECONNECTING) copyText("mobSession.bar.reconnecting") else copyText("mobSession.bar.dropped"),
            color = MaterialTheme.colorScheme.onErrorContainer,
        )
        TextButton(onClick = onReconnect) { Text(copyText("mobSession.connect.retry")) }
    }
}

/** 终端与它的动作接线。 */
@Composable
private fun ConnectedTerminal(
    hostId: String,
    s: SessionUiState.Connected,
    vm: SessionViewModel,
    onShowTmux: () -> Unit,
    onPushKey: () -> Unit,
    onCaptured: (String) -> Unit,
    onShowTabs: () -> Unit,
) {
    // tmux、切模型依赖 tmux，按远端方言的能力位（supportsTmux）开关。
    val dialect = s.dialect
    // 本 tab 当前所在的 tmux 会话名（切模型 · 抓屏按它找那台名单里的终端）。动作 lambda 在点击时经 getter 读最新值。
    val currentTmux by vm.tmuxSession.collectAsState()
    val onSwitchModel = rememberModelSwitchAction(vm.terminals) { currentTmux }
    // 面板「模型」组：那个终端里跑着的那一家能换的模型（那台给的）；没跑会话 ⇒ 空、这一组不出。
    val models by androidx.compose.runtime.produceState(emptyList<NewSessionPick>(), currentTmux) {
        value = currentTmux?.let { vm.terminals.modelsIn(it) }.orEmpty()
    }
    val onCapture = rememberCaptureAction(vm.terminals, { currentTmux }, onCaptured)
    TerminalTab(
        session = s.session,
        hostId = hostId,
        onShowTmux = onShowTmux,
        models = models,
        onModel = onSwitchModel,
        onCapture = onCapture,
        onPushKey = onPushKey,
        onShowTabs = onShowTabs,
        supportsTmux = dialect.supportsTmux,
    )
}

/**
 * 一键切模型：请那台给当前 tmux 会话里跑着的那条会话换模型（`session-model`：那台送命令、读回来认，在等人不送）。
 * 成败都照那台写好的那一句说；当前不在 tmux 会话里 ⇒ 面板里没有这一组，走不到这里。
 */
@Composable
private fun rememberModelSwitchAction(
    terminals: TerminalOps,
    currentTmux: () -> String?,
): (String) -> Unit {
    val context = LocalContext.current
    val snackbar = LocalAppSnackbar.current
    val scope = rememberCoroutineScope()
    return { model ->
        currentTmux()?.let { ts ->
            scope.launch {
                terminals
                    .switchModel(ts, model)
                    .onSuccess { said -> Toast.makeText(context, said, Toast.LENGTH_SHORT).show() }
                    .onFailure { snackbar.show(Notice(copyText("mobTerm.model.sendFailed"), it.message.orEmpty())) }
            }
        }
    }
}

/** 请那台抓当前 tmux 会话的一屏（纯文字），交给 [onCaptured]。 */
@Composable
private fun rememberCaptureAction(
    terminals: TerminalOps,
    currentTmux: () -> String?,
    onCaptured: (String) -> Unit,
): () -> Unit {
    val context = LocalContext.current
    val snackbar = LocalAppSnackbar.current
    val scope = rememberCoroutineScope()
    return {
        val ts = currentTmux()
        if (ts != null) {
            scope.launch {
                terminals
                    .capture(ts)
                    .onSuccess { onCaptured(it) }
                    .onFailure { snackbar.show(Notice(copyText("mobTerm.capture.failed"), it.message)) }
            }
        } else {
            Toast.makeText(context, copyText("mobTerm.capture.noTmux"), Toast.LENGTH_SHORT).show()
        }
    }
}

/** 已连接态的三个对话框（tmux 管理 / 抓屏 / 公钥推送），显隐状态在上层。 */
@Composable
private fun ConnectedDialogs(
    vm: SessionViewModel,
    showTmux: Boolean,
    onDismissTmux: () -> Unit,
    onAttachInNewTab: (name: String) -> Unit,
    captureText: String?,
    onDismissCapture: () -> Unit,
    showKeyPush: Boolean,
    onDismissKeyPush: () -> Unit,
) {
    // tmux 管理用该连接的 exec 通道（同一条连接多路复用，不影响终端 shell 通道）。
    val channel = vm.channel
    if (showTmux) {
        TmuxManagerDialog(
            channel = channel,
            terminals = vm.terminals,
            onAttach = { name ->
                // 附着是新开终端 tab，由它的新 shell 敲那台渲出的那一行；不往当前前台 pty 注入
                // （前台若是 Claude Code 会打进它的输入框）。
                onAttachInNewTab(name)
                onDismissTmux()
            },
            onDismiss = onDismissTmux,
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
            Toast.makeText(context, copyText("mobTerm.capture.copied", "n" to text.length), Toast.LENGTH_SHORT).show()
            onDismiss()
        },
        onDismiss = onDismiss,
    )
}

/** 把推送命令的输出映射为可读的反馈。 */
private fun pushKeyResultMessage(out: String): String =
    when {
        out.contains("CCM_KEY_ADDED") -> copyText("mobKeyPush.result.added")
        out.contains("CCM_KEY_ALREADY") -> copyText("mobKeyPush.result.already")
        else -> copyText("mobKeyPush.result.unsure") // 没有标记：命令链在写入前就失败了（权限 / 磁盘等），不假报成功
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
    val snackbar = LocalAppSnackbar.current
    val scope = rememberCoroutineScope()
    PushKeyDialog(
        identities = identities,
        onDismiss = onDismiss,
        onPick = { id ->
            onDismiss()
            if (id.publicKeyOpenSsh.isBlank()) {
                Toast.makeText(context, copyText("mobKeyPush.result.noPublic"), Toast.LENGTH_LONG).show()
            } else {
                // 在已认证的连接上 exec，不必重新认证。输出里带 CCM_KEY_ADDED / ALREADY 标记。
                scope.launch {
                    runCatching { runOnce(channel, appendAuthorizedKeyCommand(id.publicKeyOpenSsh)) }
                        .onSuccess { Toast.makeText(context, pushKeyResultMessage(it), Toast.LENGTH_SHORT).show() }
                        .onFailure { snackbar.show(Notice(copyText("mobKeyPush.result.failed"), it.message)) }
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
        title = { Text(copyText("mobKeyPush.dialog.title")) },
        text = {
            Column {
                if (identities.isEmpty()) {
                    Text(copyText("mobKeyPush.dialog.empty"), style = MaterialTheme.typography.bodySmall)
                } else {
                    Text(copyText("mobKeyPush.dialog.hint"), style = MaterialTheme.typography.bodySmall)
                    identities.forEach { id ->
                        TextButton(onClick = { onPick(id) }) { Text(id.label.ifBlank { id.id }) }
                    }
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text(copyText("mobKit.act.close")) } },
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
        title = { Text(copyText("mobCapture.dialog.title")) },
        text = {
            Text(
                text.ifBlank { copyText("mobCapture.dialog.empty") },
                style = monoSmall,
                modifier = Modifier.fillMaxWidth().heightIn(max = 420.dp).verticalScroll(rememberScrollState()),
            )
        },
        confirmButton = { TextButton(onClick = onCopy) { Text(copyText("mobCapture.dialog.copy")) } },
        dismissButton = { TextButton(onClick = onDismiss) { Text(copyText("mobKit.act.close")) } },
    )
}

/** 终端 tab：termlib 渲染，加按钮条与 ExtraKeys。 */
@Composable
private fun TerminalTab(
    session: TerminalSession,
    hostId: String,
    onShowTmux: () -> Unit,
    models: List<NewSessionPick>,
    onModel: (String) -> Unit,
    onCapture: () -> Unit,
    onPushKey: () -> Unit,
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
        CcMonitorTerminal(
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
            models = models,
            onModel = onModel,
            onCapture = onCapture,
            onPushKey = onPushKey,
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
    models: List<NewSessionPick>,
    onModel: (String) -> Unit,
    onCapture: () -> Unit,
    onPushKey: () -> Unit,
) {
    // 命令面板「自定义」段的来源，与按钮条同源，只取 group='main'：fn / vim 组的单键（Esc、方向、hjkl…）
    // 进面板只是噪声。
    val customButtonDao = koinInject<CustomButtonDao>()
    val customButtons by remember(hostId) {
        customButtonDao.observeGroup(hostId, ButtonGroups.MAIN)
    }.collectAsState(initial = emptyList())
    // 「自定义」段也过 tmux 门控（与按钮条同一条 [visibleOn]），否则 `tmux 会话` / `抓屏` 这类内建动作
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
            terminalPaletteActions(supportsTmux, paletteButtons, models, onModel, onCapture, onShowTmux, onPushKey, dispatch)
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
 * 终端 tab 的内建动作。tmux 类动作（会话 / 抓屏 / 公钥）在不支持 tmux 的主机上点了也无害。
 */
private fun terminalButtonBuiltins(
    session: TerminalSession,
    supportsTmux: Boolean,
    onShowTabs: () -> Unit,
    onCommandPalette: () -> Unit,
    onShowTmux: () -> Unit,
    onPaste: () -> Unit,
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
        onCapture = onCapture,
        onPushKey = onPushKey,
        onShowKeyboard = onShowKeyboard,
        onToggleCtrl = { session.toggleCtrl() },
        onToggleAlt = { session.toggleAlt() },
    )

/**
 * 命令面板的动作集。依赖 tmux 的项（切模型 / 抓屏 / tmux 会话 / 公钥推送）按 `supportsTmux` 开关；
 * 自定义命令总在。
 */
internal fun terminalPaletteActions(
    supportsTmux: Boolean,
    customButtons: List<CustomButton>,
    models: List<NewSessionPick>,
    onModel: (String) -> Unit,
    onCapture: () -> Unit,
    onShowTmux: () -> Unit,
    onPushKey: () -> Unit,
    onCustom: (CustomButton) -> Unit,
): List<PaletteAction> =
    buildList {
        if (supportsTmux) {
            // 那台给的可选模型（名字照那台写好的字，副标题是原值）。
            models.forEach { m -> add(PaletteAction(copyText("mobPalette.group.model"), m.text, m.value) { onModel(m.value) }) }
            add(PaletteAction(copyText("mobPalette.group.capture"), copyText("mobPalette.item.capture")) { onCapture() })
        }
        // 副标题显示组合键（如 Ctrl+C）而不是裸键；动作带整个按钮，由调用方按修饰键编码。
        customButtons.forEach { b -> add(PaletteAction(copyText("mobPalette.group.custom"), b.label, customButtonSubtitle(b)) { onCustom(b) }) }
        if (supportsTmux) {
            add(PaletteAction(copyText("mobPalette.group.tmux"), copyText("mobPalette.item.tmux")) { onShowTmux() })
            // 公钥推送用的是 POSIX shell（mkdir / chmod / grep >> authorized_keys），严格说要的是 POSIX 能力而非 tmux；
            // 按操作系统定方言时两者总是同值，所以挂在 supportsTmux 上。
            add(PaletteAction(copyText("mobPalette.group.key"), copyText("mobPalette.item.key")) { onPushKey() })
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
