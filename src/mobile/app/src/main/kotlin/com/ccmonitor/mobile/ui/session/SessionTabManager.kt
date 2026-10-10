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
         * - [cd]：初始工作目录（SFTP「在此打开终端」传入）；对 resume tab 只是兜底 cwd。
         * - [launcherId]：启动配置（cc / cct）。
         * - [tmuxSession]：首连即附着的 tmux 会话名。进去重键：不同名各开一个 tab，同名复用。
         * - [resumeSessionId]：要 resume 的 Claude 会话 id。shell 在 [cd] 起 tmux 跑 `<cc> --resume`；
         *   同 sid 复用 tab，不同 sid 各开。
         * 长按选定的 resume 命令放 [OpenSession.resumeCommand]，不进去重键：同 sid 的第二个 tab
         * 只会 attach 同一个 tmux 会话、命令永不生效，所以同 sid 总复用第一个 tab。
         */
        data class Terminal(
            val cd: String? = null,
            val launcherId: String? = null,
            val tmuxSession: String? = null,
            val resumeSessionId: String? = null,
        ) : TabTarget

        /** SFTP 文件浏览（Termius 式）。无 payload——同主机至多 1 个（去重键仅 hostId+类型）。 */
        data object Sftp : TabTarget
    }

    /**
     * 一个已开 tab。
     * - [label]：人类可读名（如 resume 会话标题），建 tab 时传入。不参与去重（去重只比 hostId＋target，
     *   见 [open]），所以放在这一层而不放进 [TabTarget.Terminal] 的 data class equals。null 时渲染处
     *   依次退到 tmux 名 / cc-<sid8> / 主机名。
     * - [resumeCommand]：历史页长按选定的 resume 命令（null = 默认候选链），经 VM 参数注入候选链首位。
     *   不进去重键；去重命中时默认不覆写，例外见 [reuseExisting]。
     */
    data class OpenSession(
        val key: String,
        val hostId: String,
        val target: TabTarget,
        val label: String? = null,
        val resumeCommand: String? = null,
    )

    private val _sessions = MutableStateFlow<List<OpenSession>>(emptyList())
    val sessions: StateFlow<List<OpenSession>> = _sessions.asStateFlow()

    // 只在 UI 线程调用（nav 回调 / 总览按钮），所以 seq / stores 不加锁。
    private val stores = mutableMapOf<String, ViewModelStore>()
    private var seq = 0

    // resume 失败后重跑的句柄表。key = (hostId, resumeSessionId)，与 resume tab 的去重身份一致，
    // 所以 VM 不需要知道自己的 tab key。只在 UI 线程调用。
    private val resumeRedrivers = mutableMapOf<Pair<String, String>, (command: String?, cwd: String?) -> Boolean>()

    /**
     * 打开 tab，返回 key。hostId＋[target] 已存在则复用（resume tab 的判等见 [dedupeKeyMatches]），
     * 所以同主机可同时有：默认终端 tab、若干 attach tab、若干 resume tab、一个 SFTP tab。
     *
     * - [forceNew]：跳过复用、强制新建（「＋新会话」），同样的 target 也各成独立 shell 与 VM store。
     * - [label]：不进去重键。复用旧 tab 时 null 可被非 null 补上（同一会话先后从阅读器和历史页 resume，
     *   后一次才带标题），已有的非 null 不覆写。
     * - [resumeCommand]：不进去重键，默认先写为准：复用 tab 的 tmux 会话已在跑，再选的命令不会生效，
     *   覆写就是谎报。唯一例外见 [reuseExisting]。
     */
    fun open(
        hostId: String,
        target: TabTarget,
        label: String? = null,
        resumeCommand: String? = null,
        forceNew: Boolean = false,
    ): String {
        if (!forceNew) {
            _sessions.value
                .firstOrNull { it.hostId == hostId && dedupeKeyMatches(it.target, target) }
                ?.let { existing -> return reuseExisting(existing, target, label, resumeCommand) }
        }
        val key = "sess-${seq++}"
        stores[key] = ViewModelStore()
        _sessions.update { it + OpenSession(key, hostId, target, label, resumeCommand) }
        return key
    }

    /**
     * 去重命中时更新已开 tab 的记录，返回复用的 key。
     * - label：null 可被非 null 覆写，非 null 不覆写。
     * - resumeCommand / target.cd：默认不覆写。只有命中「上次 resume 已失败」的 tab（[redriveIfResumeFailed]
     *   经句柄确认、活 VM 已同 tab 重跑）才覆写为新值；否则 watchdog 杀掉空会话后，同 sid 再选合法命令
     *   会静默命中死 tab，坏命令永远钉死。
     * - 运行中 / 在途 / 句柄未注册：一律不动，绝不往运行中的 Claude 打第二条命令。
     */
    private fun reuseExisting(
        existing: OpenSession,
        target: TabTarget,
        label: String?,
        resumeCommand: String?,
    ): String {
        val redriven = redriveIfResumeFailed(existing, target, resumeCommand)
        _sessions.update { list ->
            list.map { s ->
                if (s.key != existing.key) {
                    s
                } else {
                    s.copy(
                        label = s.label ?: label,
                        resumeCommand = if (redriven) resumeCommand else s.resumeCommand,
                        target = if (redriven) targetWithUpgradedCd(s.target, target) else s.target,
                    )
                }
            }
        }
        return existing.key
    }

    /**
     * 去重命中 resume tab 时，把新选的命令和 cwd 经句柄送到该 tab 的活 VM。
     * true = VM 确认上次 resume 已失败（watchdog FAILED / ABSENT）并已带新值重跑，调用方覆写记录；
     * false = 运行中 / 在途 / 连接中，或句柄未注册。
     */
    private fun redriveIfResumeFailed(
        existing: OpenSession,
        target: TabTarget,
        resumeCommand: String?,
    ): Boolean {
        val incoming = target as? TabTarget.Terminal ?: return false
        val sid = incoming.resumeSessionId ?: return false
        val handler = resumeRedrivers[existing.hostId to sid] ?: return false
        return handler(resumeCommand, incoming.cd)
    }

    /** 重跑被接受时更新兜底 cwd：新 cd 非 null 才覆写，null 不降级已有值（与 VM 侧同规则）。 */
    private fun targetWithUpgradedCd(
        old: TabTarget,
        new: TabTarget,
    ): TabTarget {
        val o = old as? TabTarget.Terminal ?: return old
        val newCd = (new as? TabTarget.Terminal)?.cd ?: return o
        return o.copy(cd = newCd)
    }

    /**
     * 注册 resume 失败重跑句柄，key = (hostId, [sessionId])。resume tab 的 VM 建成时注册、onCleared 时
     * 注销。句柄返回 true = VM 处于失败态、已接受并重跑；false = 运行中或在途。只在 UI 线程调用。
     */
    fun registerResumeRedriver(
        hostId: String,
        sessionId: String,
        handler: (command: String?, cwd: String?) -> Boolean,
    ) {
        resumeRedrivers[hostId to sessionId] = handler
    }

    /** 注销重跑句柄（VM onCleared 时调用；未注册则什么都不做）。 */
    fun unregisterResumeRedriver(
        hostId: String,
        sessionId: String,
    ) {
        resumeRedrivers.remove(hostId to sessionId)
    }

    /**
     * 去重判等。resume tab 的身份是 hostId＋resumeSessionId：tmux 会话按 sid 命名 `cc-<sid8>`，`cd` 只是
     * 兜底 cwd（真 cwd 由远端按 sid 定位），launcherId / tmuxSession 对 resume 没有区分意义。所以两边都是
     * resume tab 时只比 sid，cwd 解不出（cd=null）与 cd 非空的两次同 sid resume 落到同一个 tab。
     * 其余 target 按结构相等。
     */
    private fun dedupeKeyMatches(
        a: TabTarget,
        b: TabTarget,
    ): Boolean {
        val sidA = (a as? TabTarget.Terminal)?.resumeSessionId
        val sidB = (b as? TabTarget.Terminal)?.resumeSessionId
        return if (sidA != null && sidB != null) sidA == sidB else a == b
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
