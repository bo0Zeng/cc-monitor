package com.ccmonitor.mobile.ui.chat

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableLongStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.model.StreamItem
import com.ccmonitor.mobile.core.claude.model.TurnFolds
import com.ccmonitor.mobile.core.copy.DurationFormat
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.wire.gen.Tone
import com.ccmonitor.mobile.core.wire.gen.TurnPart
import com.ccmonitor.mobile.core.wire.gen.TurnPhase
import com.ccmonitor.mobile.core.wire.gen.TurnSpan
import kotlinx.coroutines.delay

/**
 * 一段过程的那一行：固定两行（第一行 plain 那几截；第二行行首照 `phase` ＋ now · need · fail 那几截 ＋ 右端用时），跑着 → 等批准 → 跑完行高不变。
 * 整行是开合区；点开了、有失败 ⇒ 底下一颗「跳到第一处失败」。字 · 语气 · 右端那一截全是那台写好的（[TurnFolds.lines]），这里只排版。
 */
@Composable
fun ProcessRow(
    item: StreamItem.Process,
    actions: ProcessActions,
    now: () -> Long = System::currentTimeMillis,
) {
    val tokens = LocalAppTokens.current
    val fold = item.fold
    val (top, bottom) = TurnFolds.lines(fold)
    val awaiting = fold.phase == TurnPhase.AWAITING
    val sep = copyText("kit.text.sep")
    Column(Modifier.fillMaxWidth().testTag(ChatTestTags.PROCESS)) {
        Row(
            Modifier
                .fillMaxWidth()
                .height(ROW_H)
                .clip(RoundedCornerShape(tokens.radius.m))
                .then(if (awaiting) Modifier.border(1.dp, tokens.warn, RoundedCornerShape(tokens.radius.m)) else Modifier)
                .clickable { actions.toggle(fold.after) }
                .padding(horizontal = tokens.spacing.sm),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            PhIcon(if (item.open) Ph.CaretDown else Ph.CaretRight, contentDescription = null, size = Touch.iconInline, tint = tokens.textFaint)
            Spacer(Modifier.width(tokens.spacing.sm))
            Column(Modifier.weight(1f), verticalArrangement = Arrangement.Center) {
                Text(
                    top.joinToString(sep) { it.text },
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    maxLines = 1,
                    overflow = TextOverflow.Ellipsis,
                )
                Row(verticalAlignment = Alignment.CenterVertically) {
                    PhaseMark(fold.phase)
                    Text(
                        toned(bottom, sep),
                        style = MaterialTheme.typography.bodySmall,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                        modifier = Modifier.weight(1f),
                    )
                    Text(spanText(fold.span, now, fold.phase != TurnPhase.IDLE), style = MaterialTheme.typography.bodySmall, color = tokens.textFaint, maxLines = 1)
                }
            }
        }
        val fail = fold.firstFail
        if (item.open && fail != null) {
            SecondaryButton(onClick = { actions.jumpFail(fold.after) }, modifier = Modifier.testTag(ChatTestTags.JUMP_FAIL)) {
                PhIcon(Ph.ArrowDown, contentDescription = null, size = Touch.iconInline)
                Spacer(Modifier.width(tokens.spacing.xs))
                Text(copyText("stream.proc.jumpFail"))
            }
        }
    }
}

/** 行首那一枚：在跑 ⇒ 转圈；在等人 ⇒ 琥珀点；跑完 ⇒ 不画。 */
@Composable
private fun PhaseMark(phase: TurnPhase) {
    val tokens = LocalAppTokens.current
    when (phase) {
        TurnPhase.RUNNING -> CircularProgressIndicator(Modifier.size(MARK), strokeWidth = 2.dp, color = tokens.success)
        TurnPhase.AWAITING -> Box(Modifier.size(MARK).background(tokens.warn, CircleShape))
        TurnPhase.IDLE -> return
    }
    Spacer(Modifier.width(tokens.spacing.xs))
}

/** 第二行那几截：照语气上色（闭集，不按字判）。 */
@Composable
private fun toned(
    parts: List<TurnPart>,
    sep: String,
): AnnotatedString {
    val tokens = LocalAppTokens.current
    val plain = MaterialTheme.colorScheme.onSurfaceVariant
    return buildAnnotatedString {
        parts.forEachIndexed { i, p ->
            if (i > 0) append(sep)
            withStyle(SpanStyle(color = toneColor(p.tone, tokens.warn, tokens.errorText, plain))) { append(p.text) }
        }
    }
}

private fun toneColor(
    t: Tone,
    warn: Color,
    error: Color,
    plain: Color,
): Color =
    when (t) {
        Tone.NEED, Tone.WARN -> warn
        Tone.FAIL -> error
        Tone.NOW, Tone.BUSY, Tone.PLAIN -> plain
    }

/** 右端那一截：`{dur}` 照 `from` / `to` 填（会走的钟，同桌面）；还在跑 ⇒ 每分钟重画一次。没有起点 ⇒ 照抄。 */
@Composable
private fun spanText(
    span: TurnSpan,
    now: () -> Long,
    ticking: Boolean,
): String {
    var t by remember { mutableLongStateOf(now()) }
    if (ticking) {
        LaunchedEffect(Unit) {
            while (true) {
                delay(TICK_MS)
                t = now()
            }
        }
    }
    val from = span.from ?: return span.text
    return DurationFormat.clock(span.text, from, span.to, t)
}

/** 过程行固定两行的高（跑着 → 等批准 → 跑完不变）。 */
private val ROW_H = 52.dp
private val MARK = 10.dp
private const val TICK_MS = 60_000L
