package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.AssistChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalClipboard
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.Profiles
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.data.repo.MovedLaunch
import com.ccmonitor.mobile.core.ui.button.SecondaryButton
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.Touch
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.ui.copyPlainText
import kotlinx.coroutines.launch

/** 终端屏「用哪个启动」那一排：这台的别名（名字照那台给的），点一个 ⇒ 这个 shell 里敲那台写好的那一行。 */
@Composable
internal fun LaunchAliasRow(
    names: List<Profiles.Alias>,
    onPick: (Profiles.Alias) -> Unit,
) {
    if (names.isEmpty()) return
    Row(
        Modifier.fillMaxWidth().horizontalScroll(rememberScrollState()).padding(horizontal = 8.dp, vertical = 4.dp),
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        names.forEach { a ->
            AssistChip(
                onClick = { onPick(a) },
                label = { Text(a.name, style = monoSmall) },
                leadingIcon = { PhIcon(Ph.Play, contentDescription = null, size = Touch.iconInline) },
            )
        }
    }
}

/**
 * 迁移说明（每台只出一次）：手机原来存的启动配置原样列出、可复制；带命令的给「建这条」（在那台建同名别名；建不建得了由那台判，不成照那台那一句说）。
 * 关掉 ⇒ 这一台的不再出。
 */
@Composable
internal fun MovedLaunchDialog(
    entries: List<MovedLaunch>,
    onMake: (MovedLaunch) -> Unit,
    onDone: () -> Unit,
) {
    val clipboard = LocalClipboard.current
    val scope = rememberCoroutineScope()
    var made by remember { mutableStateOf(emptySet<MovedLaunch>()) }
    AlertDialog(
        onDismissRequest = onDone,
        title = { Text(copyText("mobLaunch.migrate.title")) },
        text = {
            Column(
                Modifier.fillMaxWidth().heightIn(max = 460.dp).verticalScroll(rememberScrollState()),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                Text(copyText("mobLaunch.migrate.body"), style = MaterialTheme.typography.bodyMedium)
                entries.forEach { m ->
                    Column(verticalArrangement = Arrangement.spacedBy(2.dp)) {
                        Text(m.label, style = MaterialTheme.typography.titleSmall)
                        m.shownLines().forEach { Text(it, style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant) }
                        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                            SecondaryButton(onClick = { scope.launch { clipboard.copyPlainText(m.clipText()) } }) {
                                Text(copyText("mobLaunch.migrate.copy"))
                            }
                            if (!m.command.isNullOrBlank() && m !in made) {
                                SecondaryButton(onClick = {
                                    made = made + m
                                    onMake(m)
                                }) { Text(copyText("mobLaunch.migrate.make")) }
                            }
                        }
                    }
                }
            }
        },
        confirmButton = { SecondaryButton(onClick = onDone) { Text(copyText("mobKit.act.close")) } },
    )
}
