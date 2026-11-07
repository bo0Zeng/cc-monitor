package com.ccmonitor.mobile.ui.claude

import android.graphics.BitmapFactory
import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.Image
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.itemsIndexed
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.ImageBitmap
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.asImageBitmap
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.layout.ContentScale
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.NoticeCard
import com.ccmonitor.mobile.core.claude.model.NoticeLook
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.model.RetryLook
import com.ccmonitor.mobile.core.claude.model.RetryStrip
import com.ccmonitor.mobile.core.claude.model.SaidImage
import com.ccmonitor.mobile.core.claude.model.SaidPart
import com.ccmonitor.mobile.core.claude.model.StreamItem
import com.ccmonitor.mobile.core.claude.model.codeBlocks
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoBody
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.core.wire.gen.PendingCall
import com.ccmonitor.mobile.core.wire.gen.StepWait
import com.ccmonitor.mobile.ui.common.CopyDetailButton

/**
 * 会话只读阅读面：把流里的那几项（卡 ‖ 一段过程的那一行，[StreamItem]）渲染成卡片流，嵌在会话屏里复用。
 * LazyColumn key 用 [StreamItem.key]（卡是分类器算的 uuid#blockIndex，身份稳定），实时追加不错位。
 */
@Composable
fun ClaudeReadingPane(
    items: List<StreamItem>,
    modifier: Modifier = Modifier,
    listState: LazyListState = rememberLazyListState(),
    matchedKeys: Set<String> = emptySet(), // 搜索命中的 unit.key：高亮，并展开命中的折叠卡
    onCopyCode: (String) -> Unit = {}, // 复制代码块
    // 正文尾巴上不进时间线的那一块（聊天屏的活卡）。
    trailer: LazyListScope.() -> Unit = {},
    // 一次工具调用（tool_use id）此刻在等人 ⇒ 卡上那枚标的字（那台写好的）；不在等 ⇒ null。
    stepBadge: (String) -> String? = { null },
    // 一次还没配上结果的工具调用此刻是什么样子（会话事实 `pending[]` 那一项）；事实里没有它 ⇒ null、不画状态。
    stepWait: (String) -> PendingCall? = { null },
    // 一条重试细条此刻画什么（照会话事实那一串）；收起 ⇒ null。没有事实 ⇒ 记录那一行。
    retry: (RenderUnit.Retry) -> RetryStrip? = { RetryLook.strip(it, emptyList()) },
    // 报错卡（key）下一步那一句：紧跟在一串没接上的重试后面 ⇒ 那一串的整句；否则 null（不画那一行）。
    errorNext: (String) -> String? = { null },
    // 一张收场通知卡此刻画什么（照会话事实那一组）；并进别的组 · 整组不露 ⇒ null。没有事实 ⇒ 它自己那一行。
    notice: (RenderUnit.Notice) -> NoticeCard? = { NoticeLook.card(it, emptyList(), emptyList()) },
    // 一段过程的那一行（聊天屏画它）。
    process: @Composable (StreamItem.Process) -> Unit = {},
    // 点一个回退框（那一段的段首条 id）：开 ⇄ 合。
    branch: (String) -> Unit = {},
    // 人说的那一句的长按与就地编辑（聊天屏给）；只读阅读面不给 ⇒ 不出。
    said: SaidHooks? = null,
) {
    val tokens = LocalAppTokens.current
    LazyColumn(
        state = listState,
        modifier = modifier.fillMaxWidth(),
        contentPadding = PaddingValues(tokens.spacing.md),
        verticalArrangement = Arrangement.spacedBy(tokens.spacing.md),
    ) {
        // key 用 RenderUnit.key（uuid#blockIndex，身份稳定），流式追加不漂移。
        // `contentType` 要给：卡片有好几种，不给的话 Compose 会试图把一种卡的组合复用到另一种上。
        // 在就地编辑的那一句排第几：它后面的都淡掉（示意从这里往后会退掉）。
        val editAt = said?.editing?.let { k -> items.indexOfFirst { (it as? StreamItem.Card)?.unit?.key == k } } ?: -1
        itemsIndexed(items, key = { _, s -> s.key }, contentType = { _, s -> (s as? StreamItem.Card)?.unit?.let { u -> u::class } ?: s::class }) { i, item ->
            val fade = if (editAt in 0 until i) Modifier.alpha(EDIT_FADE) else Modifier
            when (item) {
                is StreamItem.Process -> Box(fade) { process(item) }
                is StreamItem.Branch -> Box(fade) { BranchHead(item) { branch(item.run) } }
                // 点开着的那一段里的卡：左边一道 2dp 竖线标出「这是过程」；回退框里的：淡显 ＋ 左边一道虚线。
                is StreamItem.Card ->
                    Box((if (item.fold != null) Modifier.foldRail() else Modifier).then(if (item.branch != null) Modifier.branchRail() else Modifier).then(fade)) {
                        UnitCard(item.unit, item.unit.key in matchedKeys, onCopyCode, Steps(stepBadge, stepWait, retry, errorNext, notice), said)
                    }
            }
        }
        trailer()
    }
}

/** 过程里那几张卡左边那一道竖线。 */
@Composable
private fun Modifier.foldRail(): Modifier {
    val tokens = LocalAppTokens.current
    val rail = tokens.borderStrong
    return this
        .drawBehind { drawRect(rail, size = Size(2.dp.toPx(), size.height)) }
        .padding(start = tokens.spacing.sm)
}

/** 回退框的框头：一行虚线框，箭头 ＋ 那台写好的那一句（`runs[].said`）；整行是开合区。 */
@Composable
private fun BranchHead(
    item: StreamItem.Branch,
    onToggle: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val line = tokens.borderStrong
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = BRANCH_HEAD_MIN)
            .clip(RoundedCornerShape(tokens.radius.m))
            .drawBehind {
                drawRoundRect(line, style = Stroke(1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 4f))), cornerRadius = CornerRadius(tokens.radius.m.toPx()))
            }.clickable(onClick = onToggle)
            .padding(horizontal = tokens.spacing.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PhIcon(if (item.open) Ph.CaretDown else Ph.CaretRight, contentDescription = null, size = Touch.iconInline, tint = tokens.textFaint)
        Spacer(Modifier.width(tokens.spacing.sm))
        Text(item.said, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** 点开着的回退框里那几张卡：整体淡显、左边一道虚线。 */
@Composable
private fun Modifier.branchRail(): Modifier {
    val tokens = LocalAppTokens.current
    val rail = tokens.borderStrong
    return this
        .alpha(BRANCH_FADE)
        .drawBehind {
            val w = 2.dp.toPx()
            drawLine(rail, Offset(w / 2, 0f), Offset(w / 2, size.height), strokeWidth = w, pathEffect = PathEffect.dashPathEffect(floatArrayOf(6f, 4f)))
        }.padding(start = tokens.spacing.md)
}

/** 回退框框头至少这么高（一行可点的高度）· 框里那几张卡淡到多少（看得出已作废、字还读得清）。 */
private val BRANCH_HEAD_MIN = 40.dp
private const val BRANCH_FADE = 0.6f

/** 人说的那一句的长按与就地编辑（聊天屏给）：[editing] 是正在编辑的那张卡的 key；长按交 [onLongPress]；编辑中那张卡画 [editor]。 */
class SaidHooks(
    val editing: String?,
    val onLongPress: (RenderUnit.UserText) -> Unit,
    val editor: @Composable (RenderUnit.UserText) -> Unit,
)

/** 就地编辑时它后面那几项淡到多少。 */
private const val EDIT_FADE = 0.35f

/** 卡上照会话事实摆的那几样：工具卡在等人那一枚的字 · 没配上结果时那一态（都按 tool_use id）· 重试细条 · 报错卡下一步那一句 · 通知组。 */
private class Steps(
    val badge: (String) -> String?,
    val wait: (String) -> PendingCall?,
    val retry: (RenderUnit.Retry) -> RetryStrip?,
    val errorNext: (String) -> String?,
    val notice: (RenderUnit.Notice) -> NoticeCard?,
)

/** 一张卡（照它今天的样子）。 */
@Composable
private fun UnitCard(
    unit: RenderUnit,
    matched: Boolean,
    onCopyCode: (String) -> Unit,
    steps: Steps,
    said: SaidHooks?,
) {
    when (unit) {
        is RenderUnit.UserText -> UserTextCard(unit, matched, said)
        is RenderUnit.AssistantMarkdown -> AssistantMarkdownCard(unit, matched, onCopyCode)
        is RenderUnit.Thinking -> ThinkingCard(unit, matched)
        is RenderUnit.ToolCall -> ToolCallCard(unit, matched, steps.badge(unit.toolUseId), steps.wait(unit.toolUseId).takeIf { unit.pending })
        // 斜杠命令、bash 输入输出、compact 摘要各有自己的卡片。
        is RenderUnit.SlashCommand -> SlashCommandCard(unit, matched)
        is RenderUnit.BashInput -> BashInputCard(unit, matched)
        is RenderUnit.BashOutput -> BashOutputCard(unit, matched)
        is RenderUnit.Brief -> BriefCard(unit, matched)
        is RenderUnit.CompactSummary -> CompactSummaryCard(unit, matched)
        is RenderUnit.Peer -> PeerBar(unit, matched)
        is RenderUnit.Notice, is RenderUnit.Interrupt, is RenderUnit.Unread, is RenderUnit.Retry, is RenderUnit.ApiError -> TimelineLine(unit, matched, steps)
    }
}

/** 时间线上的那几种细条与报错卡：后台任务收场 · 中断 · 认不出的那一条 · 重试 · 上游失败。 */
@Composable
private fun TimelineLine(
    unit: RenderUnit,
    matched: Boolean,
    steps: Steps,
) {
    when (unit) {
        is RenderUnit.Notice -> steps.notice(unit)?.let { NoticeLine(it, matched) }
        is RenderUnit.Interrupt -> InterruptLine(unit)
        is RenderUnit.Unread -> UnreadLine(unit, matched)
        is RenderUnit.Retry -> steps.retry(unit)?.let { RetryLine(it, matched) }
        is RenderUnit.ApiError -> ApiErrorCard(unit, steps.errorNext(unit.key), matched)
        else -> Unit
    }
}

/** 重试细条：左 2dp 琥珀、一行小字（那台写好的）；接上了 · 人打断了 ⇒ 竖线与字都淡下去。 */
@Composable
private fun RetryLine(
    strip: RetryStrip,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    val bar = if (strip.faded) tokens.borderStrong else tokens.warn
    Text(
        strip.text,
        Modifier
            .fillMaxWidth()
            .matchHighlight(matched)
            .drawBehind { drawRect(bar, size = Size(2.dp.toPx(), size.height)) }
            .padding(start = tokens.spacing.sm, top = tokens.spacing.xs, bottom = tokens.spacing.xs),
        style = monoSmall,
        color = if (strip.faded) tokens.textFaint else MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/**
 * 报错卡（上游最终失败）：左 2dp 红；抬头是那台写好的卡头 ＋ 钟面；下一步那一句（[next]：紧跟着的那一串重试没接上 ⇒ 那一串的整句，
 * 会话事实里没有 ⇒ 不画那一行）；「原文」点开是报错原文 ＋［复制详情］。
 */
@Composable
private fun ApiErrorCard(
    unit: RenderUnit.ApiError,
    next: String?,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    var open by rememberExpandable(matched)
    val bar = tokens.errorText
    Column(
        Modifier
            .fillMaxWidth()
            .matchHighlight(matched)
            .drawBehind { drawRect(bar, size = Size(2.dp.toPx(), size.height)) }
            .padding(start = tokens.spacing.sm),
        verticalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
            Text(unit.title, Modifier.weight(1f, fill = false), style = MaterialTheme.typography.labelLarge, color = tokens.errorText)
            unit.timeText?.let { Text(it, style = monoSmall, color = tokens.textFaint) }
        }
        next?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        Row(Modifier.clickable { open = !open }.padding(vertical = tokens.spacing.xs), verticalAlignment = Alignment.CenterVertically) {
            Caret(open)
            Text(copyText("apiError.card.raw"), style = monoSmall, color = tokens.textFaint)
        }
        AnimatedVisibility(open) {
            Column(Modifier.padding(bottom = tokens.spacing.xs)) {
                Text(unit.raw.ifEmpty { copyText("apiError.card.noDetail") }, style = monoSmall, color = MaterialTheme.colorScheme.onSurface)
                CopyDetailButton(unit.title, unit.raw)
            }
        }
    }
}

/** 派活卡抬头里写谁派的：派出它的那个 agent 的名字（agent 屏给）；主会话派的 ⇒ `null`。 */
val LocalBriefFrom = staticCompositionLocalOf<String?> { null }

/**
 * 派活的那段话（同桌面那张卡）：整宽、靠左、左边一道竖线、底色深一档；抬头「主会话派给它的话」（别的 agent 派的 ⇒ 它的名字）＋ 派出的时刻；
 * 正文小一号、次一级颜色，超过 [BRIEF_LINES] 行收起，［展开全部］·［收起］。搜索命中自动展开。
 */
@Composable
private fun BriefCard(
    unit: RenderUnit.Brief,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    val from = LocalBriefFrom.current
    var open by rememberExpandable(matched)
    val long = unit.text.lines().size > BRIEF_LINES
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(tokens.radius.m))
            .background(tokens.bg2)
            .matchHighlight(matched)
            .foldRail()
            .padding(tokens.spacing.sm),
        verticalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
            // 别的 agent 派的 ⇒ 抬头写派它的那个 agent 的名字（「「x」派给它的话」那一句要那台写好，界面不拼）。
            Text(
                from ?: copyText("agentWindow.brief.fromMain"),
                style = MaterialTheme.typography.labelMedium,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            unit.timeText?.let { Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        Text(
            unit.text,
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            maxLines = if (long && !open) BRIEF_LINES else Int.MAX_VALUE,
            overflow = TextOverflow.Ellipsis,
        )
        if (long) {
            SecondaryButton(onClick = { open = !open }) {
                Text(if (open) copyText("kit.dock.close") else copyText("agentWindow.brief.more"))
            }
        }
    }
}

/** 派活卡正文超过几行收起（同桌面）。 */
private const val BRIEF_LINES = 4

/**
 * 另一个会话发来的话：左 2dp 琥珀条，抬头是那台写好的那一句（`title`，「会话「X」」，同桌面那一条；没有 ⇒ 不写）＋「消息」＋ 钟面，
 * 下面是那台解析好的正文（照树排版）。
 */
@Composable
private fun PeerBar(
    unit: RenderUnit.Peer,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    val bar = tokens.warn
    Column(
        Modifier
            .fillMaxWidth()
            .matchHighlight(matched)
            .drawBehind { drawRect(bar, size = Size(2.dp.toPx(), size.height)) }
            .padding(start = tokens.spacing.sm),
        verticalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm)) {
            unit.title?.let { Text(it, Modifier.weight(1f, fill = false), style = MaterialTheme.typography.labelMedium, maxLines = 1, overflow = TextOverflow.Ellipsis) }
            Text(copyText("kit.toast.region"), style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            unit.timeText?.let { Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        }
        unit.doc?.let { MarkdownText(it, "", modifier = Modifier.fillMaxWidth()) }
    }
}

/**
 * 后台任务收场那一张：一行，字是核心写好的（一组 ⇒ 组头，点开是组里露着的那几行，同桌面那一张）；失败 ⇒ 红字。
 * 搜索命中自动展开。
 */
@Composable
private fun NoticeLine(
    card: NoticeCard,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    val plain = MaterialTheme.colorScheme.onSurfaceVariant
    val tone = { failed: Boolean -> if (failed) tokens.errorText else plain }
    if (card.single) {
        Text(card.head, Modifier.fillMaxWidth().matchHighlight(matched), style = monoSmall, color = tone(card.failed))
        return
    }
    var open by rememberExpandable(matched)
    Column(Modifier.fillMaxWidth().matchHighlight(matched)) {
        Row(Modifier.fillMaxWidth().clickable { open = !open }, verticalAlignment = Alignment.CenterVertically) {
            Caret(open)
            Text(card.head, style = monoSmall, color = tone(card.failed))
        }
        AnimatedVisibility(open) {
            Column(Modifier.padding(start = tokens.spacing.md)) {
                card.rows.forEach { Text(it.text, style = monoSmall, color = tone(it.failed)) }
            }
        }
    }
}

/** 「已中断本轮 · {time}」一道细线；字照文案表，钟面照核心。 */
@Composable
private fun InterruptLine(unit: RenderUnit.Interrupt) {
    Text(
        copyText("speaker.interrupt.line", "time" to unit.timeText.orEmpty()),
        style = monoSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

/**
 * 认不出的那一条：左 2dp 琥珀条的一行（字 ＋ 钟面），默认折起；点开是原文摘录 ＋［复制详情］（同桌面那一张）。
 * 字 · 钟面 · 摘录都是核心写好的，这里不按类名分支。搜索命中自动展开。
 */
@Composable
private fun UnreadLine(
    unit: RenderUnit.Unread,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    var expanded by rememberExpandable(matched)
    val bar = tokens.warn
    Column(
        Modifier
            .fillMaxWidth()
            .matchHighlight(matched)
            .drawBehind { drawRect(bar, size = Size(2.dp.toPx(), size.height)) }
            .padding(start = tokens.spacing.sm),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .clickable { expanded = !expanded }
                .padding(vertical = tokens.spacing.xs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(
                unit.text,
                style = monoSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
            unit.timeText?.let {
                Spacer(Modifier.width(tokens.spacing.sm))
                Text(it, style = monoSmall, color = tokens.textFaint)
            }
            Spacer(Modifier.weight(1f))
            PhIcon(if (expanded) Ph.CaretDown else Ph.CaretRight, contentDescription = null, size = Touch.iconInline, tint = tokens.textFaint)
        }
        AnimatedVisibility(expanded) {
            Column(Modifier.padding(bottom = tokens.spacing.xs)) {
                Text(unit.excerpt, style = monoSmall, color = tokens.textFaint)
                CopyDetailButton(unit.text, unit.excerpt)
            }
        }
    }
}

/** 斜杠命令 `/name args`：紧凑单行（name 强调、args 次要），与真实 prompt 区分开的低调标识。 */
@Composable
private fun SlashCommandCard(
    unit: RenderUnit.SlashCommand,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(tokens.radius.l))
            .background(tokens.bg2)
            .matchHighlight(matched)
            .padding(horizontal = tokens.spacing.md, vertical = tokens.spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text("/", style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.width(tokens.spacing.sm))
        Text(unit.name, style = monoBody.copy(fontWeight = FontWeight.SemiBold), color = MaterialTheme.colorScheme.onSurface)
        if (unit.args.isNotEmpty()) {
            Spacer(Modifier.width(tokens.spacing.sm))
            Text(
                unit.args,
                style = monoSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = Modifier.weight(1f, fill = false),
            )
        }
    }
}

/** `!bash` 输入 `❯ command`：终端风等宽。 */
@Composable
private fun BashInputCard(
    unit: RenderUnit.BashInput,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    Row(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(tokens.radius.l))
            .background(tokens.bg2)
            .matchHighlight(matched)
            .padding(tokens.spacing.md),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        PhIcon(Ph.CaretRight, contentDescription = null, size = Touch.iconInline, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Spacer(Modifier.width(tokens.spacing.sm))
        Text(unit.command, style = monoBody, color = MaterialTheme.colorScheme.onSurface)
    }
}

/** `!bash` 输出：stdout 等宽、stderr 红标；超过阈值折叠（默认展开前 N 行加展开按钮）。搜索命中自动展开。 */
@Composable
private fun BashOutputCard(
    unit: RenderUnit.BashOutput,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    var expanded by rememberExpandable(matched)
    val outLines = remember(unit) { unit.stdout.count { it == '\n' } + 1 }
    val collapsible = outLines > BASH_OUTPUT_COLLAPSE_LINES
    val shownStdout =
        if (collapsible && !expanded) {
            unit.stdout
                .lineSequence()
                .take(BASH_OUTPUT_HEAD_LINES)
                .joinToString("\n")
        } else {
            unit.stdout
        }
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(tokens.radius.l))
            .background(tokens.bg2)
            .matchHighlight(matched)
            .padding(tokens.spacing.md),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            PhIcon(Ph.TerminalWindow, contentDescription = null, size = Touch.iconInline, tint = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.width(tokens.spacing.xs))
            Text(copyText("bash.output.title"), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (unit.stdout.isNotEmpty()) {
            Spacer(Modifier.padding(top = tokens.spacing.xs))
            Text(shownStdout, style = monoBody, color = MaterialTheme.colorScheme.onSurface)
        }
        if (collapsible) {
            Row(
                Modifier.clickable { expanded = !expanded }.padding(top = tokens.spacing.xs),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Caret(expanded)
                Spacer(Modifier.width(tokens.spacing.xs))
                Text(
                    if (expanded) copyText("kit.dock.close") else copyText("bash.output.showAll", "n" to outLines),
                    style = monoSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
        }
        if (unit.stderr.isNotEmpty()) {
            Spacer(Modifier.padding(top = tokens.spacing.xs))
            Text(unit.stderr, style = monoBody, color = tokens.errorText)
        }
    }
}

/** `/compact` 续接摘要：默认折叠为 `上下文摘要 · N 字`，点开看全文。搜索命中自动展开。 */
@Composable
private fun CompactSummaryCard(
    unit: RenderUnit.CompactSummary,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    var expanded by rememberExpandable(matched)
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(tokens.radius.l))
            .background(tokens.card)
            .matchHighlight(matched),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .clickable { expanded = !expanded }
                .padding(tokens.spacing.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(copyText("mobReading.compact.title"), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.width(tokens.spacing.sm))
            Text(copyText("mobReading.compact.size", "n" to unit.text.length), style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            Spacer(Modifier.weight(1f))
            Caret(expanded)
        }
        AnimatedVisibility(expanded) {
            Text(
                unit.text,
                style = monoBody,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = tokens.spacing.md, end = tokens.spacing.md, bottom = tokens.spacing.md),
            )
        }
    }
}

private const val BASH_OUTPUT_COLLAPSE_LINES = 30
private const val BASH_OUTPUT_HEAD_LINES = 20

/**
 * 折叠卡的展开状态：`rememberSaveable` 持久，搜索命中（[matched]）自动展开。
 * 所有折叠卡都走这里，免得某张卡漏写自动展开、行为不一致。[initial] 是初始是否展开（交互工具默认展开）。
 */
@Composable
private fun rememberExpandable(
    matched: Boolean,
    initial: Boolean = false,
): androidx.compose.runtime.MutableState<Boolean> {
    val state = rememberSaveable { mutableStateOf(initial) }
    LaunchedEffect(matched) { if (matched) state.value = true }
    return state
}

/** 搜索命中卡片的高亮边框。 */
@Composable
private fun Modifier.matchHighlight(matched: Boolean): Modifier =
    if (matched) border(2.dp, LocalAppTokens.current.accent, RoundedCornerShape(LocalAppTokens.current.radius.xl)) else this

/** 展开 / 收起的那个尖：Phosphor caret-right / caret-down，行内 16 dp、`--text-2`。 */
@Composable
private fun Caret(expanded: Boolean) {
    PhIcon(
        if (expanded) Ph.CaretDown else Ph.CaretRight,
        contentDescription = null,
        size = Touch.iconInline,
        tint = MaterialTheme.colorScheme.onSurfaceVariant,
    )
}

@OptIn(ExperimentalFoundationApi::class) // combinedClickable：长按自己那句 ⇒ 编辑 · 复制 / 撤回
@Composable
private fun UserTextCard(
    unit: RenderUnit.UserText,
    matched: Boolean,
    said: SaidHooks? = null,
) {
    val tokens = LocalAppTokens.current
    val editing = said != null && said.editing == unit.key
    // 带的图片与附件画在气泡外、上方、靠右（同桌面）；没有字也不在送 ⇒ 只画附件，不画空气泡。
    val bubble = editing || unit.text.isNotEmpty() || unit.delivery != null
    Column(Modifier.fillMaxWidth(), horizontalAlignment = Alignment.End, verticalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
        if (unit.images.isNotEmpty() || unit.attachments.isNotEmpty()) SaidAttachments(unit)
        if (bubble) UserBubble(unit, matched, said)
    }
}

/**
 * 人说的那一句里的图怎么取字节（聊天屏给，见 `ChatSession.image`）：记录里只带引用，那一格组合起来才问 —— LazyColumn 只组合看得见的那几项，
 * 就是滚进视口再取。没给 ⇒ 只画占位。
 */
val LocalSaidImages = staticCompositionLocalOf<suspend (SaidImage) -> ByteArray?> { { null } }

/** 人说的那一句带的图片（小图一排）与附件（chip：类别字 · 名字 · 大小；那台已清理 ⇒ 淡着写「已清理」）。 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
private fun SaidAttachments(unit: RenderUnit.UserText) {
    val tokens = LocalAppTokens.current
    val gap = Arrangement.spacedBy(tokens.spacing.xs, Alignment.End)
    val load = LocalSaidImages.current
    if (unit.images.isNotEmpty()) {
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = gap, verticalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
            for (img in unit.images) {
                val bitmap by produceState<ImageBitmap?>(null, img) {
                    value = load(img)?.let { BitmapFactory.decodeByteArray(it, 0, it.size)?.asImageBitmap() }
                }
                val frame = Modifier.size(width = 160.dp, height = 110.dp).clip(RoundedCornerShape(tokens.radius.l))
                // 取回之前 · 取不到：定尺寸的占位（不跳版）。
                bitmap?.let {
                    Image(it, contentDescription = copyText("attach.msg.image"), modifier = frame, contentScale = ContentScale.Crop)
                } ?: Box(frame.background(MaterialTheme.colorScheme.surfaceVariant))
            }
        }
    }
    if (unit.attachments.isNotEmpty()) {
        FlowRow(Modifier.fillMaxWidth(), horizontalArrangement = gap, verticalArrangement = Arrangement.spacedBy(tokens.spacing.xs)) {
            for (a in unit.attachments) {
                val gone = a.gone == true
                Row(
                    Modifier
                        .clip(RoundedCornerShape(tokens.radius.l))
                        .border(1.dp, if (gone) tokens.borderFaint else tokens.borderStrong, RoundedCornerShape(tokens.radius.l))
                        .background(tokens.bg2)
                        .padding(horizontal = tokens.spacing.xs, vertical = 2.dp),
                    verticalAlignment = Alignment.CenterVertically,
                    horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
                ) {
                    Text(a.classText, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    Text(
                        a.name,
                        style = MaterialTheme.typography.bodySmall,
                        color = if (gone) tokens.textFaint else MaterialTheme.colorScheme.onSurface,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.widthIn(max = 180.dp),
                    )
                    val size = if (gone) copyText("attach.msg.cleared") else a.sizeText
                    size?.let { Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                }
            }
        }
    }
}

@OptIn(ExperimentalFoundationApi::class)
@Composable
private fun UserBubble(
    unit: RenderUnit.UserText,
    matched: Boolean,
    said: SaidHooks?,
) {
    val tokens = LocalAppTokens.current
    Box(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(tokens.radius.xl))
            .background(tokens.card)
            .matchHighlight(matched)
            .then(if (said != null) Modifier.combinedClickable(onClick = {}, onLongClick = { said.onLongPress(unit) }) else Modifier)
            .padding(tokens.spacing.md),
    ) {
        if (said != null && said.editing == unit.key) {
            said.editor(unit)
            return@Box
        }
        Column {
            if (unit.parts.isEmpty()) {
                Text(unit.text, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurface)
            } else {
                unit.parts.forEach { SaidPartView(it, matched) }
            }
            // 投递状态贴在这条消息下面：只有在送 · 排着 · 送达未知 · 没送到才有（没送到的那一句改了再送走输入框上方那一排的［撤回］）。
            val line = unit.deliveryText
            when (unit.delivery) {
                DeliveryState.SENDING, DeliveryState.QUEUED, DeliveryState.UNSURE ->
                    line?.let { Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                DeliveryState.FAILED, DeliveryState.FAILED_PERMANENT ->
                    line?.let { Text(it, style = monoSmall, color = tokens.errorText) }
                null -> Unit
            }
        }
    }
}

/** 人说的那一句的一段：照排的字照排；那台说要折的那一块收成一行，点开就地展开正文（命中自动展开）。 */
@Composable
private fun SaidPartView(
    part: SaidPart,
    matched: Boolean,
) {
    when (part) {
        is SaidPart.Plain -> Text(part.text, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurface)
        is SaidPart.Folded -> {
            var open by rememberExpandable(matched)
            Row(Modifier.clickable { open = !open }.padding(vertical = LocalAppTokens.current.spacing.xs), verticalAlignment = Alignment.CenterVertically) {
                Caret(open)
                // 收着那一行是那台写好的（`foldText`，「粘贴的内容 · N 行」）。
                Text(part.text, style = monoBody, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
            }
            AnimatedVisibility(open) { Text(part.body, style = monoBody, color = MaterialTheme.colorScheme.onSurface) }
        }
    }
}

@Composable
private fun AssistantMarkdownCard(
    unit: RenderUnit.AssistantMarkdown,
    matched: Boolean,
    onCopyCode: (String) -> Unit,
) {
    val codeBlocks = remember(unit.doc) { unit.doc.codeBlocks() }
    Column(Modifier.fillMaxWidth().matchHighlight(matched)) {
        MarkdownText(unit.doc, unit.more, modifier = Modifier.fillMaxWidth())
        if (codeBlocks.isNotEmpty()) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                SecondaryButton(onClick = { onCopyCode(codeBlocks.joinToString("\n\n")) }) {
                    Text(if (codeBlocks.size == 1) copyText("mobReading.code.copy") else copyText("mobReading.code.copyN", "n" to codeBlocks.size), style = monoSmall)
                }
            }
        }
    }
}

@Composable
private fun ThinkingCard(
    unit: RenderUnit.Thinking,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    var expanded by rememberExpandable(matched)
    // 正文是空串 ＝ 后端说「没写进记录」（线上约定，后端判一次）⇒ 只画一行，不给展开（点开是空的）。
    val recorded = unit.text.isNotEmpty()
    Column(Modifier.fillMaxWidth().matchHighlight(matched)) {
        Row(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(tokens.radius.l))
                .then(if (recorded) Modifier.clickable { expanded = !expanded } else Modifier)
                .padding(vertical = tokens.spacing.xs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            if (recorded) Caret(expanded)
            Spacer(Modifier.width(tokens.spacing.xs))
            Text(if (recorded) copyText("cards.thinking.title") else copyText("cards.thinking.unrecorded"), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        AnimatedVisibility(recorded && expanded) {
            ThinkingText(unit.text, Modifier.padding(start = tokens.spacing.lg, top = tokens.spacing.xs))
        }
    }
}

@Composable
private fun ToolCallCard(
    unit: RenderUnit.ToolCall,
    matched: Boolean,
    badge: String?,
    /** 还没配上结果时那一态（会话事实 `pending[]` 那一项）；事实里没有它 · 结果已到 ⇒ `null`。 */
    wait: PendingCall?,
) {
    val tokens = LocalAppTokens.current
    // 交互卡（核心判的卡型）默认展开：在等人决定，折叠会被误以为模型还在跑。
    var expanded by rememberExpandable(matched, initial = unit.interactive)
    // 结果到了照结果；没到照会话事实那一态（琥珀只给等批准 ＝ 需手动）；事实里没有它 ⇒ 不画状态。
    val statusColor =
        when {
            !unit.pending -> if (unit.isError) MaterialTheme.colorScheme.error else tokens.success
            wait == null -> tokens.borderFaint
            else ->
                when (wait.state) {
                    StepWait.RUNNING -> tokens.success
                    StepWait.AWAITING -> tokens.warn
                    StepWait.UNCLEAR -> tokens.textFaint
                }
        }
    val right = if (unit.pending) wait?.text else unit.resultLine
    // 核心算好的行差（结果那一份，没有就调用那一份）有行就画；手机不判行、不数数。
    val patchDiff = unit.diff?.takeIf { it.rows.isNotEmpty() }
    val summary = unit.summary.replace("\n", " ")

    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(tokens.radius.l))
            .background(tokens.card)
            .matchHighlight(matched),
    ) {
        Row(
            Modifier
                .fillMaxWidth()
                .clickable { expanded = !expanded }
                .padding(tokens.spacing.md),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Box(Modifier.size(tokens.dotSize).clip(CircleShape).background(statusColor))
            Spacer(Modifier.width(tokens.spacing.sm))
            Text(unit.name, style = MaterialTheme.typography.labelLarge.copy(fontWeight = FontWeight.SemiBold), color = MaterialTheme.colorScheme.onSurface)
            if (summary.isNotEmpty()) {
                Spacer(Modifier.width(tokens.spacing.sm))
                Text(
                    summary,
                    style = monoSmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                    modifier = Modifier.weight(1f, fill = false),
                )
            }
            Spacer(Modifier.weight(1f))
            right?.let {
                Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
                Spacer(Modifier.width(tokens.spacing.sm))
            }
            badge?.let {
                Text(it, style = MaterialTheme.typography.labelMedium, color = tokens.warn, maxLines = 1, modifier = Modifier.testTag(READING_STEP_BADGE))
                Spacer(Modifier.width(tokens.spacing.sm))
            }
            Caret(expanded)
        }
        AnimatedVisibility(expanded) {
            Column(
                Modifier.padding(
                    start = tokens.spacing.md,
                    end = tokens.spacing.md,
                    bottom = tokens.spacing.md,
                ),
            ) {
                if (patchDiff != null) {
                    // 行差（结果那一份带真行号）。
                    unit.patchFilePath?.let { path ->
                        Text(path, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(bottom = tokens.spacing.xs))
                    }
                    DiffView(patchDiff)
                }
                val result = unit.resultText
                if (result != null && result.isNotBlank()) {
                    if (patchDiff != null) Spacer(Modifier.padding(top = tokens.spacing.xs))
                    Text(
                        result.take(MAX_RESULT_CHARS),
                        style = monoBody,
                        color = if (unit.isError) tokens.errorText else MaterialTheme.colorScheme.onSurface,
                    )
                    if (result.length > MAX_RESULT_CHARS) {
                        Text(copyText("mobReading.tool.truncated"), style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
                    }
                }
            }
        }
    }
}

private const val MAX_RESULT_CHARS = 4000

/** 工具卡上「等批准」那一枚（冒烟按它认）。 */
const val READING_STEP_BADGE = "reading-step-badge"
