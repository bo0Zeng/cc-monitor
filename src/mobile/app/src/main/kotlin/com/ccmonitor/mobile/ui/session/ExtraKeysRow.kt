package com.ccmonitor.mobile.ui.session

import android.view.ViewConfiguration
import androidx.compose.foundation.background
import androidx.compose.foundation.border
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
import androidx.compose.material3.MaterialTheme
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
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyDao
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.cap
import com.ccmonitor.mobile.core.data.db.isRepeatable
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import kotlinx.coroutines.withTimeoutOrNull
import org.koin.compose.koinInject

/**
 * 键条：横向滚动，画当前那一组的按键（一组一排，放不下就横滑）。
 *
 * 单击派发单击那一格；修饰键单击点亮 / 熄掉粘性修饰（[onModifier]）。长按：绑了长按 ⇒ 按住到时长发一次；
 * 没绑、而单击能连发（[isRepeatable]）⇒ 按住连发；其余长按等于单击一次。长按时长取这一颗另设的，再取总的，再取系统的。
 *
 * 注意：组必须进 [key]。`collectAsState` 的 State 不带 key，只换 Flow 会残留上一组的键。
 */
@Composable
fun ExtraKeysRow(
    groupId: String,
    dispatch: (KeyBinding) -> Unit,
    onModifier: (KeyMod) -> Unit,
    modifier: Modifier = Modifier, // Compose 约定：modifier 首个可选参
    ctrlActive: Boolean = false,
    altActive: Boolean = false,
    longPressMs: Int? = null, // 总的那一份；null ＝ 跟系统
    groupName: (String) -> String? = { null },
) {
    val tokens = LocalAppTokens.current
    val dao = koinInject<KeyDao>()
    val keys by key(groupId) {
        remember { dao.observeKeys(groupId) }.collectAsState(initial = emptyList())
    }
    val systemLong = ViewConfiguration.getLongPressTimeout()
    Row(
        modifier
            .fillMaxWidth()
            .background(tokens.bg2)
            .horizontalScroll(rememberScrollState())
            .padding(horizontal = tokens.spacing.sm, vertical = tokens.spacing.xs),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.xs),
    ) {
        // key(k.id)：按 id 匹配 slot，增删 / 重排不复用别人的键帽状态。
        keys.forEach { k ->
            key(k.id) {
                val mod = k.modifier
                val tap = k.tap
                val long = k.longPress
                KeyChip(
                    label = k.cap(groupName),
                    active = (mod == KeyMod.CTRL && ctrlActive) || (mod == KeyMod.ALT && altActive),
                    onTap = {
                        when {
                            mod != null -> onModifier(mod)
                            tap != null -> dispatch(tap)
                        }
                    },
                    longPressMode =
                        when {
                            mod != null -> LongPress.NONE
                            long != null -> LongPress.ONCE
                            tap.isRepeatable() -> LongPress.REPEAT
                            else -> LongPress.NONE
                        },
                    thresholdMs = (k.longPressMs ?: longPressMs ?: systemLong).toLong(),
                    onLongPressOnce = { long?.let(dispatch) },
                )
            }
        }
    }
}

/** 长按怎么做（由按键数据推出来，不存）。 */
private enum class LongPress { NONE, REPEAT, ONCE }

/** 按住连发的间隔。 */
private const val REPEAT_MS = 50L

@Composable
private fun KeyChip(
    label: String,
    onTap: () -> Unit,
    active: Boolean = false, // 修饰键点亮态
    longPressMode: LongPress = LongPress.NONE,
    thresholdMs: Long = 400,
    onLongPressOnce: () -> Unit = {},
) {
    val tokens = LocalAppTokens.current
    Box(
        modifier =
            Modifier
                .clip(RoundedCornerShape(tokens.radius.m))
                .background(if (active) tokens.selectedBg else tokens.card)
                .border(1.dp, MaterialTheme.colorScheme.outline, RoundedCornerShape(tokens.radius.m))
                .pressGestures(longPressMode, thresholdMs, REPEAT_MS, onTap, onLongPressOnce)
                .defaultMinSize(minWidth = 40.dp, minHeight = 36.dp)
                .padding(horizontal = 10.dp, vertical = 8.dp),
        contentAlignment = Alignment.Center,
    ) {
        KeyCapLabel(label, style = monoSmall)
    }
}

/**
 * 单击加长按的手势。NONE 只认单击；REPEAT 按住到时长后每 [repeatMs] 重发 [onTap]；
 * ONCE 按住到 [thresholdMs] 发一次 [onLongPressOnce]。三种下时长前松手都是单击。
 * 回调经 [rememberUpdatedState] 保持最新，`pointerInput` 只按 mode/阈值/速率重启手势，不随重组重启。
 */
@Composable
private fun Modifier.pressGestures(
    mode: LongPress,
    thresholdMs: Long,
    repeatMs: Long,
    onTap: () -> Unit,
    onLongPressOnce: () -> Unit,
): Modifier {
    val tap by rememberUpdatedState(onTap)
    val longOnce by rememberUpdatedState(onLongPressOnce)
    return pointerInput(mode, thresholdMs, repeatMs) {
        awaitEachGesture {
            awaitFirstDown(requireUnconsumed = false)
            if (mode == LongPress.NONE) {
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
                LongPress.REPEAT -> {
                    tap() // 到阈值即首发
                    while (true) {
                        var u: PointerInputChange? = null
                        val stillHeld = withTimeoutOrNull(repeatMs) { u = waitForUpOrCancellation() } == null
                        if (!stillHeld) break // 松手/取消 → 停
                        tap()
                    }
                }
                LongPress.ONCE -> {
                    longOnce()
                    waitForUpOrCancellation() // 吞掉本次抬起，避免再触发 tap
                }
                LongPress.NONE -> {}
            }
        }
    }
}
