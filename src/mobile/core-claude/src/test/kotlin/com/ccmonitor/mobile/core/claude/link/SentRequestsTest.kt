package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.Hide
import com.ccmonitor.mobile.core.claude.link.wire.RotSeg
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 手机发出去的每条命令（`link/` 里写着命令名的那几处）：每个拼入参的口拼出来的那一份，照发出去的那一行（[Json.args]）写进跨语言金样
 * `mobile-requests.golden.json`；后端判据拿它逐份照那条命令的入参类型解（与分派同一个解码，`wire/typed_tests.rs`），解不开就红。
 * 金样跟这里算的不一样 ⇒ 重写金样、并红（提交新的那一份）。表里的命令与 `link/` 里写着的命令名两向相等：新加一条命令不进这张表就红。
 */
class SentRequestsTest {
    private val dir = RemotePath.of("/w")
    private val bin = RemotePath(byteArrayOf(0x2f, 0xff.toByte()))

    /** 命令名 ⇒ 每个拼入参的口拼出来的一份（不收入参、界面直接交空的那几条照它交的写）。 */
    private val sent: Map<String, List<Map<String, Any?>?>> =
        mapOf(
            AccountsList.COMMAND to listOf(AccountsList.args("claude"), AccountsList.args("")),
            AccountsSetDefault.COMMAND to listOf(AccountsSetDefault.args("a")),
            FilesChmod.COMMAND to listOf(FilesChmod.args(dir, bin, 420L)),
            FilesCommitUpload.COMMAND to listOf(FilesCommitUpload.args(dir, "up.bin", "k", SHA, 3L, 9L)),
            FilesCreate.COMMAND to listOf(FilesCreate.args(dir, "a.txt")),
            FilesDelete.COMMAND to listOf(FilesDelete.args(dir, bin, recursive = true), FilesDelete.args(dir, bin, recursive = false)),
            FilesHome.COMMAND to listOf(emptyMap()),
            FilesLs.COMMAND to listOf(FilesLs.args(bin), FilesLs.dirs("/w")),
            FilesMkdir.COMMAND to listOf(FilesMkdir.args(dir, "d")),
            FilesReadChunk.COMMAND to listOf(FilesReadChunk.args(bin, 0L)),
            FilesReadText.COMMAND to listOf(FilesReadText.args(bin)),
            FilesRename.COMMAND to listOf(FilesRename.args(dir, bin, "b")),
            FilesStageChunk.COMMAND to listOf(FilesStageChunk.args("k", 0L, byteArrayOf(0, -1), 2)),
            FilesStat.COMMAND to listOf(FilesStat.args(bin)),
            FilesWriteText.COMMAND to listOf(FilesWriteText.args(dir, bin, "x", SHA)),
            HistoryFacts.COMMAND to listOf(HistoryFacts.args("/p/s.jsonl")),
            HistoryLines.COMMAND to listOf(HistoryLines.args("/p/s.jsonl", 0L, 10L)),
            HistoryAnnotate.COMMAND to listOf(HistoryAnnotate.hide("s", Hide.ON), HistoryAnnotate.hide("s", Hide.OFF), HistoryAnnotate.hide("s", null)),
            HistoryList.COMMAND to listOf(HistoryList.args(false), HistoryList.args(true)),
            HistoryRead.COMMAND to listOf(HistoryRead.args("/p/s.jsonl", 0L, 10L)),
            HistoryTail.COMMAND to listOf(HistoryTail.args("/p/s.jsonl", 50)),
            Kill.COMMAND to listOf(Kill.args("demo")),
            AttachLine.COMMAND to listOf(AttachLine.args("claude", "demo")),
            Login.CANCEL to listOf(Login.cancel("j")),
            Login.FINISH to listOf(Login.finish("j", "code")),
            Login.START to listOf(Login.startAccount("claude", "a")),
            NeedsAnswer.COMMAND to
                listOf(
                    NeedsAnswer.args("s", "q", "allow"),
                    NeedsAnswer.args("s", "q", "answer", note = "n", picks = mapOf("0" to NeedsPick(setOf("A"), " t "))),
                ),
            Profiles.READ to listOf(emptyMap()),
            Profiles.FROM_LINE to listOf(Profiles.fromLineArgs("cct", "claude --resume", "~/w", "cc"), Profiles.fromLineArgs("cct", "claude", null, null)),
            Profiles.WRITE to listOf(Profiles.createArgs("f", FORM), Profiles.createArgs(null, FORM)),
            QuotaRead.COMMAND to listOf(null),
            Rotation.DAY_PAINT to listOf(Rotation.dayPaint(listOf(RotSeg(0L, 10L, 1.0)), 0L, 10L, 0.5), Rotation.dayPaint(emptyList(), 0L, 10L, null)),
            Rotation.FOR_NEW to listOf(Rotation.forNew("s1"), Rotation.forNew(null)),
            Rotation.REVERT to listOf(Rotation.revert("s")),
            Rotation.SAVE to listOf(Rotation.save(RotWhere.Session("s"), "r"), Rotation.save(RotWhere.Strategy("x"), "r", mapOf("k" to 1L))),
            Rotation.SETTINGS to listOf(Rotation.settings(RotWhere.Strategy("x"), mapOf("k" to 1L), "r")),
            Rotation.STRATEGIES to listOf(emptyMap()),
            Rotation.ADD to listOf(Rotation.add("x", "n")),
            Rotation.COPY to listOf(Rotation.copy("x", "n")),
            Rotation.DELETE to listOf(Rotation.delete("x")),
            Rotation.RENAME to listOf(Rotation.rename("x", "n", "r")),
            Rotation.TZ to listOf(Rotation.tz("x", "Asia/Shanghai", "r")),
            Rotation.TEMP to listOf(Rotation.place("s", "a", 0), Rotation.clearTemp("s")),
            Rotation.USE to listOf(Rotation.use("s", "x", discard = false), Rotation.use("s", null, discard = true)),
            Rotation.USE_NOW to listOf(Rotation.useNow("s", "a")),
            Rotation.VIEW to
                listOf(
                    Rotation.view(RotWhere.Session("s"), 12),
                    Rotation.view(RotWhere.Strategy("x"), 12, mapOf("k" to 1L), "r"),
                ),
            SessionCommandList.COMMAND to listOf(SessionCommandList.args("s")),
            SessionModel.COMMAND to listOf(SessionModel.args("s", "opus")),
            SessionNew.COMMAND to
                listOf(
                    SessionNew.args("/w", "t", "claude", "hi"),
                    SessionNew.args("/w", "t", "claude", "hi", account = "a", rotation = SessionNew.Rotation("x")),
                    SessionNew.args("/w", "t", "claude", "hi", rotation = SessionNew.Rotation(null)),
                    SessionNew.inTmux("claude", "demo", "t"),
                ),
            SessionNewDir.COMMAND to listOf(SessionNewDir.args("/w")),
            SessionNewFacts.COMMAND to listOf(emptyMap()),
            SessionPermission.COMMAND to listOf(SessionPermission.args("s", "plan")),
            SessionSay.COMMAND to listOf(SessionSay.args("s", "hi")),
            SessionsNeeds.COMMAND to listOf(null),
            TerminalInput.COMMAND to listOf(TerminalInput.text("s", "hi"), TerminalInput.key("s", "esc")),
            TerminalPreview.COMMAND to listOf(TerminalPreview.args("t1")),
            TerminalsList.COMMAND to listOf(TerminalsList.args()),
        )

    @Test
    fun theGoldenIsWhatTheBuildersSend() {
        val want =
            sent.toSortedMap().entries.joinToString(",\n", "{\n", "\n}\n") { (cmd, all) ->
                "  ${Json.line(cmd)}: " + all.joinToString(",", "[", "]") { it?.let(Json::args) ?: "null" }
            }
        // 金样只比不写：不一样时现拼的那份落到 .build/golden/ 下同一路径，对过后 tests/scripts/bless-golden.sh 拷回。
        val rel = "tests/__fixtures__/$GOLDEN"
        if (File(Fixtures.repoRoot, rel).takeIf { it.isFile }?.readText() != want) {
            File(Fixtures.repoRoot, ".build/golden/$rel").apply { parentFile.mkdirs() }.writeText(want)
            error("$GOLDEN 跟这里拼出来的不一样：现拼的在 .build/golden/$rel，对过后跑 tests/scripts/bless-golden.sh")
        }
    }

    /** 表与 `link/` 里写着的命令名两向相等；写着 `COMMAND` 的那几处必是后端命令表里有的命令。 */
    @Test
    fun theTableCoversEveryCommandTheLinkNames() {
        val typed =
            Regex("""^\s*"([a-z0-9-]+)": \{ req: """, RegexOption.MULTILINE)
                .findAll(File(Fixtures.repoRoot, "src/frontend/ui/generated/TypedCommands.ts").readText())
                .map { it.groupValues[1] }
                .toSet()
        val decl = Regex("""const val (\w+)(?:: String)? = "([a-z0-9-]+)"""")
        val named =
            LINK.listFiles { f -> f.extension == "kt" }!!.flatMap { f -> decl.findAll(f.readText()).map { it.groupValues[1] to it.groupValues[2] } }
        val unknown = named.filter { (k, v) -> k == "COMMAND" && v !in typed }.map { it.second }
        assertEquals("这几个 COMMAND 不在后端命令表里", emptyList<String>(), unknown)
        val commands = named.map { it.second }.filter { it in typed }.toSortedSet()
        assertTrue("link/ 里一条命令名都没认出来 —— 本条在空转", commands.size > 20)
        assertEquals(commands, sent.keys.toSortedSet())
    }

    private companion object {
        const val GOLDEN = "mobile-requests.golden.json"
        val SHA = "ab".repeat(32)
        val LINK = File(Fixtures.repoRoot, "src/mobile/core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link")

        /** 别名页表单那一形（「建这条」交的那几格；没交的那台按缺省）。 */
        val FORM: Map<String, Any?> = mapOf("name" to "cct", "cwd" to "~/w", "tmux" to mapOf("mode" to "fixed", "name" to "cc"), "args" to "--resume")
    }
}
