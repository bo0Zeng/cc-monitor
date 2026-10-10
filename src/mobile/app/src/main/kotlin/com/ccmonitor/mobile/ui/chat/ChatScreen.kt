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
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboardManager
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.model.searchableText
import com.ccmonitor.mobile.ui.claude.ClaudeReadingPane

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
    // 尾段里一条进界面的记录都没有时列表是空的，[readyToLoadOlder] 恒假：空判据放在这里（`state.units.size` 在 key 里）。
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
            // 会话那一层的失败：核心那一句原样（起不来 · 读不出记录）。
            Text(text = it, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp), color = MaterialTheme.colorScheme.error)
        }

        // 送没送到贴在那条消息旁边（`UserText.deliveryText`），这里不另起横幅。
        ChatInputRow(running = state.running, onSend = onSend, onStop = onStop)
    }
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

        // 只有读着一条会话才谈得上「到顶」；新建会话不出这一条。
        state.sid != null && !state.canLoadOlder && state.units.isNotEmpty() ->
            Text("已是最早", modifier = base.testTag(ChatTestTags.NO_MORE_HISTORY))
    }
}

/** 输入行：输入框 ＋ 发送 / 停止（运行中且框里没字 ⇒ 停止；二选一，不并存）。输入框永远不锁。 */
@Composable
private fun ChatInputRow(
    running: Boolean,
    onSend: (String) -> Unit,
    onStop: () -> Unit,
) {
    // `rememberSaveable`：转屏会重建 Activity，`remember` 的草稿会丢光。
    var draft by rememberSaveable { mutableStateOf("") }
    Surface(tonalElevation = 2.dp) {
        Row(
            modifier = Modifier.fillMaxWidth().padding(8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            OutlinedTextField(
                value = draft,
                onValueChange = { draft = it },
                modifier = Modifier.weight(1f).testTag(ChatTestTags.INPUT),
                placeholder = { Text("说点什么…") },
                singleLine = false,
                maxLines = INPUT_MAX_LINES,
            )
            if (running && draft.isBlank()) {
                TextButton(onClick = onStop, modifier = Modifier.testTag(ChatTestTags.STOP)) { Text("停止") }
            } else {
                TextButton(
                    onClick = {
                        onSend(draft)
                        draft = ""
                    },
                    enabled = draft.isNotBlank(),
                    modifier = Modifier.testTag(ChatTestTags.SEND),
                ) {
                    Text("发送")
                }
            }
        }
    }
}

private const val INPUT_MAX_LINES = 4
