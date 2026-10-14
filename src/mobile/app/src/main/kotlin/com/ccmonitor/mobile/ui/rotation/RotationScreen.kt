package com.ccmonitor.mobile.ui.rotation

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.wire.RotFormScope
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.feedback.LocalAppSnackbar
import com.ccmonitor.mobile.core.ui.feedback.Notice
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.common.CopyDetailButton
import com.ccmonitor.mobile.ui.common.rememberHostLink
import kotlinx.coroutines.launch
import org.koin.compose.koinInject

/**
 * 轮换屏（`设计稿/手机端/轮换屏/`）：四块竖着排，顺序同桌面——第一行（策略下拉 ·「⋯」· 说明）· 提示 · 轮换账号 · 总体设置 · 模拟；没存的改动在底部常驻。
 * 两处进来是同一屏：聊天屏「⋯」→ 轮换（[sid]，那条会话）· 设置 → 机器与账号 → 轮换（[sid] 为 `null`：新会话用的那份）。
 * 画的全是那台的回包（[RotationBoard]）；这里只排版、把手势交给它。
 */
@Composable
fun RotationScreen(
    hostId: String,
    sid: String?,
    machine: String,
    onBack: () -> Unit,
    /** 提示里的［看代码］· 旧格式那几份：用文件屏开（[path] 是那台上的全路径）。 */
    onOpenFile: (String) -> Unit,
) {
    rememberHostLink(hostId, "rotation-$hostId")
    val backends = koinInject<HostBackends>()
    val feed = remember(hostId) { backends.of(hostId) }
    val scope = rememberCoroutineScope()
    val board = remember(hostId, sid) { RotationBoard(feed, sid, machine, scope, feed.changed) }
    val ui by board.state.collectAsState()
    val linkState by feed.state.collectAsState()
    val table by feed.table.collectAsState()
    val title = sid?.let { table.sessions[it]?.name }.orEmpty()
    val snackbar = LocalAppSnackbar.current
    LaunchedEffect(ui.failure) {
        ui.failure?.let {
            snackbar.show(Notice(it.text, it.detail))
            board.failureShown()
        }
    }
    Scaffold(
        topBar = { RotTopBar(if (sid == null) copyText("rot.top.here", "machine" to machine) else title, onBack) },
        bottomBar = { if (ui.view?.dirty == true && linkState !is LinkState.Down) DirtyBar(ui.saving, board) },
    ) { inner ->
        Column(
            Modifier
                .fillMaxSize()
                .padding(inner)
                .verticalScroll(rememberScrollState())
                .padding(horizontal = 16.dp, vertical = 12.dp)
                .testTag(RotTags.SCREEN),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            val down = linkState as? LinkState.Down
            when {
                down != null -> DownBox(copyText("front.body.offline", "machine" to machine))
                !ui.loaded -> Skeleton()
                else -> {
                    ui.readFailed?.let { ReadFailed(it.text, it.detail) { scope.launch { board.refresh() } } }
                    BoardBody(ui, board, sid, table.sessions.mapValues { it.value.name.orEmpty() }, onOpenFile)
                }
            }
        }
    }
    RotAsks(ui, board)
}

/** 四块（读到了才画；不轮换 ⇒ 只剩第一行和一行灰字）。 */
@Composable
private fun BoardBody(
    ui: RotationUi,
    board: RotationBoard,
    sid: String?,
    titles: Map<String, String>,
    onOpenFile: (String) -> Unit,
) {
    FirstRow(ui, board, sid, titles)
    val view = ui.view ?: return
    Hints(ui, view, board, onOpenFile)
    if (view.strategy == null) return
    Cards(ui, view, board)
    val all = view.form.filter { it.scope == RotFormScope.ALL }
    if (all.isNotEmpty()) {
        RotHeader(copyText("rot.h.all"))
        all.forEach { CellRows(ui, view, board, it, null) }
    }
    if (view.timeline != null) Simulation(ui, view, board)
    Spacer(Modifier.height(24.dp))
}

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun RotTopBar(
    second: String,
    onBack: () -> Unit,
) {
    TopAppBar(
        navigationIcon = {
            IconButton(onClick = onBack, modifier = Modifier.semantics { contentDescription = copyText("mobKit.act.back") }) {
                Text(BACK, style = MaterialTheme.typography.titleLarge)
            }
        },
        title = {
            Column {
                Text(copyText("mobRot.top.title"), style = MaterialTheme.typography.titleMedium)
                if (second.isNotEmpty()) {
                    Text(second, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.Ellipsis)
                }
            }
        },
    )
}

/** 底部常驻：「未保存 ·［还原］·［存成这份策略］」（桌面放在第一行右边；手机页长，放底部滚到哪都够得着）。存着 ⇒ 转圈、［还原］灰。 */
@Composable
private fun DirtyBar(
    saving: Boolean,
    board: RotationBoard,
) {
    Surface(color = MaterialTheme.colorScheme.surfaceContainer, tonalElevation = 3.dp) {
        Row(
            Modifier
                .fillMaxWidth()
                .navigationBarsPadding()
                .padding(horizontal = 16.dp, vertical = 8.dp)
                .testTag(RotTags.DIRTY),
            verticalAlignment = Alignment.CenterVertically,
        ) {
            Text(copyText("rot.top.dirty"), style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
            TextButton(onClick = board::revert, enabled = !saving) { Text(copyText("rot.top.revert")) }
            Button(onClick = board::save, enabled = !saving) {
                if (saving) {
                    CircularProgressIndicator(Modifier.width(16.dp).height(16.dp), strokeWidth = 2.dp)
                    Spacer(Modifier.width(8.dp))
                }
                Text(copyText("rot.top.save"))
            }
        }
    }
}

/** 头一回读：四块骨架占位，不转圈。 */
@Composable
private fun Skeleton() {
    val c = MaterialTheme.colorScheme.surfaceContainerHigh
    Box(Modifier.fillMaxWidth().height(48.dp).background(c, RoundedCornerShape(6.dp)))
    Box(Modifier.fillMaxWidth(0.9f).height(12.dp).background(c, RoundedCornerShape(3.dp)))
    listOf(copyText("rot.h.cards") to 72.dp, copyText("rot.h.all") to 72.dp, copyText("rot.h.sim") to 160.dp).forEach { (h, tall) ->
        RotHeader(h)
        Box(Modifier.fillMaxWidth().height(tall).background(c, RoundedCornerShape(6.dp)))
    }
}

/** 无法读取：红条写那台的那一句 ·［复制详情］·［重试］。 */
@Composable
private fun ReadFailed(
    text: String,
    detail: String,
    retry: () -> Unit,
) {
    NoteBox(NoteTone.ERROR, Modifier.testTag(RotTags.READ_FAILED)) {
        Text(text, style = MaterialTheme.typography.bodyMedium)
        Row(Modifier.fillMaxWidth(), horizontalArrangement = Arrangement.End, verticalAlignment = Alignment.CenterVertically) {
            CopyDetailButton(text, detail)
            TextButton(onClick = retry) { Text(copyText("rot.read.retry")) }
        }
    }
}

/** 那台离线（常驻流断着）：中性灰虚线框「{machine} 离线」，不画旧数。 */
@Composable
private fun DownBox(text: String) {
    val line = MaterialTheme.colorScheme.outline
    Box(
        Modifier
            .fillMaxWidth()
            .drawBehind {
                drawRoundRect(line, style = Stroke(width = 1.dp.toPx(), pathEffect = PathEffect.dashPathEffect(floatArrayOf(8f, 6f))), cornerRadius = CornerRadius(6.dp.toPx()))
            }.padding(14.dp),
    ) {
        Text(text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

/** 一块的小标题（左字，右边一句灰字 ＋ 按钮）。 */
@Composable
internal fun RotHeader(
    text: String,
    note: String? = null,
    trailing: @Composable () -> Unit = {},
) {
    Row(Modifier.fillMaxWidth().padding(top = 8.dp), verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        Text(text, style = MaterialTheme.typography.labelLarge, modifier = Modifier.weight(1f))
        note?.let { Text(it, style = MaterialTheme.typography.labelSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
        trailing()
    }
}

/** 带颜色的一块（提示 · 红条）：左边一道色条，底照语气。 */
@Composable
internal fun NoteBox(
    tone: NoteTone,
    modifier: Modifier = Modifier,
    content: @Composable () -> Unit,
) {
    val ink = toneInk(tone)
    Column(
        modifier
            .fillMaxWidth()
            .background(ink.copy(alpha = 0.10f), RoundedCornerShape(6.dp))
            .border(1.dp, ink.copy(alpha = 0.45f), RoundedCornerShape(6.dp))
            .padding(horizontal = 12.dp, vertical = 8.dp),
        verticalArrangement = Arrangement.spacedBy(4.dp),
    ) { content() }
}

@Composable
internal fun toneInk(tone: NoteTone) =
    when (tone) {
        NoteTone.ERROR -> MaterialTheme.colorScheme.error
        NoteTone.WARN -> LocalAppTokens.current.warn
        NoteTone.INFO -> MaterialTheme.colorScheme.primary
        NoteTone.PLAIN -> MaterialTheme.colorScheme.onSurfaceVariant
    }

private const val BACK = "←"

/** 给冒烟与 UI 测试认的 tag（与字解耦）。 */
object RotTags {
    const val SCREEN = "rot-screen"
    const val PICK = "rot-pick"
    const val MORE = "rot-more"
    const val DIRTY = "rot-dirty"
    const val READ_FAILED = "rot-read-failed"
    const val CARD = "rot-card"
    const val LANE = "rot-lane"
    const val TOP = "rot-top"
}
