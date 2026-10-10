package com.ccmonitor.mobile.core.claude.agent

import com.ccmonitor.mobile.core.claude.catalog.ClaudeSessionCatalog
import com.ccmonitor.mobile.core.claude.catalog.CodexSessionCatalog
import com.ccmonitor.mobile.core.claude.catalog.SessionCatalog
import com.ccmonitor.mobile.core.claude.command.AgentInvocation
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation
import com.ccmonitor.mobile.core.claude.command.CodexInvocation
import com.ccmonitor.mobile.core.claude.model.AgentKind
import com.ccmonitor.mobile.core.claude.model.CodexRecordParser
import com.ccmonitor.mobile.core.claude.model.JsonlParser
import com.ccmonitor.mobile.core.claude.model.RecordParser
import com.ccmonitor.mobile.core.claude.transport.ClaudeSessionLocator
import com.ccmonitor.mobile.core.claude.transport.CodexSessionLocator
import com.ccmonitor.mobile.core.claude.transport.SessionLocator
import com.ccmonitor.mobile.core.remote.RemoteCommandChannel

/**
 * 一个 agent 一份档案。消费方只问档案，不问种类。
 *
 * 一家一句的事实（产品名、能不能上行、resume 载荷、记录解析、会话发现……）都收在这里；
 * [of] 是唯一取法，构造器与实例都是 `private`，外面建不出第三份。
 * 加一种 agent 时 [of] 的 `when` 因穷尽性编译不过，逼着补一份档案。
 *
 * resume 真正发出去的命令还要套一层 tmux 脚手架，住 `:app`；它向档案要三样：
 * 载荷（[invocation]）、定位命令（[sessionLocator]）、打不打身份标记（[hasCcmIdentity]）。
 */
class AgentProfile private constructor(
    /** 这份档案是哪一种 agent。生产代码里只有本文件持 [AgentKind] 字面。 */
    val kind: AgentKind,
    /** UI 上的产品名。界面上一律取这里，不硬写「Claude」：在 Codex 服务器上那是一句假话。 */
    val displayName: String,
    /** 守护进程帧里的 `agent_kind` 串（`"claude"` | `"codex"`）。反向映射见 [fromWire]。 */
    val wireName: String,
    /**
     * 这一家能不能说话（上行与输入行形态的唯一判定源）。
     *
     * `false` 时输入行整条不渲染（不是灰掉：灰掉是「等会儿就能发」的承诺，对这一家是假的），
     * [landsOnChatScreen] 也随之为 `false`。Codex 是 `false`：上行那半全按 Claude CLI 的管道形状写。
     */
    val supportsUplink: Boolean,
    /** 起新对话前要不要先选账号。只读档不起新对话，这道门对它没有意义。 */
    val requiresAccountGate: Boolean,
    /**
     * 记录之间靠 `parentUuid` 串成链（Claude），还是按文件序加合成 uuid（Codex 没有 parentUuid）。
     * 决定阅读面能不能走 `MainBranch` 与骨架链。
     */
    val usesParentUuidChain: Boolean,
    /**
     * 后端对这一家有没有会话身份面。`true` 时 resume 出来的 tmux 会话在新建那一支打
     * `@ccm_sid=<完整 sid>`。
     *
     * Codex 是 `false`：后端的 `@ccm_sid` 是事实标记，靠 pidfile 打，Codex 没有 pidfile，后端从不给它打。
     * 我们替它打上就是伪造事实，kill 与送键的身份门会对它放行。
     * 这一格与会话文件怎么摆（[sessionLocator]）是两件事，第三家按 cwd 编码目录存会话也不会因此拿到标记。
     */
    val hasCcmIdentity: Boolean,
    /**
     * 这一家的 agent-dir 认不认「设置」屏那个应用级默认，认的话是哪个键；`null` = 不认
     * （只有主机覆盖加内置默认）。做成枚举而非 `Boolean`，第三家进来时消费方的 `when` 会编译不过。
     */
    val appDefaultDirSetting: AgentDirSettingKey?,
    /** resume 与启动命令的载荷构造（sid 校验、命令净化、候选链、resume 串、会话名）。 */
    val invocation: AgentInvocation,
    /** 一行原始记录 → 统一内部模型。 */
    val recordParser: RecordParser,
    /** 远端会话发现命令、`path→sessionId`、agent-dir 解析。 */
    val sessionLocator: SessionLocator,
    /**
     * 会话目录工厂（会话发现、活动判定、可读标题）。
     *
     * 注意：函数型参数留在最后一个位置，尾随 lambda 才不会绑错格。
     */
    val newSessionCatalog: (channel: RemoteCommandChannel, agentDir: String) -> SessionCatalog,
) {
    /**
     * 点一台这种服务器进去，落聊天屏（`true`）还是对话总览只读档（`false`）。
     *
     * 派生自 [supportsUplink] 而不单独存：「能不能说话」与「去哪儿落地」是同一句话，
     * 拆成两格就可能一半改了一半没改。
     */
    val landsOnChatScreen: Boolean get() = supportsUplink

    override fun toString(): String = "AgentProfile(${kind.name})"

    companion object {
        private val CLAUDE_CODE =
            AgentProfile(
                kind = AgentKind.ClaudeCode,
                displayName = "Claude",
                wireName = "claude",
                supportsUplink = true,
                requiresAccountGate = true,
                usesParentUuidChain = true,
                hasCcmIdentity = true,
                appDefaultDirSetting = AgentDirSettingKey.AppDefaultClaudeDir,
                invocation = ClaudeInvocation,
                recordParser = JsonlParser,
                sessionLocator = ClaudeSessionLocator,
                newSessionCatalog = { channel, agentDir -> ClaudeSessionCatalog(channel, agentDir) },
            )

        private val CODEX =
            AgentProfile(
                kind = AgentKind.Codex,
                displayName = "Codex",
                wireName = "codex",
                // 只读档：没有输入行，落点是对话总览。
                supportsUplink = false,
                requiresAccountGate = false,
                usesParentUuidChain = false,
                hasCcmIdentity = false,
                // 只认主机覆盖与内置 `${CODEX_HOME:-$HOME/.codex}`。
                appDefaultDirSetting = null,
                invocation = CodexInvocation,
                recordParser = CodexRecordParser,
                sessionLocator = CodexSessionLocator,
                newSessionCatalog = { channel, agentDir -> CodexSessionCatalog(channel, agentDir) },
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
