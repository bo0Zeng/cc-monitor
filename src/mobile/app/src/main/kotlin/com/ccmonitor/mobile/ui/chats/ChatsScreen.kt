@file:Suppress("MatchingDeclarationName") // 会话屏的 composable 与它的测试标签放一处

package com.ccmonitor.mobile.ui.chats

import androidx.compose.foundation.ExperimentalFoundationApi
import androidx.compose.foundation.background
import androidx.compose.foundation.combinedClickable
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.RowScope
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.LazyListState
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Button
import androidx.compose.material3.InputChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.ListSort
import com.ccmonitor.mobile.core.claude.link.SessionDot
import com.ccmonitor.mobile.core.claude.link.Toned
import com.ccmonitor.mobile.core.claude.link.wire.Tone
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ui.common.CopyDetailButton
import com.ccmonitor.mobile.ui.common.DashedBar
import com.ccmonitor.mobile.ui.common.DeployStepLine
import com.ccmonitor.mobile.ui.common.StatusDot
import com.ccmonitor.mobile.ui.common.deployLabel

/** 屏上几处的测试标签。 */
object ChatsTags {
    const val LIST = "chats-list"
    const val EMPTY = "chats-empty"
    const val PROBLEM = "chats-problem"
    const val DOWN = "chats-down"
    const val DEPLOY = "chats-deploy"
    const val DEPLOYED = "chats-deployed"
    const val HIDDEN = "chats-hidden"
    const val SHOW_HIDDEN = "chats-show-hidden"
    const val SKELETON = "chats-skeleton"
    const val CHIP = "chats-chip"
}

/** 一屏上点下去的那几件事（交给板 · 导航）。 */
class ChatsActions(
    val open: (ChatRow) -> Unit,
    val menu: (ChatRow) -> Unit,
    /** 一台顶上那一条的［重试］/［重新连接］。 */
    val retry: (HostBar) -> Unit,
    val deploy: (hostId: String) -> Unit,
    val filters: ((Filters) -> Filters) -> Unit,
)

/**
 * 会话屏的列表：顶上生效的筛选（chip，点一颗去掉）· 各台的条 · 注解没并上那一句，下面几台并好的一段一段。
 * 第一次读画骨架（不转圈）；一行都没有 ⇒ 「无会话记录」或「无匹配」＋［清除筛选］。
 */
@Composable
fun ChatsScreen(
    ui: ChatsUi,
    act: ChatsActions,
    listState: LazyListState,
    modifier: Modifier = Modifier,
) {
    LazyColumn(modifier.fillMaxSize().testTag(ChatsTags.LIST), state = listState) {
        item { Chips(ui, act.filters) }
        items(ui.bars, key = { "bar-${it.hostId}" }) { HostBarView(it, act) }
        items(ui.deploying, key = { "deploying-${it.hostId}" }) { p ->
            when (val d = p.deploying) {
                is Deploying.Step -> Column(Modifier.padding(16.dp)) { DeployStepLine(d) }
                is Deploying.Done -> Text(d.text, style = monoSmall, modifier = Modifier.padding(16.dp).testTag(ChatsTags.DEPLOYED))
                else -> Unit
            }
        }
        ui.notices.forEach { n -> item { Text(n, style = monoSmall, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) } }
        when {
            ui.skeleton -> items(SKELETON_ROWS) { SkeletonRow() }
            ui.empty != Empty.NONE -> item { EmptyView(ui, act.filters) }
            else -> for (s in ui.sections) section(s, act)
        }
        if (ui.truncated) item { Text(copyText("history.list.truncated"), style = monoSmall, modifier = Modifier.padding(16.dp)) }
    }
}

/** 生效着的筛选排一排（机器 · 时间 · 排序 · 显示已隐藏），点一颗去掉那一条；没有就不画。 */
@Composable
private fun Chips(
    ui: ChatsUi,
    onFilters: ((Filters) -> Filters) -> Unit,
) {
    val f = ui.filters
    val chips =
        buildList {
            if (f.unchecked.isNotEmpty()) {
                add(ui.choices.filter { it.checked }.joinToString(copyText("kit.text.sep")) { it.label } to { x: Filters -> x.copy(unchecked = emptySet()) })
            }
            f.withinDays?.let { add(withinLabel(it) to { x: Filters -> x.copy(withinDays = null) }) }
            f.sort?.let { add(sortLabel(it) to { x: Filters -> x.copy(sort = null) }) }
            if (f.hidden) add(copyText("history.filter.showHidden") to { x: Filters -> x.copy(hidden = false) })
        }
    if (chips.isEmpty()) return
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 16.dp),
        horizontalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        chips.forEach { (label, drop) ->
            InputChip(
                selected = true,
                onClick = { onFilters(drop) },
                label = { Text(label, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                trailingIcon = { PhIcon(Ph.X, contentDescription = null, size = Touch.iconInline) },
                modifier = Modifier.testTag(ChatsTags.CHIP),
            )
        }
    }
}

internal fun withinLabel(days: Int): String = if (days == WEEK) copyText("history.filter.time7d") else copyText("history.filter.time30d")

internal fun sortLabel(s: ListSort): String =
    when (s) {
        ListSort.ACTIVITY -> copyText("history.filter.sortActivity")
        ListSort.CREATED -> copyText("history.filter.sortCreated")
    }

@Composable
private fun EmptyView(
    ui: ChatsUi,
    onFilters: ((Filters) -> Filters) -> Unit,
) {
    Column(Modifier.padding(16.dp).testTag(ChatsTags.EMPTY)) {
        when (ui.empty) {
            Empty.NO_SESSIONS -> {
                Text(copyText("history.list.empty"), style = MaterialTheme.typography.bodyMedium)
                Text(copyText("history.list.emptyHint"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
            Empty.NO_MATCH -> {
                Text(ui.query?.let { copyText("history.search.noMatch", "q" to it) } ?: copyText("history.list.noMatch"), style = MaterialTheme.typography.bodyMedium)
                if (ui.filters.active) {
                    SecondaryButton(onClick = { onFilters { Filters(view = it.view) } }) { Text(copyText("history.list.clearFilter")) }
                }
            }
            Empty.NONE -> Unit
        }
    }
}

private fun LazyListScope.section(
    s: ChatSection,
    act: ChatsActions,
) {
    s.title?.let { t -> item { SectionHeader(t) } }
    items(s.rows, key = { "${it.hostId}\n${it.sessionId}" }) { r -> SessionRow(r, act) }
}

@Composable
private fun SessionRow(
    r: ChatRow,
    act: ChatsActions,
) {
    DotRow(r.dot, r.status?.text, onClick = { act.open(r) }, onLongClick = { act.menu(r) }) {
        Column(Modifier.weight(1f)) {
            // 第一层：★ · 标题 · 时刻；第二层：机器小标签 · 状态字 · 目录短名 · 家牌 ·「已隐藏」· 分叉那一句（有的才写，用 · 连）。
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                if (r.starred) PhIcon(Ph.Star, contentDescription = null, size = Touch.iconInline, tint = LocalAppTokens.current.warn)
                Text(r.label, maxLines = 1, overflow = TextOverflow.Ellipsis, style = rowTitle(), modifier = Modifier.weight(1f))
                r.atText?.let { Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
            }
            SecondLine(r)
        }
    }
}

/** 第二层：机器小标签在最前，状态字（带语气）跟着，其余几格淡字，用 ` · ` 连。 */
@Composable
private fun SecondLine(r: ChatRow) {
    val rest = listOfNotNull(r.project, r.agentTag, r.hidden?.let { copyText("history.row.hidden") }, r.fork)
    if (r.machineTag == null && r.status == null && rest.isEmpty()) return
    Row(verticalAlignment = Alignment.CenterVertically) {
        r.machineTag?.let { MachineTag(it) }
        r.status?.let { StatusText(it) }
        val tail = rest.joinToString(copyText("kit.text.sep"))
        if (tail.isNotEmpty()) {
            Text(
                (if (r.status != null) copyText("kit.text.sep") else "") + tail,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
                modifier = if (r.hidden != null) Modifier.testTag(ChatsTags.HIDDEN) else Modifier,
            )
        }
    }
}

/** 机器小标签（等宽小字、底色一块）。 */
@Composable
internal fun MachineTag(name: String) {
    val tokens = LocalAppTokens.current
    Text(
        name,
        style = monoSmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        maxLines = 1,
        modifier = Modifier.padding(end = 6.dp).background(tokens.bg2, RoundedCornerShape(tokens.radius.s)).padding(horizontal = 4.dp),
    )
}

/** 一台顶上那一条：断着 ⇒ 虚线条 ＋［重新连接］（核心点名能装 ⇒ 红条 ＋［安装］）；读不出 ⇒ 红条 ＋［重试］［复制详情］。 */
@Composable
private fun HostBarView(
    b: HostBar,
    act: ChatsActions,
) {
    if (b.offline && b.fix == null) {
        DashedBar(b.problem.text, copyText("newSession.machine.reconnect"), { act.retry(b) }, Modifier.padding(horizontal = 16.dp, vertical = 4.dp).testTag(ChatsTags.DOWN))
        return
    }
    val step = b.deploying as? Deploying.Step
    val failed = (b.deploying as? Deploying.Failed)?.said
    Column(Modifier.fillMaxWidth().padding(16.dp).testTag(ChatsTags.PROBLEM)) {
        when {
            step != null -> DeployStepLine(step)
            failed != null -> ErrorLine(failed.text)
            else -> ErrorLine(b.problem.text)
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            b.fix?.let { fix ->
                Button(onClick = { act.deploy(b.hostId) }, enabled = step == null, modifier = Modifier.testTag(ChatsTags.DEPLOY)) { Text(deployLabel(fix)) }
            }
            if (step == null) SecondaryButton(onClick = { act.retry(b) }) { Text(copyText("front.act.retry")) }
            CopyDetailButton(failed?.text ?: b.problem.text, failed?.detail ?: b.problem.detail)
        }
    }
}

@Composable
internal fun SectionHeader(text: String) {
    Text(text, style = MaterialTheme.typography.labelMedium, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 16.dp, bottom = 4.dp))
}

@Composable
private fun StatusText(s: Toned) {
    Text(s.text, color = toneColor(s.tone), style = MaterialTheme.typography.bodySmall)
}

/** 行标题：正文字号 600。 */
@Composable
internal fun rowTitle() = MaterialTheme.typography.titleSmall

/** 骨架一行（两条灰块），不转圈。 */
@Composable
private fun SkeletonRow() {
    val c = MaterialTheme.colorScheme.surfaceContainerHigh
    val shape = RoundedCornerShape(LocalAppTokens.current.radius.s)
    Column(Modifier.fillMaxWidth().heightIn(min = Touch.rowDouble).padding(start = 44.dp, end = 16.dp, top = 12.dp).testTag(ChatsTags.SKELETON)) {
        Box(Modifier.fillMaxWidth(SKELETON_TITLE).height(12.dp).background(c, shape))
        Box(Modifier.padding(top = 8.dp).fillMaxWidth(SKELETON_SUB).height(10.dp).background(c, shape))
    }
}

/**
 * 一行：行首一颗状态点（固定一列宽）＋ 内容。双行（标题 ＋ 状态字），行高照适配层 [Touch.rowDouble]。
 * [label] 是点的读屏名（那一行的状态字）；没有 ⇒ 点不念。
 */
@OptIn(ExperimentalFoundationApi::class) // combinedClickable：点行打开，长按 ＝ 菜单
@Composable
internal fun DotRow(
    dot: SessionDot?,
    label: String?,
    onClick: () -> Unit,
    onLongClick: (() -> Unit)? = null,
    content: @Composable RowScope.() -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Touch.rowDouble)
            .combinedClickable(onClick = onClick, onLongClick = onLongClick)
            .padding(horizontal = 16.dp, vertical = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        StatusDot(dot, label)
        content()
    }
}

/**
 * 语气 ⇒ 字色：闭集，按核心给的语气选，不按字。状态字回 `--text-2`，只有需手动那句琥珀、出错那句红字；
 * 在跑不再用强调色当字色（那是状态点的事）。
 */
@Composable
private fun toneColor(t: Tone?): Color {
    val tokens = LocalAppTokens.current
    return when (t) {
        Tone.NEED, Tone.WARN -> tokens.warn
        Tone.FAIL -> tokens.errorText
        Tone.NOW, Tone.BUSY, Tone.PLAIN, null -> MaterialTheme.colorScheme.onSurfaceVariant
    }
}

internal const val WEEK = 7
internal const val MONTH = 30
private const val SKELETON_ROWS = 8
private const val SKELETON_TITLE = 0.6f
private const val SKELETON_SUB = 0.4f
