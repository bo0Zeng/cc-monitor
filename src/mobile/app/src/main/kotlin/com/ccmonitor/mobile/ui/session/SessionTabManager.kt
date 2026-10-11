package com.ccmonitor.mobile.ui.session

import androidx.lifecycle.ViewModelStore
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

/**
 * 已开会话 tab 的应用级状态：已开列表 ＋ 每个 tab 一个 [ViewModelStore]。
 * 会话 VM 落在这个 store 里（经 `LocalViewModelStoreOwner` 覆盖），总览与会话之间来回导航不清它；
 * 只有 [close] 才 `clear()` → VM.onCleared → 释放连接引用，引用归零才真断连。
 *
 * key 是自增的 `sess-<seq>`，不是 hostId：同主机可开多个 tab，底层 SSH 连接共享复用。
 * tab 目标分两种 [TabTarget]：终端（shell 会话）与 SFTP（文件浏览），两者共享同一 host 连接。
 */
class SessionTabManager {
    /** tab 目标：各类型带各自的 payload。HostedSession 按类型分派到终端屏或 SFTP 屏。 */
    sealed interface TabTarget {
        /**
         * 终端 shell 会话。
         * - [cd]：初始工作目录（SFTP「在此打开终端」传入）。
         * - [launcherId]：启动配置（cc / cct）。
         * - [tmuxSession]：首连即接回的会话名（敲那台渲出的那一行）。进去重键：不同名各开一个 tab，同名复用。
         */
        data class Terminal(
            val cd: String? = null,
            val launcherId: String? = null,
            val tmuxSession: String? = null,
        ) : TabTarget

        /** SFTP 文件浏览（Termius 式）。无 payload——同主机至多 1 个（去重键仅 hostId+类型）。 */
        data object Sftp : TabTarget
    }

    /**
     * 一个已开 tab。
     * - [label]：人类可读名，建 tab 时传入。不参与去重（去重只比 hostId＋target，见 [open]），
     *   所以放在这一层而不放进 [TabTarget.Terminal] 的 data class equals。null 时渲染处依次退到 tmux 名 / 主机名。
     */
    data class OpenSession(
        val key: String,
        val hostId: String,
        val target: TabTarget,
        val label: String? = null,
    )

    private val _sessions = MutableStateFlow<List<OpenSession>>(emptyList())
    val sessions: StateFlow<List<OpenSession>> = _sessions.asStateFlow()

    // 只在 UI 线程调用（nav 回调 / 总览按钮），所以 seq / stores 不加锁。
    private val stores = mutableMapOf<String, ViewModelStore>()
    private var seq = 0

    /**
     * 打开 tab，返回 key。hostId＋[target] 已存在则复用（按结构相等），
     * 所以同主机可同时有：默认终端 tab、若干 attach tab、一个 SFTP tab。
     *
     * - [forceNew]：跳过复用、强制新建（「＋新会话」），同样的 target 也各成独立 shell 与 VM store。
     * - [label]：不进去重键。复用旧 tab 时 null 可被非 null 补上，已有的非 null 不覆写。
     */
    fun open(
        hostId: String,
        target: TabTarget,
        label: String? = null,
        forceNew: Boolean = false,
    ): String {
        if (!forceNew) {
            _sessions.value
                .firstOrNull { it.hostId == hostId && it.target == target }
                ?.let { existing -> return reuseExisting(existing, label) }
        }
        val key = "sess-${seq++}"
        stores[key] = ViewModelStore()
        _sessions.update { it + OpenSession(key, hostId, target, label) }
        return key
    }

    /** 去重命中时补上 label（null 可被非 null 覆写，非 null 不覆写），返回复用的 key。 */
    private fun reuseExisting(
        existing: OpenSession,
        label: String?,
    ): String {
        _sessions.update { list -> list.map { s -> if (s.key != existing.key) s else s.copy(label = s.label ?: label) } }
        return existing.key
    }

    fun get(key: String): OpenSession? = _sessions.value.firstOrNull { it.key == key }

    /**
     * 该 tab 的 ViewModelStore。只对在册（[open] 过且未 [close]）的 key 返回或补建；不在册就抛，
     * 否则会复活一个永远不会被 close 的 store，里面 VM 的 onCleared 永不触发，连接引用永久泄漏。
     * 只在 UI 线程调用。
     */
    fun storeFor(key: String): ViewModelStore {
        check(_sessions.value.any { it.key == key }) { "storeFor($key)：会话不在册（已 close 或从未 open），拒绝复活幽灵 store" }
        return stores.getOrPut(key) { ViewModelStore() }
    }

    /** 关闭会话：clear store（→ VM.onCleared → 释放连接引用）+ 从列表移除。 */
    fun close(key: String) {
        stores.remove(key)?.clear()
        _sessions.update { list -> list.filterNot { it.key == key } }
    }
}
