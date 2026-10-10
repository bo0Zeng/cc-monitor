package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.data.db.ButtonGroups
import com.ccmonitor.mobile.core.data.db.CustomButton
import com.ccmonitor.mobile.core.terminal.escapeSequenceBytes
import java.util.Locale
import java.util.UUID

/** 按钮的五种类型（DB type 值 → 中文短标签）。`switch` 是组切换：载荷为目标组，点击把键排切到该组。 */
internal val BUTTON_TYPES =
    listOf("command" to "命令", "keycode" to "按键码", "escape" to "转义", "builtin" to "内建", "switch" to "切换")

/** 按钮分组（`btnGroup` 值 → 中文短标签），组 id 常量在 core-data [ButtonGroups]。
 * `main` 是按钮栏（`CustomButtonBar`）；`fn`/`alt`/`vim` 是键排的活动组（`ExtraKeysRow` 按活动组渲染，`⇌` 切换）。
 * 这里只是内建组的显示名；在「目标组」里输入新名即可建任意新组。 */
internal val BUTTON_GROUPS =
    listOf(
        ButtonGroups.MAIN to "按钮栏",
        ButtonGroups.FN to "功能键",
        ButtonGroups.ALT to "第二排",
        ButtonGroups.VIM to "vim 键",
    )

/** 长按模式（DB longPressMode 值 → 中文短标签）。none=无 / repeat=连发 / discrete=单发另设载荷。 */
internal val LONG_PRESS_MODES = listOf("none" to "无", "repeat" to "连发", "discrete" to "单发")

/**
 * 按「这个按钮做什么」分类，界面上不露内部 type（command/keycode/escape/builtin/switch）。
 * 「按一个键」把 escape 与 keycode 合成一个从预设里选的选项（[KEY_PRESETS]），不必碰转义序列。
 */
enum class ButtonPurpose(
    val label: String,
    val desc: String,
) {
    TEXT("输入文字", "打一段字并回车执行（如 ls -la）"),
    KEY("按一个键", "方向键 / Esc / Tab / 翻页 / Ctrl+C 等——从预设里选"),
    ACTION("App 动作", "粘贴 / 切模型 / 抓屏 / 标签面板 等内建功能"),
    SWITCH("切到按钮组", "点击换一排按钮（键排切到另一组）"),
    ;

    companion object {
        /** escape/keycode 都归「按一个键」；其余按 type 对应。 */
        fun of(type: String): ButtonPurpose =
            when (type) {
                "command" -> TEXT
                "builtin" -> ACTION
                "switch" -> SWITCH
                else -> KEY
            }
    }
}

/** 一个常用按键预设：界面只显示名字（↑/Esc/Ctrl+C…），底层落成 escape 或 keycode 加修饰。 */
internal data class KeyPreset(
    val label: String,
    val type: String,
    val command: String,
    val ctrl: Boolean = false,
)

internal val KEY_PRESETS: List<KeyPreset> =
    listOf(
        KeyPreset("↑", "escape", "\\e[A"),
        KeyPreset("↓", "escape", "\\e[B"),
        KeyPreset("←", "escape", "\\e[D"),
        KeyPreset("→", "escape", "\\e[C"),
        KeyPreset("Esc", "escape", "\\e"),
        KeyPreset("Tab", "escape", "\\t"),
        KeyPreset("Home", "escape", "\\e[H"),
        KeyPreset("End", "escape", "\\e[F"),
        KeyPreset("PgUp", "escape", "\\e[5~"),
        KeyPreset("PgDn", "escape", "\\e[6~"),
        KeyPreset("Delete", "escape", "\\e[3~"),
        KeyPreset("Ctrl+C", "keycode", "c", ctrl = true),
        KeyPreset("Ctrl+D", "keycode", "d", ctrl = true),
        KeyPreset("Ctrl+Z", "keycode", "z", ctrl = true),
        KeyPreset("Ctrl+L", "keycode", "l", ctrl = true),
        KeyPreset("Ctrl+A", "keycode", "a", ctrl = true),
        KeyPreset("Ctrl+E", "keycode", "e", ctrl = true),
        KeyPreset("Ctrl+R", "keycode", "r", ctrl = true),
        KeyPreset("Ctrl+U", "keycode", "u", ctrl = true),
        KeyPreset("Ctrl+K", "keycode", "k", ctrl = true),
        KeyPreset("Ctrl+W", "keycode", "w", ctrl = true),
    )

/** 编辑对话框的表单状态，由 `remember` 持有，免得向子 composable 透传一长串 state/setter。
 * `internal` 是为了让 [ButtonEditState.build] 能被单测（保 btnGroup 等不变式）。 */
internal class ButtonEditState(
    initial: CustomButton?,
    hostContext: String?,
    fixedGroup: String,
) {
    var label by mutableStateOf(initial?.label.orEmpty())
    var type by mutableStateOf(initial?.type?.takeIf { t -> BUTTON_TYPES.any { it.first == t } } ?: "command")
    var command by mutableStateOf(if (initial?.type == "builtin" || initial?.type == "switch") "" else initial?.command.orEmpty())
    var builtinAction by mutableStateOf(if (initial?.type == "builtin") initial.command else BUILTIN_ACTIONS.first().first)

    // switch 类型的目标组（复用 `command` 列作载荷）。编辑取原 command，新建默认 alt 组。
    var switchTarget by mutableStateOf(if (initial?.type == "switch") initial.command else ButtonGroups.ALT)

    // 每个按钮的长按配置。阈值与速率用字符串态便于文本编辑，build() 时解析回 Int。
    var longPressMode by mutableStateOf(initial?.longPressMode?.takeIf { m -> LONG_PRESS_MODES.any { it.first == m } } ?: "none")
    var longPressCommand by mutableStateOf(initial?.longPressCommand.orEmpty())
    var longPressThresholdMs by mutableStateOf((initial?.longPressThresholdMs ?: 400).toString())
    var longPressRepeatMs by mutableStateOf((initial?.longPressRepeatMs ?: 60).toString())
    var ctrl by mutableStateOf(initial?.ctrl == true)
    var alt by mutableStateOf(initial?.alt == true)
    var shift by mutableStateOf(initial?.shift == true)
    var scopeHost by mutableStateOf(if (initial != null) initial.hostId != null else hostContext != null)

    // 编辑保原组；新建取进入时的组上下文。
    var btnGroup by mutableStateOf(initial?.btnGroup ?: fixedGroup)

    val valid: Boolean
        get() {
            if (label.isBlank()) return false
            val payloadOk =
                when (type) {
                    "builtin" -> true
                    "switch" -> switchTarget.isNotBlank()
                    else -> command.isNotBlank()
                }
            // discrete 长按必须有载荷：否则 command 型长按会发 customButtonBytes("")，即一个裸回车，误提交命令行。
            val longPressOk = type == "switch" || longPressMode != "discrete" || longPressCommand.isNotBlank()
            return payloadOk && longPressOk
        }

    /** 组装完整 [CustomButton]：新建生成 id，编辑保 id/sortOrder/btnGroup；只有 keycode 保留修饰键。 */
    fun build(
        initial: CustomButton?,
        hostContext: String?,
    ): CustomButton {
        val key = type == "keycode"
        val lp = longPressCfg()
        return CustomButton(
            id = initial?.id ?: UUID.randomUUID().toString(),
            hostId = if (scopeHost) hostContext else null,
            label = label.trim(),
            command = payloadCommand(),
            sortOrder = initial?.sortOrder ?: 0,
            ctrl = key && ctrl,
            alt = key && alt,
            shift = key && shift,
            type = type,
            // `btnGroup` 由表单持有，编辑时取原组：Room @Upsert 整行覆写，丢了它会把 fn/vim 键静默搬出键排。
            btnGroup = btnGroup,
            longPressMode = lp.mode,
            longPressCommand = lp.command,
            longPressThresholdMs = lp.threshold,
            longPressRepeatMs = lp.repeat,
        )
    }

    /** 载荷（复用 `command` 列）：builtin 是动作 id，switch 是目标组，其余是命令文本。 */
    private fun payloadCommand(): String =
        when (type) {
            "builtin" -> builtinAction
            "switch" -> switchTarget.trim()
            else -> command.trim()
        }

    /** 解析长按 4 个字段。switch 的长按固定是开设置，这里强制 none；阈值与速率夹在范围内防误填。 */
    private fun longPressCfg(): LongPressCfg {
        val mode = if (type == "switch") "none" else longPressMode
        return LongPressCfg(
            mode = mode,
            command = if (mode == "discrete") longPressCommand.trim() else "",
            threshold = longPressThresholdMs.toIntOrNull()?.coerceIn(100, 2000) ?: 400,
            repeat = longPressRepeatMs.toIntOrNull()?.coerceIn(20, 1000) ?: 60,
        )
    }

    /** 切「做什么」时设默认 type；KEY 若已是 escape/keycode 则保持，否则落到首个预设。 */
    fun applyPurpose(p: ButtonPurpose) {
        type =
            when (p) {
                ButtonPurpose.TEXT -> "command"
                ButtonPurpose.ACTION -> "builtin"
                ButtonPurpose.SWITCH -> "switch"
                ButtonPurpose.KEY -> if (type == "escape" || type == "keycode") type else "escape"
            }
        if (p == ButtonPurpose.KEY && command.isBlank()) applyKeyPreset(KEY_PRESETS.first())
    }

    /** 选一个预设按键：落成 escape 或 keycode 加修饰；标签为空则用预设名。 */
    fun applyKeyPreset(k: KeyPreset) {
        type = k.type
        command = k.command
        ctrl = k.ctrl
        alt = false
        shift = false
        if (label.isBlank()) label = k.label
    }
}

/** [ButtonEditState.build] 的长按字段中间结果。 */
private data class LongPressCfg(
    val mode: String,
    val command: String,
    val threshold: Int,
    val repeat: Int,
)

/**
 * 新建 / 编辑按钮对话框，按钮栏的快速添加与按钮设置屏共用。
 * [initial] 为 null 是新建，非空是编辑（保 id/sortOrder）。[hostContext] 非空时显示作用域切换（全局/本机），null 则只有全局。
 */
@Composable
fun ButtonEditDialog(
    initial: CustomButton?,
    hostContext: String?,
    onDismiss: () -> Unit,
    onConfirm: (CustomButton) -> Unit,
    // 新建按钮所属的组，即进入编辑时的组上下文（组列表 → 进组 → ＋按钮）。对话框里不选组。
    fixedGroup: String,
) {
    val s = remember(initial, fixedGroup) { ButtonEditState(initial, hostContext, fixedGroup) }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text(if (initial == null) "新建按钮" else "编辑按钮") },
        text = { ButtonEditForm(s, hostContext) },
        confirmButton = {
            TextButton(enabled = s.valid, onClick = { onConfirm(s.build(initial, hostContext)) }) {
                Text(if (initial == null) "添加" else "保存")
            }
        },
        dismissButton = { TextButton(onClick = onDismiss) { Text("取消") } },
    )
}

/** 编辑表单：用途 chip、标签、随用途变化的载荷、作用域（有 host 上下文时）、长按、预览。 */
@Composable
private fun ButtonEditForm(
    s: ButtonEditState,
    hostContext: String?,
) {
    Column {
        // 按「这个按钮做什么」选，不露 command/keycode/转义/内建 这些内部类型。
        val purpose = ButtonPurpose.of(s.type)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            ButtonPurpose.entries.forEach { p ->
                FilterChip(selected = purpose == p, onClick = { s.applyPurpose(p) }, label = { Text(p.label) })
            }
        }
        Text(purpose.desc, style = MaterialTheme.typography.bodySmall, color = MaterialTheme.colorScheme.onSurfaceVariant)
        OutlinedTextField(
            value = s.label,
            onValueChange = { s.label = it },
            label = { Text("标签（按钮上显示的字）") },
            singleLine = true,
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Next),
        )
        when (purpose) {
            ButtonPurpose.TEXT ->
                OutlinedTextField(
                    value = s.command,
                    onValueChange = { s.command = it },
                    label = { Text("命令（自动加回车执行）") },
                    singleLine = true,
                )
            ButtonPurpose.KEY -> KeyPresetPicker(s)
            ButtonPurpose.ACTION -> BuiltinActionPicker(selected = s.builtinAction, onSelect = { s.builtinAction = it })
            ButtonPurpose.SWITCH -> SwitchTargetPicker(target = s.switchTarget, onTarget = { s.switchTarget = it })
        }
        if (hostContext != null) {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                FilterChip(selected = !s.scopeHost, onClick = { s.scopeHost = false }, label = { Text("全局") })
                FilterChip(selected = s.scopeHost, onClick = { s.scopeHost = true }, label = { Text("本机") })
            }
        }
        // 长按配置；switch 的长按固定是开设置，这里不显示。
        if (s.type != "switch") LongPressEditor(s)
        Text(buttonPreview(s.type, s.command, s.builtinAction, s.switchTarget), style = MaterialTheme.typography.bodySmall)
    }
}

/** 长按编辑：模式（无/连发/单发）、单发载荷、阈值与速率。连发只需速率，单发只需载荷。 */
@Composable
private fun LongPressEditor(s: ButtonEditState) {
    Column {
        Text("长按", style = MaterialTheme.typography.labelMedium)
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            LONG_PRESS_MODES.forEach { (v, name) ->
                FilterChip(selected = s.longPressMode == v, onClick = { s.longPressMode = v }, label = { Text(name) })
            }
        }
        if (s.longPressMode == "discrete") {
            OutlinedTextField(
                value = s.longPressCommand,
                onValueChange = { s.longPressCommand = it },
                label = { Text("长按载荷（按本按钮类型解释，如 \\e\\e = 两下 Esc）") },
                singleLine = true,
            )
        }
        if (s.longPressMode != "none") {
            Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
                OutlinedTextField(
                    value = s.longPressThresholdMs,
                    onValueChange = { v -> s.longPressThresholdMs = v.filter { it.isDigit() } },
                    label = { Text("触发阈值 ms") },
                    singleLine = true,
                    keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                    modifier = Modifier.weight(1f),
                )
                if (s.longPressMode == "repeat") {
                    OutlinedTextField(
                        value = s.longPressRepeatMs,
                        onValueChange = { v -> s.longPressRepeatMs = v.filter { it.isDigit() } },
                        label = { Text("连发间隔 ms") },
                        singleLine = true,
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Number),
                        modifier = Modifier.weight(1f),
                    )
                }
            }
        }
    }
}

/** 「按一个键」的预设选择器：下拉只显示名字（↑/Esc/Ctrl+C…）；不在预设里的按钮显示「自定义」，可编辑原始转义序列。 */
@Composable
private fun KeyPresetPicker(s: ButtonEditState) {
    var open by remember { mutableStateOf(false) }
    val current = KEY_PRESETS.firstOrNull { it.type == s.type && it.command == s.command && it.ctrl == s.ctrl }
    Box {
        TextButton(onClick = { open = true }) { Text("按键：${current?.label ?: "自定义"} ▾") }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            KEY_PRESETS.forEach { k ->
                DropdownMenuItem(text = { Text(k.label) }, onClick = {
                    s.applyKeyPreset(k)
                    open = false
                })
            }
        }
    }
    if (current == null) {
        OutlinedTextField(
            value = s.command,
            onValueChange = {
                s.command = it
                s.type = "escape"
            },
            label = { Text("自定义转义序列（如 \\e[1;5D）") },
            singleLine = true,
        )
    }
}

/** 组切换按钮的目标组选择：已知组用 chip 快选，也可输入新名建新组。目标组存在 command 列。 */
@Composable
private fun SwitchTargetPicker(
    target: String,
    onTarget: (String) -> Unit,
) {
    Column {
        OutlinedTextField(
            value = target,
            onValueChange = onTarget,
            label = { Text("目标组（点此 ⇌ 切到该组；可输新组名自建）") },
            singleLine = true,
        )
        Row(horizontalArrangement = Arrangement.spacedBy(6.dp)) {
            BUTTON_GROUPS.filter { it.first != ButtonGroups.MAIN }.forEach { (g, name) ->
                FilterChip(selected = target == g, onClick = { onTarget(g) }, label = { Text(name) })
            }
        }
    }
}

/** 载荷输入框的标签随 type 变化。 */
internal fun payloadFieldLabel(type: String): String =
    when (type) {
        "keycode" -> "基准键（c=Ctrl+C；Tab/Enter/Esc/Space）"
        "escape" -> "转义序列（\\e[A、\\x1b、\\t）"
        else -> "命令（回车自动追加）"
    }

/** 实时预览将发送什么：escape 显示 hex，builtin 显示动作名，命令与按键码显示文本。 */
internal fun buttonPreview(
    type: String,
    command: String,
    builtinAction: String,
    switchTarget: String = "",
): String =
    when (type) {
        "escape" -> "将发送字节: " + escapeSequenceBytes(command).joinToString(" ") { "%02X".format(Locale.ROOT, it.toInt() and 0xFF) }
        "builtin" -> "内建动作: " + (BUILTIN_ACTIONS.firstOrNull { it.first == builtinAction }?.second ?: builtinAction)
        "keycode" -> "将发送按键码（基准键 + 修饰）"
        "switch" -> "切换到组: " + (BUTTON_GROUPS.firstOrNull { it.first == switchTarget }?.second ?: switchTarget)
        else -> "将发送: $command⏎"
    }

/** 内建动作下拉选择。 */
@Composable
internal fun BuiltinActionPicker(
    selected: String,
    onSelect: (String) -> Unit,
) {
    var open by remember { mutableStateOf(false) }
    val label = BUILTIN_ACTIONS.firstOrNull { it.first == selected }?.second ?: selected
    Box {
        TextButton(onClick = { open = true }) { Text("内建动作：$label ▾") }
        DropdownMenu(expanded = open, onDismissRequest = { open = false }) {
            BUILTIN_ACTIONS.forEach { (id, name) ->
                DropdownMenuItem(text = { Text(name) }, onClick = {
                    onSelect(id)
                    open = false
                })
            }
        }
    }
}
