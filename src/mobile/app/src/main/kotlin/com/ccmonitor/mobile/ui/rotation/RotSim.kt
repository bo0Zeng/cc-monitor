package com.ccmonitor.mobile.ui.rotation

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.BoxWithConstraintsScope
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.offset
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.wire.gen.RotTryTimeline
import com.ccmonitor.mobile.core.wire.gen.RotationViewReply
import com.ccmonitor.mobile.ui.chat.newsession.AccountBadge

/** 时间轴的左右端（秒）：左 ＝ 此刻，右 ＝ 此刻往后推几小时。 */
private class Span(
    val a: Long,
    val b: Long,
) {
    fun x(t: Long): Float = ((t.coerceIn(a, b) - a).toFloat() / (b - a).coerceAtLeast(1))
}

/** 设置页的模拟：小标题（12h / 24h）＋ 时间轴（改一格就看得到接下来怎么换）。时刻的字 · 先后 · 跳过 · 停下全照那台推演的（`timeline`）。 */
@Composable
internal fun Simulation(
    ui: RotationUi,
    view: RotationViewReply,
    board: RotationBoard,
) {
    val tl = view.timeline ?: return
    RotHeader(copyText("rot.h.sim")) {
        SingleChoiceSegmentedButtonRow {
            listOf(RotationUi.HOURS_12 to copyText("rot.sim.h12"), RotationUi.HOURS_24 to copyText("rot.sim.h24")).forEachIndexed { i, (h, text) ->
                SegmentedButton(selected = ui.hours == h, onClick = { board.hours(h) }, shape = SegmentedButtonDefaults.itemShape(i, 2), icon = {}) { Text(text) }
            }
        }
    }
    val span = Span(view.now, view.now + ui.hours * SEC_PER_H)
    Surface(shape = RoundedCornerShape(8.dp), color = MaterialTheme.colorScheme.surfaceContainerLow, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.padding(8.dp), verticalArrangement = Arrangement.spacedBy(4.dp)) {
            SimRow(null) { Axis(view, tl, span) }
            SimRow(null) { Switches(view, tl, span, board) }
            SimRow(null) { TopUses(ui, tl, span, board) }
            tl.lanes.forEach { lane ->
                val head: @Composable () -> Unit = { LaneHead(lane.account, ui, board) }
                SimRow(head) { LaneTrack(view, lane.account, span) }
            }
        }
    }
}

/** 一行：左边号名一列 62 dp（没有就空着）· 右边轨道。 */
@Composable
private fun SimRow(
    head: (@Composable () -> Unit)?,
    track: @Composable BoxWithConstraintsScope.() -> Unit,
) {
    Row(Modifier.fillMaxWidth(), verticalAlignment = Alignment.CenterVertically) {
        Box(Modifier.width(HEAD)) { head?.invoke() }
        BoxWithConstraints(Modifier.weight(1f).height(ROW)) { track() }
    }
}

private fun BoxWithConstraintsScope.place(
    span: Span,
    from: Long,
    to: Long,
): Modifier = Modifier.offset(x = maxWidth * span.x(from)).width(maxWidth * (span.x(to) - span.x(from)).coerceAtLeast(0f))

/** 轴：此刻 ＋ 整点刻度（字都是那台写好的）；挨着此刻的刻度让开。 */
@Composable
private fun BoxWithConstraintsScope.Axis(
    view: RotationViewReply,
    tl: RotTryTimeline,
    span: Span,
) {
    val nowX = span.x(view.now)
    Text(view.nowText, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.primary, modifier = Modifier.offset(x = maxWidth * nowX))
    tl.ticks.forEach { t ->
        val x = span.x(t.at)
        val inside = t.at < span.b && t.at > span.a
        if (inside && kotlin.math.abs(x - nowX) >= NEAR && x <= LAST) {
            Text(t.text, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.offset(x = maxWidth * x))
        }
    }
}

/** 将来每次换号：「时刻 号」（那台写好的钟面）；挤不下的省掉（时刻在最上那条的段里看得出）。 */
@Composable
private fun BoxWithConstraintsScope.Switches(
    view: RotationViewReply,
    tl: RotTryTimeline,
    span: Span,
    board: RotationBoard,
) {
    var last = -1f
    tl.switches.filter { it.at > view.now && it.at < span.b }.forEach { w ->
        val x = span.x(w.at)
        if (x - last < GAP_X) return@forEach
        last = x
        Text(copyText("rot.sim.switch", "at" to w.atText, "account" to board.label(w.account)), style = MaterialTheme.typography.labelSmall, maxLines = 1, modifier = Modifier.offset(x = maxWidth * x))
    }
}

/** 最上那条：每段用哪个号（停发 ⇒ 停下）；轮到时用不了的写「{号} 跳过」划掉。 */
@Composable
private fun BoxWithConstraintsScope.TopUses(
    ui: RotationUi,
    tl: RotTryTimeline,
    span: Span,
    board: RotationBoard,
) {
    tl.uses.filter { it.to > span.a && it.from < span.b }.forEach { u ->
        UseSeg(place(span, u.from, u.to), u.account, u.text, ui, temp = u.fromTemp)
        u.skipped.forEach { sk ->
            Text(
                copyText("rot.sim.skip", "account" to board.label(sk)),
                style = MaterialTheme.typography.labelSmall.copy(textDecoration = TextDecoration.LineThrough),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.offset(x = maxWidth * span.x(u.from), y = -ROW),
            )
        }
    }
    // 最上那条的无障碍名：「模拟」（冒烟按它认）。
    Box(
        Modifier
            .fillMaxWidth()
            .fillMaxHeight()
            .semantics { contentDescription = copyText("rot.h.sim") }
            .testTag(RotTags.TOP),
    )
}

@Composable
private fun UseSeg(
    at: Modifier,
    account: String?,
    text: String,
    ui: RotationUi,
    temp: Boolean,
) {
    val acct = LocalAppTokens.current.acct
    // 没在用的号（停发）· 那台没给号色 ⇒ 中性底（不猜一格像真的色）。
    val (bg, ink) = account?.let { ui.plate(it)?.color }?.let { acct.getOrNull(it) }?.let { c -> c.bg to c.ink } ?: (MaterialTheme.colorScheme.surfaceContainerHighest to MaterialTheme.colorScheme.onSurface)
    Box(
        at
            .fillMaxHeight()
            .padding(horizontal = 0.5.dp)
            .background(bg.copy(alpha = FUTURE), RoundedCornerShape(2.dp))
            .then(if (temp) Modifier.border(1.5.dp, MaterialTheme.colorScheme.onSurface, RoundedCornerShape(2.dp)) else Modifier),
        contentAlignment = Alignment.CenterStart,
    ) {
        Text((if (temp) DOT else "") + text, color = ink, style = MaterialTheme.typography.labelSmall, maxLines = 1, overflow = TextOverflow.Clip, modifier = Modifier.padding(horizontal = 3.dp))
    }
}

/** 号名一列：号牌 ＋ 截断的名字。 */
@Composable
private fun LaneHead(
    account: String,
    ui: RotationUi,
    board: RotationBoard,
) {
    Row(
        Modifier.testTag("${RotTags.LANE}-$account"),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(3.dp),
    ) {
        AccountBadge(ui.plate(account)?.text, ui.plate(account)?.color, 16)
        Text(board.label(account), style = MaterialTheme.typography.labelSmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
    }
}

/** 每个号一行：只画这个号自己的额度；字只有「剩 N%」（此刻那段）·「↻HH:MM」（重置后那段）；发不出去的段画成红底。 */
@Composable
private fun BoxWithConstraintsScope.LaneTrack(
    view: RotationViewReply,
    account: String,
    span: Span,
) {
    val ok = MaterialTheme.colorScheme.surfaceContainerHighest
    val out = MaterialTheme.colorScheme.errorContainer
    view.timeline?.lanes?.firstOrNull { it.account == account }?.segs?.forEach { g ->
        if (g.to > span.a && g.from < span.b) LaneSeg(place(span, g.from, g.to), (if (g.sendable) ok else out).copy(alpha = FUTURE), g.text)
    }
}

@Composable
private fun LaneSeg(
    at: Modifier,
    color: Color,
    text: String,
) {
    Box(at.fillMaxHeight().padding(0.5.dp).background(color, RoundedCornerShape(2.dp)), contentAlignment = Alignment.CenterStart) {
        Text(text, style = MaterialTheme.typography.labelSmall, maxLines = 1, overflow = TextOverflow.Clip, modifier = Modifier.padding(horizontal = 3.dp))
    }
}

private val HEAD: Dp = 62.dp
private val ROW: Dp = 22.dp
private const val SEC_PER_H = 3600L
private const val FUTURE = 0.7f
private const val NEAR = 0.08f
private const val LAST = 0.92f
private const val GAP_X = 0.18f
private const val DOT = "• "
