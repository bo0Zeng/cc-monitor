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

    /** 权限档 chip（点开是那台给的档）。 */
    const val PERMISSION_MODE = "chat-permission-mode"

    /** 模型 chip。 */
    const val MODEL_CHIP = "chat-model-chip"

    /** chip 原位转圈（正在换）。 */
    const val CHIP_SPINNER = "chat-chip-spinner"

    /** chip 上方那一行。 */
    const val NOTE = "chat-note"

    /** 模型 · 权限档的抽屉。 */
    const val PICK_SHEET = "chat-pick-sheet"

    /** ［+］与它升起的面板 · 面板里两行下钻 · 下钻那一页的清单。 */
    const val PLUS = "chat-plus"
    const val PLUS_SHEET = "chat-plus-sheet"
    const val PLUS_COMMANDS = "chat-plus-commands"
    const val PLUS_SKILLS = "chat-plus-skills"
    const val COMMAND_LIST = "chat-command-list"

    /** 清空会话的确认框与它的确认键。 */
    const val CLEAR_DIALOG = "chat-clear-dialog"
    const val CLEAR_OK = "chat-clear-ok"

    /** 正文尾巴上的活卡（逐字长出来的那一块）。 */
    const val LIVE = "chat-live"

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
