package com.ccmonitor.mobile.core.claude.model

import com.ccmonitor.mobile.core.wire.gen.Attachment
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.LineDiff
import com.ccmonitor.mobile.core.wire.gen.NoticeRun
import com.ccmonitor.mobile.core.wire.gen.RetryOutcome
import com.ccmonitor.mobile.core.wire.gen.RetryRun
import com.ccmonitor.mobile.core.wire.gen.TextFold
import com.ccmonitor.mobile.core.wire.gen.SaidPart as WireSaidPart

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

/**
 * 人说的那一句的一段（照记录成品的 `who.parts[]`，那台切好的，这里只换个画法用的名字）：
 * 照排的字 · 折成一行的那一块（[text] 是收着时那一行，那台写好的 `fold.more`；[body] 是点开后的正文）。
 */
sealed interface SaidPart {
    data class Plain(
        val text: String,
    ) : SaidPart

    data class Folded(
        val text: String,
        val body: String,
    ) : SaidPart

    companion object {
        /** 记录里那一段 ⇒ 画法：带折法 ⇒ 折成一行，否则照排。 */
        fun of(p: WireSaidPart): SaidPart = p.fold?.let { Folded(it.more, p.text) } ?: Plain(p.text)
    }
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

    /** 有一段正文的那几种（搜索照 [text] 搜）。 */
    sealed interface Worded : RenderUnit {
        val text: String
    }

    /** 用户输入的真实 prompt（非 meta、非 tool_result）。 */
    data class UserText(
        override val key: String,
        override val text: String,
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
    ) : Worded

    /** 助手正文：后端解析好的块树（`Block.Prose` 的 `doc`），卡上照树排版；`more` 是后端写好的「展开 {大小}」那一格（照放进按钮）。 */
    data class AssistantMarkdown(
        override val key: String,
        val doc: Doc,
        val more: String,
        override val sourceUuid: String?,
    ) : Worded {
        /** 树上的字（搜索 · 跟到底那一格量长短用），取一次留着。 */
        override val text: String by lazy(LazyThreadSafetyMode.PUBLICATION) { doc.plainText() }
    }

    /** 助手思考块，默认折叠。 */
    data class Thinking(
        override val key: String,
        override val text: String,
        override val sourceUuid: String?,
    ) : Worded

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

    /** `!bash` 输出（stdout/stderr 任一可空）；各自折不折、露几行、展开那一句照那台写好的 [stdoutFold] · [stderrFold]。 */
    data class BashOutput(
        override val key: String,
        val stdout: String,
        val stderr: String,
        override val sourceUuid: String?,
        val stdoutFold: TextFold? = null,
        val stderrFold: TextFold? = null,
    ) : RenderUnit

    /**
     * 派活的那段话（子运行记录里派出它的那一方交给它的活，`speaker.agentTask`）：不是人说的，画成整宽靠左的派活卡，
     * 抬头写谁派的 ＋ [timeText]（核心写好的钟面）；太长 ⇒ 照那台写好的 [fold] 先露几行。
     */
    data class Brief(
        override val key: String,
        override val text: String,
        val timeText: String?,
        override val sourceUuid: String?,
        val fold: TextFold? = null,
    ) : Worded

    /** `/compact` 续接摘要，默认折叠（超长，点开看全文）。 */
    data class CompactSummary(
        override val key: String,
        override val text: String,
        override val sourceUuid: String?,
    ) : Worded

    /**
     * 另一个会话发来的话（`speaker.peerSession`）：来话条，抬头是核心写好的那一句（[title]，「会话「X」」）＋ [timeText]，
     * 正文是核心解析好的块树 [doc]（没有正文 ⇒ `null`，只画抬头）。
     */
    data class Peer(
        override val key: String,
        val title: String?,
        val timeText: String?,
        val doc: Doc?,
        override val sourceUuid: String?,
    ) : Worded {
        /** 树上的字（搜索用），取一次留着。 */
        override val text: String by lazy(LazyThreadSafetyMode.PUBLICATION) { doc?.plainText().orEmpty() }
    }

    /** 后台任务的收场通知（`speaker.taskNotification`）：一行，字是核心写好的那一行（`row`）；[failed] ＝ 那台说它失败了。 */
    data class Notice(
        override val key: String,
        override val text: String,
        val failed: Boolean,
        override val sourceUuid: String?,
    ) : Worded

    /**
     * 一串重试（记录 `t: retry`；接在后面的那几条那台出 `draw: none`，只首条成单元）：[key] 是首条的 id（会话事实 `retries[]` 按它给那一串），
     * [text] 是首条写好的那一行（`retry.text`）；事实到了照那一串画（[RetryLook.strip]）。
     */
    data class Retry(
        override val key: String,
        override val text: String,
        override val sourceUuid: String?,
    ) : Worded

    /**
     * 上游最终失败写的那一条回复（`reply.error`）：报错卡。卡头 [title] 是核心写好的（`error.title`），[raw] 是报错原文（进「原文」折叠）；
     * 下一步那一句：紧跟在一串没接上的重试后面 ⇒ 那一串的整句（[RetryLook.next]），否则不画那一行（不补通用那一句）。
     */
    data class ApiError(
        override val key: String,
        val title: String,
        val timeText: String?,
        val raw: String,
        override val sourceUuid: String?,
    ) : Worded {
        override val text: String get() = "$title\n$raw"
    }

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
        override val text: String,
        val timeText: String?,
        val excerpt: String,
        override val sourceUuid: String?,
    ) : Worded
}

/** 重试细条此刻的字与淡不淡（接上了 · 人打断了 ⇒ 淡）。 */
data class RetryStrip(
    val text: String,
    val faded: Boolean,
)

/**
 * 重试细条与报错卡照会话事实 `retries[]`（首条 id ⇒ 结局与写好的整句）摆：那一串在事实里 ⇒ 照它的整句；不在 ⇒ 记录那一行、在重试。
 * 没接上 ⇒ 细条收起、那一串的整句进紧跟着的报错卡。原因 · 次数 · 时刻全是核心拼好的，这里只挑哪一句放哪。
 */
object RetryLook {
    /** 细条画什么；收起 ⇒ `null`。 */
    fun strip(
        u: RenderUnit.Retry,
        runs: List<RetryRun>,
    ): RetryStrip? {
        val run = runs.firstOrNull { it.id == u.key } ?: return RetryStrip(u.text, faded = false)
        return when (run.outcome) {
            RetryOutcome.FAILED -> null
            RetryOutcome.RETRYING -> RetryStrip(run.text, faded = false)
            RetryOutcome.RECOVERED, RetryOutcome.INTERRUPTED -> RetryStrip(run.text, faded = true)
        }
    }

    /** 报错卡 [error]（它的 key）下一步那一句：紧挨着它的上一张是一串没接上的重试 ⇒ 那一串的整句；否则 `null`（不画那一行，不补通用那一句）。 */
    fun next(
        units: List<RenderUnit>,
        error: String,
        runs: List<RetryRun>,
    ): String? {
        val before = units.getOrNull(units.indexOfFirst { it.key == error } - 1) as? RenderUnit.Retry ?: return null
        return runs.firstOrNull { it.id == before.key }?.takeIf { it.outcome == RetryOutcome.FAILED }?.text
    }
}

/**
 * 一张收场通知卡此刻画什么：卡头 [head]（那一组写好的组头；没有 ⇒ 露着的那一行）· 展开里露着的几行 [rows] · [failed]；
 * [single] ⇒ 卡头就是那一行、不展开。
 */
data class NoticeCard(
    val head: String,
    val rows: List<RenderUnit.Notice>,
    val failed: Boolean,
    val single: Boolean,
)

/**
 * 收场通知照会话事实 `notices[]`（后端分好的组）摆：每组首条那一张当这一组，别的成员那几行收进它的展开、它们自己那一张不画；
 * `hidden` 里的不露，整组都不露 ⇒ 不画；不在哪一组里 ⇒ 它自己那一行。组头 · 失败都是核心写好的，这里只挑哪一句放哪。
 */
object NoticeLook {
    /** 通知 [u] 那一张画什么；不画 ⇒ `null`。 */
    fun card(
        u: RenderUnit.Notice,
        units: List<RenderUnit>,
        runs: List<NoticeRun>,
    ): NoticeCard? {
        val run = runs.firstOrNull { u.key in it.members } ?: return NoticeCard(u.text, listOf(u), u.failed, single = true)
        if (run.id != u.key) return null
        val rows = run.members.filter { it !in run.hidden }.mapNotNull { id -> units.firstOrNull { it.key == id } as? RenderUnit.Notice }
        val first = rows.firstOrNull() ?: return null
        return NoticeCard(run.head ?: first.text, rows, run.failed, single = run.head == null)
    }
}

/** 会话内搜索用的可搜索纯文本（工具调用含名/参数/输出，便于搜命令、文件名、输出）。 */
fun RenderUnit.searchableText(): String =
    when (this) {
        is RenderUnit.Worded -> text
        is RenderUnit.ToolCall -> toolCallSearchable()
        is RenderUnit.SlashCommand -> if (args.isEmpty()) name else "$name $args"
        is RenderUnit.BashInput -> command
        is RenderUnit.BashOutput -> if (stderr.isEmpty()) stdout else "$stdout\n$stderr"
        is RenderUnit.Interrupt -> ""
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
