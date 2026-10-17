package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.claude.link.wire.ArriveAsk
import com.ccmonitor.mobile.core.claude.link.wire.ArriveBy
import com.ccmonitor.mobile.core.claude.link.wire.Held
import com.ccmonitor.mobile.core.claude.link.wire.Hide
import com.ccmonitor.mobile.core.claude.link.wire.TabPref
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
            FilesCommitUpload.COMMAND to
                listOf(
                    FilesCommitUpload.args(dir, "up.bin", FilesCommitUpload.Staged("k", 3L, 9L, SHA)),
                    FilesCommitUpload.args(dir, "up.bin", FilesCommitUpload.Staged("k", 3L, 9L, SHA), "overwrite"),
                    FilesCommitUpload.args(dir, "up.bin", FilesCommitUpload.Staged("k", 3L, 9L, SHA), "keep_both"),
                ),
            FilesCreate.COMMAND to listOf(FilesCreate.args(dir, "a.txt")),
            FilesDeleteSession.COMMAND to listOf(FilesDeleteSession.args("s")),
            FilesHome.COMMAND to listOf(emptyMap()),
            FilesLs.COMMAND to listOf(FilesLs.args(bin), FilesLs.dirs("/w")),
            FilesMkdir.COMMAND to listOf(FilesMkdir.args(dir, "d")),
            FilesReadText.COMMAND to listOf(FilesReadText.args(bin, 262_144L)),
            FilesRename.COMMAND to listOf(FilesRename.args(dir, bin, "b")),
            FilesStat.COMMAND to listOf(FilesStat.args(bin)),
            FilesWriteText.COMMAND to listOf(FilesWriteText.args(dir, bin, "x", SHA)),
            HistoryFacts.COMMAND to listOf(HistoryFacts.args("/p/s.jsonl")),
            HistoryLines.COMMAND to listOf(HistoryLines.args("/p/s.jsonl", 0L, 10L)),
            HistoryAnnotate.COMMAND to
                listOf(
                    HistoryAnnotate.hide("s", Hide.ON),
                    HistoryAnnotate.hide("s", Hide.OFF),
                    HistoryAnnotate.hide("s", null),
                    HistoryAnnotate.star("s", true),
                    HistoryAnnotate.rename("s", "t"),
                    HistoryAnnotate.rename("s", "  "),
                ),
            HistoryList.COMMAND to listOf(HistoryList.args(ListAsk()), HistoryList.args(ListAsk("q", 7, ListSort.CREATED, true)), HistoryList.args(ListAsk(sort = ListSort.ACTIVITY)), HistoryList.args(ListAsk(), "e.1.2")),
            HistorySearch.COMMAND to listOf(HistorySearch.args("q")),
            HistorySearchMerge.COMMAND to listOf(HistorySearchMerge.args(listOf(HistorySearchMerge.LOCAL to listOf(SEARCH_ROW), "h2" to emptyList()))),
            HistoryRead.COMMAND to listOf(HistoryRead.args("/p/s.jsonl", 0L, 10L)),
            HistoryTail.COMMAND to listOf(HistoryTail.args("/p/s.jsonl", 50), HistoryTail.args("/p/s.jsonl", 50, Held("/p/s.jsonl", 3L, 30L, "m"))),
            Kill.COMMAND to listOf(Kill.args("demo")),
            KeyAdd.COMMAND to listOf(KeyAdd.args("ssh-ed25519 AAAA k")),
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
            MachinePrefs.READ to listOf(null),
            MachinePrefs.SET to
                listOf(
                    MachinePrefs.tab(TabKind.CHILD, TabPref.SHOW),
                    MachinePrefs.tab(TabKind.BG, TabPref.HIDE),
                    MachinePrefs.model("work", "opus"),
                    MachinePrefs.model("work", null),
                ),
            Rotation.FOR_NEW to listOf(Rotation.forNew("s1"), Rotation.forNew(null)),
            Rotation.RELOAD to listOf(Rotation.reload("x")),
            Rotation.SETTINGS to
                listOf(
                    Rotation.settings("x", RotEdit.set("k", null, 1L), "r"),
                    Rotation.settings("x", RotEdit.set("k", 0, null), "r"),
                    Rotation.settings("x", RotEdit.seg("caps", 1, "5h", 0, RotSegment(0L, 600L, 50.0)), "r"),
                    Rotation.settings("x", RotEdit.seg("caps", null, "5h", 1, null), "r"),
                    Rotation.settings("x", RotEdit.seg("caps", 0, "5h", null, RotSegment(1200L, 1380L, 0.0)), "r"),
                    Rotation.settings("x", RotEdit.same("caps", 0, "7d", true), "r"),
                    Rotation.settings("x", RotEdit.move(0, 2), "r"),
                    Rotation.settings("x", RotEdit.add("a"), "r"),
                    Rotation.settings("x", RotEdit.remove(1), "r"),
                ),
            Rotation.STRATEGIES to listOf(emptyMap()),
            Rotation.ADD to listOf(Rotation.add("x", "n"), Rotation.add(null, "n")),
            Rotation.COPY to listOf(Rotation.copy("x", "n")),
            Rotation.DELETE to listOf(Rotation.delete("x")),
            Rotation.RENAME to listOf(Rotation.rename("x", "n", "r")),
            Rotation.TZ to listOf(Rotation.tz("x", "Asia/Shanghai", "r")),
            Rotation.TEMP to listOf(Rotation.place("s", "a", 0), Rotation.set("s", listOf("b", "a")), Rotation.clearTemp("s"), Rotation.off("s", "a", true)),
            Rotation.USE to listOf(Rotation.use("s", "x"), Rotation.use("s", null)),
            Rotation.USE_NOW to listOf(Rotation.useNow("s", "a")),
            Rotation.VIEW to
                listOf(
                    Rotation.view(RotWhere.Session("s"), 12),
                    Rotation.view(RotWhere.Strategy("x"), 12),
                ),
            SessionArrive.COMMAND to
                listOf(
                    SessionArrive.args(ArriveAsk(ArriveBy("cwd", null, "/w"), 3L, "w-cc")),
                    SessionArrive.args(ArriveAsk(ArriveBy("sid", "s", null), 3L, null)),
                ),
            SessionCommandList.COMMAND to listOf(SessionCommandList.args("s")),
            SessionModel.COMMAND to listOf(SessionModel.args("s", "opus")),
            SessionOpenDir.COMMAND to listOf(SessionOpenDir.args("/w")),
            SessionOpenFacts.COMMAND to listOf(emptyMap()),
            SessionOpen.COMMAND to
                listOf(
                    SessionOpen.newIn("/w", "t", "claude", "hi"),
                    SessionOpen.newIn("/w", "t", "claude", "hi", account = "a", rotation = SessionOpen.Rotation("x")),
                    SessionOpen.newIn("/w", "t", "claude", "hi", rotation = SessionOpen.Rotation(null)),
                    SessionOpen.newInTmux("claude", "demo", "t"),
                    SessionOpen.resume("s"),
                    SessionOpen.look("demo"),
                    SessionOpen.shell("main", "~/w"),
                    SessionOpen.shell(null, "/w"),
                ),
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

        /** `history-search` 回包里的一行（交合并那一问时原样带上）。 */
        val SEARCH_ROW =
            mapOf(
                "agent" to "claude",
                "sessionId" to "s",
                "projectPath" to "/w",
                "projectName" to "w",
                "jsonlPath" to "/w/s.jsonl",
                "title" to "t",
                "updatedAt" to 1L,
                "hits" to listOf(mapOf("uuid" to "u", "tsMs" to 0L, "kind" to "user", "before" to "", "matched" to "q", "after" to "", "offset" to 0L, "line" to 0L)),
                "moreHits" to 0L,
                "hitsTruncated" to false,
                "isBg" to false,
                "can" to mapOf("resume" to "yes", "accounts" to true, "fork" to true, "delete" to "yes"),
                // 金样经 moshi 读进来数都是 Double；那台的 `order` 是整数，交回去照整数写。
                "face" to Fixtures.face("ended").mapValues { (_, v) -> (v as? Double)?.takeIf { it % 1 == 0.0 }?.toLong() ?: v },
            )
        val LINK = File(Fixtures.repoRoot, "src/mobile/core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link")

        /** 别名页表单那一形（「建这条」交的那几格；没交的那台按缺省）。 */
        val FORM: Map<String, Any?> = mapOf("name" to "cct", "cwd" to "~/w", "tmux" to mapOf("mode" to "fixed", "name" to "cc"), "args" to "--resume")
    }
}
