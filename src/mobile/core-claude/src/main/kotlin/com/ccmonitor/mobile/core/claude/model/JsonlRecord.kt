package com.ccmonitor.mobile.core.claude.model

/*
 * Claude Code 会话 JSONL 的结构化模型（从真实样本逆向）。
 * 只保留渲染/分支重建所需字段；解析容错 —— 未知/缺字段绝不抛异常，归 Unknown。
 */

/** message.content[] 内的块。 */
sealed interface ContentBlock {
    data class Text(
        val text: String,
    ) : ContentBlock

    data class Thinking(
        val thinking: String,
    ) : ContentBlock

    /** input 中的 JSON 数字经 moshi 解析为 Double（非 Int）——消费方取数值时须 `(v as? Number)`。 */
    data class ToolUse(
        val id: String,
        val name: String,
        val input: Map<String, Any?>,
    ) : ContentBlock

    data class ToolResult(
        val toolUseId: String?,
        val text: String,
        val isError: Boolean,
    ) : ContentBlock

    data class Unknown(
        val type: String,
    ) : ContentBlock
}

/**
 * 会话叉分标记（top-level `forkedFrom`），`/branch` 从某轮 fork 出新会话时写入。
 * [sessionId] 是父会话 sid（新文件里每条都一样，取首条认 parent）；
 * [messageUuid] 是该条记录在父会话里的原 uuid，逐条不同，不是整段共享的单一 fork 点。
 */
data class ForkedFrom(
    val sessionId: String,
    /** 目前没有读方，保留：后端会话列表也同时给出 `forkedFromSessionId` 与 `forkedFromMessageUuid` 两格。 */
    val messageUuid: String,
)

/**
 * assistant 消息的 token 用量（`message.usage`），供用量与上下文窗。
 * [cacheCreation] 是写缓存总量；[cacheCreation5m]/[cacheCreation1h] 是 `cache_creation.ephemeral_*` 的拆分。
 */
data class Usage(
    val input: Long,
    val output: Long,
    val cacheCreation: Long,
    val cacheRead: Long,
    val cacheCreation5m: Long = 0,
    val cacheCreation1h: Long = 0,
)

/** Edit/Write 的结构化 diff 块（`toolUseResult.structuredPatch[]`），供保真 diff 渲染。 */
data class PatchHunk(
    val oldStart: Int,
    val oldLines: Int,
    val newStart: Int,
    val newLines: Int,
    val lines: List<String>,
)

/**
 * 一行 JSONL → 一条记录。
 * uuid/parentUuid/timestamp 供 [MainBranch] 重建 ESC-fork 主分支（timestamp 为 ISO-8601，字典序=时间序）。
 */
sealed interface JsonlRecord {
    val uuid: String?
    val parentUuid: String?
    val timestamp: String?

    data class User(
        override val uuid: String?,
        override val parentUuid: String?,
        val blocks: List<ContentBlock>,
        val isMeta: Boolean,
        override val timestamp: String?,
        val sessionId: String?,
        val cwd: String?,
        // 子 agent 侧链记录（通常在独立文件，这里是防御）。MainBranch 仍纳入链，classify 跳过渲染。
        val isSidechain: Boolean = false,
        // 会话叉分标记（top-level forkedFrom）。
        val forkedFrom: ForkedFrom? = null,
        // tool result 的结构化 diff（top-level toolUseResult.structuredPatch）。
        val structuredPatch: List<PatchHunk>? = null,
        // 被 diff 的文件路径（top-level toolUseResult.filePath），显示文件名时省一次 tool_use 配对。
        val patchFilePath: String? = null,
    ) : JsonlRecord

    data class Assistant(
        override val uuid: String?,
        override val parentUuid: String?,
        val blocks: List<ContentBlock>,
        val model: String?,
        override val timestamp: String?,
        val sessionId: String?,
        val isApiError: Boolean,
        // message.stop_reason（"end_turn"=Claude 跑完一轮等用户；"tool_use"=还有后续）。
        // Claude Code 把一条消息按内容块拆成多条记录、每条共享同一 stop_reason → 检测 turn-end 必须用它。
        val stopReason: String? = null,
        // 子 agent 侧链记录（防御，同 User.isSidechain）。
        val isSidechain: Boolean = false,
        // token 用量（message.usage）。
        val usage: Usage? = null,
        // API 请求 id（top-level `requestId`）。一条响应被拆成多条 assistant 记录、usage 逐条重复，用量按它去重，防多计。
        val requestId: String? = null,
    ) : JsonlRecord

    data class System(
        override val uuid: String?,
        override val parentUuid: String?,
        /** 目前没有读方，保留：这两格是分辨 system 记录种类（compact 提示、钩子输出、告警）的唯一信息。 */
        val subtype: String?,
        val level: String?,
        val isMeta: Boolean,
        override val timestamp: String?,
    ) : JsonlRecord

    /** ai-title / custom-title —— 会话标题。无 uuid，不参与分支链。 */
    data class Title(
        val title: String,
        val sessionId: String?,
        // custom-title（用户手设）与 ai-title（AI 生成）要区分：目录标题 custom 优先。
        val custom: Boolean = false,
    ) : JsonlRecord {
        override val uuid: String? get() = null
        override val parentUuid: String? get() = null
        override val timestamp: String? get() = null
    }

    data class Attachment(
        override val uuid: String?,
        override val parentUuid: String?,
        override val timestamp: String?,
    ) : JsonlRecord

    /**
     * 未知 type / 解析失败等。保留真 uuid/parentUuid/timestamp（有则作 pass-through 参与 [MainBranch] 链，
     * 不让其后的对话变孤儿）与 [rawJson]（原始行，schema 变了可重放捞回）。blank/unparseable 无 uuid，不入链。
     */
    data class Unknown(
        val type: String,
        override val uuid: String? = null,
        override val parentUuid: String? = null,
        override val timestamp: String? = null,
        val rawJson: String? = null,
    ) : JsonlRecord
}
