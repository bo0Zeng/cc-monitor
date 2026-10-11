package com.ccmonitor.mobile.core.claude.agent

import com.ccmonitor.mobile.core.claude.command.AgentInvocation
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.command.CodexInvocation
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.claude.transport.ClaudeSessionLocator
import com.ccmonitor.mobile.core.claude.transport.CodexSessionLocator
import com.ccmonitor.mobile.core.claude.transport.SessionLocator

/**
 * 一个 agent 一份档案。消费方只问档案，不问种类。
 *
 * 一家一句的事实（产品名、启动命令、记录住哪……）都收在这里；
 * [of] 是唯一取法，构造器与实例都是 `private`，外面建不出第三份。
 * 加一种 agent 时 [of] 的 `when` 因穷尽性编译不过，逼着补一份档案。
 */
class AgentProfile private constructor(
    /** 这份档案是哪一种 agent。生产代码里只有本文件持 [AgentKind] 字面。 */
    val kind: AgentKind,
    /** UI 上的产品名。界面上一律取这里，不硬写「Claude」：在 Codex 服务器上那是一句假话。 */
    val displayName: String,
    /** 守护进程帧里的 `agent_kind` 串（`"claude"` | `"codex"`）。反向映射见 [fromWire]。 */
    val wireName: String,
    /** 起新对话前要不要先选账号。只读档不起新对话，这道门对它没有意义。 */
    val requiresAccountGate: Boolean,
    /**
     * 这一家的 agent-dir 认不认「设置」屏那个应用级默认，认的话是哪个键；`null` = 不认
     * （只有主机覆盖加内置默认）。做成枚举而非 `Boolean`，第三家进来时消费方的 `when` 会编译不过。
     */
    val appDefaultDirSetting: AgentDirSettingKey?,
    /** resume 与启动命令的载荷构造（sid 校验、命令净化、候选链、resume 串、会话名）。 */
    val invocation: AgentInvocation,
    /** 远端会话定位命令（resume 找那份记录）、`path→sessionId`、agent-dir 解析。 */
    val sessionLocator: SessionLocator,
) {
    override fun toString(): String = "AgentProfile(${kind.name})"

    companion object {
        private val CLAUDE_CODE =
            AgentProfile(
                kind = AgentKind.ClaudeCode,
                displayName = "Claude",
                wireName = "claude",
                requiresAccountGate = true,
                appDefaultDirSetting = AgentDirSettingKey.AppDefaultClaudeDir,
                invocation = ClaudeInvocation,
                sessionLocator = ClaudeSessionLocator,
            )

        private val CODEX =
            AgentProfile(
                kind = AgentKind.Codex,
                displayName = "Codex",
                wireName = "codex",
                requiresAccountGate = false,
                // 只认主机覆盖与内置 `${CODEX_HOME:-$HOME/.codex}`。
                appDefaultDirSetting = null,
                invocation = CodexInvocation,
                sessionLocator = CodexSessionLocator,
            )

        /** 唯一取法。生产代码里对 [AgentKind] 的 `when` 只有这一处。 */
        fun of(kind: AgentKind): AgentProfile =
            when (kind) {
                AgentKind.ClaudeCode -> CLAUDE_CODE
                AgentKind.Codex -> CODEX
            }

        /** 全部档案（枚举序）。列 agent 选项的 UI 遍历它，别自己数有几种。 */
        val ALL: List<AgentProfile> get() = AgentKind.entries.map(::of)

        /**
         * 种类读不出来或没存过时用的缺省档。取保守的一侧：宁可当 Claude 多问一次账号，
         * 也不把一台 Claude 服务器放行成没登录的默认账号跑（那样会得到 `api_error`）。
         */
        val DEFAULT: AgentProfile get() = CLAUDE_CODE

        /** 帧里的 `agent_kind` 串 → 档案。缺或未知值 → [DEFAULT]。 */
        fun fromWire(wire: String?): AgentProfile = ALL.firstOrNull { it.wireName == wire } ?: DEFAULT

        /** Room 里存的值（`hosts.agentKind`，即 [AgentKind.name]）→ 档案。null 或认不出 → [DEFAULT]。 */
        fun ofStoredName(name: String?): AgentProfile = ALL.firstOrNull { it.kind.name == name } ?: DEFAULT

        /**
         * 同 [ofStoredName]，但大小写不敏感。服务器编辑页回显用它；Room 边界用精确匹配的那份。
         * 两处口径不同，统一会改掉「存了小写 `"codex"` 时编辑页显示哪一档」。
         */
        fun ofStoredNameIgnoringCase(name: String?): AgentProfile =
            ALL.firstOrNull { it.kind.name.equals(name, ignoreCase = true) } ?: DEFAULT
    }
}

/** agent-dir 的应用级默认来自「设置」屏的哪个键。 */
enum class AgentDirSettingKey {
    /** `settings` 表里应用级默认的 Claude 配置目录。 */
    AppDefaultClaudeDir,
}
