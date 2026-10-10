package com.ccmonitor.mobile.core.claude.model

/**
 * 手机这边送出去、记录里还没出现的那一句的投递状态（`界面.md`「投递状态」；历史记录恒为 `null`）。
 * 送到了的不挂标；记录里出现了这句人话 ⇒ 整句换成记录里那一条。
 */
enum class DeliveryState {
    /** 正在送。 */
    SENDING,

    /** 那台回 `unsure`（或期限到了）：不知道送没送到，不重发。 */
    UNSURE,

    /** 没送到、原因改得了：内容保留，可再发一次。 */
    FAILED,

    /** 没送到、重试没用（会话已结束 · 不归这里管）：不给重试。 */
    FAILED_PERMANENT,
}

/**
 * 阅读窗的渲染单元，由 [RecordClassifier] 从 [JsonlRecord] 流折叠而来；UI 把每个 unit 映射成一张卡片。
 *
 * [key] 是稳定唯一标识（`<uuid>#<块在记录内的序号>`），供 LazyColumn 做 key。基于记录身份而非列表位置，
 * 流式追加新 unit 时已有 key 不漂移。
 *
 * 只做「加字段且带默认值」的演进：默认值必须真是缺省的意思，老的录制素材重放时不传它也能构造，
 * 且构造结果不变。注意：改既有字段的默认值、或字段名类型不变而含义变了，没有机器拦，改之前想清楚老素材重放会变成什么。
 */
sealed interface RenderUnit {
    val key: String
    val sourceUuid: String?

    /** 用户输入的真实 prompt（非 meta、非 tool_result）。 */
    data class UserText(
        override val key: String,
        val text: String,
        override val sourceUuid: String?,
        /** 本地发送状态；`null` ＝ 记录里的那一条，或已送到。 */
        val delivery: DeliveryState? = null,
        /** 气泡下那一句（文案表 `terminal.input.*` 那一族写好的），贴在这条消息旁边。 */
        val deliveryText: String? = null,
    ) : RenderUnit

    /** 助手正文，按 markdown 渲染。 */
    data class AssistantMarkdown(
        override val key: String,
        val markdown: String,
        override val sourceUuid: String?,
    ) : RenderUnit

    /** 助手思考块，默认折叠。 */
    data class Thinking(
        override val key: String,
        val text: String,
        override val sourceUuid: String?,
    ) : RenderUnit

    /**
     * 一次工具调用，已把 tool_use 与其后匹配的 tool_result 合并。
     * [resultText] 为 null 表示仍在执行（无匹配结果）。
     */
    data class ToolCall(
        override val key: String,
        val toolUseId: String,
        val name: String,
        val input: Map<String, Any?>,
        val resultText: String?,
        val isError: Boolean,
        val pending: Boolean,
        override val sourceUuid: String?,
        // Edit/Write 的权威 diff（tool_result 的 top-level structuredPatch）+ 文件名。有则渲染保真 diff（优于 input 推导）。
        val structuredPatch: List<PatchHunk>? = null,
        val patchFilePath: String? = null,
    ) : RenderUnit

    /** 斜杠命令 `/name args`（user 记录里的 `<command-*>` 回显，非真实 prompt）。 */
    data class SlashCommand(
        override val key: String,
        val name: String,
        val args: String,
        override val sourceUuid: String?,
    ) : RenderUnit

    /** `!bash` 输入 `❯ command`。 */
    data class BashInput(
        override val key: String,
        val command: String,
        override val sourceUuid: String?,
    ) : RenderUnit

    /** `!bash` 输出（stdout/stderr 任一可空）。 */
    data class BashOutput(
        override val key: String,
        val stdout: String,
        val stderr: String,
        override val sourceUuid: String?,
    ) : RenderUnit

    /** `/compact` 续接摘要，默认折叠（超长，点开看全文）。 */
    data class CompactSummary(
        override val key: String,
        val text: String,
        override val sourceUuid: String?,
    ) : RenderUnit

    /** 「已中断本轮 · {time}」那一道细线（记录里 `speaker.interrupt` 那一条）；[timeText] 是核心写好的钟面。 */
    data class Interrupt(
        override val key: String,
        val timeText: String?,
        override val sourceUuid: String?,
    ) : RenderUnit

    /**
     * 连续（非交互）工具调用合并成的一张外层折叠组卡（`工具调用 · N 个`）。
     * [calls] ≥2（单个 ToolCall 不入组）。[key] = 首 call 的 key（稳定）。交互工具（AskUserQuestion/ExitPlanMode）不入组。
     */
    data class ToolGroup(
        override val key: String,
        val calls: List<ToolCall>,
        override val sourceUuid: String?,
    ) : RenderUnit
}

/** 会话内搜索用的可搜索纯文本（工具调用含名/参数/输出，便于搜命令、文件名、输出）。 */
fun RenderUnit.searchableText(): String =
    when (this) {
        is RenderUnit.UserText -> text
        is RenderUnit.AssistantMarkdown -> markdown
        is RenderUnit.Thinking -> text
        is RenderUnit.ToolCall -> toolCallSearchable()
        is RenderUnit.SlashCommand -> if (args.isEmpty()) name else "$name $args"
        is RenderUnit.BashInput -> command
        is RenderUnit.BashOutput -> if (stderr.isEmpty()) stdout else "$stdout\n$stderr"
        is RenderUnit.CompactSummary -> text
        is RenderUnit.Interrupt -> ""
        is RenderUnit.ToolGroup -> calls.joinToString("\n") { it.toolCallSearchable() }
    }

private fun RenderUnit.ToolCall.toolCallSearchable(): String =
    buildString {
        append(name)
        input.values.forEach { v ->
            if (v != null) {
                append('\n')
                append(v.toString())
            }
        }
        resultText?.let {
            append('\n')
            append(it)
        }
    }
