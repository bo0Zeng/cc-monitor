package com.ccmonitor.mobile.ui.claude

import android.content.Context
import android.graphics.Typeface
import android.net.Uri
import android.util.TypedValue
import android.widget.TextView
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.Composable
import androidx.compose.runtime.CompositionLocalProvider
import androidx.compose.runtime.ProvidableCompositionLocal
import androidx.compose.runtime.remember
import androidx.compose.runtime.staticCompositionLocalOf
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.toArgb
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.res.ResourcesCompat
import androidx.core.graphics.ColorUtils
import com.ccmonitor.mobile.core.ui.theme.LocalAppTokens
import io.noties.markwon.AbstractMarkwonPlugin
import io.noties.markwon.LinkResolverDef
import io.noties.markwon.Markwon
import io.noties.markwon.MarkwonConfiguration
import io.noties.markwon.core.MarkwonTheme
import io.noties.markwon.ext.latex.JLatexMathPlugin
import io.noties.markwon.ext.strikethrough.StrikethroughPlugin
import io.noties.markwon.ext.tables.TablePlugin
import io.noties.markwon.linkify.LinkifyPlugin
import com.ccmonitor.mobile.core.ui.R as UiR

/**
 * 整个阅读面共享一个 Markwon 实例（避免每张 markdown 卡重建解析器 + 插件链）。
 * [ClaudeReadingPane] 在顶层 provide；[MarkdownText] 取用，无 provide 时（独立预览）回退自建。
 */
val LocalMarkwon: ProvidableCompositionLocal<Markwon?> = staticCompositionLocalOf { null }

/** 按当前主题色构建 Markwon（颜色变了才重建）。代码块用打包的 JetBrains Mono（缺则系统等宽）。 */
@Composable
fun rememberMarkwon(textColor: Color = MaterialTheme.colorScheme.onSurface): Markwon {
    val context = LocalContext.current
    val tokens = LocalAppTokens.current
    val linkArgb = MaterialTheme.colorScheme.primary.toArgb()
    val codeBgArgb = tokens.surfaceHigh.toArgb()
    val codeTextArgb = textColor.toArgb()
    return remember(context, linkArgb, codeBgArgb, codeTextArgb) {
        buildMarkwon(context, linkArgb, codeBgArgb, codeTextArgb)
    }
}

private fun buildMarkwon(
    context: Context,
    linkArgb: Int,
    codeBgArgb: Int,
    codeTextArgb: Int,
): Markwon {
    val mono: Typeface =
        runCatching {
            ResourcesCompat.getFont(context, UiR.font.jetbrains_mono_regular)
        }.getOrNull() ?: Typeface.MONOSPACE
    // 按代码块背景亮度选暗或亮的高亮主题（传入本主题色保持一致）。Prism4j 生成类的引用关在 Java 侧。
    // 强制不透明：luminance 判定与高亮主题背景都按不透明算才稳。
    val opaqueCodeBg = codeBgArgb or 0xFF000000.toInt()
    val darkCode = ColorUtils.calculateLuminance(opaqueCodeBg) < 0.5
    // LaTeX 数学字号用 px，与下方 TextView 的 15sp 一致。
    val mathTextSize = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_SP, 15f, context.resources.displayMetrics)
    return Markwon
        .builder(context)
        .usePlugin(StrikethroughPlugin.create())
        .usePlugin(TablePlugin.create(context))
        .usePlugin(LinkifyPlugin.create())
        // 语法高亮在 AbstractMarkwonPlugin 之前注册，让 JetBrains Mono 与代码块底色最后生效；
        // 高亮按 token 打 ForegroundColorSpan，不受插件顺序影响。
        .usePlugin(PrismSyntaxHighlight.create(codeBgArgb, darkCode))
        // 只支持块级数学 `$$..$$`（独占行）。markwon 4.6.2 的行内数学需要 MarkwonInlineParserPlugin，且只认成对 `$$`、
        // 不认单 `$`；开 inlinesEnabled 而不加 inline-parser 会在 build() 抛 IllegalStateException。
        .usePlugin(JLatexMathPlugin.create(mathTextSize))
        .usePlugin(
            object : AbstractMarkwonPlugin() {
                override fun configureTheme(builder: MarkwonTheme.Builder) {
                    builder
                        .codeTypeface(mono)
                        .codeBlockTypeface(mono)
                        .codeBackgroundColor(codeBgArgb)
                        .codeBlockBackgroundColor(codeBgArgb)
                        .codeTextColor(codeTextArgb)
                        .codeBlockTextColor(codeTextArgb)
                        .linkColor(linkArgb)
                }

                // 安全：阅读面渲染的是远端可控的内容，只放行 http/https/mailto，拦掉 intent:/file:/content: 等
                // 可植入恶意的 scheme（默认 LinkResolver 会无差别 startActivity）。
                override fun configureConfiguration(builder: MarkwonConfiguration.Builder) {
                    val def = LinkResolverDef()
                    builder.linkResolver { view, link ->
                        when (runCatching { Uri.parse(link).scheme?.lowercase() }.getOrNull()) {
                            "http", "https", "mailto" -> def.resolve(view, link)
                            else -> Unit // 丢弃非 web scheme 链接（不 startActivity）
                        }
                    }
                }
            },
        ).build()
}

/** 顶层包裹：为子树提供共享 Markwon。 */
@Composable
fun ProvideMarkwon(content: @Composable () -> Unit) {
    CompositionLocalProvider(LocalMarkwon provides rememberMarkwon(), content = content)
}

/**
 * Markdown 渲染（Markwon 是 View 系，经 [AndroidView] 进 Compose）。
 * 代码块语法高亮经 [PrismSyntaxHighlight]（Prism4j）在 [buildMarkwon] 接入。
 */
@Composable
fun MarkdownText(
    markdown: String,
    modifier: Modifier = Modifier,
    color: Color = MaterialTheme.colorScheme.onSurface,
) {
    val markwon = LocalMarkwon.current ?: rememberMarkwon(color)
    val linkArgb = MaterialTheme.colorScheme.primary.toArgb()
    val textArgb = color.toArgb()
    AndroidView(
        modifier = modifier,
        factory = { ctx ->
            TextView(ctx).apply {
                setTextColor(textArgb)
                setLinkTextColor(linkArgb)
                textSize = 15f
                setLineSpacing(0f, 1.25f)
                setTextIsSelectable(true)
            }
        },
        update = { tv ->
            tv.setTextColor(textArgb)
            tv.setLinkTextColor(linkArgb)
            markwon.setMarkdown(tv, markdown)
        },
    )
}
