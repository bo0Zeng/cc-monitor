package com.ccmonitor.mobile.service

/**
 * 前台转换检测，从 `ActivityLifecycleCallbacks` 抽出成纯逻辑以便 JVM 单测。
 *
 * `started` 是处于 started 态的 Activity 数，≥1 即前台。只有 0→1（后台回前台，含冷启动）触发一次
 * [onEnterForeground]；同一前台内再起一个 Activity（1→2）不触发，免得探测扎堆。
 *
 * [onEnterForeground] 接 `SshConnectionManager.probeAndReconnectStale`：每次回到 app 就主动探活、重连僵死会话，
 * 免得静默断掉的连接要等首次交互吃满 15s openShell 看门狗才发现。
 */
class ForegroundTracker(
    private val onEnterForeground: () -> Unit,
    private val setForeground: (Boolean) -> Unit,
) {
    private var started = 0

    fun onActivityStarted() {
        started++
        setForeground(true)
        if (started == 1) onEnterForeground() // 只在 0→1 触发
    }

    fun onActivityStopped() {
        started--
        if (started <= 0) setForeground(false)
    }
}
