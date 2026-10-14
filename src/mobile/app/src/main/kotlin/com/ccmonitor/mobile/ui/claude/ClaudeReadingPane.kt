package com.ccmonitor.mobile.ui.claude

import androidx.compose.animation.AnimatedVisibility
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.lazy.rememberLazyListState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.diff.structuredPatchToDiff
import com.ccmonitor.mobile.core.claude.model.DeliveryState
import com.ccmonitor.mobile.core.claude.model.RenderUnit
import com.ccmonitor.mobile.core.claude.model.extractCodeBlocks
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoBody
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ui.common.CopyDetailButton

/**
 * 会话只读阅读面：把 [RenderUnit] 列表渲染成卡片流，嵌在会话屏里复用。
 * LazyColumn key 用 [RenderUnit.key]（分类器算的 uuid#blockIndex，身份稳定），实时追加不错位。
 */
@Composable
fun ClaudeReadingPane(
    units: List<RenderUnit>,
    modifier: Modifier = Modifier,
    listState: LazyListState = rememberLazyListState(),
    matchedKeys: Set<String> = emptySet(), // 搜索命中的 unit.key：高亮，并展开命中的折叠卡
    onCopyCode: (String) -> Unit = {}, // 复制代码块
    // 本地消息发送失败时的重试。默认 no-op，只读阅读面用不到。
    onRetrySend: (String) -> Unit = {},
    // 正文尾巴上不进时间线的那一块（聊天屏的活卡）。
    trailer: LazyListScope.() -> Unit = {},
) {
    val tokens = LocalAppTokens.current
    // 整个面共享一个 Markwon 实例，避免每张卡重建解析器。
    ProvideMarkwon {
        LazyColumn(
            state = listState,
            modifier = modifier.fillMaxWidth(),
            contentPadding = PaddingValues(tokens.spacing.md),
            verticalArrangement = Arrangement.spacedBy(tokens.spacing.md),
        ) {
            // key 用 RenderUnit.key（uuid#blockIndex，身份稳定），流式追加不漂移。
            //
            // `contentType` 要给：卡片有好几种，不给的话 Compose 会试图把一种卡的组合复用到另一种上。
            // 更要紧的是它与 `AndroidView` 的 View 复用串联：Markwon 走 `AndroidView`，View 复用只在外层组合被复用时发生；
            // 没有 contentType 组合就不复用，每滑一屏都在重建 TextView、重跑 Markwon。
            items(units, key = { it.key }, contentType = { it::class }) { unit ->
                val matched = unit.key in matchedKeys
                when (unit) {
                    is RenderUnit.UserText -> UserTextCard(unit, matched, onRetrySend)
                    is RenderUnit.AssistantMarkdown -> AssistantMarkdownCard(unit, matched, onCopyCode)
                    is RenderUnit.Thinking -> ThinkingCard(unit, matched)
                    is RenderUnit.ToolCall -> ToolCallCard(unit, matched)
                    // 斜杠命令、bash 输入输出、compact 摘要各有自己的卡片。
                    is RenderUnit.SlashCommand -> SlashCommandCard(unit, matched)
                    is RenderUnit.BashInput -> BashInputCard(unit, matched)
                    is RenderUnit.BashOutput -> BashOutputCard(unit, matched)
                    is RenderUnit.CompactSummary -> CompactSummaryCard(unit, matched)
                    is RenderUnit.Interrupt -> InterruptLine(unit)
                    is RenderUnit.Unread -> UnreadLine(unit, matched)
                }
            }
            trailer()
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
            Text(if (expanded) "▾" else "▸", style = monoSmall, color = tokens.textFaint)
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
            Text(copyText("mobReading.bash.title"), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
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
                    if (expanded) copyText("mobReading.bash.collapse") else copyText("mobReading.bash.expand", "n" to outLines),
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

@Composable
private fun UserTextCard(
    unit: RenderUnit.UserText,
    matched: Boolean,
    onRetrySend: (String) -> Unit = {},
) {
    val tokens = LocalAppTokens.current
    Box(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(tokens.radius.xl))
            .background(tokens.card)
            .matchHighlight(matched)
            .padding(tokens.spacing.md),
    ) {
        Column {
            Text(unit.text, style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurface)
            // 投递状态贴在这条消息下面（字照文案表 `terminal.input.*` 那一族）：只有在送 · 送达未知 · 没送到才有。
            val line = unit.deliveryText
            when (unit.delivery) {
                DeliveryState.SENDING, DeliveryState.UNSURE ->
                    line?.let { Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                DeliveryState.FAILED ->
                    Row(verticalAlignment = Alignment.CenterVertically) {
                        line?.let { Text(it, style = monoSmall, color = tokens.errorText) }
                        TextButton(onClick = { onRetrySend(unit.key) }) { Text(copyText("terminal.input.retry"), style = monoSmall) }
                    }
                DeliveryState.FAILED_PERMANENT ->
                    line?.let { Text(it, style = monoSmall, color = tokens.errorText) }
                null -> Unit
            }
        }
    }
}

@Composable
private fun AssistantMarkdownCard(
    unit: RenderUnit.AssistantMarkdown,
    matched: Boolean,
    onCopyCode: (String) -> Unit,
) {
    val codeBlocks = remember(unit.markdown) { extractCodeBlocks(unit.markdown) }
    Column(Modifier.fillMaxWidth().matchHighlight(matched)) {
        MarkdownText(unit.markdown, modifier = Modifier.fillMaxWidth())
        if (codeBlocks.isNotEmpty()) {
            Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End) {
                TextButton(onClick = { onCopyCode(codeBlocks.joinToString("\n\n")) }) {
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
    Column(Modifier.fillMaxWidth().matchHighlight(matched)) {
        Row(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(tokens.radius.l))
                .clickable { expanded = !expanded }
                .padding(vertical = tokens.spacing.xs),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Caret(expanded)
            Spacer(Modifier.width(tokens.spacing.xs))
            Text(copyText("cards.thinking.title"), style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        AnimatedVisibility(expanded) {
            Text(
                unit.text,
                style = MaterialTheme.typography.bodyMedium.copy(fontStyle = FontStyle.Italic),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = tokens.spacing.lg, top = tokens.spacing.xs),
            )
        }
    }
}

@Composable
private fun ToolCallCard(
    unit: RenderUnit.ToolCall,
    matched: Boolean,
) {
    val tokens = LocalAppTokens.current
    // 交互工具（AskUserQuestion / ExitPlanMode）默认展开：在等人决定，折叠会被误以为模型还在跑。
    var expanded by rememberExpandable(matched, initial = isInteractiveTool(unit.name))
    val statusColor =
        when {
            unit.pending -> tokens.warn
            unit.isError -> MaterialTheme.colorScheme.error
            else -> tokens.success
        }
    // 核心给的 patch（真行号、带上下文）就画 diff；没给就不画，手机不自己从入参算 diff。
    val patchDiff = remember(unit) { unit.structuredPatch?.takeIf { it.isNotEmpty() }?.let { structuredPatchToDiff(it) } }
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
            unit.resultLine?.let {
                Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
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
                    // structuredPatch 保真 diff（真行号加上下文）。
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
                } else if (unit.pending) {
                    Text(copyText("mobReading.tool.running"), style = monoSmall, color = tokens.warn)
                }
            }
        }
    }
}

private const val MAX_RESULT_CHARS = 4000

/** 交互工具（AskUserQuestion / ExitPlanMode）在等人决定：默认展开，否则折起来看着像模型还在跑。 */
private val INTERACTIVE_TOOLS = setOf("AskUserQuestion", "ExitPlanMode")

private fun isInteractiveTool(name: String): Boolean = name in INTERACTIVE_TOOLS
