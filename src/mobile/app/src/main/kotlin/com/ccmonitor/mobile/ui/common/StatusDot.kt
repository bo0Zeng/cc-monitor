package com.ccmonitor.mobile.ui.common

import android.provider.Settings
import androidx.compose.animation.core.RepeatMode
import androidx.compose.animation.core.animateFloat
import androidx.compose.animation.core.infiniteRepeatable
import androidx.compose.animation.core.rememberInfiniteTransition
import androidx.compose.animation.core.tween
import androidx.compose.foundation.Canvas
import androidx.compose.foundation.layout.size
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.remember
import androidx.compose.ui.Modifier
import androidx.compose.ui.geometry.CornerRadius
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.geometry.Size
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.PathEffect
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.Stroke
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.semantics.clearAndSetSemantics
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.unit.dp
import com.ccmonitor.mobile.core.claude.link.wire.SessionDot
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.ThemeTokens

/** 环线宽（同桌面 1.5px）。 */
private val RING = 1.5.dp

/** 呼吸到最淡时的不透明度（同桌面 `@keyframes breathe`）。 */
private const val BREATHE_LOW = 0.6f

/**
 * 一颗会话状态点（V10，与桌面 `kit/status-dot` 同一套）：8 dp、固定一列宽，状态变了不挤字。颜色 ＝ 在干什么，形状 ＝ 进程还在不在：
 * 实心（运行中 `--success` 呼吸 · 需手动 `--warn` · 后台任务 `--bgwork` · 空闲 `--text-2`）·
 * 空心环（已退出 `--text-2` · 已结束 `--text-faint`）· 虚线环（说不清 `--text-2`）· 短横（记录已不在 `--text-faint`）。
 * [label] 是读屏念的那一句（人话状态）；系统关了动画 ⇒ 不呼吸、常亮。
 */
@Composable
fun StatusDot(
    dot: SessionDot,
    label: String,
    modifier: Modifier = Modifier,
) {
    val tokens = LocalAppTokens.current
    val text2 = MaterialTheme.colorScheme.onSurfaceVariant
    val color =
        when (dot) {
            SessionDot.RUNNING -> tokens.success
            SessionDot.NEEDS_YOU -> tokens.warn
            SessionDot.BACKGROUND -> tokens.bgwork
            SessionDot.IDLE, SessionDot.EXITED, SessionDot.UNKNOWN -> text2
            SessionDot.ENDED, SessionDot.GONE -> tokens.textFaint
        }
    Dot(Face(formOf(dot), color), label, breathing = dot == SessionDot.RUNNING, modifier)
}

/** 机器那一颗：已连接（实心 `--success`、不呼吸）· 连接失败（实心 `--error`）。 */
enum class MachineDot { UP, FAILED }

/** 机器那一颗点；[dot] 为 `null`（未连接 · 连接中）⇒ 只占位不画。 */
@Composable
fun StatusDot(
    dot: MachineDot?,
    label: String?,
    modifier: Modifier = Modifier,
) {
    val color = if (dot == MachineDot.FAILED) MaterialTheme.colorScheme.error else LocalAppTokens.current.success
    Dot(dot?.let { Face(DotForm.SOLID, color) }, label, breathing = false, modifier)
}

@Composable
private fun Dot(
    face: Face?,
    label: String?,
    breathing: Boolean,
    modifier: Modifier,
) {
    val sem = if (label == null || face == null) Modifier.clearAndSetSemantics {} else Modifier.clearAndSetSemantics { contentDescription = label }
    val alpha = if (breathing) breathe() else 1f
    Canvas(modifier.size(LocalAppTokens.current.dotSize).then(sem)) { face?.let { drawFace(it, alpha) } }
}

/** 点画成什么形。 */
internal enum class DotForm { SOLID, RING, DASHED, DASH }

/** 会话点的形 ＝ 进程还在不在：在 ⇒ 实心；退了 · 结束了 ⇒ 环；说不清 ⇒ 虚线环；记录已不在 ⇒ 短横。 */
internal fun formOf(dot: SessionDot): DotForm =
    when (dot) {
        SessionDot.RUNNING, SessionDot.NEEDS_YOU, SessionDot.BACKGROUND, SessionDot.IDLE -> DotForm.SOLID
        SessionDot.EXITED, SessionDot.ENDED -> DotForm.RING
        SessionDot.UNKNOWN -> DotForm.DASHED
        SessionDot.GONE -> DotForm.DASH
    }

private data class Face(
    val form: DotForm,
    val color: Color,
)

private fun DrawScope.drawFace(
    f: Face,
    alpha: Float,
) {
    val r = size.minDimension / 2
    val w = RING.toPx()
    when (f.form) {
        DotForm.SOLID -> drawCircle(f.color, r, alpha = alpha)
        DotForm.RING -> drawCircle(f.color, r - w / 2, style = Stroke(w))
        DotForm.DASHED -> drawCircle(f.color, r - w / 2, style = Stroke(w, pathEffect = PathEffect.dashPathEffect(floatArrayOf(w * DASH_ON, w * DASH_OFF))))
        DotForm.DASH -> drawRoundRect(f.color, topLeft = Offset(0f, (size.height - w) / 2), size = Size(size.width, w), cornerRadius = CornerRadius(w / 2))
    }
}

/** 虚线环一段实 · 一段空（环线宽的倍数）。 */
private const val DASH_ON = 1.4f
private const val DASH_OFF = 1.2f

/** 运行中那颗点的呼吸（`--dur-breathe` 一来一回）；系统「移除动画」开着 ⇒ 常亮。 */
@Composable
private fun breathe(): Float {
    val context = LocalContext.current
    val still = remember(context) { Settings.Global.getFloat(context.contentResolver, Settings.Global.ANIMATOR_DURATION_SCALE, 1f) == 0f }
    if (still) return 1f
    val t = rememberInfiniteTransition(label = "breathe")
    return t
        .animateFloat(1f, BREATHE_LOW, infiniteRepeatable(tween(ThemeTokens.DurBreathe / 2), RepeatMode.Reverse), label = "breathe")
        .value
}
