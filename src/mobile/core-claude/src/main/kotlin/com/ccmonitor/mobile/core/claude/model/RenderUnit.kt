package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.wire.gen.Attachment
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.LineDiff
import com.ccmonitor.mobile.core.wire.gen.Pasted

/**
 * 手机这边送出去、记录里还没出现的那一句的投递状态（历史记录恒为 `null`）。
 * 送到了的不挂标；记录里出现了这句人话 ⇒ 整句换成记录里那一条。
 */
enum class DeliveryState {
    /** 正在送（会话那台还没回执）。 */
    SENDING,

    /** 会话那台已回执（写进了那条会话的输入队、落了盘）：排着 · 已送进，字照回执写。 */
    QUEUED,

    /** 那台回 `unsure`（或期限到了）：不知道送没送到，不重发。 */
    UNSURE,

    /** 没送到、原因改得了：内容保留，可再发一次。 */
    FAILED,

    /** 没送到、重试没用（会话已结束 · 不归这里管）：不给重试。 */
    FAILED_PERMANENT,
}

/**
 * 人说的那一句里的一张图：记录里只带引用（类型 · 哪一行 [at] · 这一行里第几张 [n]），字节画到它那一格时按 [record]（那一条的 `id`）
 * 另取（`history-image`）。
 */
data class SaidImage(
    val mediaType: String,
    val at: Long,
    val n: Int,
    val record: String,
)

/** 人说的那一句的一段：照排的字 · 折成一行「粘贴的内容 · N 行」的那一块（[lines] 是那台数好的，[body] 是点开后的正文）。 */
sealed interface SaidPart {
    data class Plain(
        val text: String,
    ) : SaidPart

    data class Folded(
        val lines: Int,
        val body: String,
    ) : SaidPart
}

/**
 * 照记录成品的 `pasted[]`（UTF-16 下标，含两头的标记）把 [text] 切成几段：标记不上屏，只露正文那一截（两头的换行去掉）；
 * 折不折照那一块的 `fold`。折起来的那一段自己占一行 ⇒ 紧挨着它的那一个换行不再另算。相邻照排的几段并成一段。
 */
fun saidParts(
    text: String,
    pasted: List<Pasted>,
): List<SaidPart> {
    val out = ArrayList<SaidPart>()
    val plain = StringBuilder()
    fun flush() {
        if (plain.isNotEmpty()) out += SaidPart.Plain(plain.toString())
        plain.setLength(0)
    }
    var at = 0
    var afterFold = false
    for (p in pasted.sortedBy { it.start }) {
        var before = text.substring(at.coerceAtMost(p.start), p.start)
        if (afterFold) before = before.removePrefix("\r\n").removePrefix("\n")
        if (p.fold) before = before.removeSuffix("\n").removeSuffix("\r")
        plain.append(before)
        val body = text.substring(p.bodyStart, p.bodyEnd).trim('\r', '\n')
        if (p.fold) {
            flush()
            out += SaidPart.Folded(p.lines, body)
        } else {
            plain.append(body)
        }
        afterFold = p.fold
        at = p.end
    }
    val tail = text.substring(at.coerceAtMost(text.length))
    plain.append(if (afterFold) tail.removePrefix("\r\n").removePrefix("\n") else tail)
    flush()
    return out
}

/**
 * 正文的渲染单元，由 [RecordUnits] 从核心的通用记录排出来；UI 把每个 unit 映射成一张卡片。
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
        /** 这一句带的图片（记录里的图片块，自带原图），画在气泡外上方。 */
        val images: List<SaidImage> = emptyList(),
        /** 这一句带的附件（后端认出的 `attachments[]`：类别字 · 名字 · 大小 · 已清理都是那台写好的），画在气泡外上方。 */
        val attachments: List<Attachment> = emptyList(),
        /** 正文分段（记录成品带了人粘贴的块才有：只露正文那一截，那台说要折的折成一段）；空 ⇒ 照 [text] 画。 */
        val parts: List<SaidPart> = emptyList(),
    ) : RenderUnit

    /** 助手正文：后端解析好的块树（`Block.Prose` 的 `doc`），卡上照树排版；`more` 是后端写好的「展开 {大小}」那一格（照放进按钮）。 */
    data class AssistantMarkdown(
        override val key: String,
        val doc: Doc,
        val more: String,
        override val sourceUuid: String?,
    ) : RenderUnit {
        /** 树上的字（搜索 · 跟到底那一格量长短用），取一次留着。 */
        val text: String by lazy(LazyThreadSafetyMode.PUBLICATION) { doc.plainText() }
    }

    /** 助手思考块，默认折叠。 */
    data class Thinking(
        override val key: String,
        val text: String,
        override val sourceUuid: String?,
    ) : RenderUnit

    /**
     * 一次工具调用，已把 tool_use 与其后匹配的 tool_result 合并。
     * [pending] ＝ 还没配上结果（[resultText] 为 null）；此刻是在跑 · 等批准 · 说不清，由会话事实 `pending[]` 说，这里不判。
     */
    data class ToolCall(
        override val key: String,
        val toolUseId: String,
        val name: String,
        val resultText: String?,
        val isError: Boolean,
        val pending: Boolean,
        override val sourceUuid: String?,
        // 改文件的行差（核心算好的：结果那一份，没有就调用那一份）＋ 文件名；手机只照行画。
        val diff: LineDiff? = null,
        val patchFilePath: String? = null,
        // 头那一行的字，核心写好的（`reply.steps` · `said.results`）：这一步的参数（认不出的那一家 ⇒ 核心那一句），结果那一句（还没结果 ⇒ null）。
        val summary: String = "",
        val resultLine: String? = null,
        // 卡型是交互那一种（核心判的 `reply.cards`）：在等人决定，默认展开。
        val interactive: Boolean = false,
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

    /**
     * 派活的那段话（子运行记录里派出它的那一方交给它的活，`speaker.agentTask`）：不是人说的，画成整宽靠左的派活卡，
     * 抬头写谁派的 ＋ [timeText]（核心写好的钟面）；超过四行收起。
     */
    data class Brief(
        override val key: String,
        val text: String,
        val timeText: String?,
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
     * 认不出的那一条（记录 `t: unread`：那一家的这一行后端认不出，或手机这边解不开、核心换成的那一条）：
     * 一行「无法识别的记录 · {type}」，点开是原文摘录 ＋ 复制详情。字 · 钟面 · 摘录都照核心写好的，不按类名分支。
     */
    data class Unread(
        override val key: String,
        val text: String,
        val timeText: String?,
        val excerpt: String,
        override val sourceUuid: String?,
    ) : RenderUnit
}

/** 会话内搜索用的可搜索纯文本（工具调用含名/参数/输出，便于搜命令、文件名、输出）。 */
fun RenderUnit.searchableText(): String =
    when (this) {
        is RenderUnit.UserText -> text
        is RenderUnit.AssistantMarkdown -> text
        is RenderUnit.Thinking -> text
        is RenderUnit.ToolCall -> toolCallSearchable()
        is RenderUnit.SlashCommand -> if (args.isEmpty()) name else "$name $args"
        is RenderUnit.BashInput -> command
        is RenderUnit.BashOutput -> if (stderr.isEmpty()) stdout else "$stdout\n$stderr"
        is RenderUnit.Brief -> text
        is RenderUnit.CompactSummary -> text
        is RenderUnit.Interrupt -> ""
        is RenderUnit.Unread -> text
    }

private fun RenderUnit.ToolCall.toolCallSearchable(): String =
    buildString {
        append(name)
        if (summary.isNotEmpty()) {
            append('\n')
            append(summary)
        }
        resultText?.let {
            append('\n')
            append(it)
        }
    }
