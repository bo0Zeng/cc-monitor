package com.ccmonitor.mobile.ui.drawer

import com.ccmonitor.mobile.ui.overview.HistoryItem

/** 抽屉里有什么：项、顺序、文案都住这里，[ChatDrawer] 只负责画。没路可走的项不列。 */
enum class DrawerItem {
    NewConversation,
    Conversations,
    Files,
    Terminal,
}

/** 抽屉里点下去的每一件事。去哪由导航层 `drawerDestination` 定。 */
sealed interface DrawerAction {
    data object NewConversation : DrawerAction

    data object Conversations : DrawerAction

    data object Files : DrawerAction

    data object Terminal : DrawerAction

    /** 底行点名字那半 ⇒ 服务器列表。 */
    data object Servers : DrawerAction

    /** 底行 ⚙。 */
    data object Settings : DrawerAction

    /** 「最近」里点一条 ⇒ 打开它、接着聊。 */
    data class OpenRecent(
        val sessionId: String,
    ) : DrawerAction
}

/**
 * 抽屉上半的一项。
 *
 * @property accent 强调色（只有「新对话」）。
 * @property onLongPress 长按做什么。目前恒为 null：选账号要先接 `accounts-list`。
 */
data class DrawerEntry(
    val item: DrawerItem,
    val glyph: String,
    val label: String,
    val accent: Boolean = false,
    val onLongPress: DrawerAction? = null,
)

fun drawerEntries(): List<DrawerEntry> =
    listOf(
        DrawerEntry(DrawerItem.NewConversation, "⊕", "新对话", accent = true),
        DrawerEntry(DrawerItem.Conversations, "💬", "对话"),
        DrawerEntry(DrawerItem.Files, "📁", "文件"),
        DrawerEntry(DrawerItem.Terminal, "⌨", "终端"),
    )

fun DrawerItem.action(): DrawerAction =
    when (this) {
        DrawerItem.NewConversation -> DrawerAction.NewConversation
        DrawerItem.Conversations -> DrawerAction.Conversations
        DrawerItem.Files -> DrawerAction.Files
        DrawerItem.Terminal -> DrawerAction.Terminal
    }

/** 「最近」里的一行：只有标题。编号只用来打开它，不上屏。 */
data class RecentEntry(
    val sessionId: String,
    val title: String,
)

/**
 * 「最近」列什么。数据与「对话」那一屏同一个来源（核心 `history-list`，已按最后活动排好，这里不重排）。
 *
 * @return null = 整段不出（列表还没拿到，或拿到了一条都没有）。
 */
fun recentEntries(history: List<HistoryItem>?): List<RecentEntry>? =
    history
        ?.map { RecentEntry(it.sessionId, it.label) }
        ?.takeIf { it.isNotEmpty() }

/**
 * 「最近」那一截放得下几行：放满为止、不滚动、不画半行。一行都放不下 ⇒ 0（小标题也不画）。
 */
fun recentFitCount(
    availablePx: Int,
    headerPx: Int,
    rowPx: Int,
    total: Int,
): Int {
    if (total <= 0 || rowPx <= 0) return 0
    return ((availablePx - headerPx) / rowPx).coerceIn(0, total)
}

const val DRAWER_TITLE = "cc-monitor"

const val RECENT_HEADER = "最近"

fun serverRowLabel(name: String?): String = if (name.isNullOrBlank()) "服务器" else "服务器 · $name"

const val SETTINGS_GLYPH = "⚙"

/** ⚙ 念给读屏软件听的字。 */
const val SETTINGS_DESCRIPTION = "设置"

/** 聊天屏左上那颗。 */
const val DRAWER_OPEN_GLYPH = "☰"

const val DRAWER_OPEN_DESCRIPTION = "打开抽屉"

/** 抽屉里一件事的去处：[route]；[replacesChat] = 换掉当前这条对话，而不是往返回栈上叠一屏。 */
data class DrawerNav(
    val route: String,
    val replacesChat: Boolean,
)
