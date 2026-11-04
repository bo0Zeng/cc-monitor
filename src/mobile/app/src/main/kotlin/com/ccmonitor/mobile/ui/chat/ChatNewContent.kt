package com.ccmonitor.mobile.ui.chat

/**
 * 「↓ 新内容」那一颗（同桌面会话查看器那一颗，不带数）：往上翻着（不贴底）时流尾长了 ⇒ 输入框上方居中出一颗；
 * 点了滚到底、恢复跟随；人自己滚回底 ⇒ 收起。出这一颗不滚、不跳位。
 */
internal object ChatNewContent {
    /** 流尾此刻的样子：尾那一张的 key ＋ 它的字长 ＋ 活卡的长度（同跟到底那一格的触发）。往上翻载进更早的只在头上插，尾不变。 */
    data class Tail(
        val key: String?,
        val length: Int,
        val live: Int,
    )

    /** 上一刻亮不亮 [shown] · 上一刻的尾 [before]（还没见过 ⇒ `null`）· 此刻的尾 [now] · 此刻贴不贴底 [following] ⇒ 此刻亮不亮。 */
    fun shown(
        shown: Boolean,
        before: Tail?,
        now: Tail,
        following: Boolean,
    ): Boolean = !following && (shown || (before != null && before != now))
}
