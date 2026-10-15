package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.claude.link.CoreFailure
import com.ccmonitor.mobile.core.claude.link.Profiles
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.core.data.repo.MovedLaunch
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 终端屏「用哪个启动」那一排是那台的别名（`profiles-read`），按一个 ⇒ 终端里跑 `ccm @名`；手机自己不再存启动命令。
 * 迁移那一次的说明：原来那几条原样列出；`claude` / `ccm` 那一类给「建这条」，交的是别名页那条写命令（`profiles-write`）。
 */
class LaunchFromAliasesTest {
    private val book =
        mapOf(
            "home" to "/h",
            "path" to "/h/.cc-monitor/profiles.toml",
            "exists" to true,
            "fingerprint" to "fp1",
            "editedAt" to null,
            "fileProblem" to null,
            "profiles" to
                listOf(
                    row("work", usable = true),
                    row("broken", usable = false),
                    row("cx", usable = true),
                ),
            "seed" to listOf(row("seeded", usable = true)),
            "migrated" to null,
            "binDir" to "/h/.cc-monitor/bin",
            "accounts" to emptyList<String>(),
            "tmux" to true,
        )

    private fun row(
        name: String,
        usable: Boolean,
    ) = mapOf("name" to name, "from" to null, "usable" to usable, "said" to name, "kind" to "link", "start" to "'/h/.cc-monitor/bin/ccm' @$name")

    private val folded = mapOf("name" to "cc", "args" to "--resume x")

    private fun link(
        write: Reply = Reply.Ok(mapOf("wrote" to true, "fingerprint" to "fp2", "reload" to null)),
        fromLine: Reply = Reply.Ok(folded),
    ) = FakeCoreLink().apply {
        answer = { cmd, _ ->
            when (cmd) {
                Profiles.READ -> Reply.Ok(book)
                Profiles.WRITE -> write
                Profiles.FROM_LINE -> fromLine
                else -> Reply.LinkDown
            }
        }
    }

    @Test
    fun theRowReadsTheAliasesOfThatMachineAndListsOnlyUsableOnes() =
        runTest {
            val l = link()
            val names = TerminalOps(l, { "box" }).aliases().getOrThrow()
            assertEquals(listOf(emptyMap<String, Any?>()), l.calls(Profiles.READ))
            assertEquals("名字照那台给的、顺序照那台的；用不了的与首建预览不列", listOf("work", "cx"), names.map { it.name })
            assertEquals("按一个敲的是那台写好的那一行", "'/h/.cc-monitor/bin/ccm' @work", names.first().start)
        }

    @Test
    fun makingTheAliasAsksThatMachineToFoldTheLineThenWritesItsFormWithTheFingerprintJustRead() =
        runTest {
            val l = link()
            val r = TerminalOps(l, { "box" }).makeAlias(MovedLaunch("h", "cc", "~/w", "dev", "claude --resume x"))
            assertTrue(r.isSuccess)
            assertNull("终端函数那份没改 ⇒ 没有要重读那一句", r.getOrThrow())
            assertEquals(
                listOf(mapOf("name" to "cc", "line" to "claude --resume x", "cwd" to "~/w", "sessionName" to "dev")),
                l.calls(Profiles.FROM_LINE),
            )
            assertEquals(
                listOf(mapOf("changes" to listOf(mapOf("op" to "set", "was" to null, "form" to folded)), "fingerprint" to "fp1")),
                l.calls(Profiles.WRITE),
            )
        }

    @Test
    fun aLineThatMachineCannotFoldSaysWhatItSaidAndWritesNothing() =
        runTest {
            val l = link(fromLine = Reply.Failed(CoreFailure("refused", "这条命令建不了别名", "")))
            val r = TerminalOps(l, { "box" }).makeAlias(MovedLaunch("h", "top", "/srv", null, "htop -d 5"))
            assertTrue(r.exceptionOrNull()?.message.orEmpty().contains("这条命令建不了别名"))
            assertEquals(emptyList<Any?>(), l.calls(Profiles.WRITE))
        }

    @Test
    fun aRefusedWriteSaysWhatThatMachineSaid() =
        runTest {
            val l = link(write = Reply.Failed(CoreFailure("refused", "名字已被占用", "细节")))
            val r = TerminalOps(l, { "box" }).makeAlias(MovedLaunch("h", "cc", null, null, "claude"))
            assertTrue(r.isFailure)
            assertTrue(
                r
                    .exceptionOrNull()
                    ?.message
                    .orEmpty()
                    .contains("名字已被占用"),
            )
        }

    @Test
    fun theOldEntryIsListedVerbatim() {
        val cct = MovedLaunch("h", "cct", "~/w x", "cc", "claude --resume  'a b'")
        val top = MovedLaunch("h", "top", "/srv", null, "htop -d 5")
        val shown = cct.shownValues()
        assertTrue("目录 · 会话名 · 命令原样在", shown.containsAll(listOf("~/w x", "cc", "claude --resume  'a b'")))
        assertTrue("复制的那一段带着原命令", cct.clipText().contains("claude --resume  'a b'"))
        assertEquals(listOf("/srv", "htop -d 5"), top.shownValues())
    }

    /** 手机里不再有启动配置那一套：表 · 仓 · 意图 · tab 上那一格；`launchers` 只在删它的那一版迁移里出现。 */
    @Test
    fun theMobileNoLongerKeepsItsOwnLaunchCommands() {
        val root = mobileRoot()
        val sources =
            root
                .walkTopDown()
                .filter { it.isFile && it.extension == "kt" && "/src/main/" in it.invariantSeparatorsPath }
                .toList()
        assertTrue("前提：扫到了手机的生产源（实得 ${sources.size}）", sources.size > 100)
        val hits =
            sources.flatMap { f ->
                val code = KotlinSourceScanner.codeOnlyKeepingLiterals(f.readText())
                val gone = GONE.filter { it.containsMatchIn(code) }.map { "${f.name}：${it.pattern}" }
                val table = if (f.name != "Migrations.kt" && "`launchers`" in code) listOf("${f.name}：launchers 表") else emptyList()
                gone + table
            }
        assertEquals(emptyList<String>(), hits)
    }

    private fun mobileRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(6)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到手机那一棵的根")

    private companion object {
        val GONE =
            listOf(
                Regex("""\bLauncher\b"""),
                Regex("""\bLauncherDao\b"""),
                Regex("""\bLauncherRepository\b"""),
                Regex("""\bLaunchSpec\b"""),
                Regex("""\blauncherId\b"""),
                Regex("""\blauncherDao\b"""),
            )
    }
}
