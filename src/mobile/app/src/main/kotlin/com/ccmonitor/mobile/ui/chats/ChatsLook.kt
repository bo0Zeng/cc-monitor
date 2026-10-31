package com.ccmonitor.mobile.ui.chats

import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.Saver
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import com.ccmonitor.mobile.core.claude.link.ListSort
import kotlinx.coroutines.flow.dropWhile
import kotlinx.coroutines.flow.map

/** 会话页的筛选存进 `rememberSaveable`：五样平铺成一列（看法 · 去掉的几台 · 几天内 · 排序 · 显示已隐藏），枚举按名字存。 */
val CHATS_FILTERS_SAVER: Saver<Filters, Any> =
    listSaver(
        save = { listOf(it.view.name, ArrayList(it.unchecked), it.withinDays, it.sort?.name, it.hidden) },
        restore = { cells ->
            // 照存的先后一格一格取。
            val next = cells.iterator()
            Filters(
                view = ChatsView.valueOf(next.next() as String),
                unchecked = (next.next() as List<*>).filterIsInstance<String>().toSet(),
                withinDays = next.next() as Int?,
                sort = (next.next() as String?)?.let(ListSort::valueOf),
                hidden = next.next() as Boolean,
            )
        },
    )

/**
 * 跟着 [board] 存下筛选；[board] 是新起的（进程被收过）⇒ 交回存下的筛选与搜索词（[typed]：搜索框开着时框里那几个字）。
 * 看板照它重问之前不改存下的那一份：半路再被收也照样回得去。
 */
@Composable
fun KeepChatsLook(
    board: ChatsBoard,
    typed: String?,
) {
    var kept by rememberSaveable(stateSaver = CHATS_FILTERS_SAVER) { mutableStateOf(Filters()) }
    LaunchedEffect(board) {
        val back = kept
        board.filters { back }
        typed?.let(board::query)
        board.ui
            .map { it.filters }
            .dropWhile { it != back }
            .collect { kept = it }
    }
}
