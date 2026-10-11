package com.ccmonitor.mobile.ui.chat

import androidx.lifecycle.ViewModel
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

    init {
        session.open()
    }

    fun loadOlder() = session.loadOlder()

    fun send(text: String) = session.send(text)

    fun retry(localId: String) = session.retry(localId)

    fun stop() = session.stop()
}
