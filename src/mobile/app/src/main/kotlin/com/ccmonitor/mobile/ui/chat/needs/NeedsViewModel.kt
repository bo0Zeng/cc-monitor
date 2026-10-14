package com.ccmonitor.mobile.ui.chat.needs

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ccmonitor.mobile.core.claude.link.ResidentFeed
import kotlinx.coroutines.flow.Flow

/** 聊天屏那一块需手动的持有者：跟着这一屏的导航项（转屏不丢选到一半的）。 */
class NeedsViewModel(
    feed: ResidentFeed,
    sid: Flow<String?>,
    machine: String,
) : ViewModel() {
    val desk = NeedsDesk(feed, feed.pushes, sid, machine, viewModelScope, now = ::monoMs)
}

/** 本机单调钟（毫秒）：「已等」接着走表用，不受改系统时间影响。 */
internal fun monoMs(): Long = System.nanoTime() / NANOS_PER_MS

private const val NANOS_PER_MS = 1_000_000L

/**
 * 聊天屏的需手动：钉条（输入框上方）＋ 答题抽屉。[key] 同那条对话的 key（一条对话一份）。
 */
@Composable
fun ChatNeeds(
    key: String,
    feed: ResidentFeed,
    sid: Flow<String?>,
    machine: String,
) {
    val vm: NeedsViewModel = viewModel(key = "needs/$key") { NeedsViewModel(feed, sid, machine) }
    val ui by vm.desk.state.collectAsStateWithLifecycle()
    val d = vm.desk
    ui.bar?.let { NeedsBarView(it, ::monoMs, onAnswer = d::open) }
    ui.sheet?.let {
        AskSheetView(
            it,
            ::monoMs,
            AskActions(
                pick = d::pick,
                sendNote = d::sendNote,
                cancelNote = d::cancelNote,
                toggle = d::toggle,
                other = d::other,
                next = d::next,
                dismiss = d::dismiss,
            ),
        )
    }
}
