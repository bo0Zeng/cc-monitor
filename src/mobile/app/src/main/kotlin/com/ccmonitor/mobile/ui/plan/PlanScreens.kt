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
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.claude.link.ResidentFeed
import com.ccmonitor.mobile.core.claude.link.cell
import com.ccmonitor.mobile.core.claude.link.cellsOf
import com.ccmonitor.mobile.core.claude.link.hasKids
import com.ccmonitor.mobile.core.claude.link.label
import com.ccmonitor.mobile.core.claude.link.slice
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
import com.ccmonitor.mobile.core.ui.icon.BackButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.wire.gen.Command
import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.PlanCell
import com.ccmonitor.mobile.core.wire.gen.PlanCellViewReq
import com.ccmonitor.mobile.core.wire.gen.PlanList
import com.ccmonitor.mobile.core.wire.gen.PlanListReq
import com.ccmonitor.mobile.core.wire.gen.PlanLook
import com.ccmonitor.mobile.core.wire.gen.PlanNeed
import com.ccmonitor.mobile.core.wire.gen.PlanPbState
import com.ccmonitor.mobile.core.wire.gen.PlanRead
import com.ccmonitor.mobile.core.wire.gen.PlanReadReq
import com.ccmonitor.mobile.core.wire.gen.PlanSlice
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.link.problem
import com.ccmonitor.mobile.link.unreadableReply
import com.ccmonitor.mobile.ssh.machineName
import com.ccmonitor.mobile.ui.common.rememberHostLink
import kotlinx.coroutines.launch
import org.koin.compose.koinInject

/*
 * 手机计划三屏（窄屏一屏一层）：片 ⇒ 大纲 ⇒ 一格详情；一格详情里点子格再推一屏。数据是那台 `plan-list` · `plan-read` 的成品
 * （照生成类型解），随数据变的字都是后端写好的；这里只照摆、骨架字照文案表取、按后端给的码挑图标与颜色。不给整张图（只在本机）。
 */

/** 一屏读到哪：读着 · 读到了 · 没读成（那一句是那台写好的）。 */
private data class PlanLoad<T>(
    val face: T? = null,
    val failed: String? = null,
    val loading: Boolean = true,
)

/** 问那台一条计划命令，读成那一屏；没答成 ⇒ 那一句，答成了但解不开 · 片或格不在 ⇒ 读不出那一句。 */
private suspend fun <Q, R, T> ask(
    feed: ResidentFeed,
    machine: String,
    command: Command<Q, R>,
    req: Q,
    read: (R) -> T?,
): PlanLoad<T> {
    val r = feed.call(command.name, command.args(req), READ_MS)
    if (r !is Reply.Ok) return PlanLoad(failed = r.problem(machine, command.name, feed.offline.value)?.text, loading = false)
    return command.reply(r.data)?.let(read)?.let { PlanLoad(it, loading = false) }
        ?: PlanLoad(failed = unreadableReply(machine, command.name, r.data.toString()).text, loading = false)
}

/** 进屏读一次（键变了重读）。 */
@Composable
private fun <Q, R, T> rememberPlanLoad(
    hostId: String,
    key: String,
    command: Command<Q, R>,
    req: Q,
    read: (R) -> T?,
): PlanLoad<T> {
    rememberHostLink(hostId, "plan-$hostId")
    val backends = koinInject<HostBackends>()
    val hosts = koinInject<HostRepository>()
    val got by produceState(PlanLoad<T>(), hostId, key) {
        val machine = hosts.machineName(hostId)
        value = ask(backends.of(hostId), machine, command, req, read)
    }
    return got
}

/** 片那一屏：这台的工作区，一片一行；点一行进那一片的大纲。pb 不在 ⇒ 那台写的那一句；一片都没有 ⇒ 无计划那一句。 */
@Composable
fun PlanSlicesScreen(
    hostId: String,
    onBack: () -> Unit,
    onOpen: (workspace: String, slice: String) -> Unit,
) {
    val load = rememberPlanLoad(hostId, "list", Commands.PlanList, PlanListReq()) { it }
    PlanScaffold(copyText("plan.page.title"), null, onBack, load) { list: PlanList ->
        if (list.pb.state != PlanPbState.OK) {
            // 那台分好的两格：那一句 · 下一步。
            item { Note(list.pb.said.orEmpty()) }
            list.pb.hint?.let { item { Note(it) } }
            return@PlanScaffold
        }
        if (list.workspaces.all { it.slices.isEmpty() }) item { Note(copyText("plan.empty.text")) }
        val sep = copyText("kit.text.sep")
        list.workspaces.forEach { w ->
            item(key = "ws#${w.workspace}") { Head(w.workspace, mono = true) }
            items(w.slices, key = { "slice#${w.workspace}#${it.name}" }) { sl ->
                val line = sl.said.bare ?: listOfNotNull(sl.domain, sl.said.strip).filter { it.isNotEmpty() }.joinToString(sep)
                Line(sl.name, line, sl.said.root, dot = sl.needCount > 0) { onOpen(w.workspace, sl.name) }
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
    val load = rememberPlanLoad(hostId, "read#$workspace#$slice", Commands.PlanRead, PlanReadReq(workspace)) { r -> r.slice(slice)?.let { r to it } }
    PlanScaffold(slice, workspace, onBack, load) { (read, sl) ->
        val sep = copyText("kit.text.sep")
        val meta = listOfNotNull(sl.said.bare, sl.said.strip, sl.said.readAt, sl.said.top, copyText("plan.sub.docsOnly").takeIf { sl.docsOnly })
        if (meta.isNotEmpty()) item { Note(meta.joinToString(sep)) }
        items(sl.needs, key = { "need#${it.key}" }) { NeedCardView(it) }
        mine(read, sl, sid)?.let { rows ->
            item { Head(copyText("plan.blocks.block")) }
            items(rows, key = { "mine#${it.first}" }) { (k, v) -> Kv(k, v) }
        }
        item { Head(copyText("plan.overview.progress")) }
        rows(sl, sl.cellsOf(sl.top), "top", onOpenCell)
    }
}

/** 此会话接手的那一块（在这一片里才有）：块 · 阶段 · 站在哪 · 里面签了几格，没有的那一行不出。 */
private fun mine(
    read: PlanRead,
    sl: PlanSlice,
    sid: String?,
): List<Pair<String, String>>? {
    val b = sid?.let { read.bySession[it] }?.takeIf { it.slice == sl.name } ?: return null
    val block = sl.blocks.firstOrNull { it.id == b.block }
    val inside = block?.said?.inside
    return listOfNotNull(
        copyText("plan.blocks.block") to b.said.name,
        b.phase?.let { copyText("plan.blocks.phase") to it },
        b.at?.let { copyText("plan.blocks.at") to sl.label(it) },
        inside?.let { copyText("plan.blocks.inside") to it },
    )
}

/** 一格详情那一屏：状态 · 块根那一句 · 要做成什么样 · 里面（点了再推一屏）· 管的文件 · agent 站在这一格看到的（点了才问）。 */
@Composable
fun PlanCellScreen(
    hostId: String,
    workspace: String,
    slice: String,
    id: String,
    onBack: () -> Unit,
    onOpenCell: (id: String) -> Unit,
) {
    val load =
        rememberPlanLoad(hostId, "cell#$workspace#$slice#$id", Commands.PlanRead, PlanReadReq(workspace)) { r ->
            r.slice(slice)?.let { sl -> sl.cell(id)?.let { sl to it } }
        }
    val title = load.face?.let { (sl, _) -> sl.label(id) } ?: id
    val cell = load.face?.second
    val trail = cell?.said?.trail
    PlanScaffold(title, trail, onBack, load) { (sl, c) ->
        item {
            Row(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 10.dp), verticalAlignment = Alignment.CenterVertically) {
                LookIcon(c.look)
                Text(
                    listOfNotNull(c.kind, c.end ?: c.quiet.orEmpty()).joinToString(copyText("kit.text.sep")),
                    style = MaterialTheme.typography.bodyMedium,
                    modifier = Modifier.padding(start = 10.dp),
                )
            }
        }
        c.said.blockRoot?.let { item { Note(it) } }
        c.body?.takeIf { it.isNotBlank() }?.let { body ->
            item { Head(copyText("plan.cell.design")) }
            item { Text(body, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) }
        }
        val kids = sl.cellsOf(c.children)
        if (kids.isNotEmpty()) {
            item { Head(copyText("plan.cell.inside")) }
            rows(sl, kids, "kid", onOpenCell)
        }
        if (c.files.isNotEmpty()) {
            item { Head(copyText("plan.cell.files")) }
            items(c.files, key = { "file#${it.path}" }) { f -> Line(f.path, f.note, f.checkText, mono = true) }
        }
        item { AgentView(hostId, workspace, slice, id) }
    }
}

/** 一格那一屏底下：点了才问 `plan-cell-view`（起一次 pb），原样等宽摆。 */
@Composable
private fun AgentView(
    hostId: String,
    workspace: String,
    slice: String,
    id: String,
) {
    val backends = koinInject<HostBackends>()
    val hosts = koinInject<HostRepository>()
    val scope = rememberCoroutineScope()
    var got by remember(id) { mutableStateOf<PlanLoad<String>?>(null) }
    Head(copyText("plan.cell.agentView"))
    when (val g = got) {
        null ->
            SecondaryButton(onClick = {
                got = PlanLoad()
                scope.launch {
                    val machine = hosts.machineName(hostId)
                    got = ask(backends.of(hostId), machine, Commands.PlanCellView, PlanCellViewReq(workspace, slice, id)) { it.view }
                }
            }, modifier = Modifier.padding(horizontal = 8.dp)) { Text(copyText("plan.cell.agentViewHint")) }
        else -> {
            g.failed?.let { ErrorLine(it, modifier = Modifier.padding(horizontal = 16.dp, vertical = 4.dp)) }
            if (g.loading) CircularProgressIndicator(Modifier.padding(16.dp).size(20.dp))
            g.face?.let { Text(it, fontFamily = FontFamily.Monospace, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(16.dp)) }
        }
    }
}

/** 一层格（大纲顶层 · 一格的里面）：图标 · 「编号 标题」· 右端那一句（后端写好的）· 有子格 ⇒ 箭头。 */
private fun LazyListScope.rows(
    sl: PlanSlice,
    cells: List<PlanCell>,
    tag: String,
    onOpen: (String) -> Unit,
) {
    items(cells, key = { "$tag#${it.id}" }) { c ->
        Row(
            Modifier.fillMaxWidth().clickable { onOpen(c.id) }.padding(horizontal = 16.dp, vertical = 12.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            LookIcon(c.look)
            Text(sl.label(c.id), style = MaterialTheme.typography.bodyLarge, maxLines = 1, overflow = TextOverflow.Ellipsis, modifier = Modifier.weight(1f))
            c.end?.takeIf { it.isNotEmpty() }?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1) }
            if (sl.hasKids(c)) PhIcon(Ph.CaretRight, null, size = 14.dp, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        HorizontalDivider(color = LocalAppTokens.current.borderFaint)
    }
}

/** 画哪一种 ⇒ 图标（只挑图标，字是后端写好的那一句）。 */
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

/** 一条需手动：满宽的卡（琥珀底），标题 · 第几条 · 负责人那一行（问人 · 停了那两种）· 那几行，都是后端写好的。 */
@Composable
private fun NeedCardView(n: PlanNeed) {
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
            Text(n.pos.orEmpty(), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        (listOfNotNull(n.who) + n.lines).forEach {
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
