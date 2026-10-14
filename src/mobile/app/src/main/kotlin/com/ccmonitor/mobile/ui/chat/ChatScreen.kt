package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.derivedStateOf
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.model.searchableText
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
import com.ccmonitor.mobile.ui.claude.ClaudeReadingPane
import kotlinx.coroutines.flow.drop
import kotlinx.coroutines.flow.filter

/**
 * 聊天面：正文（[ChatTranscript]）＋ 输入行。
 */
@Composable
fun ChatScreen(
    state: ChatUiState,
    actions: ChatInputActions,
    modifier: Modifier = Modifier,
    /** 上滑到顶时请求再载更早的一段。 */
    onLoadOlder: () -> Unit = {},
    /** 重发一条失败的消息（传本地 key）。 */
    onRetrySend: (String) -> Unit = {},
) {
    Column(modifier.fillMaxSize()) {
        ChatTranscript(state = state, modifier = Modifier.weight(1f), onLoadOlder = onLoadOlder, onRetrySend = onRetrySend)
        // 送没送到贴在那条消息旁边（`UserText.deliveryText`），这里不另起横幅。
        ChatInputArea(state = state, actions = actions)
    }
}

/**
 * 一条会话的正文：渲染直接用 [ClaudeReadingPane]，单元来自 [ChatSession]（核心的通用记录 ⇒ `RecordUnits`）。
 * 手机上看正文只有这一处（终端屏不带阅读子页）。
 */
@Suppress("DEPRECATION") // LocalClipboard 是新 API 但只有 suspend 接口
@Composable
fun ChatTranscript(
    state: ChatUiState,
    modifier: Modifier = Modifier,
    onLoadOlder: () -> Unit = {},
    onRetrySend: (String) -> Unit = {},
) {
    val listState = rememberLazyListState()
    val clipboard = LocalClipboardManager.current

    // 前面插入历史后滚动位置靠 Compose 按 key 自己恢复，前提是 `LazyColumn` 用内容派生的稳定 key。

    // 贴着底（[following]）：一开始是；之后每次滚动停下时看停在哪 —— 停在底 ⇒ 还贴着，停在别处（人往上翻了）⇒ 不贴了。
    // 新内容来了只在贴着底时跟到底，往上翻看历史时不拽回去。
    var following by remember { mutableStateOf(true) }
    LaunchedEffect(listState) {
        snapshotFlow { listState.isScrollInProgress }.drop(1).filter { !it }.collect { following = !listState.canScrollForward }
    }

    // 到顶 ⇒ 请求更早的一段（判法见 [loadOlderNow]）。
    // 注意：判据必须整个进 `derivedStateOf`；漏在效果体里的变量不在 key 里，效果拿过时快照判一次就不再重跑。
    // 内容不满一屏时会先把视口填满，填满后 `firstVisibleItemIndex` 不再恒为 0，自然停止。
    val atTop by remember {
        derivedStateOf {
            val info = listState.layoutInfo
            info.visibleItemsInfo.isNotEmpty() && listState.firstVisibleItemIndex == 0
        }
    }
    val moreBelow by remember { derivedStateOf { listState.canScrollForward } }
    LaunchedEffect(atTop, moreBelow, following, state.canLoadOlder, state.loadingOlder, state.units.size) {
        if (loadOlderNow(atTop, state.units.isEmpty(), state.canLoadOlder, state.loadingOlder, following, moreBelow)) onLoadOlder()
    }

    // 跟到底。触发看「最后一个单元的 key + 内容长度」而不是 `units.size`：流式期间数量不变，
    // 而往顶部插历史会改 size。滚到最后一个单元的末尾（一条很长的回答也看得到它最新那几行），偏移交给列表自己夹到底。
    // 视口高度也在 key 里：键盘弹起把消息区压矮时，贴着底的照样看得到最新那几行。
    // 活卡逐字长在正文尾巴上：它的字数也在 key 里，贴着底时跟着它长。
    val lastUnit = state.units.lastOrNull()
    val liveLength = state.live.sumOf { c -> c.blocks.sumOf { it.text.length } + 1 }
    val total = state.units.size + state.live.size
    val viewport by remember { derivedStateOf { listState.layoutInfo.viewportSize.height } }
    LaunchedEffect(lastUnit?.key, lastUnit?.searchableText()?.length, liveLength, viewport) {
        if (following && total > 0) listState.scrollToItem(total - 1, Int.MAX_VALUE)
    }

    Column(modifier.fillMaxWidth()) {
        HistoryStatusBar(state)

        ClaudeReadingPane(
            units = state.units,
            modifier = Modifier.weight(1f).testTag(ChatTestTags.PANE),
            listState = listState,
            // 不传的话「复制代码」按钮照样渲染但点了没反应。
            onCopyCode = { clipboard.setText(AnnotatedString(it)) },
            onRetrySend = onRetrySend,
            trailer = { liveCards(state.live) },
        )

        state.failedWhy?.let {
            // 会话那一层的失败：核心那一句原样（起不来 · 读不出记录）。
            ErrorLine(it, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp))
        }
    }
}

/** 往上翻没读成：核心写好的那一句原样一行，「再上滑一次可重试」另起一行。 */
internal fun historyErrorLines(why: String): List<String> = listOf(why, copyText("mobChat.history.retry"))

/** 顶部状态条：正在载 / 已到最早 / 失败，三态互斥。到顶必须说出来，否则用户会以为卡了。 */
@Composable
private fun HistoryStatusBar(state: ChatUiState) {
    val base = Modifier.fillMaxWidth().padding(8.dp)
    when {
        state.historyError != null ->
            Text(
                historyErrorLines(state.historyError).joinToString("\n"),
                modifier = base.testTag(ChatTestTags.HISTORY_ERROR),
            )

        state.loadingOlder ->
            Text(copyText("mobChat.history.loading"), modifier = base.testTag(ChatTestTags.LOADING_OLDER))

        // 只有读着一条会话才谈得上「到顶」；新建会话不出这一条。
        state.sid != null && !state.canLoadOlder && state.units.isNotEmpty() ->
            Text(copyText("mobChat.history.oldest"), modifier = base.testTag(ChatTestTags.NO_MORE_HISTORY))
    }
}

/**
 * 该不该现在去载更早的一段：到顶了（或尾段里一条进界面的都没有）、允许载、没在载，
 * 而且不是「还贴着底、底下还有没滚到的」那一刻 —— 打开一条长会话时列表先排在尾段的顶上、紧接着才滚到底，
 * 那一瞬间的「到顶」不是人翻上去的，在那时载会把更早的整份插进来、人落在中间。内容不满一屏（底下没有了）照样载，先把一屏填满。
 */
internal fun loadOlderNow(
    atTop: Boolean,
    empty: Boolean,
    canLoadOlder: Boolean,
    loadingOlder: Boolean,
    following: Boolean,
    moreBelow: Boolean,
): Boolean = canLoadOlder && !loadingOlder && (empty || (atTop && !(following && moreBelow)))
