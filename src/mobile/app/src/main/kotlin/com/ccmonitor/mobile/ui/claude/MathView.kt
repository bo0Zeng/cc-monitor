package com.ccmonitor.mobile.ui.claude

import android.content.Context
import android.graphics.Canvas
import android.graphics.DashPathEffect
import android.graphics.Paint
import android.graphics.Path
import android.graphics.Typeface
import androidx.compose.runtime.Composable
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.drawscope.drawIntoCanvas
import androidx.compose.ui.graphics.nativeCanvas
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.InlNode
import com.ccmonitor.mobile.core.wire.gen.MathBox
import com.ccmonitor.mobile.core.wire.gen.MathItem
import com.ccmonitor.mobile.core.wire.gen.MathPath
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import java.util.concurrent.ConcurrentHashMap
import androidx.compose.foundation.Canvas as ComposeCanvas

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

/** 字形那一格（KaTeX 字体文件名主干）在 assets 里的文件；缺 ⇒ 没有（交系统字体）。 */
fun mathFontAsset(stem: String?): String? = stem?.let { "katex/KaTeX_$it.ttf" }

/** 一串块里每条行内公式的画单（按 TeX 原文；同一条 TeX 行内排出来是同一份）；没画单的不收（那一槽照原文排）。 */
fun inlineMathOf(blocks: List<MdBlock>): Map<String, MathBox> {
    val out = linkedMapOf<String, MathBox>()
    collectBlocks(blocks, out)
    return out
}

private fun collectBlocks(
    bs: List<MdBlock>,
    out: MutableMap<String, MathBox>,
) {
    for (b in bs) {
        b.inlineRuns().forEach { collectInls(it, out) }
        collectBlocks(b.childBlocks(), out)
    }
}

private fun collectInls(
    ks: List<Inl>,
    out: MutableMap<String, MathBox>,
) {
    for (k in ks) {
        val n = (k as? Inl.InlNode)?.value ?: continue
        if (n is InlNode.M) n.m?.let { out.putIfAbsent(n.tex, it) } else collectInls(n.innerInls(), out)
    }
}

/** 一块自己带的那几串行内（段落 · 标题 · 表格各格）。 */
private fun MdBlock.inlineRuns(): List<List<Inl>> =
    when (this) {
        is MdBlock.P -> listOf(k)
        is MdBlock.H -> listOf(k)
        is MdBlock.Table -> head + rows.flatten()
        else -> emptyList()
    }

/** 一块里套着的块（列表各项 · 引用 · 提示块 · 脚注）。 */
private fun MdBlock.childBlocks(): List<MdBlock> =
    when (this) {
        is MdBlock.List -> items.flatMap { it.blocks }
        is MdBlock.Quote -> blocks
        is MdBlock.Alert -> blocks
        is MdBlock.Notes -> items.flatMap { it.blocks }
        else -> emptyList()
    }

/** 一个行内节点里套着的行内（加粗 · 斜体 · 删除线 · 链接）。 */
private fun InlNode.innerInls(): List<Inl> =
    when (this) {
        is InlNode.B -> k
        is InlNode.I -> k
        is InlNode.Del -> k
        is InlNode.A -> k
        else -> emptyList()
    }

/** 排字那一层取行内槽的画单：[MdDocView] · [MdBlocksView] 照那一份正文的块交下来。 */
val LocalInlineMath = staticCompositionLocalOf<Map<String, MathBox>> { emptyMap() }

/** 载过的字体（进程里一份；载不到的记成系统衬线字体，不反复去载）。 */
private object MathFonts {
    private val loaded = ConcurrentHashMap<String, Typeface>()

    fun of(
        ctx: Context,
        stem: String?,
    ): Typeface {
        val asset = mathFontAsset(stem) ?: return Typeface.SERIF
        return loaded.getOrPut(asset) { runCatching { Typeface.createFromAsset(ctx.assets, asset) }.getOrDefault(Typeface.SERIF) }
    }
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
    ComposeCanvas(modifier) { drawIntoCanvas { MathPainter(ctx, fontPx, color.toArgb()).draw(it.nativeCanvas, m) } }
}

/** 一趟画：几何 × `fontPx / 1000`，颜色一律跟正文。 */
internal class MathPainter(
    private val ctx: Context,
    private val fontPx: Float,
    private val argb: Int,
) {
    private val k = fontPx / 1000f
    private val fill = Paint(Paint.ANTI_ALIAS_FLAG).apply { color = argb }

    fun draw(
        c: Canvas,
        m: MathBox,
    ) {
        for (it in m.items) {
            when (it) {
                is MathItem.G -> glyph(c, it)
                is MathItem.L -> line(c, it)
                is MathItem.R -> c.drawRect(it.x * k, it.y * k, (it.x + it.w) * k, (it.y + it.h) * k, fill)
                is MathItem.P -> path(c, it)
            }
        }
    }

    private fun glyph(
        c: Canvas,
        g: MathItem.G,
    ) {
        if (!Character.isValidCodePoint(g.u)) return
        val paint =
            Paint(fill).apply {
                typeface = MathFonts.of(ctx, g.f)
                textSize = fontPx * g.s / 1000f
            }
        c.drawText(String(Character.toChars(g.u)), g.x * k, g.y * k, paint)
    }

    private fun line(
        c: Canvas,
        l: MathItem.L,
    ) {
        val th = maxOf(1f, l.th * k)
        if (!l.dash) {
            c.drawRect(l.x * k, l.y * k - th / 2, (l.x + l.w) * k, l.y * k + th / 2, fill)
            return
        }
        val stroke =
            Paint(fill).apply {
                style = Paint.Style.STROKE
                strokeWidth = th
                pathEffect = DashPathEffect(floatArrayOf(th * 3, th * 3), 0f)
            }
        c.drawLine(l.x * k, l.y * k, (l.x + l.w) * k, l.y * k, stroke)
    }

    private fun path(
        c: Canvas,
        p: MathItem.P,
    ) {
        val dx = p.x * k
        val dy = p.y * k
        val path = Path()
        for (cmd in p.d) {
            when (cmd) {
                is MathPath.M -> path.moveTo(dx + cmd.x * k, dy + cmd.y * k)
                is MathPath.L -> path.lineTo(dx + cmd.x * k, dy + cmd.y * k)
                is MathPath.C -> path.cubicTo(dx + cmd.x1 * k, dy + cmd.y1 * k, dx + cmd.x2 * k, dy + cmd.y2 * k, dx + cmd.x * k, dy + cmd.y * k)
                is MathPath.Q -> path.quadTo(dx + cmd.x1 * k, dy + cmd.y1 * k, dx + cmd.x * k, dy + cmd.y * k)
                MathPath.Z -> path.close()
            }
        }
        c.drawPath(path, if (p.fill) fill else Paint(fill).apply { style = Paint.Style.STROKE })
    }
}
