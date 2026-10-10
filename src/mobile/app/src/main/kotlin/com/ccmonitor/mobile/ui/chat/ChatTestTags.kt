package com.ccmonitor.mobile.ui.chat

/**
 * 供 Compose UI 测试定位的 tag，与显示文案解耦：文案会改，tag 不改。
 */
object ChatTestTags {
    const val PANE = "chat-pane"
    const val INPUT = "chat-input"
    const val SEND = "chat-send"
    const val STOP = "chat-stop"

    /** 翻历史的几种状态：测试要能分辨「在载」「到顶了」「出错」。 */
    const val LOADING_OLDER = "chat-loading-older"
    const val NO_MORE_HISTORY = "chat-no-more-history"
    const val HISTORY_ERROR = "chat-history-error"

    /** 权限模式指示，只读：协议没给可选列表。 */
    const val PERMISSION_MODE = "chat-permission-mode"

    /** 会话形态提示（「只回显」/「已停止显示，远端可能还在跑」）。 */
    const val SESSION_MODE = "chat-session-mode"

    /** 命令与 skill 选择器的触发键，catalog 为空时不渲染。 */
    const val CATALOG = "chat-catalog"

    /** 加附件。 */
    const val ATTACH = "chat-attach"

    /** 重新接上下行。 */
    const val REATTACH = "chat-reattach"

    /** 「需手动」条：钉在顶栏下、不可关，关掉后用户就不知道为什么发不出去。 */
    const val WAITING = "chat-waiting"

    /** 「我知道，还是发」：等待态下总能解除的那条路。 */
    const val SEND_ANYWAY = "chat-send-anyway"

    /** 配额。流上没给百分比时不渲染，不编数。 */
    const val QUOTA = "chat-quota"

    /** 「它想做的一件事被挡住了」（下行 `system/permission_denied`）。 */
    const val BLOCKED = "chat-blocked"

    /** 顶栏左上的 ☰，拉出抽屉。 */
    const val DRAWER_OPEN = "chat-drawer-open"
}
