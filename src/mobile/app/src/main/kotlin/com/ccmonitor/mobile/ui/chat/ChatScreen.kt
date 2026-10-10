package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.bridge.CommandCatalog
import com.ccmonitor.mobile.core.claude.model.searchableText
import com.ccmonitor.mobile.core.claude.transport.BlockedSignal
import com.ccmonitor.mobile.core.claude.transport.RateLimitReading
import com.ccmonitor.mobile.core.claude.transport.WaitingCopy
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ui.claude.ClaudeReadingPane
import com.ccmonitor.mobile.ui.session.CommandPalette
import kotlinx.coroutines.launch

/**
 * 聊天面。渲染直接用 [ClaudeReadingPane]：两个生产者、一个渲染器，`RenderUnit` 加一种只改一处。
 */
@Suppress("DEPRECATION") // LocalClipboard 是新 API 但只有 suspend 接口
@Composable
fun ChatScreen(
    state: ChatUiState,
    onSend: (String) -> Unit,
    onStop: () -> Unit,
    modifier: Modifier = Modifier,
    /** 上滑到顶时请求再载更早的一段。 */
    onLoadOlder: () -> Unit = {},
    /** 重发一条失败的消息（传本地 key）。 */
    onRetrySend: (String) -> Unit = {},
    /**
     * 加附件。回调返回要插进草稿的引用（`@远端路径 `），null = 取消或失败。
     * 插草稿而不直接发：附件几乎总要配一句话。为 null 时不出这个键。
     */
    onAttach: (suspend () -> String?)? = null,
    /** 重新接上下行。为 null 时不出那个入口；没有它，内容接收结束或停止显示后只能退出重进。 */
    onReattach: (() -> Unit)? = null,
    /**
     * 「我知道，还是发」：等待态下随时可用的出口。
     *
     * 注意：为 null 时输入框也不禁（见 [inputBlockedBy]）。禁用与出口绑死，摘掉出口禁用也一起没了。
     */
    onSendAnyway: ((String) -> Unit)? = null,
) {
    val listState = rememberLazyListState()
    val clipboard = LocalClipboardManager.current

    // 前面插入历史后滚动位置靠 Compose 按 key 自己恢复，前提是 `LazyColumn` 用内容派生的稳定 key。

    // 到顶 ⇒ 请求更早的一段。
    // 注意：判据必须整个进 `derivedStateOf`；漏在效果体里的变量不在 key 里，效果拿过时快照判一次就不再重跑。
    // 内容不满一屏时会先把视口填满，填满后 `firstVisibleItemIndex` 不再恒为 0，自然停止。
    val readyToLoadOlder by remember {
        derivedStateOf {
            val info = listState.layoutInfo
            info.visibleItemsInfo.isNotEmpty() && listState.firstVisibleItemIndex == 0
        }
    }
    // 空列表也要能载：`--resume` 不回放历史，打开已有对话时列表是空的，[readyToLoadOlder] 恒假。
    // 空判据放在这里而不进 [readyToLoadOlder]：那个 `derivedStateOf` 没有 key，捕进 `state` 就是过时快照；
    // `state.units.size` 本来就在这里的 key 里。
    LaunchedEffect(readyToLoadOlder, state.canLoadOlder, state.loadingOlder, state.units.size) {
        // 「该不该现在去载」= 到顶了（或还没有内容）且允许载且没在载
        val atTop = readyToLoadOlder || state.units.isEmpty()
        val mayLoad = state.canLoadOlder && !state.loadingOlder
        if (atTop && mayLoad) onLoadOlder()
    }

    // 自动滚底。触发看「最后一个单元的 key + 内容长度」而不是 `units.size`：流式期间数量不变，
    // 而往顶部插历史会改 size。只在用户本来就贴着底时才滚，往上翻看历史时不拽回去。
    val lastUnit = state.units.lastOrNull()
    LaunchedEffect(lastUnit?.key, lastUnit?.searchableText()?.length) {
        val info = listState.layoutInfo
        val atBottom =
            info.visibleItemsInfo.isEmpty() ||
                info.visibleItemsInfo.last().index >= state.units.lastIndex - 1
        if (atBottom && state.units.isNotEmpty()) listState.animateScrollToItem(state.units.lastIndex)
    }

    Column(modifier.fillMaxSize()) {
        // 等待条钉在最上面、不可关：关掉之后就不知道为什么发不出去。
        WaitingBar(state.waiting)
        HistoryStatusBar(state)

        ClaudeReadingPane(
            units = state.units,
            modifier = Modifier.weight(1f).testTag(ChatTestTags.PANE),
            listState = listState,
            // 不传的话「复制代码」按钮照样渲染但点了没反应。
            onCopyCode = { clipboard.setText(AnnotatedString(it)) },
            onRetrySend = onRetrySend,
        )

        state.failedWhy?.let {
            // 失败不能静默：中断或报错要看得见。
            Text(
                text = "本轮未正常结束：$it",
                modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp),
            )
        }

        // 注意：这里刻意没有「发送失败」的横幅，失败原因贴在那条消息旁边（`UserText.deliveryError`）。
        // 单槽横幅会说假话：A 失败、B 成功后槽被清空，A 仍挂着「未送达」。

        BlockedLine(state.blocked)

        SessionModeLine(state)
        ReattachRow(state, onReattach)

        PermissionModeChip(state.permissionMode)
        QuotaLine(state.quota)

        ChatInputRow(
            streaming = state.streaming,
            sending = state.sending,
            catalog = state.catalog,
            // 只禁上行，不冻整屏：停止、上翻、返回、重新接上都照常。没有逃生口就不禁，见 [inputBlockedBy]。
            blocked = inputBlockedBy(state, onSendAnyway != null),
            onAttach = onAttach,
            onSend = onSend,
            onSendAnyway = onSendAnyway,
            onStop = onStop,
        )
    }
}

/**
 * 输入框该不该禁：手上的等待态真要拦（[WaitingNotice.blocksSending]，过期读数只提示不拦），且有逃生口。
 * 后一条把禁用与出口绑死，摘掉出口不会留下走不出去的屏。
 */
internal fun inputBlockedBy(
    state: ChatUiState,
    hasEscape: Boolean,
): WaitingNotice? = state.waiting?.takeIf { it.blocksSending && hasEscape }

/**
 * 等待条：钉在顶栏下、不可关。文案映射在 [WaitingCopy] 里做（[WaitingCopy.headlineFor] +
 * [WaitingCopy.ANSWER_IT_ON_THE_COMPUTER]），英文机器码不上屏。读数过期、不拦时也要显示。
 */
@Composable
private fun WaitingBar(notice: WaitingNotice?) {
    if (notice == null) return
    Surface(tonalElevation = 3.dp) {
        Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 6.dp).testTag(ChatTestTags.WAITING)) {
            Text("◆ ${notice.headline}", style = MaterialTheme.typography.bodyMedium)
            Text(notice.detail, style = MaterialTheme.typography.bodySmall)
        }
    }
}

/**
 * 「它想做的一件事被挡住了」。不是等待态：那件事已经过去，Claude 已接着往下走。
 *
 * 形态：`system/permission_denied` 帧带 `tool_name` 与一句英文 `message`。这条帧还没在真实会话里见过，
 * 帧不来就什么都不画。`message` 是远端原文，不翻译：它是用户唯一能自己判断的东西。
 */
@Composable
private fun BlockedLine(blocked: BlockedSignal?) {
    if (blocked == null) return
    val what = blocked.toolName?.let { "它想用 $it，被挡住了" } ?: "它想做的一件事被挡住了"
    Text(
        text = blocked.humanText?.let { "$what：$it" } ?: what,
        style = MaterialTheme.typography.bodySmall,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp).testTag(ChatTestTags.BLOCKED),
    )
}

/**
 * 配额行。[RateLimitReading.windows] 为空时整条不渲染：「没有百分比」不等于 0%，不编 0%，也不画空进度条。
 * 快满时不弹窗，只在行尾加一句（见 [quotaTailFor]）：配额是渐变的事实，弹窗会打断读回复。
 */
@Composable
private fun QuotaLine(quota: RateLimitReading?) {
    val window = quota?.tightest ?: return
    val pct = (window.utilization * PERCENT).toInt()
    Text(
        text = "配额：${window.name} 已用 $pct%${quotaTailFor(window.utilization)}",
        style = MaterialTheme.typography.bodySmall,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 2.dp).testTag(ChatTestTags.QUOTA),
    )
}

private const val PERCENT = 100

/**
 * 配额行末尾那句。超过 100% 是真实形状（同一帧 `rate_limit_info.status = "rejected"`），
 * 已被拒的账号不该读到「快用完了」，所以分三档。
 */
internal fun quotaTailFor(utilization: Double): String =
    when {
        utilization >= QUOTA_OVER -> " · 已经超了，得等它重置"
        utilization >= QUOTA_TIGHT -> " · 快用完了"
        else -> ""
    }

/** 「快用完了」的门槛。低于它提醒太频，高于它来不及。 */
private const val QUOTA_TIGHT = 0.9

/** 「已经超了」的门槛：1.0 就是用满。 */
private const val QUOTA_OVER = 1.0

/**
 * 会话状态行：说出用户不知道就会误解屏幕的事。没接上行时说「只回显」：点发送只在本地上屏。
 * 停止有四态，互不冒充：
 *
 * | 状态 | 说什么 | 为什么不能合并 |
 * |---|---|---|
 * | [RemoteStop.Requested] | [REMOTE_STOP_PENDING] | 中断只是投出去了，说「已经停下」是假报 |
 * | [RemoteStop.Confirmed] | [REMOTE_STOP_CONFIRMED] | 证据是远端产出的字节（`res.terminal_reason=aborted_streaming`） |
 * | [RemoteStop.Failed] | [STOPPED_LOCALLY_ONLY] + 原因 | 投不出去 ⇒ 退回本地停显示，原因不许吞 |
 * | [RemoteStop.None] + `stoppedByUser` | [STOPPED_LOCALLY_ONLY] | 没接上行，或用户按第二下不等了 |
 *
 * `stoppedByUser` 排在两条远端态之前：走到它的两条路都表示本地已不再跟着看，
 * 再说「正在让远端停下…」会让用户以为屏幕还会自己动。
 */
@Composable
private fun SessionModeLine(state: ChatUiState) {
    val text = sessionModeText(state) ?: return
    Text(
        text = text,
        style = monoSmall,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 2.dp).testTag(ChatTestTags.SESSION_MODE),
    )
}

/** 会话状态行说哪句话，null = 什么都不说。抽成纯函数好让单元测试钉住四态不被合并。 */
internal fun sessionModeText(state: ChatUiState): String? =
    when {
        // 下行断了排最前：比「已停止」「只回显」更要紧，不能静默。已收到的内容仍留在屏上。
        // 文案不出现「连接 / SSH / tmux / 会话」等技术词。
        state.downlinkError != null -> "内容中断了：${state.downlinkError} —— 上面已经收到的还在"
        // 投不出去时补上原因，否则 `TmuxSendKeysSink` 那句「这条路停不了」就被吞了。
        state.stoppedByUser ->
            (state.remoteStop as? RemoteStop.Failed)
                ?.let { "$STOPPED_LOCALLY_ONLY（${it.why}）" } ?: STOPPED_LOCALLY_ONLY
        state.remoteStop is RemoteStop.Confirmed -> REMOTE_STOP_CONFIRMED
        state.remoteStop is RemoteStop.Requested -> REMOTE_STOP_PENDING
        !state.uplinkAttached -> "只回显：未接上行，消息不会送到远端"
        else -> null
    }

/** 只停到本地时说的话。走到它的两条路（没接上行、中断投不出去或用户不等了）都让它字面为真。 */
internal const val STOPPED_LOCALLY_ONLY = "已停止显示 —— 远端那一轮可能还在跑"

/** 中断投出去了、还没等到确认时的话。注意：说「正在让」而不是「已经」，否则就是假报。 */
internal const val REMOTE_STOP_PENDING = "正在让远端停下…"

/** 远端自己说这一轮停了（`res.terminal_reason=aborted_streaming`）时的话；背后是远端产出的字节。 */
internal const val REMOTE_STOP_CONFIRMED = "远端那一轮已经停下了"

/**
 * 权限模式指示，只显示、不能选。
 *
 * `init` 帧里只有当前的 `permissionMode`（单数），没有可选模式列表，也没有会话中途改模式的上行动作。
 * 硬编码一份列表会在远端加了新模式时谎报。
 */
@Composable
private fun PermissionModeChip(current: String?) {
    if (current.isNullOrBlank()) return
    Text(
        text = current,
        style = monoSmall,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 2.dp).testTag(ChatTestTags.PERMISSION_MODE),
    )
}

/** 顶部状态条：正在载 / 已到最早 / 失败，三态互斥。到顶必须说出来，否则用户会以为卡了。 */
@Composable
private fun HistoryStatusBar(state: ChatUiState) {
    val base = Modifier.fillMaxWidth().padding(8.dp)
    when {
        state.historyError != null ->
            Text(
                "载入更早的对话失败：${state.historyError}（再上滑一次可重试）",
                modifier = base.testTag(ChatTestTags.HISTORY_ERROR),
            )

        state.loadingOlder ->
            Text("正在载入更早的对话…", modifier = base.testTag(ChatTestTags.LOADING_OLDER))

        // 只有接了翻页才谈得上「到顶」，没接的屏不该常驻一条假横幅。
        state.historyPagingAttached && !state.canLoadOlder && state.units.isNotEmpty() ->
            Text("已是最早", modifier = base.testTag(ChatTestTags.NO_MORE_HISTORY))
    }
}

/** 输入行：输入框 + 发送/停止（二选一，不并存）。 */
@Composable
private fun ChatInputRow(
    streaming: Boolean,
    sending: Boolean,
    catalog: CommandCatalog,
    blocked: WaitingNotice?,
    onAttach: (suspend () -> String?)?,
    onSend: (String) -> Unit,
    onSendAnyway: ((String) -> Unit)?,
    onStop: () -> Unit,
) {
    // `rememberSaveable`：转屏会重建 Activity，`remember` 的草稿会丢光。
    var draft by rememberSaveable { mutableStateOf("") }
    // 选择器开关；转屏不该把已打开的面板关掉。
    var paletteOpen by rememberSaveable { mutableStateOf(false) }
    if (paletteOpen) {
        // 按 catalog `remember`，否则草稿每变一个字就重建整份列表；插入回调读 `draft` 的当前值，lambda 不用进 key。
        val actions =
            remember(catalog) {
                // 选中插进草稿而不直接发：斜杠命令常要带参数（`/model <名>`、`/compact <说明>`）。
                catalog.toPaletteActions { draft = draft.appendToken(it) }
            }
        CommandPalette(actions = actions, onDismiss = { paletteOpen = false })
    }
    Surface(tonalElevation = 2.dp) {
        Column(Modifier.fillMaxWidth()) {
            // 输入框上方那句与模态拒绝路径共用同一个常量：同一事实的两个时刻，说两句会被当成两种故障。
            if (blocked != null) {
                Row(
                    modifier = Modifier.fillMaxWidth().padding(horizontal = 8.dp),
                    verticalAlignment = Alignment.CenterVertically,
                ) {
                    Text(
                        WaitingCopy.SENDING_NOW_ANSWERS_THE_QUESTION,
                        style = MaterialTheme.typography.bodySmall,
                        modifier = Modifier.weight(1f),
                    )
                    // 出口随时可用，不等等待态自己解除。
                    TextButton(
                        onClick = {
                            onSendAnyway?.invoke(draft)
                            draft = ""
                        },
                        enabled = draft.isNotBlank(),
                        modifier = Modifier.testTag(ChatTestTags.SEND_ANYWAY),
                    ) {
                        Text("我知道，还是发")
                    }
                }
            }
            InputControls(
                streaming = streaming,
                sending = sending,
                hasCatalog = !catalog.isEmpty,
                blocked = blocked != null,
                draft = draft,
                onDraft = { draft = it },
                onPalette = { paletteOpen = true },
                onAttach = onAttach,
                onSend = onSend,
                onStop = onStop,
            )
        }
    }
}

/** 输入行本体。[blocked] 只关输入与发送，停止键照常：等待不是断线。 */
@Composable
private fun InputControls(
    streaming: Boolean,
    sending: Boolean,
    hasCatalog: Boolean,
    blocked: Boolean,
    draft: String,
    onDraft: (String) -> Unit,
    onPalette: () -> Unit,
    onAttach: (suspend () -> String?)?,
    onSend: (String) -> Unit,
    onStop: () -> Unit,
) {
    Row(
        modifier = Modifier.fillMaxWidth().padding(8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(4.dp),
    ) {
        run {
            // catalog 为空（还没收到 `init`）就不出这个键。
            if (hasCatalog) {
                TextButton(
                    onClick = onPalette,
                    modifier = Modifier.testTag(ChatTestTags.CATALOG),
                ) {
                    Text("/")
                }
            }
            AttachButton(onAttach) { onDraft(draft.appendToken(it)) }
            OutlinedTextField(
                value = draft,
                onValueChange = onDraft,
                // 禁用而不隐藏：这个状态会好起来（电脑上答完就恢复）。
                enabled = !blocked,
                modifier = Modifier.weight(1f).testTag(ChatTestTags.INPUT),
                placeholder = { Text("说点什么…") },
                singleLine = false,
                maxLines = INPUT_MAX_LINES,
            )
            // 用文字按钮，不为两个图标引入 material-icons。
            // 判据是 `streaming || sending`：`streaming` 是轮次状态（由 `res` 决定），只看它的话
            // `res` 到达后再发一条时界面上没有停止键。
            if (streaming || sending) {
                TextButton(onClick = onStop, modifier = Modifier.testTag(ChatTestTags.STOP)) {
                    Text("停止")
                }
            } else {
                TextButton(
                    onClick = {
                        onSend(draft)
                        onDraft("")
                    },
                    // 等待态下发送键也灰。注意：真正的拦截在 `ChatSession.send()`，这里只是外观。
                    enabled = draft.isNotBlank() && !blocked,
                    modifier = Modifier.testTag(ChatTestTags.SEND),
                ) {
                    Text("发送")
                }
            }
        }
    }
}

private const val INPUT_MAX_LINES = 4

/** 加附件的键。没接就不出；引用插进草稿而不直接发。 */
@Composable
private fun AttachButton(
    onAttach: (suspend () -> String?)?,
    onReference: (String) -> Unit,
) {
    if (onAttach == null) return
    val scope = rememberCoroutineScope()
    TextButton(
        onClick = { scope.launch { onAttach()?.let(onReference) } },
        modifier = Modifier.testTag(ChatTestTags.ATTACH),
    ) {
        Text("＋")
    }
}

/** 下行断了或停了之后「重新接上」的入口。文案不说「连接 / 重连」。 */
@Composable
private fun ReattachRow(
    state: ChatUiState,
    onReattach: (() -> Unit)?,
) {
    if (onReattach == null || !state.canReattach) return
    // 只在「确实断了或停了」时出 —— 一条还没开始的对话不该显示它
    if (state.downlinkError == null && !state.stoppedByUser) return
    TextButton(
        onClick = onReattach,
        modifier = Modifier.padding(horizontal = 16.dp).testTag(ChatTestTags.REATTACH),
    ) {
        Text("重新接上内容")
    }
}
