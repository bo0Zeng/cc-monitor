package com.ccmonitor.mobile.detekt

import dev.detekt.api.Config
import dev.detekt.test.utils.compileContentForTest
import dev.detekt.test.utils.compileForTest
import org.jetbrains.kotlin.config.LanguageVersionSettingsImpl
import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Files

/** 两句的对照样例（探测器坏了 ⇒ 先红），连同规则照欠账名单报不报。 */
class ExitScanTest {
    private val table =
        mapOf(
            "s.k.x" to ExitScan.Entry("action", 0),
            "p.k.x" to ExitScan.Entry("body", 0),
            "b.k.x" to ExitScan.Entry("error", 0),
            "a.k.x" to ExitScan.Entry("title", 1),
        )

    private fun hits(code: String) = ExitScan.scan(compileContentForTest(code), table)

    private val cases: List<Pair<String, ExitScan.Hits>> =
        listOf(
            // 按线上原因码挑「能不能重发」：照写法正则扫时一条都撞不上（挑的是枚举，不是字）。
            """
            private val RETRYABLE = setOf("screen_changed", "ambiguous", "not_known")
            fun f(res: R) = (if (res.why in RETRYABLE) DeliveryState.FAILED else DeliveryState.FAILED_PERMANENT) to copyText("s.k.x")
            """.trimIndent() to ExitScan.Hits(0, 2),
            """fun f(e: E) = when (e.code) { "dup" -> copyText("s.k.x"); else -> copyText("s.k.x") }""" to ExitScan.Hits(0, 2),
            """fun f(e: E) = if (e.code == "dup") copyText("s.k.x") else null""" to ExitScan.Hits(0, 1),
            """fun f(e: E) = if (e.code != "dup") "已拒绝" else null""" to ExitScan.Hits(0, 1),
            """fun f(e: E) = when { e.state == "on" -> Tone.NOW; else -> Tone.PLAIN }""" to ExitScan.Hits(0, 1),
            """fun f(e: E) { if (CODES.contains(e.why)) { show(copyText("s.k.x")) } }
               val CODES = listOf("a")""" to ExitScan.Hits(0, 1),
            """fun f() = copyText("b.k.x")""" to ExitScan.Hits(1, 0),
            """fun f(n: Int) = copyText("a.k.x", "n" to n)""" to ExitScan.Hits(1, 0),
            """fun f(k: String) = copyText(k)""" to ExitScan.Hits(1, 0),
            """fun f() = copyText("no.such.key")""" to ExitScan.Hits(1, 0),
            // 阴性：骨架字照摆；条件不比码；when 没有字面码的臂；分支里再定义的具名函数不算；条件里取文不算。
            """fun f() = copyText("s.k.x")""" to ExitScan.Hits(0, 0),
            """fun f() = copyText("p.k.x")""" to ExitScan.Hits(0, 0),
            """fun f(open: Boolean) = if (open) copyText("s.k.x") else DeliveryState.FAILED""" to ExitScan.Hits(0, 0),
            """fun f(k: K) = when (k) { K.A -> copyText("s.k.x"); else -> DeliveryState.FAILED }""" to ExitScan.Hits(0, 0),
            """fun f(e: E) { if (e.code == "dup") { fun g() = copyText("s.k.x") } }""" to ExitScan.Hits(0, 0),
            """fun f() { if (copyText("s.k.x") == "y") go() }""" to ExitScan.Hits(0, 0),
            """fun f(e: E) = if (e.code == "dup") Locale.ROOT.toString() else null""" to ExitScan.Hits(0, 0),
        )

    @Test
    fun `each sample hits exactly what it says`() {
        val bad = cases.mapNotNull { (code, want) -> hits(code).takeIf { it != want }?.let { "「$code」命中 $it，该是 $want" } }
        assertEquals(bad.joinToString("\n"), emptyList<String>(), bad)
    }

    /** 假仓：一张表、一份名单、一份 `src/main` 下的文件；规则照名单报。 */
    private fun lintInRepo(debtN: Int?): List<String> {
        val repo = Files.createTempDirectory("exit-scan")
        Files.createDirectories(repo.resolve(Ledger.TABLE).parent)
        Files.writeString(repo.resolve(Ledger.TABLE), """{"entries":{"b.k.x":{"kind":"error","role":"报错","zh":"字","args":[]}}}""")
        Files.createDirectories(repo.resolve(Ledger.DEBT).parent)
        val cells = debtN?.let { """["m/src/main/A.kt","key",$it]""" } ?: ""
        Files.writeString(repo.resolve(Ledger.DEBT), """{"roots":[],"ceiling":0,"groups":[{"kind":"首登","why":"x","cells":[$cells]}]}""")
        val src = repo.resolve("m/src/main/A.kt")
        Files.createDirectories(src.parent)
        Files.writeString(src, "fun f() = copyText(\"b.k.x\")\n")
        return ExitCopyRule(Config.empty, "key").visitFile(compileForTest(src), LanguageVersionSettingsImpl.DEFAULT).map { it.message }
    }

    @Test
    fun `the rule reports a cell only when it differs from the ledger`() {
        assertEquals(emptyList<String>(), lintInRepo(1))
        assertEquals(1, lintInRepo(null).size)
        assertEquals(1, lintInRepo(2).size)
    }

    @Test
    fun `the ledger reader reads the shapes the repo files have`() {
        val l =
            Ledger.parse(
                """{"说明":["a\"b一"],"entries":{"k":{"kind":"title","args":["n"]},"j":{"kind":"body","args":[]}}}""",
                """{"roots":[{"dir":"x","lang":"kt"}],"ceiling":3,"groups":[{"kind":"首登","why":"w","cells":[["f","key",3]]}]}""",
            )
        assertEquals(ExitScan.Entry("title", 1), l.table["k"])
        assertEquals(ExitScan.Entry("body", 0), l.table["j"])
        assertEquals(mapOf("f · key" to 3), l.debt)
    }
}
