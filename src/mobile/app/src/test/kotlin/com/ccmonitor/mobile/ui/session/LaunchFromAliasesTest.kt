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
    ) = mapOf("name" to name, "from" to null, "usable" to usable, "said" to name, "kind" to "link")

    private fun link(write: Reply = Reply.Ok(mapOf("wrote" to true, "fingerprint" to "fp2", "reload" to null))) =
        FakeCoreLink().apply {
            answer = { cmd, _ ->
                when (cmd) {
                    Profiles.READ -> Reply.Ok(book)
                    Profiles.WRITE -> write
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
            assertEquals("名字照那台给的、顺序照那台的；用不了的与首建预览不列", listOf("work", "cx"), names)
        }

    @Test
    fun pressingOneRunsCcmWithThatAlias() {
        assertEquals("ccm @work", Profiles.startLine("work"))
    }

    @Test
    fun makingTheAliasSendsTheAliasPageWriteWithTheFingerprintJustRead() =
        runTest {
            val l = link()
            val form = mapOf("name" to "cc")
            val r = TerminalOps(l, { "box" }).makeAlias(form)
            assertTrue(r.isSuccess)
            assertNull("终端函数那份没改 ⇒ 没有要重读那一句", r.getOrThrow())
            assertEquals(
                listOf(mapOf("changes" to listOf(mapOf("op" to "set", "was" to null, "form" to form)), "fingerprint" to "fp1")),
                l.calls(Profiles.WRITE),
            )
        }

    @Test
    fun aRefusedWriteSaysWhatThatMachineSaid() =
        runTest {
            val l = link(write = Reply.Failed(CoreFailure("refused", "名字已被占用", "细节")))
            val r = TerminalOps(l, { "box" }).makeAlias(mapOf("name" to "cc"))
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
    fun theOldEntryIsListedVerbatimAndOnlyClaudeOrCcmOnesGetAMakeButton() {
        val cc = MovedLaunch("h", "cc", null, null, "claude")
        val cct = MovedLaunch("h", "cct", "~/w x", "cc", "claude --resume  'a b'")
        val bare = MovedLaunch("h", "c2", null, null, "ccm")
        val top = MovedLaunch("h", "top", "/srv", null, "htop -d 5")
        val ccmArgs = MovedLaunch("h", "c3", null, null, "ccm --ccm-tmux")
        assertEquals(mapOf("name" to "cc"), cc.aliasForm())
        assertEquals(
            mapOf("name" to "cct", "cwd" to "~/w x", "tmux" to mapOf("mode" to "fixed", "name" to "cc"), "args" to "--resume  'a b'"),
            cct.aliasForm(),
        )
        assertEquals(mapOf("name" to "c2"), bare.aliasForm())
        assertNull("别的命令只列出、不建", top.aliasForm())
        assertNull("ccm 带了自己的选项 ⇒ 只列出（选项怎么折成别名那几格只在那台）", ccmArgs.aliasForm())
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
