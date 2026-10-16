package com.ccmonitor.mobile.ui.drawer

import androidx.activity.compose.BackHandler
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.DrawerState
import androidx.compose.material3.DrawerValue
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalDrawerSheet
import androidx.compose.material3.ModalNavigationDrawer
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.produceState
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.icon.PhIconButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.ThemeTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.ui.overview.rememberRecent
import kotlinx.coroutines.launch
import org.koin.compose.koinInject

/**
 * 聊天屏的抽屉：左上 ☰ 或从左边缘右滑拉出，盖住约 85%，点遮罩、左滑或返回键收起。
 *
 * 项与文案住 [drawerEntries]；去哪由调用方按 [DrawerAction] 定。
 *
 * @param hostId 当前服务器。
 * @param linkReady 这台的连接通了没有 ——「最近」要等它通了才去取列表。
 */
@Composable
fun ChatDrawer(
    hostId: String,
    linkReady: Boolean,
    drawerState: DrawerState,
    onAction: (DrawerAction) -> Unit,
    content: @Composable () -> Unit,
) {
    val scope = rememberCoroutineScope()
    BackHandler(enabled = drawerState.isOpen) { scope.launch { drawerState.close() } }
    // 先当场收起再走：边收边跳走的话，这一屏的抽屉状态按「开着」存下，退回来时抽屉还开着、盖住那一屏。
    val act: (DrawerAction) -> Unit = { action ->
        scope.launch {
            drawerState.snapTo(DrawerValue.Closed)
            onAction(action)
        }
    }
    ModalNavigationDrawer(
        drawerState = drawerState,
        gesturesEnabled = true,
        scrimColor = MaterialTheme.colorScheme.scrim,
        drawerContent = {
            // 抽屉 ＝ 凸起面（`--card`）＋ `--border-medium` 边 ＋ 外角 10；遮罩 `--overlay-backdrop`。
            ModalDrawerSheet(
                Modifier.fillMaxWidth(DRAWER_WIDTH_FRACTION).border(1.dp, MaterialTheme.colorScheme.outline, DrawerShape),
                drawerShape = DrawerShape,
                drawerContainerColor = LocalAppTokens.current.card,
                drawerTonalElevation = 0.dp,
            ) {
                DrawerContent(hostId, linkReady, act)
            }
        },
        content = content,
    )
}

@Composable
private fun DrawerContent(
    hostId: String,
    linkReady: Boolean,
    onAction: (DrawerAction) -> Unit,
) {
    Column(Modifier.fillMaxHeight()) {
        Text(
            DRAWER_TITLE,
            style = MaterialTheme.typography.titleMedium,
            modifier = Modifier.padding(start = 28.dp, end = 16.dp, top = 20.dp, bottom = 12.dp),
        )
        drawerEntries().forEach { entry -> EntryRow(entry) { onAction(entry.item.action()) } }
        HorizontalDivider(Modifier.padding(horizontal = 28.dp, vertical = 8.dp))
        Box(Modifier.weight(1f).fillMaxWidth()) {
            RecentSection(recentEntries(rememberRecent(hostId, linkReady)), onAction)
        }
        HorizontalDivider()
        ServerRow(hostId, onAction)
    }
}

@Composable
private fun EntryRow(
    entry: DrawerEntry,
    onClick: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .height(Touch.rowDrawer)
            .clickable(onClick = onClick)
            .padding(horizontal = 28.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(16.dp),
    ) {
        PhIcon(entry.icon, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
        Text(entry.label, color = MaterialTheme.colorScheme.onSurface, style = MaterialTheme.typography.bodyMedium)
    }
}

/** 「最近」：只列标题，放满那一截为止，不滚动；没数据整段不出。 */
@Composable
private fun RecentSection(
    recents: List<RecentEntry>?,
    onAction: (DrawerAction) -> Unit,
) {
    if (recents == null) return
    BoxWithConstraints(Modifier.fillMaxWidth()) {
        val density = LocalDensity.current
        val fit =
            with(density) {
                recentFitCount(
                    availablePx = maxHeight.roundToPx(),
                    headerPx = RECENT_HEAD_HEIGHT.roundToPx(),
                    rowPx = Touch.rowDrawer.roundToPx(),
                    total = recents.size,
                )
            }
        if (fit == 0) return@BoxWithConstraints
        Column {
            Text(
                RECENT_HEADER,
                style = MaterialTheme.typography.labelLarge,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.height(RECENT_HEAD_HEIGHT).padding(start = 28.dp, top = 8.dp),
            )
            recents.take(fit).forEach { r ->
                Box(
                    Modifier
                        .fillMaxWidth()
                        .height(Touch.rowDrawer)
                        .clickable { onAction(DrawerAction.OpenRecent(r.sessionId)) }
                        .padding(horizontal = 28.dp),
                    contentAlignment = Alignment.CenterStart,
                ) {
                    Text(r.title, maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
    }
}

/** 最底一行：`服务器 · <名字>`（点了 ⇒ 服务器列表）＋ ⚙（点了 ⇒ 设置）。 */
@Composable
private fun ServerRow(
    hostId: String,
    onAction: (DrawerAction) -> Unit,
) {
    val hosts = koinInject<HostRepository>()
    val name by produceState<String?>(null, hostId) { value = hosts.get(hostId)?.label }
    Row(
        Modifier.fillMaxWidth().height(Touch.rowDrawer).padding(end = 8.dp),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        Box(
            Modifier
                .weight(1f)
                .fillMaxHeight()
                .clickable { onAction(DrawerAction.Servers) }
                .padding(start = 28.dp),
            contentAlignment = Alignment.CenterStart,
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(16.dp)) {
                PhIcon(Ph.HardDrives, contentDescription = null, tint = MaterialTheme.colorScheme.onSurfaceVariant)
                Text(serverRowLabel(name), maxLines = 1, overflow = TextOverflow.Ellipsis, style = MaterialTheme.typography.bodyMedium)
            }
        }
        PhIconButton(Ph.GearSix, SETTINGS_DESCRIPTION, onClick = { onAction(DrawerAction.Settings) }, tint = MaterialTheme.colorScheme.onSurfaceVariant)
    }
}

private const val DRAWER_WIDTH_FRACTION = 0.85f

/** 只有外侧那两角圆（`--radius-xl`）。 */
private val DrawerShape = RoundedCornerShape(topEnd = ThemeTokens.RadiusXl, bottomEnd = ThemeTokens.RadiusXl)

/** 「最近」那一行段头的高（不是可点的一行）。 */
private val RECENT_HEAD_HEIGHT = 36.dp
