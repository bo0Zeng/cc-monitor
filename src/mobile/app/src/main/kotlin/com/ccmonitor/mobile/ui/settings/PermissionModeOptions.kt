package com.ccmonitor.mobile.ui.settings

import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.command.ClaudeInvocation

/**
 * `claude` 自己的默认档的真名。
 *
 * `claude` 的 `permissionMode` 真值有 6 个（`default` · `plan` · `acceptEdits` · `bypassPermissions` ·
 * `dontAsk` · `auto`），归一化时把 `manual` 当 `default` 的别名，`default` 的显示名是「Manual」。
 * 这是读 `claude` 二进制得来的，不是公开契约。
 */
const val UPSTREAM_DEFAULT_PERMISSION_MODE = "default"

/**
 * 这边选的那个字，到了 `claude` 实际上是哪一档。
 *
 * - `null`（没选）：[ClaudeInvocation.permissionModeFlag] 回空串，命令里不带 `--permission-mode`，
 *   跑的就是 [UPSTREAM_DEFAULT_PERMISSION_MODE]。
 * - 不在 [ClaudeInvocation.PERMISSION_MODES] 白名单里的值：同样 fail-closed 回空串，落在默认档。
 * - `manual`：`claude` 把它当 [UPSTREAM_DEFAULT_PERMISSION_MODE] 的别名。
 */
fun effectivePermissionMode(stored: String?): String {
    val whitelisted = ClaudeInvocation.PERMISSION_MODES.firstOrNull { it == stored }
    return if (whitelisted == null || whitelisted == "manual") UPSTREAM_DEFAULT_PERMISSION_MODE else whitelisted
}

/**
 * 「新对话的默认权限」那一排上的一档。
 *
 * @property value 传给 [ClaudeInvocation.permissionModeFlag] 的那个字。
 * @property title 上屏的档名。
 * @property subtitle 上屏的副标题。
 */
data class PermissionModeOption(
    val value: String,
    val title: String,
    val subtitle: String,
)

/**
 * 三档单选，没有单独的「默认」键：不选时跑的就是第一档（`manual` 与 `default` 同义），
 * 「不选也是它」写在第一档的副标题里。
 *
 * 只列起对话时带的参数能选的三档；对话里还能切到哪几档由那台机器上的 `claude` 定，
 * 另用 [PERMISSION_MODE_ELSEWHERE_NOTE] 明说。
 *
 * 条数跟着白名单 [ClaudeInvocation.PERMISSION_MODES] 走，这里只管每个字怎么说人话，免得两份漂移。
 * `bypassPermissions` 刻意不给。
 */
val PERMISSION_MODE_OPTIONS: List<PermissionModeOption> =
    ClaudeInvocation.PERMISSION_MODES.map { mode ->
        PermissionModeOption(value = mode, title = permissionModeTitle(mode), subtitle = permissionModeSubtitle(mode))
    }

/**
 * 屏上该高亮哪一档：现在真的会传什么，落在这三档的哪一档上。
 *
 * 没选过（`null`）时高亮第一档，这是真话：不带参数跑的就是那一档（见 [effectivePermissionMode]）。
 *
 * @return 对不上任何一档时回 `null`，屏上一档都不亮：未知值退到「说不清」那一侧。
 */
fun selectedPermissionModeValue(stored: String?): String? {
    val effective = effectivePermissionMode(stored)
    return PERMISSION_MODE_OPTIONS.firstOrNull { effectivePermissionMode(it.value) == effective }?.value
}

/**
 * 这一屏管得着的只有起对话那一刻带的参数。对话起来之后还能当场换档，
 * 能换到哪几档由那台机器上的 `claude` 定，这里管不着。
 *
 * 放在常量里而不是内联在屏上，词表测试才扫得到。
 */
const val PERMISSION_MODE_ELSEWHERE_NOTE: String =
    "这里设的是开始一段新对话时带的那一档。开起来之后在那台机器上还能当场换 —— " +
        "能换到哪几档由那台上的 Claude 自己定，这里管不着，也不假装管得着。"

/** 这一排的小标题。 */
const val PERMISSION_MODE_SECTION_TITLE = "新对话的默认权限"

/**
 * 一档那一行的高度下界：Material3 的最小触控目标 48dp。
 *
 * 写成常量而不是靠 `padding` 撑：`padding` 撑不出下界，字号一小行就矮。
 */
val PERMISSION_MODE_ROW_MIN_HEIGHT: Dp = 48.dp

/**
 * 档名：说差别，不说那个字本身。
 *
 * 不认识的值原样回去：白名单多了一档而这里忘了加说法时，屏上出现裸字，难看但不说假话。
 */
private fun permissionModeTitle(mode: String): String =
    when (mode) {
        "manual" -> "每一步都问我"
        "acceptEdits" -> "改文件不用问"
        "plan" -> "只出方案，什么都不动"
        else -> mode
    }

/** 副标题。第一档那句说明「不选也是它」。 */
private fun permissionModeSubtitle(mode: String): String =
    when (mode) {
        "manual" -> "不选也是它。在那台机器上这一档叫 Manual"
        "acceptEdits" -> "改文件不再问；别的要紧动作照样问"
        "plan" -> "先说清打算怎么做，一个字都不写"
        else -> ""
    }
