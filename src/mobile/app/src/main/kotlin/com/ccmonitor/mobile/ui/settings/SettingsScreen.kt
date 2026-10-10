package com.ccmonitor.mobile.ui.settings

import android.widget.Toast
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.RadioButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.testTag
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.transport.ClaudePaths
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.ui.nav.settingsGroupScreenInsets
import org.koin.androidx.compose.koinViewModel

/**
 * 应用级设置屏：默认用哪个号、新对话的权限，以及终端那一节的入口。
 * 入口不写在本文件里，住 [settingsEntries]，本屏只负责渲染。
 *
 * @param onNavigate 去某条路由。没有默认值：漏接就等于那些入口全成了死键，「点了没反应」比「没有那个键」更糟。
 */
@Composable
fun SettingsScreen(
    onBack: () -> Unit,
    onNavigate: (String) -> Unit,
) {
    val vm = koinViewModel<SettingsViewModel>()
    val current by vm.defaultClaudeDir.collectAsState()
    val context = LocalContext.current
    val tokens = LocalAppTokens.current
    var text by remember(current) { mutableStateOf(current ?: "") } // 预填当前值（Flow 载入后重填一次）

    // 必须能滚：不滚的话「终端」那一节落在屏外，对用户等于被删了。加节之后要在真机上再看一遍。
    // 窗口内边距垫在最外层、`verticalScroll` 的外面：键盘弹起时缩的是可滚视口；
    // 垫在滚动之后只是在内容里加一段空白，焦点那个框照样在键盘底下。
    // 多包一层 `Box`：ktlint 的 `chain-method-continuation` 到第 4 个 `.` 就要换行，
    // 而 `fillMaxSize()` 与 `verticalScroll(...)` 要留在同一行。
    Box(Modifier.fillMaxSize().settingsGroupScreenInsets()) {
        Column(
            Modifier.fillMaxSize().padding(16.dp).verticalScroll(rememberScrollState()),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically, horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                TextButton(onClick = onBack) { Text("← 返回") }
                Text("设置", style = MaterialTheme.typography.titleLarge)
            }
            Text("应用级默认 Claude 配置目录", style = MaterialTheme.typography.titleMedium)
            OutlinedTextField(
                value = text,
                onValueChange = { text = it },
                // 标签不说 `claudeDir` / 配置目录这类实现词。
                label = { Text("默认用哪个号") },
                placeholder = { Text("留空 = 内置默认 ${ClaudePaths.DEFAULT_CLAUDE_DIR}") }, // 引用常量，防漂移
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )
            // 说明只说两件事：没单独指定的服务器都用这个号；改完对下一段新对话生效。
            Text(
                "没有单独指定过的服务器，都用这里填的这个号。改完之后，下一段新开始的对话才算。",
                style = MaterialTheme.typography.bodySmall,
                color = tokens.textFaint,
            )
            Button(onClick = {
                vm.save(text)
                Toast.makeText(context, "已保存", Toast.LENGTH_SHORT).show()
            }) { Text("保存") }
            PermissionModeSection(vm, tokens.textFaint)
            // 终端那一节：每一行都从 [settingsEntries] 来，本文件不写死任何一条。
            EntrySections(onNavigate)
        }
    }
}

/**
 * 把 [settingsEntries] 画出来，这里一条都不写死。
 *
 * 分节按 [SettingsEntry.section] 保序分组（`groupBy` 保留首次出现顺序），不按字典序：节的先后是设计定的。
 */
@Composable
private fun EntrySections(onNavigate: (String) -> Unit) {
    settingsEntries().groupBy { it.section }.forEach { (section, entries) ->
        Text(section, style = MaterialTheme.typography.titleMedium)
        entries.forEach { entry ->
            TextButton(onClick = { onNavigate(entry.route) }) { Text(entry.title) }
        }
    }
}

/**
 * 新对话的权限模式。
 *
 * 权限判定全在 Claude 那边，aterm 只把选择传下去，所以文案不声称一个守不住的边界，不能让人以为这是沙箱。
 * 哪三档、各自怎么说、现在落在哪一档，全部在 [PERMISSION_MODE_OPTIONS] 那份数据里。选中用真的单选件。
 */
@Composable
private fun PermissionModeSection(
    vm: SettingsViewModel,
    faint: Color,
) {
    val current by vm.permissionMode.collectAsState()
    val selected = selectedPermissionModeValue(current)

    Text(PERMISSION_MODE_SECTION_TITLE, style = MaterialTheme.typography.titleMedium)
    Column(Modifier.fillMaxWidth().testTag(TAG_PERMISSION_MODE)) {
        PERMISSION_MODE_OPTIONS.forEach { option ->
            PermissionModeRow(option, option.value == selected) { vm.setPermissionMode(option.value) }
        }
    }
    Text(
        "权限判定由远端那个 Claude 自己做，aterm 只是把这里的选择传过去 —— 它不是沙箱，也不声称是。" +
            "「完全跳过确认」这一档刻意不提供（官方手机端也不给）。",
        style = MaterialTheme.typography.bodySmall,
        color = faint,
    )
    // 这一屏只管起对话那一刻带的参数，另一半明说出来（见那个常量的头注）。
    Text(PERMISSION_MODE_ELSEWHERE_NOTE, style = MaterialTheme.typography.bodySmall, color = faint)
}

/**
 * 一档，整行可点（与 `HostScreens` 里挑钥匙那一排同一写法）。
 *
 * 行高下界显式写成 [PERMISSION_MODE_ROW_MIN_HEIGHT]：`padding` 撑不出下界，字号一小行就矮。
 */
@Composable
private fun PermissionModeRow(
    option: PermissionModeOption,
    selected: Boolean,
    onSelect: () -> Unit,
) {
    Row(
        Modifier
            .fillMaxWidth()
            .heightIn(min = PERMISSION_MODE_ROW_MIN_HEIGHT)
            .clickable(onClick = onSelect),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        RadioButton(selected = selected, onClick = onSelect)
        Column(Modifier.padding(vertical = 4.dp)) {
            Text(option.title, style = MaterialTheme.typography.bodyLarge)
            Text(option.subtitle, style = MaterialTheme.typography.bodySmall, color = LocalAppTokens.current.textFaint)
        }
    }
}

const val TAG_PERMISSION_MODE = "settings-permission-mode"
