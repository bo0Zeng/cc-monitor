package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.MutableState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.snapshotFlow

/** 聊天页往上翻着时滚到哪：视口顶上那一行的 key（[StreamItem.key][com.ccmonitor.mobile.core.claude.model.StreamItem.key]）与它卷上去的那几像素。贴着底 ⇒ 不存。 */
data class ChatAnchor(
    val key: String,
    val offset: Int,
) {
    /** 回来那一页（[keys]）里滚到哪：那一页还没到 ⇒ `null`（先等）。 */
    fun backIn(keys: List<String>): ChatScroll? =
        when {
            keys.isEmpty() -> null
            else -> keys.indexOf(key).takeIf { it >= 0 }?.let { ChatScroll.To(it, offset) } ?: ChatScroll.Bottom
        }
}

/** 回来时滚去哪：那一行（[To]）· 那一行在更早的页里 ⇒ 照贴着底（[Bottom]）。 */
sealed interface ChatScroll {
    data class To(
        val index: Int,
        val offset: Int,
    ) : ChatScroll

    data object Bottom : ChatScroll
}

/** 只存 key 与偏移（不存正文）。 */
val CHAT_ANCHOR_SAVER: Saver<ChatAnchor?, Any> =
    Saver(
        save = { a -> a?.let { listOf(it.key, it.offset) } },
        restore = { (it as List<*>).let { (k, o) -> ChatAnchor(k as String, o as Int) } },
    )

/**
 * 聊天页滚到哪跟着进程重建回来：不贴着底（[following]）时记下视口顶上那一行；
 * 新起这一屏时照存下的滚回那一行，那一行不在回来那一页里 ⇒ 照贴着底（[toBottom]）。
 */
@Composable
internal fun KeepChatScroll(
    listState: LazyListState,
    keys: List<String>,
    following: MutableState<Boolean>,
    toBottom: suspend () -> Unit,
) {
    var anchor by rememberSaveable(stateSaver = CHAT_ANCHOR_SAVER) { mutableStateOf(null) }
    var back by remember { mutableStateOf(anchor.takeIf { !following.value }) }
    LaunchedEffect(listState) {
        snapshotFlow {
            val first = listState.layoutInfo.visibleItemsInfo.firstOrNull()
            val top = first?.key as? String
            if (top == null || following.value) null else ChatAnchor(top, listState.firstVisibleItemScrollOffset)
        }.collect { if (back == null) anchor = it }
    }
    LaunchedEffect(back, keys) {
        val want = back ?: return@LaunchedEffect
        when (val to = want.backIn(keys) ?: return@LaunchedEffect) {
            is ChatScroll.To -> listState.scrollToItem(to.index, to.offset)
            ChatScroll.Bottom -> {
                following.value = true
                toBottom()
            }
        }
        back = null
    }
}
