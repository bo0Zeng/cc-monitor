package com.ccmonitor.mobile.core.claude.bridge

/**
 * aterm 帧的 Kotlin 表示，手机侧收到的东西。
 *
 * 字段名以编码器 `bridge/tools/reference_encoder.py` 为准，按它的全量字段面建模。
 * 编码器丢掉值为 `None` 的键，所以录制样本里看不到 `p`（子 agent 归属）、`trunc`、
 * `api`/`den`、`ut` 这些：照样本建模会漏掉它们，而漏 `p` 会让子 agent 的正文插进主对话流。
 */
sealed interface BridgeFrame {
    /** 线上的 `t` 值。反序列化后仍保留，便于日志与未知帧透传。 */
    val t: String

    /** 会话初始化 + catalog（命令/skill/工具/agent/MCP）。「点击式 UI」的地基。 */
    data class Init(
        val sid: String?,
        val model: String?,
        val cwd: String?,
        val cmds: List<String>,
        val skills: List<String>,
        val tools: List<String>,
        val agents: List<String>,
        val mcp: List<McpServer>,
        val plugins: List<String>,
        val raw: Map<String, Any?>?,
    ) : BridgeFrame {
        override val t get() = "init"
    }

    /**
     * MCP 服务器条目，init 五个清单里唯一带结构的。
     *
     * [source]（来源，用于分组）可以整个缺席，缺席不默认成 `"user"`，要有自己的画法。
     * [status] 有五个值（`connected` · `failed` · `needs-auth` · `pending` · `disabled`）。
     * 两格都原样透传，由消费方套白名单，这里不折叠未知值。
     */
    data class McpServer(
        val name: String,
        val status: String?,
        val source: String? = null,
    )

    /** 块开始。`bt` = 块类型（text/thinking/tool_use），`id`/`name` 仅 tool_use 块有。 */
    data class BlockStart(
        val m: String?,
        val i: Int,
        val bt: String?,
        val id: String?,
        val name: String?,
        /**
         * 子 agent（Task）的归属，非空表示这帧是子 agent 产的。
         * 带默认值：帧只按「加字段且有默认」演进，已录样本里它恒为 null。
         */
        val p: String? = null,
    ) : BridgeFrame {
        override val t get() = "bs"
    }

    /** 正文增量。 */
    data class TextDelta(
        val m: String?,
        val i: Int,
        val x: String,
        /**
         * 子 agent（Task）的归属，非空表示这帧是子 agent 产的。
         * 带默认值：帧只按「加字段且有默认」演进，已录样本里它恒为 null。
         */
        val p: String? = null,
    ) : BridgeFrame {
        override val t get() = "d"
    }

    /** 思考增量。 */
    data class ThinkingDelta(
        val m: String?,
        val i: Int,
        val x: String,
        /**
         * 子 agent（Task）的归属，非空表示这帧是子 agent 产的。
         * 带默认值：帧只按「加字段且有默认」演进，已录样本里它恒为 null。
         */
        val p: String? = null,
    ) : BridgeFrame {
        override val t get() = "td"
    }

    /** 工具入参的 JSON 增量（拼起来才是完整 JSON，中途都是残缺的）。 */
    data class InputJsonDelta(
        val m: String?,
        val i: Int,
        val x: String,
        /**
         * 子 agent（Task）的归属，非空表示这帧是子 agent 产的。
         * 带默认值：帧只按「加字段且有默认」演进，已录样本里它恒为 null。
         */
        val p: String? = null,
    ) : BridgeFrame {
        override val t get() = "ij"
    }

    /**
     * 块结束。
     *
     * 注意：不保证到达（中断时块可能永远不收口）。它只是「可以切 Markdown 渲染」的时机提示；
     * 正文以 [AssistantText] 为准，turn 状态以 [Result] 为准。
     */
    data class BlockEnd(
        val m: String?,
        val i: Int,
        /**
         * 子 agent（Task）的归属，非空表示这帧是子 agent 产的。
         * 带默认值：帧只按「加字段且有默认」演进，已录样本里它恒为 null。
         */
        val p: String? = null,
    ) : BridgeFrame {
        override val t get() = "be"
    }

    /** 助手正文全文，无条件覆盖 delta 拼出来的内容。 */
    data class AssistantText(
        val m: String?,
        val i: Int,
        val p: String?,
        val x: String,
    ) : BridgeFrame {
        override val t get() = "at"
    }

    /** 思考全文，同 [AssistantText] 的覆盖语义。 */
    data class ThinkingText(
        val m: String?,
        val i: Int,
        val p: String?,
        val x: String,
    ) : BridgeFrame {
        override val t get() = "tt"
    }

    /** 一次工具调用（含完整入参）。 */
    data class ToolUse(
        val m: String?,
        val i: Int,
        val p: String?,
        val id: String?,
        val name: String?,
        val inp: Map<String, Any?>?,
    ) : BridgeFrame {
        override val t get() = "tu"
    }

    /** 工具结果。`ok` 与 [Result.ok] 同名同极性。 */
    data class ToolResult(
        val id: String?,
        val p: String?,
        val ok: Boolean,
        val x: String,
        val truncatedFullLength: Int?,
    ) : BridgeFrame {
        override val t get() = "tr"
    }

    /** 用户消息是纯字符串时的帧（`UserMessage.content` 的 SDK 类型是 `str | list`）。 */
    data class UserText(
        val p: String?,
        val x: String,
    ) : BridgeFrame {
        override val t get() = "ut"
    }

    /** turn 收口。它到了这一轮就结束了，不管有没有开着的块。 */
    data class Result(
        val sid: String?,
        val ok: Boolean,
        val cost: Double?,
        val turns: Int?,
        val why: String?,
        val api: String?,
        val dur: Long?,
        val usage: Map<String, Any?>?,
        val den: List<Any?>?,
        val mu: Map<String, Any?>?,
        val raw: Map<String, Any?>?,
    ) : BridgeFrame {
        override val t get() = "res"
    }

    /** 限流事件。正常路径上也会来，不只是被限流时。 */
    data class RateLimit(
        val raw: Map<String, Any?>?,
    ) : BridgeFrame {
        override val t get() = "rl"
    }

    /** 错误。认证失败在这里暴露（而 `res.subtype` 那时仍是 `success`）。 */
    data class Err(
        val m: String?,
        val code: String?,
    ) : BridgeFrame {
        override val t get() = "err"
    }

    /** 已知的透传帧：编码器认得这是什么，但没有专门的帧型。 */
    data class Event(
        val k: String?,
        val i: Int?,
        val raw: Map<String, Any?>?,
    ) : BridgeFrame {
        override val t get() = "ev"
    }

    /**
     * 解码器不认识的帧型，不丢弃。
     *
     * 与 [Event] 不同：`ev` 是编码器主动打的透传包，`Unknown` 是远端比手机新。
     * 分开才能在手机上看出「远端升级了协议」。
     */
    data class Unknown(
        override val t: String,
        val raw: Map<String, Any?>,
    ) : BridgeFrame
}
