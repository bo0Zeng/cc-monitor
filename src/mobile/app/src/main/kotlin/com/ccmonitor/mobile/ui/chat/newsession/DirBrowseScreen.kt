package com.ccmonitor.mobile.ui.chat.newsession

import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material3.Button
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.window.Dialog
import androidx.compose.ui.window.DialogProperties
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import com.ccmonitor.mobile.core.copy.copyText
import com.ccmonitor.mobile.core.ui.theme.monoBody
import com.ccmonitor.mobile.core.ui.theme.monoSmall

/**
 * 浏览屏（全屏）：顶栏 ← ＋「选目录」＋ 机器名；路径一行（↑ 上一级）；只列文件夹；底部路径 ＋「选这个目录」。
 * 点一行 ＝ 进去；长按 ＝ 选中它（淡底，底部写的是它）；← 退出浏览、不改选择。
 */
@Composable
fun DirBrowseScreen(
    browser: DirBrowser,
    machine: String,
    home: String?,
    onPick: (String) -> Unit,
    onDismiss: () -> Unit,
) {
    val s by browser.state.collectAsStateWithLifecycle()
    Dialog(onDismissRequest = onDismiss, properties = DialogProperties(usePlatformDefaultWidth = false, decorFitsSystemWindows = false)) {
        Surface(Modifier.fillMaxSize(), color = MaterialTheme.colorScheme.background) {
            Column(Modifier.fillMaxSize().statusBarsPadding().navigationBarsPadding()) {
                Row(Modifier.fillMaxWidth().heightIn(min = 56.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
                    IconButton(onClick = onDismiss, modifier = Modifier.semantics { contentDescription = copyText("mobKit.act.close") }) { Text(BACK, style = MaterialTheme.typography.titleLarge) }
                    Text(copyText("mobNewSession.dir.browseTitle"), style = MaterialTheme.typography.titleMedium, fontWeight = FontWeight.SemiBold)
                    Text(machine, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(start = 8.dp))
                }
                PathBar(s.path?.let { homeShort(it, home) }.orEmpty(), onUp = browser::up)
                HorizontalDivider()
                s.failed?.let { Text(it, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant, modifier = Modifier.padding(16.dp)) }
                LazyColumn(Modifier.weight(1f).fillMaxWidth()) {
                    items(s.dirs, key = { it.path }) { d ->
                        PickRow(
                            title = d.name,
                            tag = NewSessionTags.ROW,
                            mono = true,
                            selected = d.path == s.selected,
                            onClick = { browser.enter(d.path) },
                            onLongClick = { browser.select(d.path) },
                        )
                    }
                }
                Footer(browser.picked()?.let { homeShort(it, home) }, enabled = browser.picked() != null && !s.loading) { browser.picked()?.let(onPick) }
            }
        }
    }
}

@Composable
private fun PathBar(
    path: String,
    onUp: () -> Unit,
) {
    Row(Modifier.fillMaxWidth().heightIn(min = 48.dp).padding(horizontal = 4.dp), verticalAlignment = Alignment.CenterVertically) {
        IconButton(onClick = onUp, modifier = Modifier.semantics { contentDescription = copyText("mobNewSession.dir.up") }) { Text(UP, style = MaterialTheme.typography.titleMedium) }
        Text(path, style = monoBody, maxLines = 1, overflow = TextOverflow.StartEllipsis)
    }
}

@Composable
private fun Footer(
    path: String?,
    enabled: Boolean,
    onPick: () -> Unit,
) {
    Column(Modifier.fillMaxWidth().padding(horizontal = 16.dp, vertical = 8.dp)) {
        Text(path.orEmpty(), style = monoSmall, color = MaterialTheme.colorScheme.onSurfaceVariant, maxLines = 1, overflow = TextOverflow.StartEllipsis)
        Button(
            onClick = onPick,
            enabled = enabled,
            modifier =
                Modifier
                    .fillMaxWidth()
                    .padding(top = 8.dp)
                    .heightIn(min = 48.dp)
                    .testTag(NewSessionTags.PICK),
        ) {
            Text(copyText("mobNewSession.dir.pick"))
        }
    }
}

private const val BACK = "←"
private const val UP = "↑"
