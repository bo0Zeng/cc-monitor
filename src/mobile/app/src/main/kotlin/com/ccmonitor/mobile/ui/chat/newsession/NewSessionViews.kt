package com.ccmonitor.mobile.ui.chat.newsession

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.AccountsList
import com.ccmonitor.mobile.core.claude.link.RotationStrategies
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.monoBody

/** 供 UI 测试与冒烟定位的 tag（与文案解耦）。 */
object NewSessionTags {
    const val BAR = "ns-bar"
    const val MACHINE = "ns-chip-machine"
    const val AGENT = "ns-chip-agent"
    const val ACCOUNT = "ns-chip-account"
    const val CWD = "ns-chip-cwd"
    const val ROW = "ns-row"
    const val BROWSE = "ns-browse"
    const val PICK = "ns-pick"
}

/** 选项那几个底部抽屉。 */
enum class NewSessionSheet { Machine, Agent, Account, Cwd }

/** 手机上的一台机器（机器抽屉一行）：[up] ＝ 那台常驻流此刻通不通（还没连过 ⇒ `null`）。 */
data class MachineRow(
    val hostId: String,
    val name: String,
    val up: Boolean?,
)

/**
 * 新建会话的选项：顶栏下一排 chip：机器 · agent · 账号（只有一家 / 一个号不画那一颗），下一行工作目录（长路径从左省略）。
 * 发出第一条之后整块不出（[com.ccmonitor.mobile.ui.chat.ChatUiState.drafting]）。
 */
@OptIn(ExperimentalLayoutApi::class)
@Composable
fun NewSessionBar(
    ui: NewSessionUi,
    machine: String,
    onOpen: (NewSessionSheet) -> Unit,
    modifier: Modifier = Modifier,
) {
    Column(modifier.fillMaxWidth().padding(horizontal = 12.dp, vertical = 8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Chip(onClick = { onOpen(NewSessionSheet.Machine) }, tag = NewSessionTags.MACHINE) { Text(machine, style = MaterialTheme.typography.bodyMedium) }
            if (ui.showAgentChip) {
                Chip(onClick = { onOpen(NewSessionSheet.Agent) }, tag = NewSessionTags.AGENT) { Text(ui.agentText.orEmpty(), style = MaterialTheme.typography.bodyMedium) }
            }
            if (ui.showAccountChip) {
                Chip(onClick = { onOpen(NewSessionSheet.Account) }, tag = NewSessionTags.ACCOUNT) {
                    val a = ui.accounts.firstOrNull { it.name == ui.account }
                    a?.let { AccountBadge(it.name, it.color, size = 18) }
                    Text(a?.label ?: ui.account.orEmpty(), style = MaterialTheme.typography.bodyMedium)
                }
            }
        }
        Chip(onClick = { onOpen(NewSessionSheet.Cwd) }, tag = NewSessionTags.CWD, modifier = Modifier.fillMaxWidth()) {
            Text(homeShort(ui.cwd, ui.home), style = monoBody, maxLines = 1, overflow = TextOverflow.StartEllipsis, modifier = Modifier.weight(1f))
        }
        ui.factsFailed?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
    }
}

/** 底部抽屉的外壳（`--card` 底、上两角圆）。 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
internal fun Sheet(
    onDismiss: () -> Unit,
    content: @Composable () -> Unit,
) {
    ModalBottomSheet(onDismissRequest = onDismiss, containerColor = MaterialTheme.colorScheme.surfaceContainer) {
        Column(
            Modifier
                .fillMaxWidth()
                .heightIn(max = 640.dp)
                .verticalScroll(rememberScrollState())
                .padding(bottom = 16.dp),
        ) { content() }
    }
}

/** 机器抽屉：手机上的机器表；每台一颗状态点；离线的照样能选。选另一台 ⇒ 那台上的新建会话。 */
@Composable
fun MachineSheet(
    machines: List<MachineRow>,
    current: String,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) = Sheet(onDismiss) {
    SheetHead(copyText("newSession.label.machine"), null)
    machines.forEach { m ->
        PickRow(
            title = m.name,
            tag = NewSessionTags.ROW,
            second = if (m.up == false) copyText("newSession.machine.down") else null,
            selected = m.hostId == current,
            onClick = { onPick(m.hostId) },
            lead = { MachineDot(m.up) },
        )
    }
}

/** agent 抽屉：那台能起的几家，照注册表序；名字照那台写的。 */
@Composable
fun AgentSheet(
    ui: NewSessionUi,
    machine: String,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) = Sheet(onDismiss) {
    SheetHead(copyText("newSession.label.agent"), machine)
    ui.agents.forEach { a -> PickRow(title = a.text, tag = NewSessionTags.ROW, selected = a.kind == ui.agent, onClick = { onPick(a.kind) }) }
}

/** 工作目录抽屉：「最近的」＋「浏览…」。 */
@Composable
fun CwdSheet(
    ui: NewSessionUi,
    machine: String,
    onPick: (String) -> Unit,
    onBrowse: () -> Unit,
    onDismiss: () -> Unit,
) = Sheet(onDismiss) {
    SheetHead(copyText("newSession.label.cwd"), machine)
    SheetSection(copyText("newSession.recent.label"))
    if (ui.recent.isEmpty()) {
        Text(
            copyText("newSession.recent.none"),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 20.dp, vertical = 8.dp),
        )
    }
    ui.recent.forEach { r ->
        PickRow(
            title = homeShort(r.cwd, ui.home),
            tag = NewSessionTags.ROW,
            mono = true,
            second = if (r.missing) copyText("newSession.cwd.missing") else null,
            selected = r.cwd == ui.cwd && !r.missing,
            enabled = !r.missing,
            onClick = { if (!r.missing) onPick(r.cwd) },
            trail = { Text(r.lastText, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) },
        )
    }
    HorizontalDivider(Modifier.padding(horizontal = 20.dp, vertical = 4.dp))
    PickRow(
        title = copyText("mobNewSession.dir.browse"),
        tag = NewSessionTags.BROWSE,
        onClick = onBrowse,
        trail = { Text(MORE, style = MaterialTheme.typography.titleMedium, color = MaterialTheme.colorScheme.onSurfaceVariant) },
    )
}

/** 账号抽屉：上面账号，下面一节轮换。长按一个号 ⇒「设为默认」。 */
@Composable
fun AccountSheet(
    ui: NewSessionUi,
    machine: String,
    picker: NewSessionPicker,
    onLogin: (String) -> Unit,
    onDismiss: () -> Unit,
) = Sheet(onDismiss) {
    SheetHead(copyText("newSession.label.account"), machine)
    ui.accountsFailed?.let { Note(it) }
    ui.note?.let { Note(it) }
    ui.accounts.forEach { a ->
        AccountRow(a, ui, onPick = {
            picker.pickAccount(a.name)
            onDismiss()
        }, onSetDefault = { picker.setDefault(a.name) }, onLogin = { onLogin(a.name) })
    }
    HorizontalDivider(Modifier.padding(horizontal = 20.dp, vertical = 4.dp))
    SheetSection(copyText("rot.new.label"))
    ui.rotationFailed?.let { Note(it) }
    ui.strategies?.forEach { st ->
        PickRow(
            title = st.id,
            tag = NewSessionTags.ROW,
            second = listOfNotNull(copyText("rot.tag.forNew").takeIf { st.forNew }, RotationStrategies.firstLine(st.description).ifEmpty { null }).joinToString(copyText("kit.text.sep")).ifEmpty { null },
            selected = ui.rotation == st.id,
            onClick = { picker.pickRotation(st.id) },
        )
    }
    if (ui.strategies != null) {
        PickRow(
            title = copyText("rot.none.label"),
            tag = NewSessionTags.ROW,
            second = copyText("rot.tag.forNew").takeIf { ui.forNew == null },
            selected = ui.rotation == null,
            onClick = { picker.pickRotation(null) },
        )
    }
}

@Composable
private fun AccountRow(
    a: AccountsList.Account,
    ui: NewSessionUi,
    onPick: () -> Unit,
    onSetDefault: () -> Unit,
    onLogin: () -> Unit,
) {
    var menu by remember { mutableStateOf(false) }
    Box {
        PickRow(
            title = a.label,
            tag = NewSessionTags.ROW,
            second = if (a.selectable) null else copyText("newSession.account.needLogin"),
            selected = a.name == ui.account,
            enabled = a.selectable,
            onClick = onPick,
            onLongClick = { if (a.selectable && a.name != ui.defaultAccount) menu = true },
            lead = { AccountBadge(a.name, a.color) },
            trail = {
                if (a.name == ui.defaultAccount) DefaultTag()
                if (a.selectable) {
                    ui.quota[a.name]?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                } else {
                    OutlinedButton(onClick = onLogin) { Text(copyText("newSession.account.login")) }
                }
            },
        )
        DropdownMenu(expanded = menu, onDismissRequest = { menu = false }) {
            DropdownMenuItem(text = { Text(copyText("acctPage.row.setDefault")) }, onClick = {
                menu = false
                onSetDefault()
            })
        }
    }
}

/** 「默认」小标（号名旁边那一格）。 */
@Composable
private fun DefaultTag() {
    androidx.compose.material3.Surface(
        shape =
            androidx.compose.foundation.shape
                .RoundedCornerShape(4.dp),
        color = MaterialTheme.colorScheme.surfaceContainerHigh,
    ) {
        Text(
            copyText("newSession.account.default"),
            style = MaterialTheme.typography.labelSmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(horizontal = 6.dp, vertical = 2.dp),
        )
    }
}

@Composable
private fun Note(text: String) =
    Text(
        text,
        style = MaterialTheme.typography.bodySmall,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.padding(horizontal = 20.dp, vertical = 6.dp),
    )
