package com.ccmonitor.mobile.ui.chat.newsession

import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.link.HostBackends
import org.koin.compose.koinInject

/**
 * 新建会话那一块（能用门槛稿 §1–3）：选项那两排 chip ＋ 它们开的几个抽屉 ＋ 浏览屏 ＋ 登录抽屉。
 * 选项的事实与点过的住 [picker]（随这张草稿、应用级）；抽屉开着哪一个只是这一屏的事。换机器 ⇒ [onSwitchMachine]（那台上的新建会话）。
 */
@Composable
fun NewSessionPane(
    hostId: String,
    machine: String,
    picker: NewSessionPicker,
    onSwitchMachine: (String) -> Unit,
    modifier: Modifier = Modifier,
) {
    val ui by picker.state.collectAsStateWithLifecycle()
    var sheet by remember { mutableStateOf<NewSessionSheet?>(null) }
    var browsing by remember { mutableStateOf(false) }
    var login by remember { mutableStateOf<LoginFlow?>(null) }
    val backends = koinInject<HostBackends>()
    val scope = rememberCoroutineScope()
    val browser = remember(hostId) { DirBrowser(backends.of(hostId), machine, scope) }

    NewSessionBar(ui, machine, onOpen = { s ->
        if (s == NewSessionSheet.Account) picker.openAccounts()
        sheet = s
    }, modifier = modifier.testTag(NewSessionTags.BAR))

    val close = { sheet = null }
    when (sheet) {
        NewSessionSheet.Machine ->
            MachineSheet(rememberMachines(), hostId, onPick = { id ->
                close()
                if (id != hostId) onSwitchMachine(id)
            }, onDismiss = close)
        NewSessionSheet.Agent ->
            AgentSheet(ui, machine, onPick = {
                picker.pickAgent(it)
                close()
            }, onDismiss = close)
        NewSessionSheet.Account ->
            AccountSheet(ui, machine, picker, onLogin = { name ->
                close()
                login = LoginFlow(backends.of(hostId), machine, ui.agent.orEmpty(), name, scope).also { it.start() }
            }, onDismiss = close)
        NewSessionSheet.Cwd ->
            CwdSheet(ui, machine, onPick = { picker.pickRecent(it) { picked -> if (picked) close() } }, onBrowse = {
                close()
                browser.open(ui.cwd)
                browsing = true
            }, onDismiss = close)
        null -> Unit
    }
    if (browsing) {
        DirBrowseScreen(browser, machine, ui.home, onPick = {
            picker.pickDir(it)
            browsing = false
        }, onDismiss = { browsing = false })
    }
    login?.let { f ->
        val target by f.state.collectAsStateWithLifecycle()
        val color = ui.accounts.firstOrNull { it.name == target.account }?.color
        LoginSheet(f, color, machine, onClosed = {
            login = null
            picker.openAccounts()
        })
    }
}

/** 手机上的机器表，每台带那台常驻流此刻通不通（还没连过 ⇒ 不画点）。 */
@Composable
private fun rememberMachines(): List<MachineRow> {
    val hosts by koinInject<HostRepository>().observeAll().collectAsState(initial = emptyList())
    val backends = koinInject<HostBackends>()
    return hosts.map { h ->
        key(h.id) {
            val st by backends.of(h.id).state.collectAsState()
            MachineRow(h.id, h.label, upOf(st))
        }
    }
}

/** 通着 ⇒ 真；断着 ⇒ 假；在接 · 还没接过 ⇒ 说不出（不画点）。 */
internal fun upOf(s: LinkState): Boolean? =
    when (s) {
        is LinkState.Up -> true
        is LinkState.Down -> false
        LinkState.Connecting -> null
    }
