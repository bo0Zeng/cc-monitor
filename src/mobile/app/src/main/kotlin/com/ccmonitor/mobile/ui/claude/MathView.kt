package com.ccmonitor.mobile.ui.claude

import android.content.Context
import android.graphics.DashPathEffect
import android.graphics.Paint
import android.graphics.Typeface
import androidx.compose.foundation.Canvas
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.DrawScope
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.InlNode
import com.ccmonitor.mobile.core.wire.gen.MathBox
import com.ccmonitor.mobile.core.wire.gen.MathFont
import com.ccmonitor.mobile.core.wire.gen.MathItem
import com.ccmonitor.mobile.core.wire.gen.MathPath
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import java.util.concurrent.ConcurrentHashMap

// 公式照核心排好的画单（`MathBox`：千分之一 em 的整数几何，原点左上、y 朝下，基线在 y = h）画；手机不排版。
// 字形用 KaTeX 字体（`src/shared/fonts/katex/`，构建时铺进 assets 的 `katex/`），头一回用到哪一族才载哪一族。

/** 一条公式在某个字号下占多大（像素）：宽 · 总高（基线上 ＋ 基线下）· 基线下那一截。 */
data class MathSize(
    val width: Float,
    val height: Float,
    val depth: Float,
)

/** 画单 × 字号（像素）。 */
fun mathSizePx(
    m: MathBox,
    fontPx: Float,
): MathSize {
    val k = fontPx / 1000f
    return MathSize(m.w * k, (m.h + m.d) * k, m.d * k)
}

/** 那一族字体在 assets 里的文件；`SYS` ⇒ 没有（交系统字体）。 */
fun mathFontAsset(f: MathFont): String? =
    when (f) {
        MathFont.AMS -> "AMS-Regular"
        MathFont.CALIGRAPHIC_REGULAR -> "Caligraphic-Regular"
        MathFont.FRAKTUR_REGULAR -> "Fraktur-Regular"
        MathFont.FRAKTUR_BOLD -> "Fraktur-Bold"
        MathFont.MAIN_REGULAR -> "Main-Regular"
        MathFont.MAIN_BOLD -> "Main-Bold"
        MathFont.MAIN_ITALIC -> "Main-Italic"
        MathFont.MAIN_BOLD_ITALIC -> "Main-BoldItalic"
        MathFont.MATH_ITALIC -> "Math-Italic"
        MathFont.MATH_BOLD_ITALIC -> "Math-BoldItalic"
        MathFont.SANS_SERIF_REGULAR -> "SansSerif-Regular"
        MathFont.SANS_SERIF_BOLD -> "SansSerif-Bold"
        MathFont.SANS_SERIF_ITALIC -> "SansSerif-Italic"
        MathFont.SCRIPT_REGULAR -> "Script-Regular"
        MathFont.SIZE1 -> "Size1-Regular"
        MathFont.SIZE2 -> "Size2-Regular"
        MathFont.SIZE3 -> "Size3-Regular"
        MathFont.SIZE4 -> "Size4-Regular"
        MathFont.TYPEWRITER -> "Typewriter-Regular"
        MathFont.SYS -> null
    }?.let { "katex/KaTeX_$it.ttf" }

/** 一串块里每条行内公式的画单（按 TeX 原文；同一条 TeX 行内排出来是同一份）；没画单的不收（那一槽照原文排）。 */
fun inlineMathOf(blocks: List<MdBlock>): Map<String, MathBox> {
    val out = linkedMapOf<String, MathBox>()

    fun inls(ks: List<Inl>) {
        for (k in ks) {
            val n = (k as? Inl.InlNode)?.value ?: continue
            when (n) {
                is InlNode.M -> n.m?.let { out.putIfAbsent(n.tex, it) }
                is InlNode.B -> inls(n.k)
                is InlNode.I -> inls(n.k)
                is InlNode.Del -> inls(n.k)
                is InlNode.A -> inls(n.k)
                else -> Unit
            }
        }
    }

    fun walk(bs: List<MdBlock>) {
        for (b in bs) {
            when (b) {
                is MdBlock.P -> inls(b.k)
                is MdBlock.H -> inls(b.k)
                is MdBlock.List -> b.items.forEach { walk(it.blocks) }
                is MdBlock.Quote -> walk(b.blocks)
                is MdBlock.Alert -> walk(b.blocks)
                is MdBlock.Table -> (b.head + b.rows.flatten()).forEach(::inls)
                is MdBlock.Notes -> b.items.forEach { walk(it.blocks) }
                is MdBlock.Code, is MdBlock.Math, MdBlock.Rule -> Unit
            }
        }
    }
    walk(blocks)
    return out
}

/** 排字那一层取行内槽的画单：[MdDocView] · [MdBlocksView] 照那一份正文的块交下来。 */
val LocalInlineMath = staticCompositionLocalOf<Map<String, MathBox>> { emptyMap() }

/** 载过的字体（进程里一份；载不到的记成系统衬线字体，不反复去载）。 */
private val typefaces = ConcurrentHashMap<MathFont, Typeface>()

private fun typefaceOf(
    ctx: Context,
    f: MathFont,
): Typeface {
    val asset = mathFontAsset(f) ?: return Typeface.SERIF
    return typefaces.getOrPut(f) { runCatching { Typeface.createFromAsset(ctx.assets, asset) }.getOrDefault(Typeface.SERIF) }
}

/** 照画单画一条公式，填满给的那一块（大小由调用方照 [mathSizePx] 定）。 */
@Composable
fun MathCanvas(
    m: MathBox,
    fontPx: Float,
    color: Color,
    modifier: Modifier = Modifier,
) {
    val ctx = LocalContext.current.applicationContext
    Canvas(modifier) { drawMath(ctx, m, fontPx, color) }
}

private fun DrawScope.drawMath(
    ctx: Context,
    m: MathBox,
    fontPx: Float,
    color: Color,
) {
    val k = fontPx / 1000f
    val argb = color.toArgb()
    val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { this.color = argb }
    drawIntoCanvas { c ->
        val nc = c.nativeCanvas
        for (it in m.items) {
            when (it) {
                is MathItem.G -> {
                    if (!Character.isValidCodePoint(it.u)) continue
                    val glyph = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                        this.color = argb
                        typeface = typefaceOf(ctx, it.f)
                        textSize = fontPx * it.s / 1000f
                    }
                    nc.drawText(String(Character.toChars(it.u)), it.x * k, it.y * k, glyph)
                }
                is MathItem.L -> {
                    val th = maxOf(1f, it.th * k)
                    if (it.dash) {
                        val stroke = Paint(Paint.ANTI_ALIAS_FLAG).apply {
                            this.color = argb
                            style = Paint.Style.STROKE
                            strokeWidth = th
                            pathEffect = DashPathEffect(floatArrayOf(th * 3, th * 3), 0f)
                        }
                        nc.drawLine(it.x * k, it.y * k, (it.x + it.w) * k, it.y * k, stroke)
                    } else {
                        nc.drawRect(it.x * k, it.y * k - th / 2, (it.x + it.w) * k, it.y * k + th / 2, fill)
                    }
                }
                is MathItem.R -> nc.drawRect(it.x * k, it.y * k, (it.x + it.w) * k, (it.y + it.h) * k, fill)
                is MathItem.P -> {
                    val path = android.graphics.Path()
                    val dx = it.x * k
                    val dy = it.y * k
                    for (cmd in it.d) {
                        when (cmd) {
                            is MathPath.M -> path.moveTo(dx + cmd.x * k, dy + cmd.y * k)
                            is MathPath.L -> path.lineTo(dx + cmd.x * k, dy + cmd.y * k)
                            is MathPath.C ->
                                path.cubicTo(dx + cmd.x1 * k, dy + cmd.y1 * k, dx + cmd.x2 * k, dy + cmd.y2 * k, dx + cmd.x * k, dy + cmd.y * k)
                            is MathPath.Q -> path.quadTo(dx + cmd.x1 * k, dy + cmd.y1 * k, dx + cmd.x * k, dy + cmd.y * k)
                            MathPath.Z -> path.close()
                        }
                    }
                    val paint = if (it.fill) fill else Paint(fill).apply { style = Paint.Style.STROKE }
                    nc.drawPath(path, paint)
                }
            }
        }
    }
}
