package com.ccmonitor.mobile.ui.common

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.feedback.ErrorLine
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineFix
import com.ccmonitor.mobile.core.wire.gen.MachineFixButton
import com.ccmonitor.mobile.core.wire.gen.StripTone
import com.ccmonitor.mobile.link.HostBackends
import org.koin.compose.koinInject

/** 机器条的判据按它找。 */
const val MACHINE_BAR_TAG = "machine-bar"

/** 条的样子就是核心给的那一格语气（`deploy::machine::strip`）：忙（灰，装那一段带进度）· 离线（灰虚线框）· 警示（琥珀）· 故障（红）。 */
typealias BarTone = StripTone

/** 机器条那一条要画的：那一句 · 样子 · 核心点名的那一颗修法（没有 ⇒ `null`）· 进度条 · 复制详情。 */
data class BarLook(
    val text: String,
    val tone: StripTone,
    val fix: MachineFixButton? = null,
    val progress: Boolean = false,
    val detail: String = "",
)

/**
 * 核心交的那一行（那台的状态成品，`deploy::machine` 定，桌面同一份）⇒ 机器条：成品不带条上那一格（`strip`）⇒ `null`（不画、不占位）；核心还没交 ⇒ `null`（不猜）。
 * 字 · 按钮字 · 语气都是成品的；手机只照抄，不按态挑样子、不按 SSH 层的通没通自己推，也不自己去抖。[deploying] 装没装上那一趟的结局（失败那一句盖过成品那一句）。
 */
fun barOf(
    line: LinkLine?,
    deploying: Deploying? = null,
): BarLook? {
    val m = line?.machine ?: return null
    val fix = m.fixes.firstOrNull()
    (deploying as? Deploying.Failed)?.let { return BarLook(it.said.text, StripTone.FAULT, fix, detail = it.said.detail) }
    val strip = m.strip ?: return null
    return BarLook(m.said, strip.tone, fix, strip.progress, m.detail.orEmpty())
}

/**
 * 机器条：读一台机器的屏都摆它，放在顶栏下那一格；通着且无事不画、不占位，连上收起不弹提示。
 * 按钮照核心点名的那一颗：［重试］·［连接］⇒ 重接；［安装］·［更新］⇒ 叫核心装（进度就在条里换）；改设置那几种 ⇒ [onEditHost]（不给 ⇒ 不画按钮）。
 */
@Composable
fun MachineBar(
    hostId: String,
    modifier: Modifier = Modifier,
    onEditHost: ((hostId: String) -> Unit)? = null,
) {
    val backends = koinInject<HostBackends>()
    val feed = remember(hostId) { backends.of(hostId) }
    val line by feed.state.collectAsState()
    val deploying by feed.deploying.collectAsState()
    val look = barOf(line, deploying) ?: return
    val deploy = rememberDeployPress(hostId)
    val act: (() -> Unit)? =
        when (look.fix?.fix) {
            null -> null
            MachineFix.RETRY, MachineFix.CONNECT -> feed::reconnect
            MachineFix.INSTALL, MachineFix.UPDATE -> deploy.press
            MachineFix.CONN_SETTINGS, MachineFix.PUSH_KEY, MachineFix.COMPARE_FINGERPRINT -> onEditHost?.let { { it(hostId) } }
        }
    MachineBarView(look, act, modifier)
}

/** [MachineBar] 的样子（只画 [look]）。 */
@Composable
fun MachineBarView(
    look: BarLook,
    onFix: (() -> Unit)?,
    modifier: Modifier = Modifier,
) {
    val m = modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 4.dp).testTag(MACHINE_BAR_TAG)
    val line = MaterialTheme.colorScheme.outline
    Column(if (look.tone == StripTone.OFFLINE) m.dashedFrame(line).padding(horizontal = 14.dp, vertical = 8.dp) else m) {
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                when (look.tone) {
                    StripTone.FAULT -> ErrorLine(look.text)
                    StripTone.WARN -> Text(look.text, style = MaterialTheme.typography.bodyMedium, color = LocalAppTokens.current.warn)
                    StripTone.BUSY, StripTone.OFFLINE -> Text(look.text, style = MaterialTheme.typography.bodyMedium, color = MaterialTheme.colorScheme.onSurfaceVariant)
                }
            }
            val fix = look.fix
            // 正在装 ⇒ 按钮灰着不消失。
            if (fix != null && onFix != null) SecondaryButton(onClick = onFix, enabled = look.tone != StripTone.BUSY) { Text(fix.label) }
            CopyDetailButton(look.text, look.detail)
        }
        if (look.progress) LinearProgressIndicator(Modifier.fillMaxWidth().padding(top = 8.dp))
    }
}
