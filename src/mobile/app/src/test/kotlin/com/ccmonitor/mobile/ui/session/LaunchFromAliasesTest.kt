package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.claude.link.Profiles
import com.ccmonitor.mobile.core.claude.link.Reply
import com.ccmonitor.mobile.ui.chat.FakeCoreLink
import kotlinx.coroutines.test.runTest
import org.junit.Assert.assertEquals
import org.junit.Test

/** 终端屏「用哪个启动」那一排是那台的别名（`profiles-read`），按一个 ⇒ 终端里跑 `ccm @名`；手机自己不存启动命令。 */
class LaunchFromAliasesTest {
    private val book =
        mapOf(
            "path" to "/h/.cc-monitor/profiles.toml",
            "pathShown" to "~/.cc-monitor/profiles.toml",
            "exists" to true,
            "rev" to "fp1",
            "editedAt" to null,
            "fileProblem" to null,
            "profiles" to
                listOf(
                    row("work", usable = true),
                    row("broken", usable = false),
                    row("cx", usable = true),
                ),
            "seed" to listOf(row("seeded", usable = true)),
            "seedHint" to "",
            "seedCreate" to "",
            "migrated" to null,
            "binDir" to "/h/.cc-monitor/bin",
            "accounts" to emptyList<String>(),
            "tmux" to true,
            "slots" to emptyList<Any>(),
            "tmuxNaming" to emptyList<Any>(),
            "sets" to emptyList<Any>(),
        )

    private fun row(
        name: String,
        usable: Boolean,
    ) = mapOf(
        "name" to name,
        "from" to null,
        "own" to emptyList<Any>(),
        "agent" to emptyList<Any>(),
        "usable" to usable,
        "problem" to null,
        "kind" to "link",
        "functionWhy" to null,
        "functionLine" to null,
        "said" to name,
        "form" to mapOf("name" to name, "from" to null, "account" to null, "tmux" to null, "cwdIf" to null, "cwd" to null, "agent" to null, "args" to null, "launcher" to null, "tmuxSize" to null, "detach" to false, "tab" to null),
        "accountShape" to null,
        "start" to "'/h/.cc-monitor/bin/ccm' @$name",
    )

    private fun link() =
        FakeCoreLink().apply {
            answer = { cmd, _ -> if (cmd == Profiles.READ) Reply.Ok(book) else Reply.LinkDown }
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
}
