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
import com.ccmonitor.mobile.core.claude.link.SessionDot
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.ThemeTokens

/** 环线宽（同桌面 1.5px）。 */
private val RING = 1.5.dp

/** 呼吸到最淡时的不透明度（同桌面 `@keyframes breathe`）。 */
private const val BREATHE_LOW = 0.6f

/**
 * 一颗状态点（V10，与桌面 `kit/status-dot` 同一套）：8 dp、固定一列宽，状态变了不挤字。颜色 ＝ 在干什么，形状 ＝ 进程还在不在：
 * 实心（运行中 `--success` 呼吸 · 机器已连接 `--success` 不呼吸 · 需手动 `--warn` · 后台任务 `--bgwork` · 空闲 `--text-2` · 出错 `--error`）·
 * 空心环（已退出 `--text-2` · 已结束 `--text-faint`）· 虚线环（说不清 `--text-2`）· 短横（记录已不在 `--text-faint`）。
 * [dot] 为 `null` ⇒ 只占位不画。[label] 是读屏念的那一句（人话状态）；系统关了动画 ⇒ 不呼吸、常亮。
 */
@Composable
fun StatusDot(
    dot: SessionDot?,
    label: String?,
    modifier: Modifier = Modifier,
) {
    val tokens = LocalAppTokens.current
    val face = dot?.let { faceOf(it) }
    val sem = if (label == null || dot == null) Modifier.clearAndSetSemantics {} else Modifier.clearAndSetSemantics { contentDescription = label }
    val alpha = if (dot == SessionDot.RUNNING) breathe() else 1f
    Canvas(modifier.size(tokens.dotSize).then(sem)) { face?.let { drawFace(it, alpha) } }
}

/** 点画成什么样：形 ＋ 色。 */
private enum class Form { SOLID, RING, DASHED, DASH }

private data class Face(
    val form: Form,
    val color: Color,
)

@Composable
private fun faceOf(dot: SessionDot): Face {
    val tokens = LocalAppTokens.current
    val text2 = MaterialTheme.colorScheme.onSurfaceVariant
    return when (dot) {
        SessionDot.RUNNING, SessionDot.UP -> Face(Form.SOLID, tokens.success)
        SessionDot.NEEDS_YOU -> Face(Form.SOLID, tokens.warn)
        SessionDot.BACKGROUND -> Face(Form.SOLID, tokens.bgwork)
        SessionDot.IDLE -> Face(Form.SOLID, text2)
        SessionDot.FAILED -> Face(Form.SOLID, MaterialTheme.colorScheme.error)
        SessionDot.EXITED -> Face(Form.RING, text2)
        SessionDot.ENDED -> Face(Form.RING, tokens.textFaint)
        SessionDot.UNKNOWN -> Face(Form.DASHED, text2)
        SessionDot.GONE -> Face(Form.DASH, tokens.textFaint)
    }
}

private fun DrawScope.drawFace(
    f: Face,
    alpha: Float,
) {
    val r = size.minDimension / 2
    val w = RING.toPx()
    when (f.form) {
        Form.SOLID -> drawCircle(f.color, r, alpha = alpha)
        Form.RING -> drawCircle(f.color, r - w / 2, style = Stroke(w))
        Form.DASHED -> drawCircle(f.color, r - w / 2, style = Stroke(w, pathEffect = PathEffect.dashPathEffect(floatArrayOf(w * DASH_ON, w * DASH_OFF))))
        Form.DASH -> drawRoundRect(f.color, topLeft = Offset(0f, (size.height - w) / 2), size = Size(size.width, w), cornerRadius = CornerRadius(w / 2))
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
