package com.ccmonitor.mobile.ui.usage

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.listSaver
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clipToBounds
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.draw.rotate
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.wire.QuotaCell
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.claude.link.wire.UsageTone
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.icon.BackButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.link.Fetched
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.chat.newsession.AccountBadge
import com.ccmonitor.mobile.ui.chat.newsession.DefaultTag
import com.ccmonitor.mobile.ui.common.CopyDetailButton
import com.ccmonitor.mobile.ui.common.rememberHostLink
import com.ccmonitor.mobile.ui.nav.settingsGroupScreenInsets
import com.ccmonitor.mobile.ui.rotation.NoteBox
import com.ccmonitor.mobile.ui.rotation.NoteTone
import com.ccmonitor.mobile.ui.settings.Here
import com.ccmonitor.mobile.ui.settings.MachineSettings
import org.koin.compose.koinInject

/**
 * 用量屏：这台每个号还能不能发、几点重置。读的是 [MachineSettings] 那一份 `quota-read`（与设置屏同一个读法），
 * 字与语气全照回包；手机只按号名对上账号库那一行（号牌 · 默认）。点一张卡就地展开那个号的全部行；展开着的号重读后照样展开。
 */
@Composable
fun UsageScreen(
    here: Here,
    onBack: () -> Unit,
) {
    rememberHostLink(here.hostId, "usage-${here.hostId}")
    val backends = koinInject<HostBackends>()
    val feed = remember(here.hostId) { backends.of(here.hostId) }
    val scope = rememberCoroutineScope()
    val board = remember(here.hostId) { MachineSettings(feed, here.machine, feed.changed, scope) }
    val ui by board.state.collectAsState()
    val link by feed.state.collectAsState()
    var open by rememberSaveable(stateSaver = OPEN_SAVER) { mutableStateOf(emptySet<String>()) }
    Column(Modifier.fillMaxSize().settingsGroupScreenInsets()) {
        Row(Modifier.padding(horizontal = 8.dp).heightIn(min = Touch.topBar), verticalAlignment = Alignment.CenterVertically) {
            BackButton(onBack)
            Column {
                Text(copyText("mobUsage.page.title"), style = MaterialTheme.typography.titleLarge)
                Text(here.machine, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        Column(
            Modifier
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 8.dp)
                .testTag(UsageTags.SCREEN),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (link is LinkState.Down) {
                DashedBar(copyText("front.body.offline", "machine" to here.machine), copyText("newSession.machine.reconnect"), feed::reconnect)
            } else {
                ui.quotaRereadFailed?.let { DashedBar(it.text, copyText("front.act.retry"), board::reloadQuota) }
            }
            when (val q = ui.quota) {
                Fetched.Loading -> repeat(SKELETON_CARDS) { SkeletonCard() }
                is Fetched.Failed -> if (link !is LinkState.Down) DashedBar(q.problem.text, copyText("front.act.retry"), board::reloadQuota)
                is Fetched.Got -> {
                    val view = usageView(q.value, ui.accounts, ui.defaultAccount)
                    Head(view, board::reloadQuota)
                    view.cards.forEach { c ->
                        Card(c, c.account in open) { open = if (c.account in open) open - c.account else open + c.account }
                    }
                }
            }
        }
    }
}

/** 展开着的号名集（存成列表才进得了 `rememberSaveable`）。 */
private val OPEN_SAVER = listSaver<Set<String>, String>({ it.toList() }, { it.toSet() })

/** 结论两行（大字 · 小字，都不能发 ⇒ 红）；没有结论 ⇒ 那台那一句（读不出 ⇒ 红条 ＋［复制详情］［重试］）。 */
@Composable
private fun Head(
    view: UsageUi,
    retry: () -> Unit,
) {
    val v = view.verdict
    val failed = view.failed
    when {
        v != null ->
            Column(Modifier.testTag(UsageTags.VERDICT)) {
                Text(v.text, style = MaterialTheme.typography.headlineSmall, fontWeight = FontWeight.SemiBold, color = usageInk(v.tone, MaterialTheme.colorScheme.onSurface))
                v.sub?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
        failed != null ->
            NoteBox(NoteTone.ERROR, Modifier.testTag(UsageTags.READ_FAILED)) {
                Text(view.text.orEmpty(), style = MaterialTheme.typography.bodyMedium)
                Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
                    CopyDetailButton(view.text.orEmpty(), failed.detail)
                    SecondaryButton(onClick = retry) { Text(copyText("front.act.retry")) }
                }
            }
        else -> view.text?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

/** 一张卡：卡头 ＋（两条计量条 ＋ 采样 | 按量那一格 | 没出过数的那几行）；点开 ⇒ 第一行之后的全部行。 */
@Composable
private fun Card(
    c: UsageCard,
    expanded: Boolean,
    onToggle: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val shape = RoundedCornerShape(tokens.radius.m)
    val canOpen = c.detail.isNotEmpty()
    Column(
        Modifier
            .fillMaxWidth()
            .background(tokens.card, shape)
            .border(1.dp, tokens.borderFaint, shape)
            .then(if (canOpen) Modifier.clickable(role = Role.Button, onClick = onToggle) else Modifier)
            .padding(14.dp)
            .testTag(UsageTags.CARD),
        verticalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            c.plate?.let { AccountBadge(it, c.color) }
            c.head.forEachIndexed { i, cell ->
                if (i == 0) {
                    Text(cell.text, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold, color = cellInk(cell.tone, MaterialTheme.colorScheme.onSurface))
                } else {
                    Text(cell.text, style = MaterialTheme.typography.bodySmall, color = cellInk(cell.tone, MaterialTheme.colorScheme.onSurfaceVariant))
                }
            }
            if (c.isDefault) DefaultTag()
            Box(Modifier.weight(1f))
            if (canOpen) {
                PhIcon(Ph.CaretDown, contentDescription = null, size = Touch.iconInline, tint = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.rotate(if (expanded) HALF_TURN else 0f))
            }
        }
        c.bars.forEach { Bar(it) }
        c.usage?.let { u -> LabeledLine(copyText("acct.hover.usage"), listOf(QuotaCell(u.text, toneOf(u.tone)))) }
        c.plain.forEach { r -> LabeledLine(r.first().text, r.drop(1)) }
        c.seen?.let { r -> LabeledLine(r.first().text, r.drop(1)) }
        if (expanded) {
            HorizontalDivider(Modifier.padding(vertical = 4.dp), color = tokens.borderFaint)
            Column(Modifier.testTag(UsageTags.DETAIL), verticalArrangement = Arrangement.spacedBy(4.dp)) {
                c.detail.forEach { r -> LabeledLine(r.first().text, r.drop(1)) }
            }
        }
    }
}

/** 一条计量条：位名 · 条（长 ＝ pct，色 ＝ 语气，数旧画斜纹）· 右边那一格；下一行 ↻ · 距今 ……「1% ≈ 多少」。 */
@Composable
private fun Bar(b: UsageBar) {
    val faint = MaterialTheme.colorScheme.onSurfaceVariant
    val ink = usageInk(b.slot.tone, faint)
    val track = MaterialTheme.colorScheme.surfaceContainerHigh
    val fill = usageInk(b.slot.tone, MaterialTheme.colorScheme.onSurfaceVariant)
    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(10.dp)) {
            Text(b.label, style = MaterialTheme.typography.bodySmall, color = faint, modifier = Modifier.width(LABEL_W.dp))
            Box(
                Modifier
                    .weight(1f)
                    .height(BAR_H.dp)
                    .clipToBounds()
                    .drawBehind {
                        val r = CornerRadius(size.height / 2)
                        drawRoundRect(track, cornerRadius = r)
                        val w = size.width * b.fill
                        drawRoundRect(fill, size = size.copy(width = w), cornerRadius = r)
                        if (b.stale) {
                            var x = -size.height
                            while (x < w) {
                                drawLine(track, Offset(x, size.height), Offset(x + size.height, 0f), strokeWidth = 1.dp.toPx())
                                x += STRIPE.dp.toPx()
                            }
                        }
                    }.testTag(UsageTags.BAR),
            )
            Text(b.slot.text, style = MaterialTheme.typography.bodySmall, color = ink)
        }
        val reset = listOfNotNull(b.slot.resetText, b.slot.resetsAtRelText).joinToString("  ")
        if (reset.isNotEmpty() || b.slot.perPctText != null) {
            Row(Modifier.padding(start = (LABEL_W + 10).dp)) {
                Text(reset, style = MaterialTheme.typography.labelSmall, color = faint, modifier = Modifier.weight(1f))
                b.slot.perPctText?.let { Text(it, style = MaterialTheme.typography.labelSmall, color = faint) }
            }
        }
    }
}

/** 一行：左边那一格当名字，后面几格用 ` · ` 连，每格色照它自己的语气。 */
@Composable
private fun LabeledLine(
    label: String,
    cells: List<QuotaCell>,
) {
    val faint = MaterialTheme.colorScheme.onSurfaceVariant
    Row(horizontalArrangement = Arrangement.spacedBy(10.dp)) {
        Text(label, style = MaterialTheme.typography.bodySmall, color = faint, modifier = Modifier.width(LABEL_W.dp))
        Row {
            cells.forEachIndexed { i, cell ->
                if (i > 0) Text(copyText("kit.text.sep"), style = MaterialTheme.typography.bodySmall, color = faint)
                Text(cell.text, style = MaterialTheme.typography.bodySmall, color = cellInk(cell.tone, MaterialTheme.colorScheme.onSurface))
            }
        }
    }
}

/** 中性灰虚线条：那一句 ＋ 一颗按钮（离线 ⇒［重新连接］；无应答 ⇒［重试］）。 */
@Composable
private fun DashedBar(
    text: String,
    action: String,
    onAction: () -> Unit,
) {
    val line = MaterialTheme.colorScheme.outline
    Row(
        Modifier
            .fillMaxWidth()
            .drawBehind {
                drawRoundRect(line, style = Stroke(width = 1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(DASH_ON, DASH_OFF))), cornerRadius = CornerRadius(6.dp.toPx()))
            }.padding(horizontal = 14.dp, vertical = 8.dp)
            .testTag(UsageTags.DOWN),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.weight(1f))
        SecondaryButton(onClick = onAction) { Text(action) }
    }
}

/** 头一回读：一张空卡（不转圈）。 */
@Composable
private fun SkeletonCard() {
    val c = MaterialTheme.colorScheme.surfaceContainerHigh
    Box(
        Modifier
            .fillMaxWidth()
            .height(SKELETON_H.dp)
            .background(c, RoundedCornerShape(LocalAppTokens.current.radius.m))
            .testTag(UsageTags.SKELETON),
    )
}

@Composable
private fun usageInk(
    tone: UsageTone,
    plain: Color,
): Color =
    when (tone) {
        UsageTone.FAIL -> LocalAppTokens.current.errorText
        UsageTone.WARN -> LocalAppTokens.current.warn
        UsageTone.PLAIN -> plain
    }

@Composable
private fun cellInk(
    tone: Tone,
    plain: Color,
): Color =
    when (tone) {
        Tone.FAIL -> LocalAppTokens.current.errorText
        Tone.WARN, Tone.NEED -> LocalAppTokens.current.warn
        Tone.PLAIN, Tone.NOW, Tone.BUSY -> plain
    }

private fun toneOf(t: UsageTone): Tone =
    when (t) {
        UsageTone.FAIL -> Tone.FAIL
        UsageTone.WARN -> Tone.WARN
        UsageTone.PLAIN -> Tone.PLAIN
    }

/** 给冒烟与 UI 测试认的 tag（与字解耦）。 */
object UsageTags {
    const val SCREEN = "usage-screen"
    const val VERDICT = "usage-verdict"
    const val CARD = "usage-card"
    const val DETAIL = "usage-detail"
    const val BAR = "usage-bar"
    const val DOWN = "usage-down"
    const val READ_FAILED = "usage-read-failed"
    const val SKELETON = "usage-skeleton"
}

private const val SKELETON_CARDS = 3
private const val SKELETON_H = 132
private const val LABEL_W = 36
private const val BAR_H = 6
private const val STRIPE = 6
private const val DASH_ON = 8f
private const val DASH_OFF = 6f
private const val HALF_TURN = 180f
