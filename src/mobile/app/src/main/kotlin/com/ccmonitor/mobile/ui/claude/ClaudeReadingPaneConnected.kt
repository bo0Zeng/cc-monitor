package com.ccmonitor.mobile.ui.claude

import android.widget.Toast
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.core.claude.catalog.ClaudeSessionCatalog
import com.ccmonitor.mobile.core.claude.catalog.SessionLight
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.model.UsageSummary
import com.ccmonitor.mobile.core.claude.model.contextLimitFor
import com.ccmonitor.mobile.core.claude.model.searchableText
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel
import com.ccmonitor.mobile.core.remote.RemoteExecutor
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ui.copyPlainText
import kotlinx.coroutines.launch
import org.koin.androidx.compose.koinViewModel
import org.koin.core.parameter.parametersOf

/**
 * 经第二条 exec 通道实时 tail 远端会话 JSONL → 解析 → 主分支 → 分类 → [ClaudeReadingPane]。
 * 数据层（会话发现与 tail 管线）在 [ReadingPaneViewModel]，`state` 以 `WhileSubscribed(5s)` 续命，
 * 切回终端 5s 内回来不重 tail。本 Composable 只管渲染、搜索 UI 与滚动。
 *
 * 顶部工具条是会话选择器加会话内搜索；列表在底部时自动跟随，可跳最新；代码块可复制。
 */
@Composable
fun ClaudeReadingPaneConnected(
    channel: RemoteCommandChannel,
    // 同一条连接的一次性执行（带退出码），截断检测用，见 `RemoteExecutor`。
    executor: RemoteExecutor,
    cwd: String?,
    claudeDir: String?,
    // 本会话 agent 种类：决定 parser / usage / locator、排序与 VM key。
    agentKind: AgentKind = AgentProfile.DEFAULT.kind,
    modifier: Modifier = Modifier,
    // 连接死否由错误态 [ClaudeReadState.Error.connectionDead] 携带（exec 抛 ConnectionDeadException 或 tail EOF 时为 true），见 onRetry。
    onReconnectSsh: () -> Unit = {}, // 连接已死时驱动整条 SSH/会话重连（即 SessionViewModel.retry）
    // 钉住任意历史 JSONL（历史页「点击=阅读」）：非空则 VM 首载即 tail 该 path，下拉并入钉住项。
    pinnedPath: String? = null,
    pinnedLabel: String? = null,
    // resume 当前会话：新 tab 跑 `<cc> --resume`。title 是会话可读标题；command 是尾参，阅读器「续接」恒传 null（默认命令链）。
    onResume: (sessionId: String, cwd: String?, title: String?, command: String?) -> Unit = { _, _, _, _ -> },
    onShowHistory: (() -> Unit)? = null, // 非空时工具条显「历史」（全项目历史记录面板，按项目分组，可 resume）
) {
    val copyCode = rememberCodeCopier()

    // 数据层 VM 按 cwd 加 key：cwd 变就换 VM tail 新项目，旧 VM 由 WhileSubscribed 自停；cwd 为 null 用空串哨兵（Koin 不传 null）。
    // key 并入 `-pin-<path>`：钉住、换钉、解钉各是独立 VM，互不污染缓存。
    val vm =
        koinViewModel<ReadingPaneViewModel>(
            // kind 也折进 key：切 agent 种类重建 VM，不复用旧 tail 缓存。
            key = "reading-${cwd ?: "~"}" + (pinnedPath?.let { "-pin-$it" } ?: "") + "-${agentKind.name}",
        ) { parametersOf(channel, cwd ?: "", claudeDir ?: "", pinnedPath ?: "", pinnedLabel ?: "", agentKind, executor) }
    val state by vm.state.collectAsState()
    val sessions by vm.sessions.collectAsState()
    val selectedPath by vm.selectedPath.collectAsState()
    val resumeTarget by vm.resumeTarget.collectAsState()
    // resume 目标是否活会话（pidfile 判活）：活则禁「▶ 续接」，对活会话再起 --resume 会让两个 Claude 双写同一 JSONL
    // （与历史页的 !live 守卫同义）。
    val liveSessionIds by vm.liveSessionIds.collectAsState()
    val sessionLights by vm.sessionLights.collectAsState() // 每个 sid 的状态灯
    val resumeTargetLive = resumeTarget?.sessionId?.let { it in liveSessionIds } ?: false

    // 搜索状态与 listState 滚动强耦合，留在 Composable 里。
    val search = remember { ReadingSearchState() }
    val listState = rememberLazyListState()

    // Ready 取 units；断连态取断开前最后一屏 lastUnits（保留内容，搜索仍可用）；否则空。
    val units = state.renderUnits()
    // 命中的 unit 在列表中的下标（有序），用于 ↑↓ 跳转 + 高亮。
    val matchIndices = rememberMatchIndices(units, search.query)
    val matchedKeys = remember(matchIndices, units) { matchIndices.map { units[it].key }.toSet() }

    val activePath = state.contentPath() ?: selectedPath
    Column(modifier.fillMaxSize()) {
        ReadingPaneToolbar(
            search = search,
            sessions = sessions,
            sessionLights = sessionLights,
            activePath = activePath,
            onSelectSession = { vm.selectSession(it) },
            matchIndices = matchIndices,
            listState = listState,
            resumeTarget = resumeTarget,
            resumeTargetLive = resumeTargetLive, // 活会话禁「续接」，防双写
            // resume 当前会话。label 按 rt.sessionId 匹配 path：切会话的瞬间 activePath 可能还是上一会话，
            // 而 resumeTarget 已随新 tail 更新。command=null 用默认链。
            onResume = {
                resumeTarget?.let { rt ->
                    val label = sessions.firstOrNull { ClaudeSessionCatalog.sessionIdOf(it.path) == rt.sessionId }?.label
                    onResume(rt.sessionId, rt.cwd, label, null)
                }
            },
            onShowHistory = onShowHistory,
        )

        Box(Modifier.fillMaxWidth().weight(1f)) {
            ReadingStateContent(
                state = state,
                matchedKeys = matchedKeys,
                listState = listState,
                onCopyCode = copyCode,
                // 连接死（错误态 connectionDead）时重挂 tail 无用，驱动整条 SSH/会话重连；否则只重挂 tail。
                onRetry = { if ((state as? ClaudeReadState.Error)?.connectionDead == true) onReconnectSsh() else vm.retry() },
                onLoadFull = { vm.loadFullSession(it) }, // 载完整会话（放弃有界窗口）
                onLoadOlder = { vm.loadOlderHistory(it) }, // 上滑再载前一段
            )
        }
    }
}

/** 搜索 UI 状态：开关、词、当前命中序号。 */
private class ReadingSearchState {
    var searching by mutableStateOf(false)
    var query by mutableStateOf("")
    var matchIdx by mutableStateOf(0)
}

/** 工具条接线：搜索开合、词变更、命中导航滚动。 */
@Composable
private fun ReadingPaneToolbar(
    search: ReadingSearchState,
    sessions: List<JsonlSession>,
    sessionLights: Map<String, SessionLight>,
    activePath: String?,
    onSelectSession: (String) -> Unit,
    matchIndices: List<Int>,
    listState: LazyListState,
    resumeTarget: ResumeTarget?,
    resumeTargetLive: Boolean, // resume 目标是活会话时禁「续接」，防双写
    onResume: () -> Unit,
    onShowHistory: (() -> Unit)?,
) {
    val scope = rememberCoroutineScope()
    ReadingToolbar(
        sessions = sessions,
        sessionLights = sessionLights,
        activePath = activePath,
        onSelectSession = onSelectSession,
        searching = search.searching,
        onToggleSearch = {
            search.searching = !search.searching
            if (!search.searching) search.query = ""
        },
        query = search.query,
        onQueryChange = {
            search.query = it
            search.matchIdx = 0
        },
        matchCount = matchIndices.size,
        // % 兜底：units 实时刷新使命中数缩小、matchIdx 仍是旧大值时，显示不越界（下次 ↑↓ 取模纠正）。
        matchPos = if (matchIndices.isEmpty()) 0 else (search.matchIdx % matchIndices.size) + 1,
        onPrevMatch = {
            if (matchIndices.isNotEmpty()) {
                search.matchIdx = (search.matchIdx - 1 + matchIndices.size) % matchIndices.size
                scope.launch { listState.animateScrollToItem(matchIndices[search.matchIdx]) }
            }
        },
        onNextMatch = {
            if (matchIndices.isNotEmpty()) {
                search.matchIdx = (search.matchIdx + 1) % matchIndices.size
                scope.launch { listState.animateScrollToItem(matchIndices[search.matchIdx]) }
            }
        },
        resumeTarget = resumeTarget,
        resumeTargetLive = resumeTargetLive,
        onResume = onResume,
        onShowHistory = onShowHistory,
    )
}

/** 命中 unit 的下标列表（搜索高亮与跳转）。 */
@Composable
private fun rememberMatchIndices(
    units: List<RenderUnit>,
    query: String,
): List<Int> =
    remember(units, query) {
        if (query.isBlank()) {
            emptyList()
        } else {
            units.mapIndexedNotNull { i, u -> if (u.searchableText().contains(query, ignoreCase = true)) i else null }
        }
    }

/** 复制代码块到剪贴板并 Toast 反馈。 */
@Composable
private fun rememberCodeCopier(): (String) -> Unit {
    val context = LocalContext.current
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    return { code ->
        scope.launch { clipboard.copyPlainText(code) }
        Toast.makeText(context, "已复制代码 ${code.length} 字符", Toast.LENGTH_SHORT).show()
    }
}

/** 当前应渲染的 units：Ready 取其 units；断连 Error 取断开前最后一屏 lastUnits；否则空。 */
private fun ClaudeReadState.renderUnits(): List<RenderUnit> =
    (this as? ClaudeReadState.Ready)?.units ?: (this as? ClaudeReadState.Error)?.lastUnits ?: emptyList()

/** 当前内容对应的会话 path：Ready.path 或断连 Error.lastPath（供工具条高亮当前项）。 */
private fun ClaudeReadState.contentPath(): String? =
    (this as? ClaudeReadState.Ready)?.path ?: (this as? ClaudeReadState.Error)?.lastPath

/** 阅读面状态渲染分发。 */
@Composable
private fun ReadingStateContent(
    state: ClaudeReadState,
    matchedKeys: Set<String>,
    listState: LazyListState,
    onCopyCode: (String) -> Unit,
    onRetry: () -> Unit,
    onLoadFull: (String) -> Unit,
    onLoadOlder: (String) -> Unit,
) {
    when (val s = state) {
        ClaudeReadState.Loading -> Centered { CircularProgressIndicator() }
        ClaudeReadState.Empty ->
            Centered {
                Text("未找到 Claude 会话（该工作目录下无 .jsonl）", color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        is ClaudeReadState.Error ->
            if (s.lastUnits.isNotEmpty()) {
                // 断连不清零：保留断开前最后一屏（数据一直在缓存），顶部断开条加内容照渲；重挂 tail 后自动续渲。
                Column(Modifier.fillMaxSize()) {
                    DisconnectedBanner(message = s.message, onRetry = onRetry)
                    ReadyContent(
                        units = s.lastUnits,
                        path = s.lastPath ?: "",
                        windowed = s.windowed,
                        usage = s.usage,
                        matchedKeys = matchedKeys,
                        listState = listState,
                        onCopyCode = onCopyCode,
                        onLoadFull = { s.lastPath?.let(onLoadFull) },
                        // 断连态下不翻历史：连接都没了，拉一段字节只会再报一次错。
                        // 注意：不能拿 historyExhausted 兼这个开关，那会让 UI 谎称「已是最早」，而历史其实没到底。
                        onLoadOlder = {},
                        canLoadOlder = false,
                        historyExhausted = false,
                    )
                }
            } else {
                Centered {
                    Column(horizontalAlignment = Alignment.CenterHorizontally, verticalArrangement = Arrangement.spacedBy(12.dp)) {
                        Text(s.message, color = MaterialTheme.colorScheme.error)
                        // 一键重连：从缓存 offset 续传，不重下整份。
                        TextButton(onClick = onRetry) { Text("重连") }
                    }
                }
            }
        is ClaudeReadState.Ready ->
            if (s.units.isEmpty()) {
                Centered { Text("会话暂无可渲染内容", color = MaterialTheme.colorScheme.onSurfaceVariant) }
            } else {
                ReadyContent(
                    units = s.units,
                    path = s.path,
                    windowed = s.windowed,
                    usage = s.usage,
                    matchedKeys = matchedKeys,
                    listState = listState,
                    onCopyCode = onCopyCode,
                    onLoadFull = { onLoadFull(s.path) },
                    onLoadOlder = { onLoadOlder(s.path) },
                    canLoadOlder = s.windowed && !s.historyExhausted,
                    historyExhausted = s.historyExhausted,
                )
            }
    }
}

/** 断连横幅：保留断开前内容时置顶，提示「显示断开前内容·重连后自动刷新」，并可手动触发 SSH 重连。 */
@Composable
private fun DisconnectedBanner(
    message: String,
    onRetry: () -> Unit,
) {
    Surface(color = MaterialTheme.colorScheme.errorContainer, modifier = Modifier.fillMaxWidth()) {
        Row(
            Modifier.fillMaxWidth().padding(start = 12.dp, end = 4.dp, top = 2.dp, bottom = 2.dp),
            horizontalArrangement = Arrangement.SpaceBetween,
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                "$message · 显示断开前内容，重连后自动刷新",
                color = MaterialTheme.colorScheme.onErrorContainer,
                style = MaterialTheme.typography.bodySmall,
                modifier = Modifier.weight(1f),
            )
            TextButton(onClick = onRetry) { Text("重连") }
        }
    }
}

/** Ready 渲染：底部跟随自动滚、跳最新、列表，顶部用量条。 */
@Composable
private fun ReadyContent(
    units: List<RenderUnit>,
    path: String,
    windowed: Boolean,
    usage: UsageSummary?,
    matchedKeys: Set<String>,
    listState: LazyListState,
    onCopyCode: (String) -> Unit,
    onLoadFull: () -> Unit,
    onLoadOlder: () -> Unit,
    canLoadOlder: Boolean,
    historyExhausted: Boolean,
) = Column(Modifier.fillMaxSize()) {
    ScrollUpHistoryLoader(listState, units.size, enabled = canLoadOlder, onLoadOlder = onLoadOlder)
    if (historyExhausted) {
        Text("已是最早", modifier = Modifier.fillMaxWidth().padding(8.dp))
    }
    // 只读用量条（有 usage 记录才显），只读已有记录，不发命令。
    usage?.takeIf { it.requests > 0 }?.let { UsageBar(it, windowed) }
    Box(Modifier.fillMaxWidth().weight(1f)) {
        val unitCount = units.size
        // 底部跟随：只有已在底部时新内容才自动滚（不打断向上翻阅）；末项很高时还要让它的底边滚入视口。
        val atBottom by remember(unitCount) {
            derivedStateOf {
                val info = listState.layoutInfo
                val last = info.visibleItemsInfo.lastOrNull()
                last == null || (last.index >= unitCount - 1 && last.offset + last.size <= info.viewportEndOffset)
            }
        }
        val scope = rememberCoroutineScope()
        // 进屏或换会话时首帧滚到底（每个 path 一次）：listState 初值是 index 0，高内容会停在顶。
        // 用瞬时 scrollToItem，没有可见回滚；只做一次，不打断之后的上滑；不覆盖搜索跳转（那由搜索导航另触发）。
        var initializedPath by remember { mutableStateOf<String?>(null) }
        LaunchedEffect(path, unitCount) {
            if (unitCount > 0 && initializedPath != path) {
                listState.scrollToItem(unitCount - 1)
                initializedPath = path
            }
        }
        // 注意：自动滚到底不能按 `unitCount` 触发，上滑翻历史的 prepend 也会改变它，会把刚翻上来的位置甩回底部。
        // 所以按「最后一条的 key + 正文长度」触发（与聊天面一致）。`atBottom` 在 `visibleItemsInfo` 为空时默认 true，键必须精确。
        val lastKey = units.lastOrNull()?.key
        val lastLen = units.lastOrNull()?.searchableText()?.length
        LaunchedEffect(lastKey, lastLen) {
            if (atBottom && unitCount > 0) listState.animateScrollToItem(unitCount - 1)
        }
        ClaudeReadingPane(
            units = units,
            modifier = Modifier.fillMaxSize(),
            listState = listState,
            matchedKeys = matchedKeys,
            onCopyCode = onCopyCode,
        )
        if (!atBottom) {
            TextButton(
                onClick = { scope.launch { listState.animateScrollToItem(unitCount - 1) } },
                modifier = Modifier.align(Alignment.BottomEnd).padding(16.dp),
            ) { Text("↓ 最新") }
        }
        // 有界窗口首载（只显示最近 N 条）时顶部提供「载完整会话」，从 byte 0 全量载更早历史。
        if (windowed) {
            TextButton(
                onClick = onLoadFull,
                modifier = Modifier.align(Alignment.TopCenter).padding(top = 4.dp),
            ) { Text("↑ 载完整会话", style = monoSmall) }
        }
    }
}

/** 只读用量条：tokens（in/out/缓存）与上下文窗占用。 */
@Composable
private fun UsageBar(
    usage: UsageSummary,
    windowed: Boolean,
) {
    Text(
        usageSummaryText(usage, windowed),
        style = monoSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier =
            Modifier
                .fillMaxWidth()
                .background(MaterialTheme.colorScheme.surfaceVariant)
                .padding(horizontal = 12.dp, vertical = 4.dp),
    )
}

/** token 数简写（K/M）。用 Locale.ROOT 定小数点，防逗号 locale 显成 `1,5K`。 */
internal fun fmtTokens(n: Long): String =
    when {
        n >= 1_000_000 -> "%.1fM".format(java.util.Locale.ROOT, n / 1_000_000.0)
        n >= 1_000 -> "%.1fK".format(java.util.Locale.ROOT, n / 1_000.0)
        else -> n.toString()
    }

/** 用量汇总单行文本。窗口模式标「窗口内」（用量只覆盖已载窗口）。不显示花费。 */
internal fun usageSummaryText(
    usage: UsageSummary,
    windowed: Boolean,
): String {
    // Codex 带真实上限 contextWindow（model_context_window）时优先用；Claude 为 null，走启发式 contextLimitFor。
    val ctxLimit = usage.contextWindow?.takeIf { it > 0 } ?: contextLimitFor(usage.lastModel, usage.lastContextTokens)
    // 钳到 100%：Codex 占用在 compact 前偶尔短暂超过 contextWindow。
    val ctxPct = if (usage.lastContextTokens > 0) (usage.lastContextTokens * 100 / ctxLimit).coerceAtMost(100) else 0
    val win = if (windowed) " · 窗口内" else ""
    return "↑${fmtTokens(usage.input)} ↓${fmtTokens(usage.output)}" +
        " · 缓存 ${fmtTokens(usage.cacheWrite5m + usage.cacheWrite1h)}写/${fmtTokens(usage.cacheRead)}读" +
        " · 上下文 ${fmtTokens(usage.lastContextTokens)}/${fmtTokens(ctxLimit)} $ctxPct%" +
        win
}

/** 阅读面顶部工具条：会话下拉、搜索切换与输入、命中导航。 */
@Composable
private fun ReadingToolbar(
    sessions: List<JsonlSession>,
    sessionLights: Map<String, SessionLight> = emptyMap(), // sid → 状态灯
    activePath: String?,
    onSelectSession: (String) -> Unit,
    searching: Boolean,
    onToggleSearch: () -> Unit,
    query: String,
    onQueryChange: (String) -> Unit,
    matchCount: Int,
    matchPos: Int,
    onPrevMatch: () -> Unit,
    onNextMatch: () -> Unit,
    resumeTarget: ResumeTarget? = null, // 非空表示可 resume（cwd 已知）
    resumeTargetLive: Boolean = false, // 目标是活会话时禁「续接」，防对活会话双写 JSONL
    onResume: () -> Unit = {},
    onShowHistory: (() -> Unit)? = null, // 非空时显「历史」
) {
    Row(
        Modifier.fillMaxWidth().padding(horizontal = 8.dp),
        horizontalArrangement = Arrangement.spacedBy(4.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        if (searching) {
            OutlinedTextField(
                value = query,
                onValueChange = onQueryChange,
                singleLine = true,
                placeholder = { Text("搜索会话内容") },
                modifier = Modifier.weight(1f),
            )
            Text(if (matchCount == 0) "0" else "$matchPos/$matchCount", style = monoSmall)
            TextButton(onClick = onPrevMatch) { Text("↑") }
            TextButton(onClick = onNextMatch) { Text("↓") }
            TextButton(onClick = onToggleSearch) { Text("✕") }
        } else {
            var menuOpen by remember { mutableStateOf(false) }
            // label 是会话目录给的可读标题（活动会话优先；无 pidfile 兜底最近列表）。
            val label = sessions.firstOrNull { it.path == activePath }?.label ?: "最新会话"
            // 当前会话状态灯（activePath → sid → 灯；没有或未写则不点）。
            val activeLight = activePath?.let { sessionLights[ClaudeSessionCatalog.sessionIdOf(it)] } ?: SessionLight.Unknown
            Box(Modifier.weight(1f)) {
                TextButton(onClick = { menuOpen = true }) {
                    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(4.dp)) {
                        SessionLightDot(activeLight)
                        // 可读标题可长达约 120 字符，单行省略，防撑爆工具条。
                        Text("会话: $label ▾", style = monoSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
                DropdownMenu(expanded = menuOpen, onDismissRequest = { menuOpen = false }) {
                    if (sessions.isEmpty()) {
                        DropdownMenuItem(text = { Text("（无会话）") }, onClick = { menuOpen = false })
                    }
                    sessions.forEach { sess ->
                        // 下拉里每个会话一个状态灯，看清哪个在工作、等待或空闲。
                        val light = sessionLights[ClaudeSessionCatalog.sessionIdOf(sess.path)] ?: SessionLight.Unknown
                        DropdownMenuItem(
                            text = {
                                Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                                    SessionLightDot(light)
                                    Text(sess.label, style = monoSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                                }
                            },
                            onClick = {
                                onSelectSession(sess.path)
                                menuOpen = false
                            },
                        )
                    }
                }
            }
            // resume 当前会话：cwd 已从 JSONL 解出才启用（新 tab 跑 `<cc> --resume`）。
            // 活会话守卫：目标会话正被 Claude 跑（pidfile 判活）时禁用，再起 --resume 会两个进程双写同一 JSONL；
            // 活会话本就在跑，阅读即可。
            TextButton(onClick = onResume, enabled = resumeTarget != null && !resumeTargetLive) { Text("▶ resume", style = monoSmall) }
            // 全项目历史记录（按项目分组，点会话 resume）。
            onShowHistory?.let { TextButton(onClick = it) { Text("历史", style = monoSmall) } }
            TextButton(onClick = onToggleSearch) { Text("搜索") }
        }
    }
}

/**
 * 会话状态灯圆点，只给活会话点灯：Working 绿 / WaitingInput 琥珀 / Idle 灰 / Shell 蓝。
 * Stopped（非活、历史）与 Unknown（没有 status）不显点，下拉不被历史会话的灯刷屏。
 */
@Composable
private fun SessionLightDot(light: SessionLight) {
    val color =
        when (light) {
            SessionLight.Working -> Color(0xFF3DDC84) // 绿=工作中
            SessionLight.WaitingInput -> Color(0xFFFFB300) // 琥珀=等待输入
            SessionLight.Idle -> Color(0xFF9AA0A6) // 灰=空闲
            SessionLight.Shell -> Color(0xFF4C8DF6) // 蓝=shell
            SessionLight.Stopped, SessionLight.Unknown -> return // 非活/未知 → 不显点
        }
    Box(Modifier.size(8.dp).clip(CircleShape).background(color))
}

@Composable
private fun Centered(content: @Composable () -> Unit) {
    Box(Modifier.fillMaxSize().padding(24.dp), Alignment.Center) { content() }
}

/**
 * 滚到顶就再载前一段。
 *
 * 注意：判据整个放进 `derivedStateOf`，并让 [LaunchedEffect] 以它为 key。若把其中一个变量留在效果体里而不是 key 里，
 * 第一次以过时快照判完就再也不重判，表现为上滑永远触发不了。
 *
 * 内容不满一屏时 `firstVisibleItemIndex` 恒为 0，会先把视口填满再停；填满之后 index 不再恒为 0，自然终止。
 */
@Composable
private fun ScrollUpHistoryLoader(
    listState: LazyListState,
    unitCount: Int,
    enabled: Boolean,
    onLoadOlder: () -> Unit,
) {
    val atTop by remember {
        derivedStateOf { listState.layoutInfo.visibleItemsInfo.isNotEmpty() && listState.firstVisibleItemIndex == 0 }
    }
    LaunchedEffect(atTop, enabled, unitCount) {
        if (atTop && enabled) onLoadOlder()
    }
}
