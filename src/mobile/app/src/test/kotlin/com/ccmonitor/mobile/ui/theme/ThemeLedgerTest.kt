package com.ccmonitor.mobile.ui.theme

import androidx.compose.material3.ColorScheme
import androidx.compose.material3.darkColorScheme
import androidx.compose.ui.graphics.Color
import com.ccmonitor.mobile.core.ui.theme.AppColorScheme
import com.ccmonitor.mobile.core.ui.theme.ThemeTokens
import com.ccmonitor.mobile.testing.CopyTableForTests
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机的颜色只从桌面那份令牌来（`src/shared/theme/tokens.css`，两端唯一的正本；`core-ui` 构建时生成 `ThemeTokens`）。
 *
 * - ① 两向对账：`tokens.css` 里每个颜色令牌，要么在映射文件里被 `ThemeTokens.X` 引到，要么列在 [MOBILE_UNUSED]（带理由）；
 *   名单里的不许又被引；映射里引到的必须在 `tokens.css` 里。新加一个颜色令牌不表态 ⇒ 红。
 * - ② 手机全部生产源码不写色值字面量：`Color(0x…)` · `Color(r, g, b)` · `Color.Red` 这类具名色（同网页「组件里不写色值」）。
 *   `Color.Transparent` / `Color.Unspecified` 不是色值，许用。
 *
 * 不防：映射到错的令牌（`primary` 映成 `--error`）—— 那是设计问题，截图对稿时看。
 */
class ThemeLedgerTest {
    private val repo: File =
        generateSequence(File("").absoluteFile) { it.parentFile }
            .first { File(it, "src/shared/copy/table.json").isFile }
    private val mobile = File(repo, "src/mobile")
    private val tokensCss = File(repo, "src/shared/theme/tokens.css")

    /** 只做映射的那几份：哪个槽 / 字段取哪个令牌。 */
    private val mappingFiles =
        listOf(
            "core-ui/src/main/kotlin/com/ccmonitor/mobile/core/ui/theme/AppTheme.kt",
            "core-ui/src/main/kotlin/com/ccmonitor/mobile/core/ui/theme/Tokens.kt",
            "app/src/main/kotlin/com/ccmonitor/mobile/ui/theme/TerminalTheme.kt",
        )

    /** 手机不用的颜色令牌 → 为什么。 */
    private val MOBILE_UNUSED =
        mapOf(
            "--group-fill" to "标签页栏里一个组画成的那一块：手机没有标签页栏",
            "--live" to "与 --success 同值的旧名；手机只用 --success",
            "--border" to "--border-strong 的别名",
            "--overlay-hover" to "触屏没有悬停",
            "--state-hover" to "触屏没有悬停",
            "--overlay-dim" to "桌面浮层叠暗；手机遮罩只用 --overlay-backdrop",
            "--overlay-deep" to "同上",
            "--overlay-shadow" to "同上；手机阴影走 --shadow-*",
            "--state-focus" to "= --accent；焦点环只在接了实体键盘时画，由 M3 自己按 primary 系画",
            "--field-border" to "= --border-medium，已经是 outline 槽",
            "--field-border-focus" to "= --accent，已经是 AppTokens.accent",
            "--field-placeholder" to "= --text-faint，已经是 AppTokens.textFaint",
        )

    private fun read(rel: String): String {
        val f = File(mobile, rel)
        assertTrue("映射文件不在（判据自己失效了）：${f.path}", f.isFile)
        return f.readText()
    }

    /** `tokens.css` 第一块 `:root` 里的 `--名: 值`。 */
    private fun rootTokens(): Map<String, String> {
        assertTrue("令牌正本不在：${tokensCss.path}", tokensCss.isFile)
        val css = tokensCss.readText().replace(Regex("""/\*[\s\S]*?\*/"""), "")
        val at = css.indexOf(":root {")
        assertTrue("tokens.css 里没有 :root 块", at >= 0)
        val body = css.substring(at + ":root {".length, css.indexOf('}', at))
        return Regex("""(--[a-z0-9-]+)\s*:\s*([^;]+);""").findAll(body).associate { it.groupValues[1] to it.groupValues[2].trim() }
    }

    private fun colorTokens(): Set<String> {
        val all = rootTokens()

        fun isColor(v: String): Boolean =
            v.startsWith("#") ||
                v.startsWith("rgb") ||
                v.startsWith("color-mix(") ||
                Regex("""^var\((--[a-z0-9-]+)\)$""").find(v)?.let { all[it.groupValues[1]]?.let(::isColor) } == true
        return all.filterValues(::isColor).keys
    }

    private fun pascal(token: String): String =
        token.removePrefix("--").split('-').joinToString("") { it.replaceFirstChar(Char::uppercaseChar) }

    @Test
    fun everyColorTokenIsMappedOrListedAsUnusedAndNothingElse() {
        val colors = colorTokens()
        assertTrue("tokens.css 里一个颜色令牌都没认出来（判据自己失效了）", colors.size > 40)
        val byPascal = rootTokens().keys.associateBy(::pascal)
        val used =
            mappingFiles
                .flatMap { f -> Regex("""\bThemeTokens\.([A-Z][A-Za-z0-9]*)""").findAll(KotlinSourceScanner.codeOnlyDroppingLiterals(read(f))).map { it.groupValues[1] }.toList() }
                .toSet()
        val unknown = used.filterNot(byPascal::containsKey)
        assertEquals("映射引了 tokens.css 里没有的令牌", emptyList<String>(), unknown)
        val usedTokens = used.mapNotNull(byPascal::get).toSet()
        val silent = colors.filterNot { it in usedTokens || it in MOBILE_UNUSED }
        assertEquals("这几个颜色令牌手机没表态：映射进 AppTheme / AppTokens / 终端画布，或列进 MOBILE_UNUSED 写理由", emptyList<String>(), silent)
        val both = MOBILE_UNUSED.keys.filter { it in usedTokens }
        assertEquals("列在「手机不用」里却又被映射引到", emptyList<String>(), both)
        val stale = MOBILE_UNUSED.keys.filterNot { it in colors }
        assertEquals("「手机不用」名单里有 tokens.css 已经没有的（或不是颜色）", emptyList<String>(), stale)
    }

    /**
     * ③ M3 的每个颜色槽都显式给了：逐槽比 M3 自带的 `darkColorScheme()` 默认值，相等 ⇒ 漏设了（M3 默认紫就是这么漏出来的：
     * 今天的悬浮按钮 · tab 抽屉选中项取 `primaryContainer`、终端错误条取 `errorContainer`）。槽的清单从 `ColorScheme` 的 getter 现读。
     */
    @Test
    fun everyColorSchemeSlotIsSetExplicitly() {
        val getters =
            ColorScheme::class.java.methods.filter {
                it.parameterCount == 0 && it.returnType == java.lang.Long.TYPE && it.name.startsWith("get")
            }
        assertTrue("ColorScheme 的颜色槽一个都没读出来（判据自己失效了）", getters.size >= 30)
        val m3 = darkColorScheme()
        val unset = getters.filter { it.invoke(AppColorScheme) == it.invoke(m3) }.map { it.name.removePrefix("get").substringBefore('-') }
        assertEquals("这几个 M3 槽没在 AppTheme.kt 里映射到令牌（落回了 M3 默认值）", emptyList<String>(), unset)
    }

    /**
     * ④ 生成器照抄得对：`ThemeTokens` 里每个颜色常量 ＝ 照 `tokens.css` 那一格算出来的值（`#…` 照抄；`var()` 跟到底；
     * `color-mix(in srgb, var(--x) N%, transparent)` ＝ x 的透明度乘 N%）。常量从生成物现读（反射 getter）。
     */
    @Test
    fun generatedColorsEqualTheCss() {
        val all = rootTokens()

        fun argb(v: String): Long {
            Regex("""^#([0-9a-fA-F]{3,8})$""").find(v)?.let { m ->
                val d = m.groupValues[1].let { h -> if (h.length <= 4) h.map { "$it$it" }.joinToString("") else h }
                val a = if (d.length == 8) d.substring(6).toLong(16) else 0xFFL
                return (a shl 24) or d.substring(0, 6).toLong(16)
            }
            Regex("""^var\((--[a-z0-9-]+)\)$""").find(v)?.let { return argb(all.getValue(it.groupValues[1])) }
            Regex("""^color-mix\(in srgb, var\((--[a-z0-9-]+)\) ([0-9.]+)%, transparent\)$""").find(v)?.let { m ->
                val base = argb(all.getValue(m.groupValues[1]))
                val a = Math.round(((base ushr 24) and 0xFF) * m.groupValues[2].toDouble() / 100)
                return (a shl 24) or (base and 0xFFFFFF)
            }
            error("判据认不出这个颜色值：$v")
        }
        val getters = ThemeTokens::class.java.methods.filter { it.parameterCount == 0 && it.returnType == java.lang.Long.TYPE }
        val byName = getters.associateBy { it.name.removePrefix("get").substringBefore('-') }
        val wrong =
            colorTokens().mapNotNull { t ->
                val g = byName[pascal(t)] ?: return@mapNotNull "$t：ThemeTokens 里没有 ${pascal(t)}"
                val got = g.invoke(ThemeTokens) as Long
                val want = Color(argb(all.getValue(t))).value.toLong()
                if (got != want) "$t：生成 ${java.lang.Long.toHexString(got)} ≠ 应为 ${java.lang.Long.toHexString(want)}" else null
            }
        assertEquals("生成的颜色与 tokens.css 对不上", emptyList<String>(), wrong)
    }

    private fun mainSources(): List<File> =
        mobile
            .listFiles()
            .orEmpty()
            .map { File(it, "src/main") }
            .filter { it.isDirectory }
            .flatMap { d -> d.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }

    @Test
    fun noColorLiteralInMobileSources() {
        val files = mainSources()
        assertTrue("手机生产源码一份都没扫到（判据自己失效了）", files.size > 100)
        val literal = Regex("""\bColor\(\s*(0x|[0-9])""")
        val named = Regex("""\bColor\.(Red|Green|Blue|White|Black|Gray|LightGray|DarkGray|Yellow|Cyan|Magenta)\b""")
        val hits =
            files.flatMap { f ->
                KotlinSourceScanner.codeOnlyDroppingLiterals(f.readText()).lines().withIndex().mapNotNull { (i, l) ->
                    if (literal.containsMatchIn(l) || named.containsMatchIn(l)) "${f.relativeTo(mobile)}:${i + 1}  ${l.trim()}" else null
                }
            }
        assertEquals("手机代码里写了色值：换成 tokens.css 的令牌（经 MaterialTheme 槽或 LocalAppTokens）", emptyList<String>(), hits)
    }

    /**
     * 图标位上的字符（稿 `外观对齐` 第 13 节）一律换 Phosphor：界面代码（app · core-ui）的字面量、
     * 以及它引到的文案（键换成那句 zh 再看）里，整条就是一个这样的字符，或以「字符＋空格」开头、以「空格＋字符」结尾 ⇒ 红。
     * 方向箭头另算：它们也是方向键的名字（按钮的数据），按钮预览里的「→ 动作」是说明文字（稿第 13 节留成字）；
     * 只有「← 返回」这种当返回图标用的形状红。
     */
    @Test
    fun noGlyphStandsInForAnIcon() {
        val icons = listOf("☰", "⊕", "💬", "📁", "📄", "⌨", "▷", "⚙", "▸", "▾", "▤", "❯", "⇌", "⏎", "⬆", "★", "☆", "＋", "+", "×", "›", "▶")
        val arrows = listOf("←")
        // 内容记号，不是图标（稿第 13 节）：diff 行首的 + / −。
        // 键帽名 → 图标的那张表本身（`KeyCap.kt`：库里的按钮名是数据，这张表就是把那几个字符画成图标的那一处）。
        // 导航路由拼目录参数：`URLEncoder` 把空格编成「+」，换成 `%20`（`Screen.kt`，不上屏）。
        val contentMarks = mapOf("DiffView.kt" to setOf("+"), "KeyCap.kt" to setOf("↑", "↓", "←", "→", "⇌", "▤", "＋", "+"), "Screen.kt" to setOf("+"))
        val files =
            listOf("app", "core-ui").flatMap { m ->
                File(mobile, "$m/src/main").walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
            }
        val hits =
            files.flatMap { f ->
                KotlinSourceScanner.literalsOf(f.readText()).mapNotNull { lit ->
                    val s = CopyTableForTests.shown(lit.body)
                    val bad =
                        contentMarks[f.name]?.contains(s) != true &&
                            (
                                icons.any { g -> s == g || s.startsWith("$g ") || s.endsWith(" $g") } ||
                                    arrows.any { g -> s.startsWith("$g ") || s.endsWith(" $g") }
                            )
                    if (bad) "${f.name}:${lit.line}  「$s」" else null
                }
            }
        assertEquals("字符当图标用：换成 PhIcon（core-ui 的 Ph 表）", emptyList<String>(), hits)
    }
}
