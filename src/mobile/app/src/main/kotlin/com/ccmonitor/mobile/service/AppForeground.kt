package com.ccmonitor.mobile.service

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow

/**
 * 进程前台标志，由 [com.ccmonitor.mobile.AtermApp] 经 [ForegroundTracker] 维护。
 * turn-end watcher 据此决定发不发「完成」通知：app 在前台时不发。
 * 终端自动重挂用可观测的 [foreground]：回到前台且该 tab 在用时才重挂僵死的 shell。
 */
object AppForeground {
    private val _foreground = MutableStateFlow(false)

    /** 可观测的前台标志。 */
    val foreground: StateFlow<Boolean> = _foreground.asStateFlow()

    /** 同步读的前台标志。只由本模块（[ForegroundTracker]）写。 */
    var isForeground: Boolean
        get() = _foreground.value
        internal set(value) {
            _foreground.value = value
        }
}
