package com.ccmonitor.mobile.ui.claude

import androidx.compose.foundation.background
import androidx.compose.foundation.border
import androidx.compose.foundation.horizontalScroll
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.BoxWithConstraints
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.IntrinsicSize
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxHeight
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.layout.widthIn
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.InlineTextContent
import androidx.compose.foundation.text.appendInlineContent
import androidx.compose.foundation.text.selection.SelectionContainer
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.Stable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.key
import androidx.compose.runtime.mutableStateListOf
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.draw.drawBehind
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.layout.Layout
import androidx.compose.ui.platform.LocalDensity
import androidx.compose.ui.text.AnnotatedString
import androidx.compose.ui.text.LinkAnnotation
import androidx.compose.ui.text.Placeholder
import androidx.compose.ui.text.PlaceholderVerticalAlign
import androidx.compose.ui.text.SpanStyle
import androidx.compose.ui.text.TextLinkStyles
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.buildAnnotatedString
import androidx.compose.ui.text.font.FontStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.BaselineShift
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextDecoration
import androidx.compose.ui.text.withLink
import androidx.compose.ui.text.withStyle
import androidx.compose.ui.unit.Constraints
import androidx.compose.ui.unit.Dp
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.em
import com.ccmonitor.mobile.core.ui.icon.Ph
import com.ccmonitor.mobile.core.ui.icon.PhIcon
import com.ccmonitor.mobile.core.ui.theme.AppTokens
import com.ccmonitor.mobile.core.ui.theme.JetBrainsMono
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import com.ccmonitor.mobile.core.ui.theme.ThemeTokens
import com.ccmonitor.mobile.core.ui.theme.monoBody
import com.ccmonitor.mobile.core.ui.theme.monoSmall
import com.ccmonitor.mobile.core.wire.gen.AlertKind
import com.ccmonitor.mobile.core.wire.gen.Doc
import com.ccmonitor.mobile.core.wire.gen.HlCode
import com.ccmonitor.mobile.core.wire.gen.Inl
import com.ccmonitor.mobile.core.wire.gen.InlNode
import com.ccmonitor.mobile.core.wire.gen.MdAlign
import com.ccmonitor.mobile.core.wire.gen.MdBlock
import com.ccmonitor.mobile.core.wire.gen.MdItem
import com.ccmonitor.mobile.core.wire.gen.SegHl
import com.ccmonitor.mobile.core.wire.gen.MathBox
import kotlinx.coroutines.CompletableDeferred

// 正文排版器：后端解析好的块树（`cmd_wire::md::Doc`）→ Compose。只照树排，不解析 Markdown、不判高亮类别。
//
// - 两层：[layoutSeg] 把一段的块排成 [MdPiece]（字是 `AnnotatedString`：`SpanStyle` ＋ `LinkAnnotation`），纯函数、JVM 上就测得了；
//   [MdDocView] 照 [MdPiece] 画（段落 · 标题 · 列表 · 引用 · 提示块 · 代码块 · 表格网格 · 公式 · 脚注）。
// - 高亮先出字、后上色：一段先照纯字建；那一段的代码块（[codesOf]，先序）交后端 `prose-hl` 拉，拉到了 [MdDocState.paint] 带色重排那一段。
//   第 n 块取 `hl[n]`，三元组 `[起点差, 长, 类]`，偏移按 UTF-16 码元（Kotlin 串直接切）；类的颜色是主题的终端色票（[rememberMdStyle]），不上线。
// - 公式照核心排好的画单画（`MathView.kt`）；行内的占一个行内槽（[MATH_TAG] 标着 TeX 原文，画单按它从 [LocalInlineMath] 取）；
//   核心排不出的（画单空）照原文等宽排。
// - 图也占一个行内槽（[IMG_TAG] 标着地址）：按地址取（`MdImage.kt`），取到了换成真图，取到之前 · 取不到照替代字。
// - 长正文按 `segs` 分段：[MdDocState] 只建第 0 段；点了「展开」（[MdDocState.expand]）才 [MdDocState.buildNext] 一段段往下接。

/** 排版要的几样样式（主题给，测试里另给一份好认的）。`hl` 的下标就是 `HlClass` 的序号（`src/common/cmd-wire/src/md.rs`）。 */
@Stable
data class MdStyle(
    val code: SpanStyle,
    val link: TextLinkStyles,
    /** 图的替代字。 */
    val faint: SpanStyle,
    /** 正文里的脚注号。 */
    val fnRef: SpanStyle,
    val hl: List<SpanStyle>,
)

/** 排好的一块：怎么画只看这一层。 */
sealed interface MdPiece {
    /** 段落（`level` 0）或标题 1–6。 */
    data class Text(
        val level: Int,
        val text: AnnotatedString,
    ) : MdPiece

    /** 代码块：字已照高亮着色；`cont` ⇒ 接着上一块（不再出语言那一行）。 */
    data class Code(
        val lang: String?,
        val text: AnnotatedString,
        val cont: Boolean,
    ) : MdPiece

    /** 块级公式：`m` 是核心排好的画单，空 ⇒ 照 `tex` 原文等宽排。 */
    data class Math(
        val tex: String,
        val m: MathBox?,
    ) : MdPiece

    /** 列表：有序的每项带号（`number`），任务项带勾（`task`）。 */
    data class Bullets(
        val tight: Boolean,
        val items: List<Item>,
    ) : MdPiece

    data class Item(
        val number: Int?,
        val task: Boolean?,
        val pieces: List<MdPiece>,
    )

    data class Quote(
        val pieces: List<MdPiece>,
    ) : MdPiece

    data class Alert(
        val kind: AlertKind,
        val pieces: List<MdPiece>,
    ) : MdPiece

    /** 表格：每行补齐到同样几格。 */
    data class Table(
        val align: List<MdAlign>,
        val head: List<AnnotatedString>,
        val rows: List<List<AnnotatedString>>,
    ) : MdPiece

    data object Rule : MdPiece

    data class Notes(
        val items: List<Note>,
    ) : MdPiece

    data class Note(
        val n: Int,
        val pieces: List<MdPiece>,
    )
}

/** 行内公式那个槽上的注记：值是 TeX 原文（同时是行内槽的 id 后半）。 */
const val MATH_TAG = "md-math"

/** 第 i 段占 `blocks` 的哪一截。 */
fun segRange(
    doc: Doc,
    i: Int,
): IntRange {
    val n = doc.blocks.size
    val from = if (i == 0) 0 else doc.segs.getOrNull(i - 1)?.coerceIn(0, n) ?: n
    val to = doc.segs.getOrNull(i)?.coerceIn(from, n) ?: n
    return from until to
}

/** 把第 i 段排成几块；`hl` 是那一段的高亮回包（没拉到 ⇒ 代码照纯字排）。 */
fun layoutSeg(
    doc: Doc,
    i: Int,
    hl: SegHl?,
    style: MdStyle,
): List<MdPiece> {
    val ctx = Ctx(hl?.hl.orEmpty(), style)
    return segRange(doc, i).map { ctx.block(doc.blocks[it]) }
}

/** 一段里每个行内公式的 TeX，照出现的先后。 */
fun mathTexOf(text: AnnotatedString): List<String> = text.getStringAnnotations(MATH_TAG, 0, text.length).map { it.item }

/** 图那个槽上的注记：值是图的地址（同时是行内槽的 id 后半）。 */
const val IMG_TAG = "md-img"

/** 一段里每张图的地址，照出现的先后。 */
fun imagesOf(text: AnnotatedString): List<String> = text.getStringAnnotations(IMG_TAG, 0, text.length).map { it.item }

/** 照三元组切：越过串尾的截在串尾，认不出的类当没着色，不成三的尾巴不看。 */
fun highlighted(
    x: String,
    hl: List<Long>,
    style: MdStyle,
): AnnotatedString =
    buildAnnotatedString {
        append(x)
        var at = 0L
        var i = 0
        while (i + 2 < hl.size) {
            val from = minOf(at + hl[i], x.length.toLong())
            val to = minOf(from + hl[i + 1], x.length.toLong())
            val cls = style.hl.getOrNull(hl[i + 2].coerceAtMost(Int.MAX_VALUE.toLong()).toInt())
            if (cls != null && to > from) addStyle(cls, from.toInt(), to.toInt())
            at = to
            i += 3
        }
    }

/** 第 i 段里的代码块（顶层与嵌在列表 · 引用 · 提示块 · 脚注里的，先序）：拉高亮交的就是它，回包的 `hl[n]` 对第 n 块。 */
fun codesOf(
    doc: Doc,
    i: Int,
): List<HlCode> {
    val out = mutableListOf<HlCode>()

    fun walk(bs: List<MdBlock>) {
        for (b in bs) {
            when (b) {
                is MdBlock.Code -> out += HlCode(lang = b.lang, x = b.x)
                is MdBlock.List -> b.items.forEach { walk(it.blocks) }
                is MdBlock.Quote -> walk(b.blocks)
                is MdBlock.Alert -> walk(b.blocks)
                is MdBlock.Notes -> b.items.forEach { walk(it.blocks) }
                is MdBlock.P, is MdBlock.H, is MdBlock.Math, is MdBlock.Table, MdBlock.Rule -> {}
            }
        }
    }
    walk(segRange(doc, i).map { doc.blocks[it] })
    return out
}

/** 一份正文的排版状态：建好了哪几段。建出来就有第 0 段（代码照纯字）；点了「展开」调 [expand]，往下接一段调 [buildNext]，拉到一段的高亮调 [paint]。 */
@Stable
class MdDocState(
    val doc: Doc,
    private val style: MdStyle,
) {
    val segCount: Int = doc.segs.size + 1
    private val built0 = mutableStateListOf<List<MdPiece>>()

    /** 已建的那几段，每段一串块。 */
    val pieces: List<List<MdPiece>> get() = built0

    val built: Int get() = built0.size

    private var asked by mutableStateOf(false)
    private val go = CompletableDeferred<Unit>()

    /** 还有没建的段、也还没点「展开」⇒ 画那一颗。 */
    val more: Boolean get() = built0.size < segCount && !asked

    /** 点了「展开」：余下几段由 [awaitExpand] 那一头一帧接一段建下去。 */
    fun expand() {
        asked = true
        go.complete(Unit)
    }

    suspend fun awaitExpand() = go.await()

    init {
        buildNext()
    }

    /** 建下一段（代码照纯字）；都建完了 ⇒ false。 */
    fun buildNext(): Boolean {
        if (built0.size >= segCount) return false
        built0 += layoutSeg(doc, built0.size, null, style)
        return true
    }

    /** 第 i 段拉到了高亮：带色重排那一段（还没建的段不动）。 */
    fun paint(
        i: Int,
        hl: SegHl,
    ) {
        if (i < built0.size) built0[i] = layoutSeg(doc, i, hl, style)
    }
}

private class Ctx(
    private val hl: List<List<Long>>,
    private val style: MdStyle,
) {
    /** 下一个代码块在先序里是第几个。 */
    private var code = 0

    fun blocks(bs: List<MdBlock>): List<MdPiece> = bs.map(::block)

    fun block(b: MdBlock): MdPiece =
        when (b) {
            is MdBlock.P -> MdPiece.Text(0, inlines(b.k))
            is MdBlock.H -> MdPiece.Text(b.level.coerceIn(1, 6), inlines(b.k))
            is MdBlock.Code -> MdPiece.Code(b.lang, highlighted(b.x, hl.getOrNull(code++).orEmpty(), style), b.cont)
            is MdBlock.Math -> MdPiece.Math(b.tex, b.m)
            is MdBlock.List -> MdPiece.Bullets(b.tight, b.items.mapIndexed { n, one -> item(b.start?.plus(n), one) })
            is MdBlock.Quote -> MdPiece.Quote(blocks(b.blocks))
            is MdBlock.Alert -> MdPiece.Alert(b.kind, blocks(b.blocks))
            is MdBlock.Table -> table(b)
            MdBlock.Rule -> MdPiece.Rule
            is MdBlock.Notes -> MdPiece.Notes(b.items.map { MdPiece.Note(it.n, blocks(it.blocks)) })
        }

    private fun item(
        number: Int?,
        it: MdItem,
    ) = MdPiece.Item(number, it.task, blocks(it.blocks))

    private fun table(b: MdBlock.Table): MdPiece.Table {
        val cols = maxOf(b.head.size, b.rows.maxOfOrNull { it.size } ?: 0)

        fun row(cells: List<List<Inl>>) = List(cols) { inlines(cells.getOrNull(it).orEmpty()) }
        return MdPiece.Table(List(cols) { b.align.getOrNull(it) ?: MdAlign.NONE }, row(b.head), b.rows.map(::row))
    }

    fun inlines(ks: List<Inl>): AnnotatedString = buildAnnotatedString { ks.forEach { inline(it) } }

    private fun AnnotatedString.Builder.inline(k: Inl) {
        when (k) {
            is Inl.Text -> append(k.value)
            is Inl.InlNode -> node(k.value)
        }
    }

    private fun AnnotatedString.Builder.node(n: InlNode) {
        when (n) {
            is InlNode.B -> withStyle(SpanStyle(fontWeight = FontWeight.Bold)) { n.k.forEach { inline(it) } }
            is InlNode.I -> withStyle(SpanStyle(fontStyle = FontStyle.Italic)) { n.k.forEach { inline(it) } }
            is InlNode.Del -> withStyle(SpanStyle(textDecoration = TextDecoration.LineThrough)) { n.k.forEach { inline(it) } }
            // 树里只有放行的 scheme（http · https · mailto）才成链接；点了交系统去开。
            is InlNode.A -> withLink(LinkAnnotation.Url(n.href, style.link)) { n.k.forEach { inline(it) } }
            else -> leaf(n)
        }
    }

    /** 不套别的行内的那几种：行内代码 · 公式 · 图 · 换行 · 脚注号 · 计划格引用。 */
    private fun AnnotatedString.Builder.leaf(n: InlNode) {
        when (n) {
            is InlNode.C -> withStyle(style.code) { append(n.x) }
            is InlNode.M -> {
                pushStringAnnotation(MATH_TAG, n.tex)
                appendInlineContent(MATH_TAG + n.tex, n.tex)
                pop()
            }
            // 图占一个行内槽（[IMG_TAG] 标着地址），取到之前 · 取不到画替代字；没有替代字的占一个零宽字（槽不能是空串）。
            is InlNode.Img ->
                withStyle(style.faint) {
                    pushStringAnnotation(IMG_TAG, n.src)
                    appendInlineContent(IMG_TAG + n.src, n.alt.ifEmpty { "​" })
                    pop()
                }
            InlNode.Br -> append('\n')
            is InlNode.Fn -> withStyle(style.fnRef) { append(n.n.toString()) }
            // 计划格引用：编号原样，后端写好的标题跟在后面、淡色（手机不接点）。
            is InlNode.Ref -> {
                append(n.x)
                if (n.label.isNotEmpty()) withStyle(style.faint) { append(" ${n.label}") }
            }
            // 套行内的那几种在 `node` 里排完了，到不了这里。
            is InlNode.B, is InlNode.I, is InlNode.Del, is InlNode.A -> Unit
        }
    }
}

// ───────────────────────────── 画 ─────────────────────────────

/** 主题给的那一份样式：高亮十三类取终端色票（类的先后跟着 `HlClass`），链接 · 行内代码 · 脚注号照令牌。 */
@Composable
fun rememberMdStyle(): MdStyle {
    val tokens = LocalAppTokens.current
    return remember(tokens) {
        MdStyle(
            code = SpanStyle(fontFamily = JetBrainsMono, fontSize = 0.9.em, background = tokens.bg2),
            link = TextLinkStyles(SpanStyle(color = tokens.link)),
            faint = SpanStyle(color = tokens.textFaint, fontStyle = FontStyle.Italic),
            fnRef = SpanStyle(color = tokens.link, fontSize = 0.75.em, baselineShift = BaselineShift.Superscript),
            hl =
                listOf(
                    SpanStyle(color = ThemeTokens.TermMagenta),
                    SpanStyle(color = ThemeTokens.TermGreen),
                    SpanStyle(color = ThemeTokens.TermBrightBlack, fontStyle = FontStyle.Italic),
                    SpanStyle(color = ThemeTokens.TermYellow),
                    SpanStyle(color = ThemeTokens.TermBlue),
                    SpanStyle(color = ThemeTokens.TermCyan),
                    SpanStyle(color = ThemeTokens.TermBrightYellow),
                    SpanStyle(color = ThemeTokens.TermRed),
                    SpanStyle(color = ThemeTokens.TermBrightCyan),
                    SpanStyle(color = ThemeTokens.TermBrightBlue),
                    SpanStyle(color = ThemeTokens.TermBrightGreen, background = tokens.diffAddBg),
                    SpanStyle(color = ThemeTokens.TermBrightRed, background = tokens.diffDelBg),
                    SpanStyle(color = ThemeTokens.TermBrightMagenta),
                ),
        )
    }
}

/** 一份正文的排版状态（树或主题变了重建，只建第 0 段）。 */
@Composable
fun rememberMdDocState(doc: Doc): MdDocState {
    val style = rememberMdStyle()
    return remember(doc, style) { MdDocState(doc, style) }
}

/** 一个顶层块排成一块（活卡那一路：不分段、不拉高亮，代码照纯字）。 */
fun layoutBlock(
    b: MdBlock,
    style: MdStyle,
): MdPiece = Ctx(emptyList(), style).block(b)

/**
 * 一串顶层块逐块画（活卡：那台只换了尾巴时，前面的块值不变）：按块号作键、块的值没变就沿用排好的那一块 ⇒
 * 前面没变的块不重排、跳过重组，只重建尾巴那一两块。
 */
@Composable
fun MdBlocksView(
    blocks: List<MdBlock>,
    modifier: Modifier = Modifier,
    color: Color = MaterialTheme.colorScheme.onSurface,
) {
    val tokens = LocalAppTokens.current
    val style = rememberMdStyle()
    val maths = remember(blocks) { inlineMathOf(blocks) }
    CompositionLocalProvider(LocalInlineMath provides maths) {
        Column(modifier, verticalArrangement = Arrangement.spacedBy(tokens.spacing.s)) {
            blocks.forEachIndexed { i, b ->
                key(i) {
                    val piece = remember(b, style) { layoutBlock(b, style) }
                    Piece(piece, color, tokens)
                }
            }
        }
    }
}

/** 照 [MdDocState] 里已建的那几段画；字可选、链接可点。 */
@Composable
fun MdDocView(
    state: MdDocState,
    modifier: Modifier = Modifier,
    color: Color = MaterialTheme.colorScheme.onSurface,
) {
    val tokens = LocalAppTokens.current
    val maths = remember(state.doc) { inlineMathOf(state.doc.blocks) }
    CompositionLocalProvider(LocalInlineMath provides maths) {
        SelectionContainer(modifier) {
            Column(verticalArrangement = Arrangement.spacedBy(tokens.spacing.s)) {
                state.pieces.forEach { seg -> seg.forEach { Piece(it, color, tokens) } }
            }
        }
    }
}

@Composable
private fun Pieces(
    pieces: List<MdPiece>,
    color: Color,
    tokens: AppTokens,
    gap: Dp = tokens.spacing.s,
) {
    Column(verticalArrangement = Arrangement.spacedBy(gap)) { pieces.forEach { Piece(it, color, tokens) } }
}

@Composable
private fun Piece(
    p: MdPiece,
    color: Color,
    tokens: AppTokens,
) {
    when (p) {
        is MdPiece.Text -> Prose(p.text, textStyleOf(p.level), color)
        is MdPiece.Code -> CodeView(p, color, tokens)
        is MdPiece.Math ->
            Box(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())) {
                MathBlock(p, color, MaterialTheme.typography.bodyLarge.fontSize.value * 1.1f)
            }
        is MdPiece.Bullets ->
            Column(verticalArrangement = Arrangement.spacedBy(if (p.tight) tokens.spacing.xxs else tokens.spacing.s)) {
                p.items.forEach { ItemView(it, color, tokens) }
            }
        is MdPiece.Quote -> Barred(tokens.borderStrong, tokens) { Pieces(p.pieces, MaterialTheme.colorScheme.onSurfaceVariant, tokens) }
        is MdPiece.Alert -> Barred(alertColor(p.kind, tokens), tokens) { Pieces(p.pieces, color, tokens) }
        is MdPiece.Table -> TableView(p, color, tokens)
        MdPiece.Rule ->
            Box(
                Modifier
                    .fillMaxWidth()
                    .height(1.dp)
                    .background(tokens.borderFaint),
            )
        is MdPiece.Notes ->
            Column(
                Modifier
                    .fillMaxWidth()
                    .drawBehind { drawRect(tokens.borderFaint, size = size.copy(height = 1.dp.toPx())) }
                    .padding(top = tokens.spacing.s),
                verticalArrangement = Arrangement.spacedBy(tokens.spacing.xxs),
            ) {
                p.items.forEach { n ->
                    Row {
                        Text(n.n.toString(), style = MaterialTheme.typography.bodySmall, color = tokens.link, modifier = Modifier.width(tokens.spacing.lg))
                        Pieces(n.pieces, MaterialTheme.colorScheme.onSurfaceVariant, tokens, tokens.spacing.xxs)
                    }
                }
            }
    }
}

@Composable
private fun textStyleOf(level: Int): TextStyle =
    when (level) {
        0 -> MaterialTheme.typography.bodyLarge
        1, 2 -> MaterialTheme.typography.titleMedium
        else -> MaterialTheme.typography.bodyLarge.copy(fontWeight = MaterialTheme.typography.titleMedium.fontWeight)
    }

/** 一段字：带行内公式的给每条 TeX 备一个行内槽（量好图的大小再排）；带图的取到一张换上一张（[mdImageSlots]）。 */
@Composable
private fun Prose(
    text: AnnotatedString,
    style: TextStyle,
    color: Color,
) {
    val texs = remember(text) { mathTexOf(text) }
    val hasImg = remember(text) { imagesOf(text).isNotEmpty() }
    if (texs.isEmpty() && !hasImg) {
        Text(text, style = style, color = color)
        return
    }
    val math = mathSlots(texs, style, color)
    if (!hasImg) {
        Text(text, style = style, color = color, inlineContent = math)
        return
    }
    BoxWithConstraints {
        Text(text, style = style, color = color, inlineContent = math + mdImageSlots(text, maxWidth))
    }
}

/** 每条 TeX 一个行内槽：大小照画单 × 字号；树里那一条没画单 ⇒ 不备槽，那一处照原文的字。 */
@Composable
private fun mathSlots(
    texs: List<String>,
    style: TextStyle,
    color: Color,
): Map<String, InlineTextContent> {
    val density = LocalDensity.current
    val sizePx = with(density) { style.fontSize.toPx() }
    val boxes = LocalInlineMath.current
    return remember(texs, sizePx, color, boxes) {
        texs
            .distinct()
            .mapNotNull { tex ->
                val m = boxes[tex] ?: return@mapNotNull null
                val s = mathSizePx(m, sizePx)
                val w = with(density) { s.width.toSp() }
                val h = with(density) { s.height.toSp() }
                (MATH_TAG + tex) to
                    InlineTextContent(Placeholder(w, h, PlaceholderVerticalAlign.TextCenter)) {
                        MathCanvas(m, sizePx, color, Modifier.fillMaxWidth().fillMaxHeight())
                    }
            }.toMap()
    }
}

/** 块级公式：照画单画；核心排不出（画单空）⇒ 照原文等宽排。 */
@Composable
private fun MathBlock(
    p: MdPiece.Math,
    color: Color,
    sizeSp: Float,
) {
    val m = p.m
    if (m == null) {
        Text(p.tex, style = monoBody, color = color)
        return
    }
    val density = LocalDensity.current
    val sizePx = with(density) { sizeSp * fontScale * this.density }
    val s = mathSizePx(m, sizePx)
    with(density) {
        MathCanvas(m, sizePx, color, Modifier.size(s.width.toDp(), s.height.toDp()))
    }
}

@Composable
private fun CodeView(
    p: MdPiece.Code,
    color: Color,
    tokens: AppTokens,
) {
    Column(
        Modifier
            .fillMaxWidth()
            .clip(RoundedCornerShape(tokens.radius.l))
            .background(tokens.bg2),
    ) {
        if (!p.cont && p.lang != null) {
            Text(
                p.lang,
                style = monoSmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = tokens.spacing.sm, end = tokens.spacing.sm, top = tokens.spacing.xs),
            )
        }
        Box(Modifier.fillMaxWidth().horizontalScroll(rememberScrollState())) {
            Text(p.text, style = monoBody, color = color, softWrap = false, modifier = Modifier.padding(horizontal = tokens.spacing.sm, vertical = tokens.spacing.xs))
        }
    }
}

@Composable
private fun ItemView(
    it: MdPiece.Item,
    color: Color,
    tokens: AppTokens,
) {
    Row {
        Box(Modifier.width(tokens.spacing.lg), contentAlignment = Alignment.TopStart) {
            when {
                it.task != null ->
                    PhIcon(
                        if (it.task) Ph.CheckSquare else Ph.Square,
                        contentDescription = null,
                        size = 16.dp,
                        tint = if (it.task) tokens.accent else MaterialTheme.colorScheme.onSurfaceVariant,
                        modifier = Modifier.padding(top = 3.dp),
                    )
                it.number != null -> Text("${it.number}.", style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
                else -> Text("•", style = MaterialTheme.typography.bodyLarge, color = MaterialTheme.colorScheme.onSurfaceVariant)
            }
        }
        Pieces(it.pieces, color, tokens, tokens.spacing.xxs)
    }
}

/** 引用与提示块：左边一道竖条。 */
@Composable
private fun Barred(
    bar: Color,
    tokens: AppTokens,
    content: @Composable () -> Unit,
) {
    Row(Modifier.fillMaxWidth().height(IntrinsicSize.Min)) {
        Box(
            Modifier
                .width(3.dp)
                .fillMaxHeight()
                .background(bar),
        )
        Box(Modifier.padding(start = tokens.spacing.sm)) { content() }
    }
}

@Composable
private fun alertColor(
    kind: AlertKind,
    tokens: AppTokens,
): Color =
    when (kind) {
        AlertKind.NOTE -> tokens.link
        AlertKind.TIP -> tokens.success
        AlertKind.IMPORTANT -> tokens.bgwork
        AlertKind.WARNING -> tokens.warn
        AlertKind.CAUTION -> MaterialTheme.colorScheme.error
    }

/** 表格：网格，列宽取那一列最宽的格（一格最宽 240 dp，再宽折行），整张表横着滑。 */
@Composable
private fun TableView(
    p: MdPiece.Table,
    color: Color,
    tokens: AppTokens,
) {
    val cols = p.head.size
    if (cols == 0) return
    val line = tokens.borderFaint
    val cellPad = Modifier.padding(horizontal = tokens.spacing.xs, vertical = tokens.spacing.xxs)
    val bodyStyle = MaterialTheme.typography.bodyMedium
    val headStyle = bodyStyle.copy(fontWeight = MaterialTheme.typography.titleMedium.fontWeight)
    Box(Modifier.horizontalScroll(rememberScrollState())) {
        Layout(
            content = {
                (listOf(p.head) + p.rows).forEachIndexed { r, row ->
                    row.forEachIndexed { c, cell ->
                        Box(
                            Modifier
                                .border(0.5.dp, line)
                                .then(if (r == 0) Modifier.background(tokens.bg2) else Modifier)
                                .widthIn(max = 240.dp)
                                .then(cellPad),
                        ) {
                            Text(cell, style = if (r == 0) headStyle else bodyStyle, color = color, textAlign = alignOf(p.align[c]), modifier = Modifier.fillMaxWidth())
                        }
                    }
                }
            },
        ) { ms, _ ->
            val rows = ms.size / cols
            val widths = IntArray(cols) { c -> (0 until rows).maxOf { r -> ms[r * cols + c].maxIntrinsicWidth(Constraints.Infinity) } }
            val heights = IntArray(rows) { r -> (0 until cols).maxOf { c -> ms[r * cols + c].minIntrinsicHeight(widths[c]) } }
            val placed = ms.mapIndexed { i, m -> m.measure(Constraints.fixed(widths[i % cols], heights[i / cols])) }
            layout(widths.sum(), heights.sum()) {
                var y = 0
                for (r in 0 until rows) {
                    var x = 0
                    for (c in 0 until cols) {
                        placed[r * cols + c].place(x, y)
                        x += widths[c]
                    }
                    y += heights[r]
                }
            }
        }
    }
}

private fun alignOf(a: MdAlign): TextAlign =
    when (a) {
        MdAlign.NONE, MdAlign.LEFT -> TextAlign.Start
        MdAlign.CENTER -> TextAlign.Center
        MdAlign.RIGHT -> TextAlign.End
    }
