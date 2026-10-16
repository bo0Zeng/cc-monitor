plugins {
    alias(libs.plugins.android.library)
    alias(libs.plugins.kotlin.android)
    alias(libs.plugins.kotlin.compose)
}

android {
    namespace = "com.ccmonitor.mobile.core.ui"
    // compileSdk / minSdk / compileOptions / jvmTarget 在根 subprojects{} 里统一配置。
    buildFeatures {
        compose = true
    }
}

// 设计令牌：和桌面同一份 `src/shared/theme/tokens.css`（两端唯一的正本），Kotlin 侧不另写值（`Color.kt` 已删）。
// 构建时读它第一块 `:root`，把每个 `--名: 值` 换成 `ThemeTokens` 里一个常量：
//   颜色（`#…` · `rgb()` · `var()` · `color-mix(in srgb, var(--x) N%, transparent)`）⇒ `Color(0x…)` 定值；
//   `--font-size-*` 的 `px` ⇒ `sp`，其余 `px` ⇒ `dp`；无单位数 ⇒ `Float`；`ms` / `s` ⇒ 毫秒 `Int`；
//   `--font-*` 字体栈 ⇒ `String`；阴影 `0 6px 24px #…` ⇒ `ThemeShadow`。
// 名字不合 `--族-角色[-变体]`、或值读不懂 ⇒ 构建失败，不猜。哪个槽取哪个令牌只写在 `AppTheme.kt` · `Tokens.kt`（判据 `ThemeLedgerTest`）。
val genThemeTokens =
    tasks.register("genThemeTokens") {
        // 输入输出只在任务里定：doLast 里引脚本顶层的 val 会把整个脚本对象带进配置缓存（存不下）。
        val tokensCss = rootProject.file("../shared/theme/tokens.css")
        val themeGenDir = layout.buildDirectory.dir("generated/themeTokens")
        inputs.file(tokensCss)
        outputs.dir(themeGenDir)
        doLast {
            val css = tokensCss.readText().replace(Regex("""/\*[\s\S]*?\*/"""), "")
            val at = css.indexOf(":root {")
            if (at < 0) throw GradleException("tokens.css 里没有 :root 块")
            val body = css.substring(at + ":root {".length, css.indexOf('}', at))
            val decls =
                body
                    .split(';')
                    .map { it.trim().replace(Regex("""\s+"""), " ") }
                    .filter { it.startsWith("--") }
                    .map { it.substringBefore(':').trim() to it.substringAfter(':').trim() }
            val raw = decls.toMap()
            val nameOk = Regex("""^--[a-z][a-z0-9]*(-[a-z0-9]+)*$""")
            decls.forEach { (n, _) -> if (!nameOk.matches(n)) throw GradleException("tokens.css 令牌名不合命名语法：$n") }

            fun bad(
                n: String,
                v: String,
            ): Nothing = throw GradleException("tokens.css 的 $n 值读不懂：$v（生成器只认颜色 · px · 数 · 时长 · 字体栈 · 阴影）")

            fun hexArgb(h: String): Long? {
                if (!h.startsWith("#")) return null
                val d = h.removePrefix("#")
                val full =
                    when (d.length) {
                        3, 4 -> d.map { "$it$it" }.joinToString("")
                        6, 8 -> d
                        else -> return null
                    }
                if (!full.all { it.isDigit() || it.lowercaseChar() in 'a'..'f' }) return null
                val rgb = full.substring(0, 6).toLong(16)
                val a = if (full.length == 8) full.substring(6).toLong(16) else 0xFFL
                return (a shl 24) or rgb
            }

            fun color(
                n: String,
                v: String,
            ): Long? {
                hexArgb(v)?.let { return it }
                Regex("""^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)(?:\s*[,/]\s*([0-9.]+)(%?))?\s*\)$""").find(v)?.let { m ->
                    val (r, g, b) = (1..3).map { m.groupValues[it].toLong() }
                    val av =
                        m.groupValues[4]
                            .takeIf { it.isNotEmpty() }
                            ?.toDouble()
                            ?.let { if (m.groupValues[5] == "%") it / 100 else it } ?: 1.0
                    return (Math.round(av * 255) shl 24) or (r shl 16) or (g shl 8) or b
                }
                Regex("""^var\((--[a-z0-9-]+)\)$""").find(v)?.let { m ->
                    val ref = m.groupValues[1]
                    return color(ref, raw[ref] ?: bad(n, v))
                }
                Regex("""^color-mix\(in srgb, var\((--[a-z0-9-]+)\) ([0-9.]+)%, transparent\)$""").find(v)?.let { m ->
                    val ref = m.groupValues[1]
                    val base = color(ref, raw[ref] ?: bad(n, v)) ?: bad(n, v)
                    val a = Math.round(((base ushr 24) and 0xFF) * m.groupValues[2].toDouble() / 100)
                    return (a shl 24) or (base and 0xFFFFFF)
                }
                return null
            }

            fun kname(n: String) = n.removePrefix("--").split('-').joinToString("") { it.replaceFirstChar(Char::uppercaseChar) }

            fun hex(c: Long) = "Color(0x" + c.toString(16).uppercase().padStart(8, '0') + ")"

            fun num(s: String) = s.toDouble().let { if (it == Math.floor(it)) it.toLong().toString() else it.toString() }

            fun lit(s: String) = "\"" + s.replace("\\", "\\\\").replace("\"", "\\\"").replace("$", "\\$") + "\""

            fun value(
                n: String,
                v: String,
            ): String {
                color(n, v)?.let { return hex(it) }
                Regex("""^var\((--[a-z0-9-]+)\)$""").find(v)?.let { return value(n, raw[it.groupValues[1]] ?: bad(n, v)) }
                Regex("""^(-?[0-9.]+)px$""").find(v)?.let { return num(it.groupValues[1]) + if (n.startsWith("--font-size-")) ".sp" else ".dp" }
                Regex("""^([0-9.]+)ms$""").find(v)?.let { return num(it.groupValues[1]) }
                Regex("""^([0-9.]+)s$""").find(v)?.let { return Math.round(it.groupValues[1].toDouble() * 1000).toString() }
                Regex("""^-?[0-9.]+$""").find(v)?.let { return num(v) + "f" }
                Regex("""^(-?[0-9.]+)(?:px)? (-?[0-9.]+)px ([0-9.]+)px (#[0-9a-fA-F]+)$""").find(v)?.let { m ->
                    val c = hexArgb(m.groupValues[4]) ?: bad(n, v)
                    return "ThemeShadow(x = ${num(m.groupValues[1])}.dp, y = ${num(m.groupValues[2])}.dp, blur = ${num(m.groupValues[3])}.dp, color = ${hex(c)})"
                }
                if (n.startsWith("--font-") && !n.startsWith("--font-size-") && !n.startsWith("--font-weight-")) return lit(v)
                bad(n, v)
            }
            val lines = decls.joinToString("\n") { (n, v) -> "    /** `$n: $v` */\n    val ${kname(n)} = ${value(n, v)}" }
            val out = themeGenDir.get().file("com/ccmonitor/mobile/core/ui/theme/ThemeTokens.kt").asFile
            out.parentFile.mkdirs()
            out.writeText(
                """
                |// 生成物：core-ui/build.gradle.kts 的 genThemeTokens 从 src/shared/theme/tokens.css 抄来（两端唯一的正本），勿手改。
                |package com.ccmonitor.mobile.core.ui.theme
                |
                |import androidx.compose.ui.graphics.Color
                |import androidx.compose.ui.unit.dp
                |import androidx.compose.ui.unit.sp
                |
                |@Suppress("unused", "MagicNumber")
                |object ThemeTokens {
                |$lines
                |}
                |
                """.trimMargin(),
            )
        }
    }

tasks.matching { it.name.startsWith("runKtlint") || it.name.startsWith("ktlint") || it.name.startsWith("detekt") }.configureEach {
    dependsOn(genThemeTokens)
}

kotlin {
    sourceSets.named("main") {
        kotlin.srcDir(genThemeTokens)
    }
}

dependencies {
    // 文案取文口住 core-copy（纯 JVM，上屏有字的模块依赖它）；core-ui 自己的默认文案（如「撤销」）也从那里取。
    api(project(":core-copy"))
    implementation(platform(libs.androidx.compose.bom))
    implementation(libs.androidx.ui)
    implementation(libs.androidx.ui.graphics)
    implementation(libs.androidx.material3)
    implementation(libs.androidx.ui.tooling.preview)
    debugImplementation(libs.androidx.ui.tooling)
    testImplementation(libs.junit)
    testImplementation(libs.kotlinx.coroutines.test) // 撤销窗：假时钟
}
