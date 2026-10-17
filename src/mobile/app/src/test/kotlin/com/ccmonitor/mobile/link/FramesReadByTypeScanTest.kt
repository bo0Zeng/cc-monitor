package com.ccmonitor.mobile.link

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机读帧只经线上类型（`Wire.bind`），会话此刻什么样只照那台给的整份脸画：
 *
 * | # | 问什么 |
 * |---|---|
 * | ① | 读帧的那四份（`Products.kt` · `Live.kt` · `Sessions.kt` · `Rotation.kt`）里没有按键取格的手读（`.str("…")` · `.num(` · `.bool(` · `.obj(` · `.objs(` · `.strs(`）；只许认帧种类的 `.str("kind")` |
 * | ② | 生产源里没有折出会话点的分支：`Tone.X -> 点` 与 `-> SessionDot.X`（点是那台给的 `face.dot`） |
 *
 * 判别力押在阳性对照上（先喂一份合成样本证明认得出、豁免那一处不算）。只扫代码（注释剥掉），防手滑不防拼接。
 * 住 `app`：`:app:testDebugUnitTest` 的运行时 classpath 含全部 core 模块产物，任何模块的代码改动都会让它重跑。
 */
class FramesReadByTypeScanTest {
    @Test
    fun theFrameFilesReadNoCellByKeyExceptTheKind() {
        assertEquals(
            "阳性对照：认得出手读、认帧种类那一处不算",
            listOf(".str(\"sid\")", ".obj(\"face\")"),
            handReads("if (frame.str(\"kind\") != KIND) return null\nval s = frame.str(\"sid\")\nval f = frame.obj(\"face\")\n// frame.str(\"x\") 注释不算"),
        )
        val root = repoRoot()
        val files = FRAME_FILES.map { File(root, "core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link/$it") }
        assertTrue("前提：四份都在（${files.filterNot { it.isFile }}）", files.all { it.isFile })
        val hits = files.mapNotNull { f -> handReads(f.readText()).takeIf { it.isNotEmpty() }?.let { "${f.name}: $it" } }
        assertEquals("读帧那几份又按键取格了：照 cmd-wire 那一份类型声明进 WireTypes.kt、经 Wire.bind 装", emptyList<String>(), hits)
    }

    @Test
    fun noProductionBranchFoldsASessionDot() {
        val sample = "when (tone) {\n    null, Tone.NOW -> RUNNING\n    Tone.NEED -> NEEDS_YOU\n}\nwhen (s) { \"ended\" -> SessionDot.ENDED }\nTone.NEED, Tone.WARN -> tokens.warn\nSessionDot.GONE -> Face(DotForm.DASH, c)"
        assertEquals("阳性对照：语气折点 · 别的东西折点都认得出；语气 ⇒ 字色、点 ⇒ 画法不算", 3, dotFolds(sample).size)
        val hits = sources().mapNotNull { (rel, text) -> dotFolds(text).takeIf { it.isNotEmpty() }?.let { "$rel: $it" } }
        assertEquals("会话点由那台给（face.dot），手机不另折", emptyList<String>(), hits)
    }

    // ---- 扫描器 ---------------------------------------------------------------

    private fun handReads(text: String): List<String> =
        HAND_READ
            .findAll(KotlinSourceScanner.codeOnlyKeepingLiterals(text))
            .map { it.value }
            .filterNot { it == KIND_READ }
            .toList()

    private fun dotFolds(text: String): List<String> {
        val code = KotlinSourceScanner.codeOnlyDroppingLiterals(text)
        return (TONE_TO_DOT.findAll(code) + TO_SESSION_DOT.findAll(code)).map { it.value.trim() }.toList()
    }

    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根：${File(".").absolutePath}")

    /** 扫描面：`app` ＋ 每个 `core-…` 模块的 `src/main/kotlin` 下的全部 `.kt`（仓相对路径 → 原文）。 */
    private fun sources(): List<Pair<String, String>> {
        val root = repoRoot()
        val modules = listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList())
        val files = modules.map { File(it, "src/main/kotlin") }.filter { it.isDirectory }.flatMap { r -> r.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
        assertTrue("前提：得真扫到源文件（实得 ${files.size} 个）", files.size > 100)
        return files.map { it.relativeTo(root).invariantSeparatorsPath to it.readText() }
    }

    private companion object {
        const val MAX_WALK_UP = 6
        val FRAME_FILES = listOf("Products.kt", "Live.kt", "Sessions.kt", "Rotation.kt")
        val HAND_READ = Regex("""\.(str|num|bool|obj|objs|strs)\("[^"]*"\)""")
        const val KIND_READ = ".str(\"kind\")"
        const val DOTS = "RUNNING|NEEDS_YOU|BACKGROUND|IDLE|EXITED|ENDED|GONE|UNKNOWN"
        val TONE_TO_DOT = Regex("""Tone\.\w+(\s*,\s*(Tone\.\w+|null))*\s*->\s*(SessionDot\.)?($DOTS)\b""")
        val TO_SESSION_DOT = Regex("""->\s*SessionDot\.($DOTS)\b""")
    }
}
