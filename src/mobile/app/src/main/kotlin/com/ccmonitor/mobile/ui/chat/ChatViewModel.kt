package com.ccmonitor.mobile.ui.chat

import androidx.lifecycle.ViewModel
import com.ccmonitor.mobile.core.claude.bridge.BridgeFrame
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.StateFlow

/**
 * 聊天面的 ViewModel，只是一层薄壳，实现全在 [ChatSession] 里。
 *
 * 下行不挂在 `viewModelScope` 上：离开聊天屏时 [ChatSession] 照收，回复完成照样能推送。
 * 对话何时结束由 [ChatController.release] 决定，不由屏幕生命周期决定，所以不覆写 `onCleared`。
 */
class ChatViewModel(
    private val session: ChatSession,
) : ViewModel() {
    val state: StateFlow<ChatUiState> get() = session.state

    fun start(
        frames: Flow<BridgeFrame>,
        smooth: Boolean = true,
        prompt: String? = null,
    ) = session.start(frames, smooth, prompt)

    fun attachHistoryPaging(loader: suspend () -> List<BridgeFrame>?) = session.attachHistoryPaging(loader)

    fun loadOlder() = session.loadOlder()

    fun send(text: String) = session.send(text)

    fun retry(localId: String) = session.retry(localId)

    fun stop() = session.stop()

    /** 把下行重新接上，见 [ChatSession.reattach]。 */
    fun reattach(
        frames: Flow<BridgeFrame>,
        smooth: Boolean = true,
    ) = session.reattach(frames, smooth)

    /** 开屏与「重新接上内容」共用的入口，见 [ChatSession.openOrReattach]。 */
    fun openOrReattach(
        frames: Flow<BridgeFrame>,
        smooth: Boolean = true,
    ) = session.openOrReattach(frames, smooth)

    /** 起管道失败也要说出来，与「下行断了」走同一条通道，见 [ChatSession.reportStartFailure]。 */
    fun reportStartFailure(reason: String) = session.reportStartFailure(reason)
}
