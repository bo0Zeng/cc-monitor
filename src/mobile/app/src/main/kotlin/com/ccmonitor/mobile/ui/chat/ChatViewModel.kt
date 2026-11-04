package com.ccmonitor.mobile.ui.chat

import androidx.lifecycle.ViewModel
import com.ccmonitor.mobile.core.claude.model.SaidImage
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.HlCode
import com.ccmonitor.mobile.core.wire.gen.SegHl
import com.ccmonitor.mobile.ui.chat.attach.AttachActions
import com.ccmonitor.mobile.ui.chat.attach.AttachUi
import com.ccmonitor.mobile.ui.chat.newsession.NewSessionPicker
import kotlinx.coroutines.flow.StateFlow

/**
 * 聊天面的 ViewModel，只是一层薄壳，实现全在 [ChatSession] 里。
 *
 * 下行不挂在 `viewModelScope` 上：离开聊天屏时 [ChatSession] 照收。
 * 对话何时结束由 [ChatController.release] 决定，不由屏幕生命周期决定，所以不覆写 `onCleared`。
 */
class ChatViewModel(
    private val session: ChatSession,
) : ViewModel() {
    val state: StateFlow<ChatUiState> get() = session.state

    /** 新建会话那一张的选项；已有的那条 ⇒ `null`。 */
    val picker: NewSessionPicker? get() = session.picker

    init {
        session.open()
    }

    fun loadOlder() = session.loadOlder()

    /** 人说的那一句里的一张图（画到那一格时取，见 [ChatSession.image]）。 */
    suspend fun image(img: SaidImage): ByteArray? = session.image(img)

    /** 助手正文一段的代码高亮（建好那一段时拉，见 [ChatSession.segHl]）。 */
    suspend fun segHl(codes: List<HlCode>): SegHl? = session.segHl(codes)

    /** 思考展开那一刻的块树（见 [ChatSession.mdDoc]）。 */
    suspend fun mdDoc(text: String): Doc? = session.mdDoc(text)

    /** 视口贴不贴着底（见 [ChatSession.follow]）。 */
    fun follow(on: Boolean) = session.follow(on)

    fun send(text: String) = session.send(text)

    fun retry(localId: String) = session.retry(localId)

    fun stop() = session.stop()

    fun rename(title: String) = session.rename(title)

    /** 点一行过程（开 ⇄ 合）· 「跳到第一处失败」。 */
    fun toggle(after: String) = session.toggle(after)

    fun jumpFail(after: String) = session.jumpFail(after)

    /** 点一个回退框（开 ⇄ 合）。 */
    fun toggleBranch(run: String) = session.toggleBranch(run)

    /** 跳到一处（会话里找的一条命中 · 大纲的一句）：那一行的字节起点 · 落在哪次工具调用的卡里（见 [ChatSession.jump]）。 */
    fun jump(
        at: Long,
        tool: String?,
    ) = session.jump(at, tool)

    /** 输入框上方那一排附件；新建会话 ⇒ `null`。 */
    val attach: StateFlow<AttachUi>? get() = session.attach?.state

    /** 输入那一圈（➤ · ■ · 两颗 chip · ［+］面板 · 附件那一排）。 */
    val actions: ChatInputActions =
        ChatInputActions(
            send = session::send,
            stop = session::stop,
            switchModel = session::switchModel,
            switchPermission = session::switchPermission,
            loadPicks = session::loadPicks,
            loadCommands = session::loadCommands,
            say = session::say,
            cancel = session::cancel,
            cancelQueued = session::cancelQueued,
            retry = session::retry,
            editDraft = session::editDraft,
            attach = session.attach?.let { t -> AttachActions(add = t::add, remove = t::remove, retry = t::retry, local = t::local) },
            said = SaidActions(edit = session.edits::text, options = session.edits::options, send = session.edits::send, recall = session::cancelQueued),
        )

    /** 聊天屏进 / 出（`stream-watch` 名单跟着它）。 */
    fun visible(on: Boolean) = session.visible(on)
}
