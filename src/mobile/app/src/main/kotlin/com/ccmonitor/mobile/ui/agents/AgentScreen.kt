package com.ccmonitor.mobile.ui.agents

import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.HistoryFacts
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.ResidentFeed
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.core.wire.gen.RunInfo
import com.ccmonitor.mobile.core.wire.gen.RunLine
import com.ccmonitor.mobile.core.wire.gen.RunState
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.chat.ProcessActions
import com.ccmonitor.mobile.ui.chat.ProcessRow
import com.ccmonitor.mobile.ui.chat.needs.ChatNeeds
import com.ccmonitor.mobile.ui.chat.needs.rememberChatNeeds
import com.ccmonitor.mobile.ui.claude.ClaudeReadingPane
import com.ccmonitor.mobile.ui.claude.LocalBriefFrom
import com.ccmonitor.mobile.ui.common.MachineBar
import com.ccmonitor.mobile.ui.common.rememberHostLink
import kotlinx.coroutines.flow.flowOf
import org.koin.compose.koinInject

/**
 * agent 屏（只读）：一个子运行。顶栏 ＝ 名字 · 类别 · 状态（那台写好的）；顶上机器条；事实（几点起打头）· 说明 · 失败的报错原话照摆；正文同聊天屏那一套卡（派活那段话是一张派活卡）、过程照分段折（[AgentRun]）；
 * 尾巴上是结束线；它派出的那几个点了推屏（同一个运行栈里只出现一次，由导航层照 [AgentStack] 定）。
 * 它在等人（`needs` 有）⇒ 底下同一个需手动钉条（父会话那一条的，答题走同一套）。
 */
@Composable
fun AgentScreen(
    hostId: String,
    sid: String,
    run: String,
    onBack: () -> Unit,
    onOpen: (RunInfo) -> Unit,
) {
    val scope = rememberCoroutineScope()
    rememberHostLink(hostId, "agents-$hostId")
    val backends = koinInject<HostBackends>()
    val feed = remember(hostId) { backends.of(hostId) }
    val table by feed.table.collectAsState()
    val runs = table.runs[sid]
    val info = runs?.runs?.firstOrNull { it.run == run }
    // 进屏那一刻在不在跑定首屏从哪读；之后运行表变了只往下接着读。
    val running = remember(run) { info?.state == RunState.RUNNING }
    val machine = rememberMachine(hostId) ?: return
    val parent = rememberParentPath(feed, sid, table.live[sid]?.path) ?: return
    val body = remember(parent, run) { AgentRun(feed, machine, parent, run, running, scope).also { it.open() } }
    val ui by body.state.collectAsState()
    // 在跑的那一个记录在长（运行表里它最近动的时刻变了 · 收场了）⇒ 往下接着读。
    LaunchedEffect(info?.activeMs, info?.state) { if (!ui.loading) body.newer() }
    val all = runs?.runs.orEmpty()
    val kids = all.filter { it.parent == run }
    val from = info?.parent?.let { p -> all.firstOrNull { it.run == p }?.labelText }
    Scaffold(
        topBar = { AgentsTopBar(info?.labelText.orEmpty(), info?.let(::secondLine).orEmpty(), onBack) },
    ) { inner ->
        Column(Modifier.fillMaxSize().padding(inner)) {
            MachineBar(hostId)
            info?.let { Head(it, onRefresh = body::open) }
            ui.failed?.let { ErrorLine(it, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) }
            CompositionLocalProvider(LocalBriefFrom provides from) {
                Transcript(body, ui, Modifier.weight(1f), toBottom = running)
            }
            info?.end?.let { e -> Line(listOf(e.head, clock(e.sub)).joinToString(copyText("kit.text.sep"))) }
            if (kids.isNotEmpty()) {
                Text(
                    copyText("agentWindow.kids.title"),
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 8.dp),
                )
                kids.forEach { k -> RunRowView(k) { onOpen(k) } }
            }
            if (info?.needs != null) {
                ChatNeeds(rememberChatNeeds("agent/$hostId/$sid", hostId, feed, remember(sid) { flowOf(sid) }, machine))
            }
        }
    }
}

/** 事实一行 · 说明（打头那一格 · 那一句 · 小一号的补一句；状态不明带［刷新］）。 */
@Composable
private fun Head(
    info: RunInfo,
    onRefresh: () -> Unit,
) {
    val facts = listOfNotNull(info.startedText) + info.facts.orEmpty().map(::clock)
    if (facts.isNotEmpty()) Line(facts.joinToString(copyText("kit.text.sep")))
    info.explain?.let { x ->
        Line(listOfNotNull(x.lead, clock(x.line)).joinToString(copyText("kit.text.sep")))
        x.note?.let { Line(it, small = true) }
        if (x.refresh == true) {
            SecondaryButton(onClick = onRefresh, modifier = Modifier.padding(horizontal = 8.dp)) { Text(copyText("accountChip.menu.refresh")) }
        }
    }
    if (info.state == RunState.FAILED) info.error?.takeIf { it.isNotEmpty() }?.let { ErrorRaw(it) }
}

/** 失败的报错原话（那台交的 `error`，等宽红字、描红框）。 */
@Composable
private fun ErrorRaw(text: String) {
    val tokens = LocalAppTokens.current
    Text(
        text,
        style = monoSmall,
        color = tokens.errorText,
        modifier =
            Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp, vertical = 4.dp)
                .border(1.dp, MaterialTheme.colorScheme.error, RoundedCornerShape(tokens.radius.m))
                .padding(8.dp),
    )
}

/** 顶栏第二行：类别 · 状态（字都是那台交的）。 */
private fun secondLine(info: RunInfo): String = listOfNotNull(info.kind?.takeIf { it.isNotEmpty() }, info.statusText).joinToString(copyText("kit.text.sep"))

@Composable
private fun Line(
    text: String,
    small: Boolean = false,
) {
    Text(
        text,
        style = if (small) MaterialTheme.typography.bodySmall else MaterialTheme.typography.bodyMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(horizontal = 16.dp, vertical = 2.dp),
    )
}

/** 会走的钟：`{dur}` 照此刻 − `from` 填（共用的时长格式）；没有 `from` ⇒ 整句照抄。 */
private fun clock(l: RunLine): String = l.from?.let { DurationFormat.clock(l.text, it, null, System.currentTimeMillis()) } ?: l.text

/** 正文：到顶再读前一页；在跑的那一个首屏贴着底。 */
@Suppress("DEPRECATION") // LocalClipboard 是新 API 但只有 suspend 接口
@Composable
private fun Transcript(
    body: AgentRun,
    ui: AgentRunUi,
    modifier: Modifier,
    toBottom: Boolean,
) {
    val listState = rememberLazyListState()
    val clipboard = LocalClipboardManager.current
    val atTop by remember { derivedStateOf { listState.layoutInfo.visibleItemsInfo.isNotEmpty() && listState.firstVisibleItemIndex == 0 } }
    LaunchedEffect(atTop, ui.canOlder, ui.stream.size) { if (atTop && ui.canOlder) body.older() }
    val landed = ui.stream.isNotEmpty()
    LaunchedEffect(landed) { if (landed && toBottom) listState.scrollToItem(ui.stream.size - 1) }
    ClaudeReadingPane(
        items = ui.stream,
        modifier = modifier,
        listState = listState,
        onCopyCode = { clipboard.setText(AnnotatedString(it)) },
        process = { ProcessRow(it, ProcessActions(toggle = body::toggle)) },
    )
}

@Composable
private fun rememberMachine(hostId: String): String? {
    val hosts = koinInject<HostRepository>()
    val got by produceState<String?>(null, hostId) { value = runCatching { hosts.get(hostId) }.getOrNull()?.label ?: hostId }
    return got
}

/** 父会话的记录路径：簿里活着的那一条带着；不在簿里（走了）⇒ 按 sid 问那台（`history-facts` 回包的 `path`）。 */
@Composable
private fun rememberParentPath(
    feed: ResidentFeed,
    sid: String,
    live: String?,
): String? {
    val got by produceState(live, sid, live) {
        if (value == null) value = (feed.call(HistoryFacts.COMMAND, HistoryFacts.bySid(sid)) as? Reply.Ok)?.let { HistoryFacts.of(it.data) }?.path
    }
    return got
}
