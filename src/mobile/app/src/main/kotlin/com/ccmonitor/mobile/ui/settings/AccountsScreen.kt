package com.ccmonitor.mobile.ui.settings

import androidx.compose.foundation.background
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
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.Login
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.icon.BackButton
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.wire.gen.AccountRow
import com.ccmonitor.mobile.core.wire.gen.LinkLine
import com.ccmonitor.mobile.core.wire.gen.MachineFix
import com.ccmonitor.mobile.core.wire.gen.MachineFixButton
import com.ccmonitor.mobile.core.wire.gen.StripTone
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.chat.newsession.AccountBadge
import com.ccmonitor.mobile.ui.chat.newsession.DefaultTag
import com.ccmonitor.mobile.ui.chat.newsession.PickRow
import com.ccmonitor.mobile.ui.chat.newsession.Sheet
import com.ccmonitor.mobile.ui.chat.newsession.SheetHead
import com.ccmonitor.mobile.ui.common.BarLook
import com.ccmonitor.mobile.ui.common.MachineBar
import com.ccmonitor.mobile.ui.common.MachineBarView
import com.ccmonitor.mobile.ui.common.rememberHostLink
import com.ccmonitor.mobile.ui.login.LoginFlow
import com.ccmonitor.mobile.ui.login.LoginSheet
import com.ccmonitor.mobile.ui.nav.settingsGroupScreenInsets
import org.koin.compose.koinInject

/**
 * 账号一览（设置 → 账号）：那台账号库里的号，每号一行（号牌 · 名 ·「默认」/ 要人动手的那一枚 · 类型；右边用量那一格）。
 * 点一个号升起底部抽屉：登录（选不了的号）· 设为默认（照 `selectable` 灰着）· 默认模型（再升一层：跟随 ＋ 那一家的清单）。
 * 新建、删除账号留在电脑上。
 */
@Composable
fun AccountsScreen(
    here: Here,
    onBack: () -> Unit,
    onUsage: () -> Unit,
) {
    rememberHostLink(here.hostId, "accounts-${here.hostId}")
    val backends = koinInject<HostBackends>()
    val feed = remember(here.hostId) { backends.of(here.hostId) }
    val scope = rememberCoroutineScope()
    val board = remember(here.hostId) { MachineSettings(feed, here.machine, feed.changed, scope) }
    val ui by board.state.collectAsState()
    val line by feed.state.collectAsState()
    var open by remember { mutableStateOf<String?>(null) }
    var model by remember { mutableStateOf<String?>(null) }
    var login by remember { mutableStateOf<LoginFlow?>(null) }
    Box(Modifier.fillMaxSize().settingsGroupScreenInsets()) {
        Column(Modifier.fillMaxSize()) {
            AccountsHead(here, onBack)
            // 顶栏下那一格：那台的连接态（离线 · 正在连 · 要更新 …），各屏同一条。
            MachineBar(here.hostId)
            val look = accountsLook(ui, line)
            look.failed?.let { MachineBarView(it, board::reloadAccounts) }
            Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(vertical = 8.dp)) {
                Text(
                    copyText("mobAccounts.list.section", "machine" to here.machine),
                    style = MaterialTheme.typography.labelMedium,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                )
                ui.note?.let {
                    Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall, modifier = Modifier.padding(horizontal = 16.dp))
                }
                if (look.loading) repeat(SKELETON_ROWS) { SkeletonRow() }
                look.rows.forEach { a -> AccountLine(a, a.name == ui.defaultAccount, ui, look.stale) { open = a.name } }
            }
        }
    }
    ui.accounts.firstOrNull { it.name == open }?.let { a ->
        AccountSheet(a, a.name == ui.defaultAccount, ui, onDismiss = { open = null }, onLogin = {
            open = null
            login = LoginFlow(feed, here.hostId, here.machine, Login.account(ui.agent?.kind.orEmpty(), a.name), scope).also { it.start() }
        }, onSetDefault = {
            open = null
            board.setDefault(a.name)
        }, onModel = {
            open = null
            board.loadModels()
            model = a.name
        }, onUsage = {
            open = null
            onUsage()
        })
    }
    ui.accounts.firstOrNull { it.name == model }?.let { a ->
        ModelSheet(a, ui, onDismiss = { model = null }) { v ->
            model = null
            board.setModel(a.name, v)
        }
    }
    login?.let { f -> LoginFor(f, ui, here.machine) { login = null } }
}

/** 登录那一张：号牌与色照此刻在登的那一号（登录流里换了号就跟着换）。 */
@Composable
private fun LoginFor(
    f: LoginFlow,
    ui: MachineSettingsUi,
    machine: String,
    onClosed: () -> Unit,
) {
    val step by f.state.collectAsState()
    val row = ui.accounts.firstOrNull { it.name == step.target.account }
    LoginSheet(f, row?.plate, row?.color, machine, onClosed = onClosed)
}

/** 顶栏：← · 账号 · 那台的名字。 */
@Composable
private fun AccountsHead(
    here: Here,
    onBack: () -> Unit,
) {
    Row(Modifier.padding(horizontal = 8.dp).heightIn(min = Touch.topBar), verticalAlignment = Alignment.CenterVertically) {
        BackButton(onBack)
        Column {
            Text(copyText("mobSettings.entry.accounts"), style = MaterialTheme.typography.titleLarge)
            Text(here.machine, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
    }
}

/** 账号屏此刻画哪几样：读中（骨架）· 行（手上那一份）· 行是不是上次所知（那台离线：用量格不出数）· 读不出那一条（那台通着时）。 */
data class AccountsLook(
    val loading: Boolean,
    val rows: List<AccountRow>,
    val stale: Boolean,
    val failed: BarLook?,
)

/**
 * 号列表那一问的结局 ＋ 核心交的那一行 ⇒ 账号屏（同用量屏）：离线那一句在机器条上（[MachineBar]），这里不另出红字；
 * 行照手上那一份（这一屏开着时答过的，没有 ⇒ 没有行，不画假行）。
 */
fun accountsLook(
    ui: MachineSettingsUi,
    line: LinkLine?,
): AccountsLook {
    val down = line is LinkLine.Down
    val failed =
        ui.accountsFailed?.takeIf { !down }?.let {
            BarLook(it.text, StripTone.FAULT, MachineFixButton(MachineFix.RETRY, copyText("front.act.retry")), detail = it.detail)
        }
    return AccountsLook(loading = !down && !ui.accountsRead && ui.accountsFailed == null, rows = ui.accounts, stale = down, failed = failed)
}

/** 读中那一行骨架（两条灰块，不转圈）。 */
@Composable
private fun SkeletonRow() {
    val c = MaterialTheme.colorScheme.surfaceContainerHigh
    val shape = RoundedCornerShape(LocalAppTokens.current.radius.s)
    Column(Modifier.fillMaxWidth().heightIn(min = Touch.rowDouble).padding(horizontal = 16.dp, vertical = 12.dp)) {
        Box(Modifier.fillMaxWidth(SKELETON_TITLE).height(12.dp).background(c, shape))
        Box(
            Modifier
                .padding(top = 8.dp)
                .fillMaxWidth(SKELETON_SUB)
                .height(10.dp)
                .background(c, shape),
        )
    }
}

/** 一个号一行：号牌 · 名 ·「默认」/ 要人动手的那一枚 · 类型；右边用量那一格（色照语气；上次所知 ⇒「—」）。 */
@Composable
private fun AccountLine(
    a: AccountRow,
    isDefault: Boolean,
    ui: MachineSettingsUi,
    stale: Boolean,
    onClick: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Touch.rowDouble)
            .clickable(role = Role.Button, onClick = onClick)
            .padding(horizontal = 16.dp, vertical = 8.dp)
            .testTag(AccountsTags.ROW),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        AccountBadge(a.plate, a.color)
        Column(Modifier.weight(1f)) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                Text(a.label, style = MaterialTheme.typography.bodyLarge, fontWeight = FontWeight.SemiBold)
                if (isDefault) DefaultTag()
                if (a.badge.warn) WarnTag(a.badge.text)
            }
            Text(kindText(a.authKind), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        }
        if (stale) {
            Text(copyText("acct.val.none"), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        } else {
            ui.usage(a.name)?.let { UsageText(it.text, it.tone) }
        }
    }
}

/** 要人动手的那一枚（需登录 · 没配端点 …，那台写的字）：琥珀。 */
@Composable
private fun WarnTag(text: String) {
    val warn = LocalAppTokens.current.warn
    Text(
        text,
        style = MaterialTheme.typography.labelSmall,
        color = warn,
        modifier = Modifier.background(warn.copy(alpha = TAG_BG), RoundedCornerShape(4.dp)).padding(horizontal = 6.dp, vertical = 2.dp),
    )
}

/** 号的类型（订阅 · 按量；`authKind` 是那台账号库里的事实）。 */
private fun kindText(authKind: String): String = if (authKind == API_KEY) copyText("acct.kind.api") else copyText("acct.kind.sub")

@Composable
private fun AccountSheet(
    a: AccountRow,
    isDefault: Boolean,
    ui: MachineSettingsUi,
    onDismiss: () -> Unit,
    onLogin: () -> Unit,
    onSetDefault: () -> Unit,
    onModel: () -> Unit,
    onUsage: () -> Unit,
) {
    Sheet(onDismiss = onDismiss) {
        Row(
            Modifier.fillMaxWidth().padding(start = 20.dp, end = 20.dp, top = 4.dp, bottom = 8.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(8.dp),
        ) {
            AccountBadge(a.plate, a.color)
            Text(a.label, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        }
        if (!a.selectable) PickRow(copyText("newSession.account.login"), AccountsTags.LOGIN, second = a.unselectable, onClick = onLogin)
        if (!isDefault) {
            PickRow(
                copyText("acctPage.row.setDefault"),
                AccountsTags.SET_DEFAULT,
                second = a.unselectable,
                enabled = a.selectable,
                onClick = { if (a.selectable) onSetDefault() },
            )
        }
        PickRow(copyText("acctPage.detail.model"), AccountsTags.MODEL, second = modelText(ui, ui.model(a.name)), onClick = onModel)
        PickRow(copyText("mobUsage.page.title"), AccountsTags.USAGE, second = ui.usage(a.name)?.text, onClick = onUsage)
    }
}

/** 默认模型那一层：第一项「跟随」（那一家的默认），再是那一家的清单；勾在那台存的那一条上。 */
@Composable
private fun ModelSheet(
    a: AccountRow,
    ui: MachineSettingsUi,
    onDismiss: () -> Unit,
    onPick: (String?) -> Unit,
) {
    val now = ui.model(a.name)
    Sheet(onDismiss = onDismiss) {
        SheetHead(copyText("acctPage.detail.model") + copyText("kit.text.sep") + a.label, null)
        PickRow(
            copyText("mobAccounts.model.follow"),
            AccountsTags.MODEL_PICK,
            second = ui.agent?.let { copyText("acctPage.detail.modelDefault", "agent" to it.text) },
            selected = now == null,
            onClick = { onPick(null) },
        )
        ui.agent?.models?.forEach { m -> PickRow(m.text, AccountsTags.MODEL_PICK, selected = now == m.value, onClick = { onPick(m.value) }) }
    }
}

/** 默认模型那一行的第二行：跟随 ⇒「跟随」；设了 ⇒ 那一家清单里那一项的字（清单里没有 ⇒ 原值）。 */
private fun modelText(
    ui: MachineSettingsUi,
    value: String?,
): String =
    value?.let { v ->
        ui.agent
            ?.models
            ?.firstOrNull { it.value == v }
            ?.text ?: v
    } ?: copyText("mobAccounts.model.follow")

/** 测试标签（冒烟按它点）。 */
object AccountsTags {
    const val ROW = "accounts-row"
    const val LOGIN = "accounts-login"
    const val SET_DEFAULT = "accounts-set-default"
    const val MODEL = "accounts-model"
    const val MODEL_PICK = "accounts-model-pick"
    const val USAGE = "accounts-usage"
}

private const val API_KEY = "api-key"
private const val TAG_BG = 0.12f
private const val SKELETON_ROWS = 3
private const val SKELETON_TITLE = 0.5f
private const val SKELETON_SUB = 0.3f
