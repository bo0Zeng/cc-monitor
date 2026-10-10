package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.bridge.CatalogEntry
import com.ccmonitor.mobile.core.claude.bridge.CommandCatalog
import com.ccmonitor.mobile.ui.session.PaletteAction

/**
 * 把 [CommandCatalog] 铺成 [PaletteAction] 列表，交给 `CommandPalette` 渲染。
 *
 * 分组顺序按用户最可能找的在前：skill、命令、agent、MCP、工具、插件；后四组只读。
 */
fun CommandCatalog.toPaletteActions(onInsert: (String) -> Unit): List<PaletteAction> =
    buildList {
        addAll(skills.toActions(GROUP_SKILL, onInsert))
        addAll(commands.toActions(GROUP_COMMAND, onInsert))
        addAll(agents.toActions(GROUP_AGENT, onInsert))
        addAll(mcp.toActions(GROUP_MCP, onInsert))
        addAll(tools.toActions(GROUP_TOOL, onInsert))
        addAll(plugins.toActions(GROUP_PLUGIN, onInsert))
    }

private fun List<CatalogEntry>.toActions(
    group: String,
    onInsert: (String) -> Unit,
): List<PaletteAction> =
    map { entry ->
        val insertable = entry.insertable
        PaletteAction(
            group = group,
            label = entry.name,
            subtitle = entry.note,
            // 判据是 `insertable != null`，不看分组：只读条目没有 lambda，点了不会插进空东西。
            run = insertable?.let { s -> { onInsert(s) } },
        )
    }

/**
 * 把选中的条目接到草稿末尾：草稿空就是它，已以空白结尾直接接，否则补一个空格。
 *
 * 不 trim 草稿，也不去重：用户可能还要接着写，连点两次也是用户自己的意思。
 */
fun String.appendToken(token: String): String =
    when {
        isEmpty() -> token
        last().isWhitespace() -> this + token
        else -> "$this $token"
    }

const val GROUP_SKILL = "skill"
const val GROUP_COMMAND = "命令"
const val GROUP_AGENT = "agent（只读）"
const val GROUP_MCP = "MCP（只读）"

/**
 * 只读组在组名里写明，免得让人以为点了会有事发生。
 * 工具是模型的能力；agent 与 MCP 服务器名插进草稿也调用不了。
 */
const val GROUP_TOOL = "工具（只读）"
const val GROUP_PLUGIN = "插件（只读）"
