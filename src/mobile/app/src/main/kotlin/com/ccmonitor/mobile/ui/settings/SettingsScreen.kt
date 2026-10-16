package com.ccmonitor.mobile.ui.settings

import android.content.ActivityNotFoundException
import android.content.Intent
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.SegmentedButton
import androidx.compose.material3.SegmentedButtonDefaults
import androidx.compose.material3.SingleChoiceSegmentedButtonRow
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.alpha
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.Role
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.BuildConfig
import com.ccmonitor.mobile.core.claude.link.Deploying
import com.ccmonitor.mobile.core.claude.link.Fix
import com.ccmonitor.mobile.core.claude.link.LinkState
import com.ccmonitor.mobile.core.claude.link.TabKind
import com.ccmonitor.mobile.core.claude.link.wire.PrefsState
import com.ccmonitor.mobile.core.claude.link.wire.TabPref
import com.ccmonitor.mobile.core.claude.link.wire.UsageTone
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.HostRepository
import com.ccmonitor.mobile.core.data.repo.IdentityRepository
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.icon.BackButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.link.HostBackends
import com.ccmonitor.mobile.ui.chat.newsession.AccountBadge
import com.ccmonitor.mobile.ui.chat.newsession.DefaultTag
import com.ccmonitor.mobile.ui.common.CopyDetailButton
import com.ccmonitor.mobile.ui.common.HostLink
import com.ccmonitor.mobile.ui.common.deployLabel
import com.ccmonitor.mobile.ui.common.rememberDeployPress
import com.ccmonitor.mobile.ui.common.rememberHostLink
import com.ccmonitor.mobile.ui.nav.settingsGroupScreenInsets
import org.koin.compose.koinInject

/** 当前那台（上次那台）：设置屏读写的就是它。 */
data class Here(
    val hostId: String,
    val machine: String,
)

/**
 * 设置屏：账号卡（点开 ⇒ 用量屏）＋ 机器与账号 · 那台上的偏好 · 外观 · 关于。读那台的几格全是回包（[MachineSettings]），手机只数自己的机器表与保险箱。
 * 「机器与账号」那几条住 [settingsEntries]，本屏只渲染。没有当前那台（还没连过）⇒ 账号卡与读那台的几行不画。
 */
@Composable
fun SettingsScreen(
    here: Here?,
    onBack: () -> Unit,
    onNavigate: (String) -> Unit,
    onUsage: () -> Unit,
) {
    Box(Modifier.fillMaxSize().settingsGroupScreenInsets()) {
        Column(Modifier.fillMaxSize()) {
            Row(Modifier.padding(horizontal = 8.dp).heightIn(min = Touch.topBar), verticalAlignment = Alignment.CenterVertically) {
                BackButton(onBack)
                Text(copyText("rsLib.settings.windowTitle"), style = MaterialTheme.typography.titleLarge)
            }
            Column(Modifier.fillMaxSize().verticalScroll(rememberScrollState()).padding(horizontal = 16.dp, vertical = 8.dp)) {
                if (here == null) Body(null, null, null, onNavigate) else MachineBody(here, onNavigate, onUsage)
            }
        }
    }
}

@Composable
private fun MachineBody(
    here: Here,
    onNavigate: (String) -> Unit,
    onUsage: () -> Unit,
) {
    rememberHostLink(here.hostId, "settings-${here.hostId}")
    val backends = koinInject<HostBackends>()
    val feed = remember(here.hostId) { backends.of(here.hostId) }
    val scope = rememberCoroutineScope()
    val board = remember(here.hostId) { MachineSettings(feed, here.machine, feed.changed, scope) }
    val link by feed.state.collectAsState()
    val deploying by feed.deploying.collectAsState()
    AccountCard(here, board, link, deploying, onReconnect = feed::reconnect, onOpen = onUsage)
    Body(here, board, readable(link, deploying), onNavigate)
}

/** 读那台的几行此刻能不能读：离线 · 要装 / 要更新 · 正在装 ⇒ 不能（灰着、不画旧值）。 */
private fun readable(
    link: LinkState,
    deploying: Deploying?,
): Boolean = link !is LinkState.Down && (link as? LinkState.Up)?.fix == null && deploying !is Deploying.Step

@Composable
private fun Body(
    here: Here?,
    board: MachineSettings?,
    readable: Boolean?,
    onNavigate: (String) -> Unit,
) {
    val hosts by koinInject<HostRepository>().observeAll().collectAsState(initial = emptyList())
    val keys by koinInject<IdentityRepository>().observeAll().collectAsState(initial = emptyList())
    SectionHead(copyText("mobSettings.section.machine"))
    settingsEntries().forEach { e ->
        if (e.readsMachine && here == null) return@forEach
        val n =
            when (e.count) {
                EntryCount.MACHINES -> hosts.size
                EntryCount.KEYS -> keys.size
                null -> null
            }
        NavRow(e.icon, e.title, n?.toString(), enabled = !e.readsMachine || readable == true) { onNavigate(e.route) }
    }
    if (here != null && board != null) Prefs(here, board, readable == true)
    SectionHead(copyText("mobSettings.section.look"))
    val context = LocalContext.current
    NavRow(Ph.GearSix, copyText("mobSettings.entry.fontSize"), copyText("mobSettings.entry.fontSystem")) {
        // 字号用系统那一个：打开系统「显示」设置。
        try {
            context.startActivity(Intent(android.provider.Settings.ACTION_DISPLAY_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        } catch (_: ActivityNotFoundException) {
            context.startActivity(Intent(android.provider.Settings.ACTION_SETTINGS).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK))
        }
    }
    SectionHead(copyText("mobSettings.section.about"))
    Text(
        copyText("mobSettings.about.version", "version" to BuildConfig.VERSION_NAME),
        style = MaterialTheme.typography.bodyLarge,
        modifier = Modifier.fillMaxWidth().heightIn(min = Touch.rowSetting).padding(vertical = 16.dp),
    )
}

/**
 * 账号卡：那台的状态点 · 名字；通着 ⇒ 新会话默认号（号牌 · 名 ·「默认」）与它的用量那一格；离线 ⇒ 虚线、空心点、那一句 ＋［重新连接］；
 * 要装 / 要更新 ⇒ 琥珀点、那一句 ＋ 核心点名的那颗按钮（走机器设置同一个部署）；正在装 ⇒ 那一步 ＋ 转圈；没装成 ⇒ 红字 ＋ 复制详情。
 */
@Composable
private fun AccountCard(
    here: Here,
    board: MachineSettings,
    link: LinkState,
    deploying: Deploying?,
    onReconnect: () -> Unit,
    onOpen: () -> Unit,
) {
    val tokens = LocalAppTokens.current
    val ui by board.state.collectAsState()
    val fix = (link as? LinkState.Down)?.fix ?: (link as? LinkState.Up)?.fix
    val step = deploying as? Deploying.Step
    val offline = link is LinkState.Down && fix == null
    val shape = RoundedCornerShape(tokens.radius.m)
    val border = tokens.borderFaint
    val frame =
        if (offline) {
            Modifier.drawBehind {
                val w = 1.dp.toPx()
                drawRoundRect(border, cornerRadius = CornerRadius(tokens.radius.m.toPx()), style = Stroke(w, pathEffect = PathEffect.dashPathEffect(floatArrayOf(DASH * w, DASH * w))))
            }
        } else {
            Modifier.background(tokens.card, shape).border(1.dp, border, shape)
        }
    Column(
        Modifier
            .fillMaxWidth()
            .padding(vertical = 8.dp)
            .then(frame)
            .clickable(role = Role.Button, onClick = onOpen)
            .padding(16.dp)
            .testTag(SettingsTags.CARD),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            CardDot(
                when {
                    step != null || offline -> null
                    fix != null -> tokens.warn
                    link is LinkState.Up -> tokens.success
                    else -> null
                },
            )
            Text(here.machine, style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
        }
        when {
            step != null ->
                Row(verticalAlignment = Alignment.CenterVertically) {
                    Text(step.text, style = MaterialTheme.typography.bodyMedium, modifier = Modifier.weight(1f))
                    CircularProgressIndicator(Modifier.size(SPIN.dp), strokeWidth = 2.dp)
                }
            fix != null -> NeedsFix(here, fix, (link as? LinkState.Down)?.said?.text, deploying)
            offline ->
                CardLine(copyText("newSession.machine.offline", "machine" to here.machine), MaterialTheme.colorScheme.onSurfaceVariant) {
                    SecondaryButton(onClick = onReconnect) { Text(copyText("newSession.machine.reconnect")) }
                }
            else -> DefaultAccount(ui)
        }
    }
}

@Composable
private fun NeedsFix(
    here: Here,
    fix: Fix,
    /** 断着时核心说的那一句（没装 ⇒ 那台没有 cc-monitor 那一句）。 */
    down: String?,
    deploying: Deploying?,
) {
    val deploy = rememberDeployPress(here.hostId)
    val said = if (fix == Fix.UPDATE) copyText("machineState.problem.needsUpdate", "machine" to here.machine) else down.orEmpty()
    CardLine(said, LocalAppTokens.current.warn) {
        Button(onClick = deploy.press, modifier = Modifier.testTag(SettingsTags.DEPLOY)) { Text(deployLabel(fix)) }
    }
    (deploy.held as? HostLink.Failed)?.let { Text(it.why, color = MaterialTheme.colorScheme.error) }
    (deploying as? Deploying.Failed)?.let {
        Text(it.said.text, color = MaterialTheme.colorScheme.error)
        CopyDetailButton(it.said.text, it.said.detail)
    }
}

/** 通着：新会话默认号（号牌 · 名 ·「默认」）＋ 它的用量那一格（色照语气）。还没读到 ⇒ 不画。 */
@Composable
private fun DefaultAccount(ui: MachineSettingsUi) {
    val a = ui.accounts.firstOrNull { it.name == ui.defaultAccount } ?: return
    Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
        AccountBadge(a.plate, a.color)
        Text(a.label, style = MaterialTheme.typography.bodyMedium)
        DefaultTag()
        Box(Modifier.weight(1f))
        ui.usage(a.name)?.let { UsageText(it.text, it.tone) }
    }
}

/** 用量那一格：用满 · 被拒 ⇒ 红；快满 · 超额在兜 ⇒ 琥珀；否则淡字。 */
@Composable
internal fun UsageText(
    text: String,
    tone: UsageTone,
) {
    val tokens = LocalAppTokens.current
    val color =
        when (tone) {
            UsageTone.FAIL -> tokens.errorText
            UsageTone.WARN -> tokens.warn
            UsageTone.PLAIN -> MaterialTheme.colorScheme.onSurfaceVariant
        }
    Text(text, style = MaterialTheme.typography.bodySmall, color = color)
}

@Composable
private fun CardLine(
    text: String,
    color: Color,
    button: @Composable () -> Unit,
) {
    Row(verticalAlignment = Alignment.CenterVertically) {
        Text(text, style = MaterialTheme.typography.bodyMedium, color = color, modifier = Modifier.weight(1f).padding(end = 8.dp))
        button()
    }
}

/** 那台的状态点：[color] 为 `null` ⇒ 空心（离线 · 说不清 · 正在装）。 */
@Composable
private fun CardDot(color: Color?) {
    val m = Modifier.size(LocalAppTokens.current.dotSize)
    if (color == null) Box(m.border(1.dp, MaterialTheme.colorScheme.onSurfaceVariant, CircleShape)) else Box(m.background(color, CircleShape))
}

/**
 * 「{machine} 上的偏好」：子会话 · 分身会话两行分段，拨了马上存（[MachineSettings.setTab]）。
 * 读不懂那份文件 ⇒ 段顶一条红条（那一句 ＋ 复制详情），两行照那台照答的值画、照常能改。读不了那台 ⇒ 灰着、不画选中。
 */
@Composable
private fun Prefs(
    here: Here,
    board: MachineSettings,
    readable: Boolean,
) {
    val ui by board.state.collectAsState()
    SectionHead(copyText("mobSettings.section.prefs", "machine" to here.machine))
    val p = ui.prefs?.takeIf { readable }
    if (p?.state == PrefsState.UNREADABLE) {
        val text = listOfNotNull(copyText("machinePrefs.read.failed"), p.reason).joinToString(copyText("kit.text.sep"))
        Column(
            Modifier
                .fillMaxWidth()
                .padding(vertical = 4.dp)
                .border(1.dp, MaterialTheme.colorScheme.error, RoundedCornerShape(LocalAppTokens.current.radius.m))
                .padding(12.dp),
        ) {
            Text(text, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyMedium)
            CopyDetailButton(text, p.detail)
        }
    }
    if (readable) ui.prefsFailed?.let { Text(it.text, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodySmall) }
    TabKind.entries.forEach { kind ->
        TabRow(kind, if (readable) ui.shown(kind) else null, ui.notSaved[kind]?.takeIf { readable }, enabled = readable && ui.prefs != null) { board.setTab(kind, it) }
    }
}

@Composable
private fun TabRow(
    kind: TabKind,
    shown: TabPref?,
    notSaved: String?,
    enabled: Boolean,
    onPick: (TabPref) -> Unit,
) {
    Row(Modifier.fillMaxWidth().heightIn(min = Touch.rowDouble).alpha(if (enabled) 1f else LocalAppTokens.current.disabledAlpha), verticalAlignment = Alignment.CenterVertically) {
        Column(Modifier.weight(1f).padding(end = 8.dp)) {
            Text(MachineSettings.tabLabel(kind), style = MaterialTheme.typography.bodyLarge)
            Text(MachineSettings.tabTip(kind), style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
            notSaved?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.error) }
        }
        val choices = listOf(TabPref.SHOW to copyText("settings.machine.tabShow"), TabPref.HIDE to copyText("settings.machine.tabHide"))
        SingleChoiceSegmentedButtonRow(Modifier.testTag(SettingsTags.tab(kind))) {
            choices.forEachIndexed { i, (v, label) ->
                SegmentedButton(
                    selected = shown == v,
                    onClick = { onPick(v) },
                    enabled = enabled,
                    shape = SegmentedButtonDefaults.itemShape(i, choices.size),
                    icon = {},
                ) { Text(label) }
            }
        }
    }
}

@Composable
private fun SectionHead(title: String) {
    Text(
        title,
        style = MaterialTheme.typography.labelMedium,
        color = MaterialTheme.colorScheme.onSurfaceVariant,
        modifier = Modifier.fillMaxWidth().padding(top = 16.dp, bottom = 4.dp),
    )
}

/** 一行去处：图标 · 名字 · 行尾的字 · caret（56 dp）。[enabled] 为假 ⇒ 主字灰、点不了。 */
@Composable
internal fun NavRow(
    icon: Ph,
    title: String,
    trail: String? = null,
    enabled: Boolean = true,
    onClick: () -> Unit,
) {
    val faint = MaterialTheme.colorScheme.onSurfaceVariant
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = Touch.rowSetting)
            .clickable(enabled = enabled, role = Role.Button, onClick = onClick),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(12.dp),
    ) {
        val dim = Modifier.alpha(if (enabled) 1f else LocalAppTokens.current.disabledAlpha)
        PhIcon(icon, contentDescription = null, size = Touch.iconInline, tint = faint, modifier = dim)
        Text(title, style = MaterialTheme.typography.bodyLarge, modifier = Modifier.weight(1f).then(dim))
        trail?.let { Text(it, style = MaterialTheme.typography.bodyMedium, color = faint) }
        PhIcon(Ph.CaretRight, contentDescription = null, size = Touch.iconInline, tint = faint)
    }
}

/** 测试标签（冒烟按它点）。 */
object SettingsTags {
    const val CARD = "settings-card"
    const val DEPLOY = "settings-deploy"

    fun tab(kind: TabKind): String = "settings-tab-" + kind.wire
}

private const val DASH = 4f
private const val SPIN = 18
