package com.ccmonitor.mobile.core.claude.bridge

/**
 * 把 `init` 帧的命令、skill、工具、agent、MCP、plugin 清单整理成可直接渲染的分区视图。纯函数。
 *
 * - `skills` 是 `cmds` 的子集：skill 与其余命令互斥分区，否则同一条显示两遍。
 * - 「点了有没有用」是唯一的可点判据。`tools` 是模型的能力，`mcp` 和 `plugins` 的名字插进草稿
 *   也不是可调用的东西，三者只读；`agents` 有调用形式 `@agent-<名>`，可点。
 * - 同名的 skill 与 MCP 服务器各出现一次：它们是不同的东西，MCP 那条还带着状态。
 *
 * 这里不做过滤，过滤由面板组件自己做。
 */
data class CommandCatalog(
    val skills: List<CatalogEntry> = emptyList(),
    /** `init.cmds` 减去 `init.skills`，与 [skills] 互斥。 */
    val commands: List<CatalogEntry> = emptyList(),
    val agents: List<CatalogEntry> = emptyList(),
    val mcp: List<CatalogEntry> = emptyList(),
    /** 只读：模型能力，不是用户可调用的命令。 */
    val tools: List<CatalogEntry> = emptyList(),
    /** 只读。 */
    val plugins: List<CatalogEntry> = emptyList(),
) {
    /** 全空（没有 `init` 帧，或对端什么都没报）时 UI 不弹空壳面板。 */
    val isEmpty: Boolean
        get() = all.isEmpty()

    /** 六个分区拼起来；[isEmpty] 由它派生，加分区时不会漏。 */
    val all: List<CatalogEntry>
        get() = skills + commands + agents + mcp + tools + plugins

    companion object {
        /** `init` 缺席（还没收到，或重放的片段里没有）时返回空 catalog。 */
        fun from(init: BridgeFrame.Init?): CommandCatalog {
            if (init == null) return CommandCatalog()
            val skillNames = init.skills.toSet()
            return CommandCatalog(
                skills = init.skills.map { CatalogEntry(it, insertable = "/$it ") },
                // 减掉 skill，否则同一条显示两遍。
                commands = init.cmds.filterNot { it in skillNames }.map { CatalogEntry(it, insertable = "/$it ") },
                // agent 的调用形式是 `@agent-<名>`：CLI 的 mention 正则是
                // `(^|[\s。、？！])@(agent-[\w:.@-]+)`，剥掉 `agent-` 前缀去匹配 `agentType`。
                // 注意：必须在行首或空白之后，`appendToken` 保证这一点。
                agents = init.agents.map { CatalogEntry(it, insertable = "$AGENT_MENTION_PREFIX$it ") },
                // status 带给 UI 显示（`needs-auth` 值得看到），不当可点判据。
                mcp = init.mcp.map { CatalogEntry(it.name, insertable = null, note = it.status) },
                tools = init.tools.map { CatalogEntry(it, insertable = null) },
                plugins = init.plugins.map { CatalogEntry(it, insertable = null) },
            )
        }

        /** CLI 的 mention 正则认的前缀（解析时会把 `agent-` 剥掉再匹配 `agentType`）。 */
        const val AGENT_MENTION_PREFIX = "@agent-"
    }
}

/**
 * catalog 里的一条。
 *
 * @param insertable 点中之后往草稿里插什么；`null` = 只读、不可点。
 * @param note 附注（`mcp` 的 status）。
 */
data class CatalogEntry(
    val name: String,
    val insertable: String?,
    val note: String? = null,
)
