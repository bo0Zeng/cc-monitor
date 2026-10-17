package com.ccmonitor.mobile.ui.chat.needs

import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.lifecycle.ViewModel
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.lifecycle.viewModelScope
import androidx.lifecycle.viewmodel.compose.viewModel
import com.ccmonitor.mobile.core.claude.link.ResidentFeed
import com.ccmonitor.mobile.ui.needs.NeedsBoard
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.map
import org.koin.compose.koinInject

/** 聊天屏那一块需手动的持有者：跟着这一屏的导航项（转屏不丢选到一半的）。 */
class NeedsViewModel(
    feed: ResidentFeed,
    board: NeedsBoard,
    hostId: String,
    sid: Flow<String?>,
    machine: String,
) : ViewModel() {
    val desk = NeedsDesk(feed, board.state.map { it[hostId] }, { board.reask(hostId) }, sid, machine, viewModelScope, now = ::monoMs)
}

/** 本机单调钟（毫秒）：「已等」接着走表用，不受改系统时间影响。 */
internal fun monoMs(): Long = System.nanoTime() / NANOS_PER_MS

private const val NANOS_PER_MS = 1_000_000L

/** 聊天屏那一块需手动的持有者。[key] 同那条对话的 key（一条对话一份）。 */
@Composable
fun rememberChatNeeds(
    key: String,
    hostId: String,
    feed: ResidentFeed,
    sid: Flow<String?>,
    machine: String,
): NeedsViewModel {
    val board = koinInject<NeedsBoard>()
    return viewModel(key = "needs/$key") { NeedsViewModel(feed, board, hostId, sid, machine) }
}

/** 聊天屏的需手动：钉条（输入框上方）· 替人答过的那一行 ＋ 答题抽屉。 */
@Composable
fun ChatNeeds(vm: NeedsViewModel) {
    val ui by vm.desk.state.collectAsStateWithLifecycle()
    val d = vm.desk
    ui.bar?.let { NeedsBarView(it, ::monoMs, onAnswer = d::open) }
    ui.answered?.let { NeedsAnsweredView(it) }
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
