package com.ccmonitor.mobile.core.ui

import androidx.compose.runtime.Composable
import androidx.compose.runtime.SideEffect
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue

/**
 * 一格输入框手上的字：读到的那份（[offer]）变了，只有手上没改过（字 ＝ 上一次读到的）才跟着换；
 * 打了一半、没交的留着 —— 推送来一次不冲掉。交成了，读回来的就是打的那个，之后又跟着读到的走。
 */
@Stable
class Held(
    shown: String,
) {
    private var base = shown

    /** 框里此刻的字。 */
    var text by mutableStateOf(shown)

    /** 读到新的一份。 */
    fun offer(shown: String) {
        if (text == base) text = shown
        base = shown
    }

    /** 不交（空着 · 写法不对）⇒ 回到上一次读到的那份。 */
    fun reset() {
        text = base
    }
}

/** 一格输入框的 [Held]：[shown] 是回包里那一格此刻的字（拿回包当 `remember` 的 key 会把打了一半的字冲掉）。 */
@Composable
fun rememberHeld(shown: String): Held {
    val held = remember { Held(shown) }
    SideEffect { held.offer(shown) }
    return held
}
