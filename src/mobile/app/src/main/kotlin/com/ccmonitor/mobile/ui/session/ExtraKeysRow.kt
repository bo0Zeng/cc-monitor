package com.ccmonitor.mobile.ui.session

import android.os.SystemClock
import android.view.ViewConfiguration
import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.defaultMinSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
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
import androidx.compose.ui.input.pointer.PointerEventPass
import androidx.compose.ui.input.pointer.PointerInputScope
import androidx.compose.ui.input.pointer.changedToDown
import androidx.compose.ui.input.pointer.changedToUp
import androidx.compose.ui.input.pointer.isOutOfBounds
import androidx.compose.ui.input.pointer.pointerInput
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.data.db.KeyBinding
import com.ccmonitor.mobile.core.data.db.KeyDao
import com.ccmonitor.mobile.core.data.db.KeyMod
import com.ccmonitor.mobile.core.data.db.TermKey
import com.ccmonitor.mobile.core.data.db.cap
import com.ccmonitor.mobile.core.data.db.isRepeatable
import com.ccmonitor.mobile.core.terminal.ModGesture
import com.ccmonitor.mobile.core.terminal.ModState
import com.ccmonitor.mobile.core.terminal.ModStates
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import org.koin.compose.koinInject

/**
 * 键条：横向滚动，画当前那一组的按键（一组一排，放不下就横滑）。
 *
 * 每颗的手势照它的数据定（[gestureSpec]）：单击 · 双击 · 长按派发各自那一格（[dispatch]）；修饰键单击点亮、
 * 双击锁住、长按按住期间一直带（[onModifier]）。两个时长取这一颗另设的，再取总的，再取系统的。
 *
 * 注意：组必须进 [key]。`collectAsState` 的 State 不带 key，只换 Flow 会残留上一组的键。
 */
@Composable
fun ExtraKeysRow(
    groupId: String,
    dispatch: (KeyBinding) -> Unit,
    onModifier: (KeyMod, ModGesture) -> Unit,
    modifier: Modifier = Modifier, // Compose 约定：modifier 首个可选参
    mods: ModStates = ModStates(),
    doubleTapMs: Int? = null, // 总的那一份；null ＝ 跟系统
    longPressMs: Int? = null,
    groupName: (String) -> String? = { null },
) {
    val tokens = LocalAppTokens.current
    val dao = koinInject<KeyDao>()
    val keys by key(groupId) {
        remember { dao.observeKeys(groupId) }.collectAsState(initial = emptyList())
    }
    val sys =
        SystemTimings(
            doubleTapMs = ViewConfiguration.getDoubleTapTimeout(),
            longPressMs = ViewConfiguration.getLongPressTimeout(),
            repeatMs = ViewConfiguration.getKeyRepeatDelay(),
        )
    Row(
        modifier
            .fillMaxWidth()
            .background(tokens.bg2)
            .horizontalScroll(rememberScrollState())
            .padding(start = tokens.spacing.sm, end = tokens.spacing.sm, top = 4.dp, bottom = 6.dp),
        horizontalArrangement = Arrangement.spacedBy(tokens.spacing.sm),
        verticalAlignment = Alignment.CenterVertically,
    ) {
        // key(k.id)：按 id 匹配 slot，增删 / 重排不复用别人的键帽状态。
        keys.forEach { k ->
            key(k.id) {
                val mod = k.modifier
                KeyChip(
                    label = k.cap(groupName),
                    look = mod?.let { mods.of(it.term()) } ?: ModState.OFF,
                    spec = gestureSpec(k, doubleTapMs, longPressMs, sys),
                    onGesture = { out ->
                        if (mod != null) {
                            onModifier(mod, modGesture(out))
                        } else {
                            k.bindingFor(out)?.let(dispatch)
                        }
                    },
                )
            }
        }
    }
}

@Composable
private fun KeyChip(
    label: String,
    look: ModState,
    spec: GestureSpec,
    onGesture: (GestureOut) -> Unit,
) {
    val tokens = LocalAppTokens.current
    val shape = RoundedCornerShape(tokens.radius.m)
    val scheme = MaterialTheme.colorScheme
    val (bg, edge) =
        when (look) {
            ModState.LOCKED -> scheme.primary to scheme.primary
            ModState.ONCE, ModState.HELD -> tokens.selectedBg to tokens.accent
            ModState.OFF -> tokens.card to scheme.outlineVariant
        }
    Box(
        modifier =
            Modifier
                .height(40.dp)
                .defaultMinSize(minWidth = 44.dp)
                .clip(shape)
                .background(bg)
                .border(1.dp, edge, shape)
                .keyGestures(spec, onGesture)
                .padding(horizontal = 9.dp),
        contentAlignment = Alignment.Center,
    ) {
        KeyCapLabel(label, style = monoSmall, color = if (look == ModState.LOCKED) scheme.onPrimary else scheme.onSurface)
    }
}

/** 修饰键：单击点亮 / 熄 · 双击锁住 · 长按按下与松开。 */
private fun modGesture(out: GestureOut): ModGesture =
    when (out) {
        GestureOut.TAP -> ModGesture.TAP
        GestureOut.DOUBLE -> ModGesture.LOCK
        GestureOut.LONG -> ModGesture.HOLD
        GestureOut.LONG_END -> ModGesture.RELEASE
    }

/**
 * 把一颗键帽上的触摸喂给 [KeyGesture]，时刻用事件自带的时间（与 [SystemClock.uptimeMillis] 同一个钟），
 * 到了它要的时刻（[KeyGesture.deadline]）就推一下。横滑键条（别人吃掉了移动）或滑出键帽 ＝ 取消。
 */
@Composable
private fun Modifier.keyGestures(
    spec: GestureSpec,
    onGesture: (GestureOut) -> Unit,
): Modifier {
    val emit by rememberUpdatedState(onGesture)
    return pointerInput(spec) { driveGesture(KeyGesture(spec)) { outs -> outs.forEach { emit(it) } } }
}

private suspend fun PointerInputScope.driveGesture(
    g: KeyGesture,
    emit: (List<GestureOut>) -> Unit,
) = awaitPointerEventScope {
    while (true) {
        val due = g.deadline()
        val ev =
            if (due == null) {
                awaitPointerEvent()
            } else {
                withTimeoutOrNull((due - SystemClock.uptimeMillis()).coerceAtLeast(0)) { awaitPointerEvent() }
            }
        if (ev == null) {
            emit(g.advance(SystemClock.uptimeMillis()))
            continue
        }
        val c = ev.changes.firstOrNull() ?: continue
        when {
            c.changedToDown() -> emit(g.down(c.uptimeMillis))
            c.changedToUp() -> emit(if (c.isConsumed) g.cancel() else g.up(c.uptimeMillis))
            !c.pressed -> {}
            c.isConsumed || c.isOutOfBounds(size, extendedTouchPadding) -> emit(g.cancel())
            // 键条横滑在 Main 那一趟里、比键帽晚，吃没吃掉移动要到 Final 那一趟才看得到。
            awaitPointerEvent(PointerEventPass.Final).changes.any { it.isConsumed } -> emit(g.cancel())
        }
    }
}

/** 双击怎么认：NONE 不认（单击不等）；WAIT 单击等一个双击窗确认没有第二下；EAGER 单击当场出、窗内第二下再出双击（修饰键用）。 */
internal enum class DoubleTap { NONE, WAIT, EAGER }

/** 按住到长按时长：TAP 等于单击一次（松手时出）；REPEAT 当场出单击、之后每 repeatMs 一下；FIRE 出长按、松手出长按完。 */
internal enum class LongMode { TAP, REPEAT, FIRE }

internal enum class GestureOut { TAP, DOUBLE, LONG, LONG_END }

internal data class GestureSpec(
    val double: DoubleTap,
    val doubleTapMs: Long,
    val longPressMs: Long,
    val long: LongMode,
    val repeatMs: Long,
)

/** 系统那三个数（双击判定 · 长按 · 按键重复间隔）。 */
internal data class SystemTimings(
    val doubleTapMs: Int,
    val longPressMs: Int,
    val repeatMs: Int,
)

/** 这一颗的手势：时长先取这颗另设的（修饰键不单设），再取总的，再取系统的。 */
internal fun gestureSpec(
    k: TermKey,
    globalDoubleTapMs: Int?,
    globalLongPressMs: Int?,
    sys: SystemTimings,
): GestureSpec {
    val isMod = k.modifier != null
    val dbl = (if (isMod) null else k.doubleTapMs) ?: globalDoubleTapMs ?: sys.doubleTapMs
    val lng = (if (isMod) null else k.longPressMs) ?: globalLongPressMs ?: sys.longPressMs
    return GestureSpec(
        double =
            when {
                isMod -> DoubleTap.EAGER
                k.doubleTap != null -> DoubleTap.WAIT
                else -> DoubleTap.NONE
            },
        doubleTapMs = dbl.toLong(),
        longPressMs = lng.toLong(),
        long =
            when {
                isMod || k.longPress != null -> LongMode.FIRE
                k.tap.isRepeatable() -> LongMode.REPEAT
                else -> LongMode.TAP
            },
        repeatMs = sys.repeatMs.toLong(),
    )
}

/** 手势出的那一下对应这颗的哪个触发（连发的每一下都是单击）。 */
internal fun TermKey.bindingFor(out: GestureOut): KeyBinding? =
    when (out) {
        GestureOut.TAP -> tap
        GestureOut.DOUBLE -> doubleTap
        GestureOut.LONG -> longPress
        GestureOut.LONG_END -> null
    }

/**
 * 一颗键帽的手势时序（纯状态机，时刻由调用方给，不读墙钟）。按下 · 抬起 · 取消 · 推到某一刻，各回这一下要出的几样。
 * [deadline] 是下一个要推的时刻（长按到点 · 连发下一下 · 双击窗关上），没有就 null。
 */
internal class KeyGesture(
    private val spec: GestureSpec,
) {
    private sealed interface St {
        data object Idle : St

        data class Down(
            val at: Long,
        ) : St

        /** 抬起了，等双击窗里的第二下（WAIT 欠着一下单击；EAGER 已经出过）。 */
        data class Window(
            val upAt: Long,
        ) : St

        /** 第二下按着：双击已出，抬起不再出。 */
        data object Second : St

        data object Held : St

        data object LongTap : St

        data class Repeating(
            val next: Long,
        ) : St
    }

    private var st: St = St.Idle

    fun deadline(): Long? =
        when (val s = st) {
            is St.Down -> s.at + spec.longPressMs
            is St.Window -> if (spec.double == DoubleTap.WAIT) s.upAt + spec.doubleTapMs else null
            is St.Repeating -> s.next
            else -> null
        }

    fun down(t: Long): List<GestureOut> {
        val s = st
        st = St.Down(t)
        if (s !is St.Window) return emptyList()
        if (t - s.upAt <= spec.doubleTapMs) {
            st = St.Second
            return listOf(GestureOut.DOUBLE)
        }
        return if (spec.double == DoubleTap.WAIT) listOf(GestureOut.TAP) else emptyList()
    }

    fun up(t: Long): List<GestureOut> {
        val s = st
        st = St.Idle
        return when {
            s is St.Down && spec.double == DoubleTap.NONE -> listOf(GestureOut.TAP)
            s is St.Down -> {
                st = St.Window(t)
                if (spec.double == DoubleTap.EAGER) listOf(GestureOut.TAP) else emptyList()
            }
            s == St.LongTap -> listOf(GestureOut.TAP)
            s == St.Held -> listOf(GestureOut.LONG_END)
            else -> emptyList()
        }
    }

    fun cancel(): List<GestureOut> {
        val s = st
        if (s !is St.Window) st = St.Idle
        return if (s == St.Held) listOf(GestureOut.LONG_END) else emptyList()
    }

    fun advance(t: Long): List<GestureOut> {
        val due = deadline()?.takeIf { t >= it } ?: return emptyList()
        return when (st) {
            is St.Down ->
                when (spec.long) {
                    LongMode.FIRE -> {
                        st = St.Held
                        listOf(GestureOut.LONG)
                    }
                    LongMode.REPEAT -> {
                        st = St.Repeating(due + spec.repeatMs)
                        listOf(GestureOut.TAP)
                    }
                    LongMode.TAP -> {
                        st = St.LongTap
                        emptyList()
                    }
                }
            is St.Repeating -> {
                // 晚到的一推只补一下，不连珠炮。
                val next = due + spec.repeatMs
                st = St.Repeating(if (next <= t) t + spec.repeatMs else next)
                listOf(GestureOut.TAP)
            }
            else -> {
                st = St.Idle
                listOf(GestureOut.TAP)
            }
        }
    }
}
