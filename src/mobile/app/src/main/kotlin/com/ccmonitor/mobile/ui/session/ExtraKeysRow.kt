package com.ccmonitor.mobile.ui.session

import androidx.compose.foundation.background
import androidx.compose.foundation.gestures.awaitEachGesture
import androidx.compose.foundation.gestures.awaitFirstDown
import androidx.compose.foundation.gestures.waitForUpOrCancellation
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.collectAsState
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberUpdatedState
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.input.pointer.PointerInputChange
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.data.db.ButtonGroups
import com.ccmonitor.mobile.core.data.db.CustomButton
import com.ccmonitor.mobile.core.data.db.CustomButtonDao
import com.ccmonitor.mobile.core.data.db.CustomButtonType
import com.ccmonitor.mobile.core.data.db.LongPressMode
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoBody
import kotlinx.coroutines.withTimeoutOrNull
import org.koin.compose.koinInject

/**
 * 终端 extra-keys 行：横向滚动，渲染当前活动组的 DB 按钮。
 *
 * 整排完全由 DB 驱动，没有硬编码的特殊 chip：组切换是 `switch` 类型（`command` = 目标组），
 * 粘贴与粘性 Ctrl/Alt 是 `builtin` 类型。全部经 [dispatch] 派发；`switch` 的 `onSwitchGroup` 由 SessionScreen
 * 接到 [activeGroup] 的 setter，点它整排就换成目标组。
 *
 * 每个按钮可配长按，按 `CustomButton.longPressMode` 分支（默认 `none` 只认 tap）：
 * - `repeat`：按住到阈值后每 `longPressRepeatMs` 重发 tap（如 `←` 连发）；
 * - `discrete`：按住到 `longPressThresholdMs` 发一次 `longPressCommand`，按本按钮 type 解释。
 * 阈值与速率按按钮单独存。`switch` 按钮长按固定为开按钮设置屏。
 *
 * 注意：组必须进 [key]。`collectAsState` 的 State 不带 key，只换 Flow 会残留上一组的键。
 * `Ctrl`/`Alt` 的点亮态由 [ctrlActive]/[altActive] 按 `command` 绑定（见 [litState]）。
 */
@Composable
fun ExtraKeysRow(
    hostId: String,
    activeGroup: String,
    dispatch: (CustomButton) -> Unit,
    modifier: Modifier = Modifier, // Compose 约定：modifier 首个可选参
    ctrlActive: Boolean = false,
    altActive: Boolean = false,
    onOpenButtonSettings: () -> Unit = {}, // switch 按钮长按开设置屏
) {
    val tokens = LocalAppTokens.current
    val dao = koinInject<CustomButtonDao>()
    val keys by key(hostId, activeGroup) {
        remember { dao.observeGroup(hostId, activeGroup) }.collectAsState(initial = emptyList())
    }
    Row(
        modifier
            .fillMaxWidth()
            .background(tokens.surfaceRaised)
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = tokens.spacing.sm, vertical = tokens.spacing.xs),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
    ) {
        // 活动组的 DB 按钮：tap 按 type 派发，长按按各按钮的 longPressMode。
        // key(b.id)：按 id 匹配 slot，增删/重排不复用别人的 chip 状态。
        keys.forEach { b ->
            key(b.id) {
                // switch 按钮长按开设置屏（固定 DISCRETE，不看存储的 longPressMode）；其余按各按钮配置。
                val isSwitch = CustomButtonType.fromDb(b.type) == CustomButtonType.SWITCH
                val onLong: () -> Unit =
                    if (isSwitch) {
                        onOpenButtonSettings
                    } else {
                        { dispatch(b.copy(command = b.longPressCommand)) } // discrete：长按载荷按本按钮 type 解释
                    }
                KeyChip(
                    label = b.label,
                    active = litState(b, ctrlActive, altActive),
                    onTap = { dispatch(b) },
                    longPressMode = if (isSwitch) LongPressMode.DISCRETE else LongPressMode.fromDb(b.longPressMode),
                    thresholdMs = b.longPressThresholdMs.toLong(),
                    repeatMs = b.longPressRepeatMs.toLong(),
                    onLongPressDiscrete = onLong,
                )
            }
        }
        // 活动组不是 fn 且没有 switch 键（⇌ 指向了没有出口的自建组，或某组的 ⇌ 被删光）时补一个切回 fn 的逃生键，
        // 键排永不会进得去出不来。加载中的瞬时空排会闪一下逃生键，无害。
        if (activeGroup != ButtonGroups.FN && keys.none { CustomButtonType.fromDb(it.type) == CustomButtonType.SWITCH }) {
            key("__escape_to_fn__") {
                KeyChip(label = "⇌", onTap = { dispatch(CustomButton(id = "__escape__", label = "⇌", command = ButtonGroups.FN, type = "switch")) })
            }
        }
    }
}

/**
 * 粘性修饰键（`builtin` + command `toggle_ctrl`/`toggle_alt`）的点亮态；其余按钮一律不点亮。
 */
internal fun litState(
    b: CustomButton,
    ctrlActive: Boolean,
    altActive: Boolean,
): Boolean {
    if (CustomButtonType.fromDb(b.type) != CustomButtonType.BUILTIN) return false
    return when (b.command.trim().lowercase()) {
        "toggle_ctrl" -> ctrlActive
        "toggle_alt" -> altActive
        else -> false
    }
}

@Composable
private fun KeyChip(
    label: String,
    onTap: () -> Unit,
    active: Boolean = false, // 粘性修饰键点亮态
    longPressMode: LongPressMode = LongPressMode.NONE,
    thresholdMs: Long = 400,
    repeatMs: Long = 60,
    onLongPressDiscrete: () -> Unit = {},
) {
    val tokens = LocalAppTokens.current
    Box(
        modifier =
            Modifier
                .clip(RoundedCornerShape(6.dp))
                .background(if (active) tokens.accentSoft else tokens.surfaceHigh)
                .pressGestures(longPressMode, thresholdMs, repeatMs, onTap, onLongPressDiscrete)
                .defaultMinSize(minWidth = 40.dp, minHeight = 36.dp)
                .padding(horizontal = 10.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) {
        Text(text = label, style = monoBody)
    }
}

/**
 * tap 加可配置长按的手势。`none` 只认 tap；`repeat` 按住到阈值后每 [repeatMs] 重发 [onTap]；
 * `discrete` 按住到 [thresholdMs] 发一次 [onLongPressDiscrete]。三种模式下阈值前松手都是普通 tap。
 * 回调经 [rememberUpdatedState] 保持最新，`pointerInput` 只按 mode/阈值/速率重启手势，不随重组重启。
 */
@Composable
private fun Modifier.pressGestures(
    mode: LongPressMode,
    thresholdMs: Long,
    repeatMs: Long,
    onTap: () -> Unit,
    onLongPressDiscrete: () -> Unit,
): Modifier {
    val tap by rememberUpdatedState(onTap)
    val longDiscrete by rememberUpdatedState(onLongPressDiscrete)
    return pointerInput(mode, thresholdMs, repeatMs) {
        awaitEachGesture {
            awaitFirstDown(requireUnconsumed = false)
            if (mode == LongPressMode.NONE) {
                if (waitForUpOrCancellation() != null) tap()
                return@awaitEachGesture
            }
            // 阈值前松手 = 普通 tap；到阈值仍按住 → 长按行为。
            var up: PointerInputChange? = null
            val heldPastThreshold = withTimeoutOrNull(thresholdMs) { up = waitForUpOrCancellation() } == null
            if (!heldPastThreshold) {
                if (up != null) tap() // 干净抬起（非取消）→ tap
                return@awaitEachGesture
            }
            when (mode) {
                LongPressMode.REPEAT -> {
                    tap() // 到阈值即首发
                    while (true) {
                        var u: PointerInputChange? = null
                        val stillHeld = withTimeoutOrNull(repeatMs) { u = waitForUpOrCancellation() } == null
                        if (!stillHeld) break // 松手/取消 → 停
                        tap()
                    }
                }
                LongPressMode.DISCRETE -> {
                    longDiscrete()
                    waitForUpOrCancellation() // 吞掉本次抬起，避免再触发 tap
                }
                LongPressMode.NONE -> {}
            }
        }
    }
}
