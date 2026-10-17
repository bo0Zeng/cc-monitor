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
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
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
import androidx.core.content.ContextCompat
import androidx.core.view.ViewCompat
import androidx.core.view.WindowInsetsCompat
import com.ccmonitor.mobile.core.claude.link.wire.AgentPick
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.db.Identity
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyDao
import com.ccmonitor.mobile.core.data.db.KeyGroup
import com.ccmonitor.mobile.core.data.db.appActionName
import com.ccmonitor.mobile.core.data.repo.SettingsRepository
import com.ccmonitor.mobile.core.ssh.SessionStatus
import com.ccmonitor.mobile.core.ssh.SshConnectEvent
import com.ccmonitor.mobile.core.terminal.CcMonitorTerminal
import com.ccmonitor.mobile.core.terminal.TerminalSession
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.AppSnackbarController
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.feedback.Notice
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.service.AppForeground
import com.ccmonitor.mobile.service.SshKeepAliveService
import com.ccmonitor.mobile.ui.common.CopyDetailButton
import com.ccmonitor.mobile.ui.copyPlainText
import com.ccmonitor.mobile.ui.pastePlainText
import com.ccmonitor.mobile.ui.theme.AppTerminalPalette
import kotlinx.coroutines.flow.combine
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.launch
import org.koin.androidx.compose.koinViewModel
import org.koin.compose.koinInject
import org.koin.core.parameter.parametersOf
import kotlin.math.roundToInt

/**
 * 连接指定主机并渲染实时终端。连接编排在 [SessionViewModel]，这里只渲染。
 * 终端屏只有终端：看一条会话的正文回聊天屏。
 */
@Composable
fun SessionScreen(
    hostId: String,
    cd: String? = null,
    tmuxSession: String? = null, // 进终端 tab 首连即接回的会话名；普通 tab 为 null
    resume: String? = null, // 终端没了的那条会话：首连先让那台恢复再接回；普通 tab 为 null
    onAttachInNewTab: (name: String) -> Unit = {}, // tmux「附着」：开新终端 tab
    onShowTabs: () -> Unit = {}, // 唤出 tab 面板
    onEditKeys: () -> Unit, // 进编辑按键页（⋯「编辑按键」· 绑了「编辑按键」的触发 · 空组那一行）
) {
    CompositionLocalProvider(LocalKeysEnv provides KeysEnv(onEditKeys)) {
        SessionContent(hostId, cd, tmuxSession, resume, onAttachInNewTab, onShowTabs)
    }
}

/** 终端屏深处的键条要用、又不该一层层往下传的：进编辑按键页。 */
private data class KeysEnv(
    val onEditKeys: () -> Unit,
)

private val LocalKeysEnv = staticCompositionLocalOf<KeysEnv> { error("LocalKeysEnv not provided") }

@Composable
private fun SessionContent(
    hostId: String,
    cd: String?,
    tmuxSession: String?,
    resume: String?,
    onAttachInNewTab: (name: String) -> Unit,
    onShowTabs: () -> Unit,
) {
    // Koin 参数不能传 null，可空参数一律用空串表示 null。
    val vm =
        koinViewModel<SessionViewModel> {
            parametersOf(hostId, cd ?: "", tmuxSession ?: "", resume ?: "")
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
        ErrorLine(copyText("mobSession.connect.failed"))
        // 下层给的原话（或这一侧判出的原因）不上屏，进复制详情；连接日志照旧给原文。
        CopyDetailButton(copyText("mobSession.connect.failed"), message)
        if (log.isNotEmpty()) ConnectLogView(log, Modifier.fillMaxWidth().weight(1f))
        SecondaryButton(onClick = { vm.retry() }) { Text(copyText("mobSession.connect.retry")) }
    }
}

/**
 * 非连接态（连接中 / 失败 / 选身份）顶部的 tab 按钮。这些态没有工具条，靠它在慢连、断开、待选身份时
 * 仍能唤出 tab 面板切换或关闭会话。
 */
@Composable
private fun SessionStateTabButton(onShowTabs: () -> Unit) {
    Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.Start) {
        PhIconButton(Ph.Tabs, copyText("mobTabs.open.aria"), onClick = onShowTabs, tint = MaterialTheme.colorScheme.onSurfaceVariant)
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
            SecondaryButton(onClick = { onPick(id.id) }) { Text(id.label.ifBlank { id.id }) }
        }
    }
}

/**
 * 主机指纹变更的恢复页：与桌面那张比对框同一套字（标题 · 提醒 · 记下的 / 当前两枚 · 信任新的指纹）。
 *
 * 做成一整屏而不是一行红字：这是唯一一个「连接被拒但能做点什么」的失败态，触发它的又常是合法操作
 * （重装系统、重建 VPS、轮换密钥）；删掉机器重加也没用（主机钥匙那份库按地址认，不随主机删）。
 * 这一页本身就是那一次确认：按「信任新的指纹」直接改记并重连，不再弹第二个框。
 *
 * 首次连接故意静默接受：对不会核对指纹的人，首次弹窗只会训练他闭眼点通过。唯一一次打断留给真正异常的时刻。
 */
@Composable
private fun HostKeyChangedContent(
    state: SessionUiState.HostKeyChanged,
    onShowTabs: () -> Unit,
    onTrustNew: () -> Unit,
) {
    val none = copyText("hostKey.compare.none")
    Column(
        Modifier.fillMaxSize().statusBarsPadding().padding(24.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        SessionStateTabButton(onShowTabs)
        Text(
            copyText("hostKey.compare.title", "machine" to LocalKeysEnv.current.hostLabel),
            style = MaterialTheme.typography.titleMedium,
            color = LocalAppTokens.current.errorText,
        )
        Text(copyText("hostKey.compare.caution"), style = MaterialTheme.typography.bodyMedium)
        // 两枚指纹给会核对的人，放在提醒之下、小字
        listOf(
            copyText("hostKey.compare.pinned") to state.expectedFingerprint.ifBlank { none },
            copyText("hostKey.compare.seen") to state.offeredFingerprint.ifBlank { none },
        ).forEach { (label, value) ->
            Column {
                Text(label, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(value, style = MaterialTheme.typography.bodySmall)
            }
        }
        SecondaryButton(onClick = onTrustNew) { Text(copyText("hostKey.compare.trust")) }
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
                        LocalAppTokens.current.errorText
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
        s = s,
        vm = vm,
        onReconnect = onReconnect,
        onShowTmux = { showTmux = true },
        onPushKey = { showKeyPush = true },
        onCaptured = { captureText = it },
        onShowTabs = onShowTabs,
    )
    // 迁移说明：手机原来存的启动配置，这一台的列一次。
    val moved by vm.moved.collectAsState()
    if (moved.isNotEmpty()) MovedLaunchDialog(moved, vm::makeAlias, vm::ackMoved)
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
    // 「用哪个启动」那一排：这台的别名，按过一个就收起。
    val aliases by vm.aliases.collectAsState()
    val aliasPicked by vm.aliasPicked.collectAsState()
    Column(Modifier.fillMaxSize().statusBarsPadding()) {
        if (dropped) DroppedBanner(status, onReconnect)
        if (!aliasPicked) LaunchAliasRow(aliases, vm::startAlias)
        Box(Modifier.fillMaxWidth().weight(1f)) {
            ConnectedTerminal(s, vm, onShowTmux, onPushKey, onCaptured, onShowTabs)
        }
    }
}

/** 掉线条幅与重连按钮。 */
@Composable
private fun DroppedBanner(
    status: SessionStatus?,
    onReconnect: () -> Unit,
) {
    // 错误条：`--card` 底（errorContainer）＋ 左 3 dp `--error` 条，字用正文色。
    Row(
        Modifier.fillMaxWidth().background(MaterialTheme.colorScheme.errorContainer).padding(end = 12.dp),
        horizontalArrangement = Arrangement.SpaceBetween,
        verticalAlignment = Alignment.CenterVertically,
    ) {
        ErrorLine(
            if (status == SessionStatus.RECONNECTING) copyText("mobSession.bar.reconnecting") else copyText("mobSession.bar.dropped", "machine" to LocalKeysEnv.current.hostLabel),
            modifier = Modifier.weight(1f).padding(vertical = 12.dp),
        )
        SecondaryButton(onClick = onReconnect) { Text(copyText("mobSession.connect.retry")) }
    }
}

/** 终端与它的动作接线。 */
@Composable
private fun ConnectedTerminal(
    s: SessionUiState.Connected,
    vm: SessionViewModel,
    onShowTmux: () -> Unit,
    onPushKey: () -> Unit,
    onCaptured: (String) -> Unit,
    onShowTabs: () -> Unit,
) {
    // tmux 会话 · 切模型 · 抓屏要那台有 tmux，推公钥要那台后端连着：都照那台说的（[Reach]）。
    val reach by vm.reach.collectAsState()
    // 本 tab 当前所在的 tmux 会话名（切模型 · 抓屏按它找那台名单里的终端）。动作 lambda 在点击时经 getter 读最新值。
    val currentTmux by vm.tmuxSession.collectAsState()
    val onSwitchModel = rememberModelSwitchAction(vm.terminals) { currentTmux }
    // 面板「模型」组：那个终端里跑着的那一家能换的模型（那台给的）；没跑会话 ⇒ 空、这一组不出。
    val models by androidx.compose.runtime.produceState(emptyList<AgentPick>(), currentTmux) {
        value = currentTmux?.let { vm.terminals.modelsIn(it) }.orEmpty()
    }
    val onCapture = rememberCaptureAction(vm.terminals, { currentTmux }, onCaptured)
    TerminalTab(
        session = s.session,
        onShowTmux = onShowTmux,
        models = models,
        onModel = onSwitchModel,
        onCapture = onCapture,
        onPushKey = onPushKey,
        onShowTabs = onShowTabs,
        reach = reach,
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
    if (showTmux) {
        TmuxManagerDialog(
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
    PushKeyControl(showKeyPush, identities, vm.terminals, onDismissKeyPush)
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

/** 公钥推送对话框的宿主：选身份，请那台并进它的 `authorized_keys`，照那台的结局说一句。 */
@Composable
private fun PushKeyControl(
    show: Boolean,
    identities: List<Identity>,
    terminals: TerminalOps,
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
                scope.launch {
                    terminals
                        .pushKey(id.publicKeyOpenSsh)
                        .onSuccess { Toast.makeText(context, it, Toast.LENGTH_SHORT).show() }
                        .onFailure { snackbar.show(Notice(copyText("mobKeyPush.result.failed"), it.message)) }
                }
            }
        },
    )
}

/** 选一个身份，把它的 OpenSSH 公钥推到这台主机的 `authorized_keys`（那台后端写）。 */
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
                        SecondaryButton(onClick = { onPick(id) }) { Text(id.label.ifBlank { id.id }) }
                    }
                }
            }
        },
        confirmButton = {},
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("mobKit.act.close")) } },
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
        confirmButton = { SecondaryButton(onClick = onCopy) { Text(copyText("mobCapture.dialog.copy")) } },
        dismissButton = { SecondaryButton(onClick = onDismiss) { Text(copyText("mobKit.act.close")) } },
    )
}

/** 终端 tab：termlib 渲染，加工具条与键条。 */
@Composable
private fun TerminalTab(
    session: TerminalSession,
    onShowTmux: () -> Unit,
    models: List<AgentPick>,
    onModel: (String) -> Unit,
    onCapture: () -> Unit,
    onPushKey: () -> Unit,
    onShowTabs: () -> Unit,
    reach: Reach,
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
            palette = AppTerminalPalette,
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
            reach = reach,
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

/** 终端底部控件区：固定工具条、键条（当前那一组）、命令面板。 */
@Composable
private fun TerminalBottomControls(
    session: TerminalSession,
    reach: Reach,
    showKeyboard: () -> Unit,
    onShowTabs: () -> Unit,
    onShowTmux: () -> Unit,
    models: List<AgentPick>,
    onModel: (String) -> Unit,
    onCapture: () -> Unit,
    onPushKey: () -> Unit,
) {
    val keyDao = koinInject<KeyDao>()
    val settings = koinInject<SettingsRepository>()
    val groups by remember { keyDao.observeGroups() }.collectAsState(initial = emptyList())
    val savedGroup by remember { settings.observeCurrentKeyGroup() }.collectAsState(initial = null)
    val doubleTapMs by remember { settings.observeKeyDoubleTapMs() }.collectAsState(initial = null)
    val longPressMs by remember { settings.observeKeyLongPressMs() }.collectAsState(initial = null)
    // 当前组记在这台手机上；没记过 · 那组已删 ⇒ 第一组。一个组都没有 ⇒ 键条整条不画。
    val current = currentGroup(groups, savedGroup)
    var showPalette by rememberSaveable { mutableStateOf(false) }
    var showMore by rememberSaveable { mutableStateOf(false) }
    val env = LocalKeysEnv.current
    val mods by session.modifiers.states.collectAsState()
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    val onPaste: () -> Unit = {
        scope.launch { clipboard.pastePlainText()?.takeIf { it.isNotEmpty() }?.let { session.send(it.toByteArray()) } }
    }
    val switchTo: (String) -> Unit = { id -> scope.launch { settings.setCurrentKeyGroup(id) } }
    val actions =
        KeyActions(
            noTmux = reach.noTmux,
            noBackend = reach.noBackend,
            onShowTabs = onShowTabs,
            onCommandPalette = { showPalette = true },
            onTmuxSessions = onShowTmux,
            onPaste = onPaste,
            onCapture = onCapture,
            onPushKey = onPushKey,
            onShowKeyboard = showKeyboard,
            onEditKeys = env.onEditKeys,
            onNextGroup = { nextGroupId(groups, savedGroup)?.let(switchTo) },
            onGotoGroup = { id -> groups.any { it.id == id }.also { if (it) switchTo(id) } },
        )
    TerminalToolbar(
        supportsTmux = reach.noTmux == null,
        onShowTabs = onShowTabs,
        onCommandPalette = { showPalette = true },
        onTmuxSessions = onShowTmux,
        onMore = { showMore = true },
    )
    KeyBar(keyDao, KeyBarNav(groups, current, switchTo, LocalAppSnackbar.current, env), showMore, onDismissMore = { showMore = false }) { g, nav ->
        ExtraKeysRow(
            groupId = g.id,
            dispatch = { b -> dispatchBinding(b, session::send, actions, session.modifiers::take)?.let { f -> nav.fail(f, b) } },
            onModifier = { m, gesture -> session.modifiers.on(m.term(), gesture) },
            mods = mods,
            doubleTapMs = doubleTapMs,
            longPressMs = longPressMs,
            groupName = { id -> groups.firstOrNull { it.id == id }?.name },
        )
    }
    // 命令面板：模型、抓屏、tmux 会话、推公钥整合成可搜索的动作。
    if (showPalette) {
        val paletteActions = terminalPaletteActions(reach, models, onModel, onCapture, onShowTmux, onPushKey)
        CommandPalette(actions = paletteActions, onDismiss = { showPalette = false })
    }
}

/**
 * 键条那一块要的：组（照先后）· 当前组 · 切组 · 做不成时那一句。做不成的出不自己走的 toast（点了才关）：
 * 切到的组已删 ⇒ 带「编辑」；那台做不了 ⇒ 写哪个动作、为什么。
 */
private class KeyBarNav(
    val groups: List<KeyGroup>,
    val current: KeyGroup?,
    val switchTo: (String) -> Unit,
    private val snackbar: AppSnackbarController,
    private val env: KeysEnv,
) {
    fun fail(
        f: KeyActionFail,
        b: KeyBinding,
    ) = when (f) {
        KeyActionFail.GroupGone -> snackbar.showSticky(copyText("mobKeys.err.groupGone"), copyText("mobKeys.err.edit"), env.onEditKeys)
        is KeyActionFail.Unavailable -> {
            val action = (b as? KeyBinding.App)?.action?.let(::appActionName).orEmpty()
            snackbar.showSticky(copyText("mobKeys.err.actionUnavailable", "action" to action, "why" to f.why))
        }
    }
}

/** ⋯ 抽屉 ＋ 键条（当前那一组，[row] 画）；当前组没有按键 ⇒ 一行灰字 ＋「编辑按键」；一个组都没有 ⇒ 整条不画。 */
@Composable
private fun KeyBar(
    keyDao: KeyDao,
    nav: KeyBarNav,
    showMore: Boolean,
    onDismissMore: () -> Unit,
    row: @Composable (KeyGroup, KeyBarNav) -> Unit,
) {
    val env = LocalKeysEnv.current
    val allKeys by remember { keyDao.observeAllKeys() }.collectAsState(initial = null)
    val counts = allKeys.orEmpty().groupingBy { it.groupId }.eachCount()
    val current = nav.current
    if (showMore) TerminalMoreSheet(nav.groups, counts, current, onSwitch = nav.switchTo, onEditKeys = env.onEditKeys, onDismiss = onDismissMore)
    current ?: return
    if (allKeys != null && counts[current.id] == null) {
        EmptyGroupLine(current.name, env.onEditKeys)
        return
    }
    row(current, nav)
}

/**
 * 命令面板的动作集：要 tmux 的（切模型 / 抓屏 / tmux 会话）那台没有 tmux 就不出；推公钥要那台后端，没连上 ⇒ 灰着、写原因。
 */
internal fun terminalPaletteActions(
    reach: Reach,
    models: List<AgentPick>,
    onModel: (String) -> Unit,
    onCapture: () -> Unit,
    onShowTmux: () -> Unit,
    onPushKey: () -> Unit,
): List<PaletteAction> =
    buildList {
        if (reach.noTmux == null) {
            // 那台给的可选模型（名字照那台写好的字，副标题是原值）。
            models.forEach { m -> add(PaletteAction(copyText("mobPalette.group.model"), m.text, m.value) { onModel(m.value) }) }
            add(PaletteAction(copyText("mobPalette.group.capture"), copyText("mobPalette.item.capture")) { onCapture() })
            add(PaletteAction(copyText("mobPalette.group.tmux"), copyText("mobPalette.item.tmux")) { onShowTmux() })
        }
        add(PaletteAction(copyText("mobPalette.group.key"), copyText("mobPalette.item.key"), reach.noBackend, onPushKey.takeIf { reach.noBackend == null }))
    }

/** 当前组没有按键：键条位置一行灰字「「git」无按键」＋ 一颗文字按钮「编辑按键」（C16 至多一颗）。 */
@Composable
private fun EmptyGroupLine(
    name: String,
    onEditKeys: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier.fillMaxWidth().background(tokens.bg2).padding(start = tokens.spacing.lg),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(
            copyText("mobKeys.empty.group", "name" to name),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.weight(1f),
        )
        SecondaryButton(onClick = onEditKeys) { Text(copyText("mobKeys.action.editKeys")) }
    }
}
