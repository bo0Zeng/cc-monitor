package com.ccmonitor.mobile.ui.plan

import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.LazyListScope
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.produceState
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.text.font.FontFamily
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.PlanCellFace
import com.ccmonitor.mobile.core.claude.link.PlanCellView
import com.ccmonitor.mobile.core.claude.link.PlanList
import com.ccmonitor.mobile.core.claude.link.PlanLook
import com.ccmonitor.mobile.core.claude.link.PlanNeedCard
import com.ccmonitor.mobile.core.claude.link.PlanOutlineFace
import com.ccmonitor.mobile.core.claude.link.PlanRead
import com.ccmonitor.mobile.core.claude.link.PlanRowFace
import com.ccmonitor.mobile.core.claude.link.PlanSlicesFace
import com.ccmonitor.mobile.core.claude.link.Plans
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.ResidentFeed
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
import com.ccmonitor.mobile.core.ui.icon.BackButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ui.common.rememberHostLink
import kotlinx.coroutines.launch
import org.koin.compose.koinInject

/*
 * 手机计划三屏（窄屏一屏一层）：片 ⇒ 大纲 ⇒ 一格详情；一格详情里点子格再推一屏。数据是那台 `plan-list` · `plan-read` 的成品，
 * 字由内嵌核心照成品写好（[Plans]），这里只照摆、按核心给的码挑图标与颜色。不给整张图（只在本机）。
 */

/** 一屏读到哪：读着 · 读到了 · 没读成（那一句是那台或核心写好的）。 */
private data class PlanLoad<T>(
    val face: T? = null,
    val failed: String? = null,
    val loading: Boolean = true,
)

/** 问那台一条计划命令，读成那一屏；没答成 ⇒ 那一句，答成了但读不成那一屏 ⇒ 读不出那一句。 */
private suspend fun <T> ask(
    feed: ResidentFeed,
    machine: String,
    command: String,
    args: Map<String, Any?>,
    read: (Any?) -> T?,
): PlanLoad<T> {
    val r = feed.call(command, args, READ_MS)
    if (r !is Reply.Ok) return PlanLoad(failed = r.problem(machine, command, feed.offline.value)?.text, loading = false)
    return read(r.data)?.let { PlanLoad(it, loading = false) } ?: PlanLoad(failed = unreadableReply(machine, command, r.data.toString()).text, loading = false)
}

/** 进屏读一次（键变了重读）。 */
@Composable
private fun <T> rememberPlanLoad(
    hostId: String,
    key: String,
    command: String,
    args: Map<String, Any?>,
    read: (Any?) -> T?,
): PlanLoad<T> {
    rememberHostLink(hostId, "plan-$hostId")
    val backends = koinInject<HostBackends>()
    val hosts = koinInject<HostRepository>()
    val got by produceState(PlanLoad<T>(), hostId, key) {
        val machine = runCatching { hosts.get(hostId) }.getOrNull()?.label ?: hostId
        value = ask(backends.of(hostId), machine, command, args, read)
    }
    return got
}

/** 片那一屏：这台的工作区，一片一行；点一行进那一片的大纲。 */
@Composable
fun PlanSlicesScreen(
    hostId: String,
    onBack: () -> Unit,
    onOpen: (workspace: String, slice: String) -> Unit,
) {
    val load = rememberPlanLoad<PlanSlicesFace>(hostId, "list", PlanList.COMMAND, PlanList.args()) { Plans.slices(it) }
    PlanScaffold(copyText("plan.page.title"), null, onBack, load) { face ->
        face.said?.let { item { Note(it) } }
        face.groups.forEach { g ->
            item(key = "ws#${g.workspace}") { Head(g.head, mono = true) }
            items(g.rows, key = { "slice#${g.workspace}#${it.slice}" }) { r ->
                Line(r.slice, r.line, r.end, dot = r.need) { onOpen(g.workspace, r.slice) }
            }
        }
    }
}

/** 大纲那一屏：需手动排最前 · 此会话接手的块 · 顶层一层；点一行进那一格。 */
@Composable
fun PlanOutlineScreen(
    hostId: String,
    workspace: String,
    slice: String,
    sid: String?,
    onBack: () -> Unit,
    onOpenCell: (id: String) -> Unit,
) {
    val load = rememberPlanLoad<PlanOutlineFace>(hostId, "read#$workspace#$slice", PlanRead.COMMAND, PlanRead.args(workspace)) { Plans.outline(it, slice, sid) }
    PlanScaffold(slice, workspace, onBack, load) { face ->
        if (face.meta.isNotEmpty()) item { Note(face.meta.joinToString(copyText("kit.text.sep"))) }
        items(face.needs, key = { "need#${it.kind}#${it.pos}" }) { NeedCardView(it) }
        face.mine?.let { m ->
            item { Head(m.head) }
            items(m.rows, key = { "mine#${it.k}" }) { kv -> Kv(kv.k, kv.v) }
        }
        item { Head(face.head) }
        rows(face.rows, "top", onOpenCell)
    }
}

/** 一格详情那一屏：状态 · 权力范围 · 要做成什么样 · 里面（点了再推一屏）· 管的文件 · agent 站在这一格看到的（点了才问）。 */
@Composable
fun PlanCellScreen(
    hostId: String,
    workspace: String,
    slice: String,
    id: String,
    onBack: () -> Unit,
    onOpenCell: (id: String) -> Unit,
) {
    val load = rememberPlanLoad<PlanCellFace>(hostId, "cell#$workspace#$slice#$id", PlanRead.COMMAND, PlanRead.args(workspace)) { Plans.cell(it, slice, id) }
    PlanScaffold(load.face?.label ?: id, load.face?.path, onBack, load) { face ->
        item {
            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                LookIcon(face.look)
                Text(
                    listOfNotNull(face.kind, face.state).joinToString(copyText("kit.text.sep")),
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(start = 10.dp),
                )
            }
        }
        face.block?.let { item { Note(it) } }
        face.design?.let { d ->
            item { Head(d.head) }
            item { Text(d.body, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) }
        }
        face.inside?.let { s ->
            item { Head(s.head) }
            rows(s.rows, "kid", onOpenCell)
        }
        face.files?.let { s ->
            item { Head(s.head) }
            items(s.rows, key = { "file#${it.path}" }) { f -> Line(f.path, f.note, f.state, mono = true) }
        }
        item { AgentView(hostId, workspace, slice, id, face) }
    }
}

/** 一格那一屏底下：点了才问 `plan-cell-view`（起一次 pb），原样等宽摆。 */
@Composable
private fun AgentView(
    hostId: String,
    workspace: String,
    slice: String,
    id: String,
    face: PlanCellFace,
) {
    val backends = koinInject<HostBackends>()
    val hosts = koinInject<HostRepository>()
    val scope = rememberCoroutineScope()
    var got by remember(id) { mutableStateOf<PlanLoad<String>?>(null) }
    Head(face.agentHead)
    when (val g = got) {
        null ->
            SecondaryButton(onClick = {
                got = PlanLoad()
                scope.launch {
                    val machine = runCatching { hosts.get(hostId) }.getOrNull()?.label ?: hostId
                    got = ask(backends.of(hostId), machine, PlanCellView.COMMAND, PlanCellView.args(workspace, slice, id), PlanCellView::of)
                }
            }, modifier = Modifier.padding(horizontal = 8.dp)) { Text(face.agentHint) }
        else -> {
            g.failed?.let { ErrorLine(it, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) }
            if (g.loading) CircularProgressIndicator(Modifier.padding(16.dp).size(20.dp))
            g.face?.let { Text(it, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(16.dp)) }
        }
    }
}

private fun LazyListScope.rows(
    rows: List<PlanRowFace>,
    tag: String,
    onOpen: (String) -> Unit,
) {
    items(rows, key = { "$tag#${it.id}" }) { r ->
        Row(
            Modifier.fillMaxWidth().clickable { onOpen(r.id) }.padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            LookIcon(r.look)
            Text(r.label, style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            if (r.end.isNotEmpty()) Text(r.end, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1)
            if (r.kids) PhIcon(Ph.CaretRight, null, size = 14.dp, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        HorizontalDivider(color = LocalAppTokens.current.borderFaint)
    }
}

/** 状态码 ⇒ 图标（只挑图标，字是核心写好的那一句）。 */
fun lookIcon(look: PlanLook): Ph =
    when (look) {
        PlanLook.DONE -> Ph.Check
        PlanLook.UNDECIDED -> Ph.WarningCircle
        PlanLook.OPEN, PlanLook.NONE -> Ph.Circle
    }

@Composable
private fun lookTint(look: PlanLook): Color {
    val t = LocalAppTokens.current
    return when (look) {
        PlanLook.DONE -> t.success
        PlanLook.UNDECIDED -> t.warn
        PlanLook.OPEN -> MaterialTheme.colorScheme.onSurfaceVariant
        PlanLook.NONE -> t.textFaint
    }
}

@Composable
private fun LookIcon(look: PlanLook) = PhIcon(lookIcon(look), null, size = 16.dp, tint = lookTint(look))

/** 一条需手动：满宽的卡（琥珀底），标题 · 第几条 · 那几行。 */
@Composable
private fun NeedCardView(n: PlanNeedCard) {
    val t = LocalAppTokens.current
    Column(
        Modifier
            .fillMaxWidth()
            .padding(horizontal = 12.dp, vertical = 6.dp)
            .background(t.warn.copy(alpha = NEED_BG), RoundedCornerShape(10.dp))
            .padding(12.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            PhIcon(Ph.WarningCircle, null, size = 16.dp, tint = t.warn)
            Text(n.title, style = MaterialTheme.typography.titleSmall, modifier = Modifier.weight(1f).padding(start = 8.dp))
            Text(n.pos, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        n.lines.forEach {
            Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(start = 24.dp, top = 4.dp))
        }
    }
}

@Composable
private fun Head(
    text: String,
    mono: Boolean = false,
) {
    Text(
        text,
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        fontFamily = if (mono) FontFamily.Monospace else null,
        maxLines = 1,
        overflow = TextOverflow.Ellipsis,
        modifier = Modifier.padding(start = 16.dp, end = 16.dp, top = 14.dp, bottom = 4.dp),
    )
}

@Composable
private fun Note(text: String) {
    Text(text, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp))
}

@Composable
private fun Kv(
    k: String,
    v: String,
) {
    Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp)) {
        Text(k, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(end = 12.dp).size(width = 72.dp, height = 20.dp))
        Text(v, style = MaterialTheme.typography.bodyMedium)
    }
}

/** 一行：主字 · 第二行 · 行尾；[dot] ⇒ 行尾一颗琥珀点（有需手动）。 */
@Composable
private fun Line(
    main: String,
    second: String?,
    end: String?,
    dot: Boolean = false,
    mono: Boolean = false,
    onClick: (() -> Unit)? = null,
) {
    val t = LocalAppTokens.current
    Row(
        Modifier.fillMaxWidth().let { if (onClick != null) it.clickable(onClick = onClick) else it }.padding(horizontal = 16.dp, vertical = 10.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(10.dp),
    ) {
        Column(Modifier.weight(1f)) {
            Text(main, style = MaterialTheme.typography.bodyLarge, fontFamily = if (mono) FontFamily.Monospace else null, maxLines = 1, overflow = TextOverflow.Ellipsis)
            second?.takeIf { it.isNotEmpty() }?.let {
                Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 2, overflow = TextOverflow.Ellipsis)
            }
        }
        end?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        if (dot) Box(Modifier.size(6.dp).background(t.warn, CircleShape))
    }
    HorizontalDivider(color = t.borderFaint)
}

/** 三屏同一个架子：顶栏（标题 · 第二行）· 读着 ⇒ 转圈 · 没读成 ⇒ 那一句 · 读到了 ⇒ [body]。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun <T> PlanScaffold(
    title: String,
    second: String?,
    onBack: () -> Unit,
    load: PlanLoad<T>,
    body: LazyListScope.(T) -> Unit,
) {
    Scaffold(
        topBar = {
            TopAppBar(
                navigationIcon = { BackButton(onBack) },
                title = {
                    Column {
                        Text(title, style = MaterialTheme.typography.titleMedium, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        second?.takeIf { it.isNotEmpty() }?.let {
                            Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                        }
                    }
                },
            )
        },
    ) { inner ->
        val face = load.face
        when {
            face != null -> LazyColumn(Modifier.fillMaxSize().padding(inner)) { body(face) }
            load.loading -> Box(Modifier.fillMaxSize().padding(inner), contentAlignment = Alignment.Center) { CircularProgressIndicator() }
            else -> Box(Modifier.fillMaxSize().padding(inner).padding(16.dp)) { ErrorLine(load.failed.orEmpty()) }
        }
    }
}

private const val READ_MS = 15_000L
private const val NEED_BG = 0.1f
