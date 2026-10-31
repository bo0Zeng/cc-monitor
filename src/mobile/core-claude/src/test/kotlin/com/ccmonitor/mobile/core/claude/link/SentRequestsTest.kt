package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.ArriveAsk
import com.ccmonitor.mobile.core.wire.gen.ArriveBy
import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.Held
import com.ccmonitor.mobile.core.wire.gen.Hide
import com.ccmonitor.mobile.core.wire.gen.LoginStartReq
import com.ccmonitor.mobile.core.wire.gen.ProfileForm
import com.ccmonitor.mobile.core.wire.gen.ProfileTmuxMode
import com.ccmonitor.mobile.core.wire.gen.ProfileTmuxPick
import com.ccmonitor.mobile.core.wire.gen.Rewind
import com.ccmonitor.mobile.core.wire.gen.RewindHow
import com.ccmonitor.mobile.core.wire.gen.RewindWhen
import com.ccmonitor.mobile.core.wire.gen.Staged
import com.ccmonitor.mobile.core.wire.gen.Tab
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
            Bookmarks.READ to listOf(emptyMap()),
            Bookmarks.EDIT to listOf(Bookmarks.edit("toggle", listOf("/w")), Bookmarks.edit("remove", listOf("/w")), Bookmarks.edit("add", listOf("/w", "/v"))),
            FilesCommitUpload.COMMAND to
                listOf(
                    FilesCommitUpload.args(dir, "up.bin", Staged("k", 3L, 9L, SHA)),
                    FilesCommitUpload.args(dir, "up.bin", Staged("k", 3L, 9L, SHA), "overwrite"),
                    FilesCommitUpload.args(dir, "up.bin", Staged("k", 3L, 9L, SHA), "keep_both"),
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
            Groups.COMMAND to listOf(Groups.ARGS),
            HistoryFacts.COMMAND to listOf(HistoryFacts.args("/p/s.jsonl"), HistoryFacts.bySid("0a1b2c3d-0000-4000-8000-000000000001")),
            HistoryLines.COMMAND to listOf(HistoryLines.args("/p/s.jsonl", 0L, 10L)),
            HistoryImage.COMMAND to listOf(HistoryImage.args("/p/s.jsonl", 120L, 0, "u-a")),
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
            HistoryRead.COMMAND to listOf(HistoryRead.args("/p/s.jsonl", 0L, 10L)),
            HistoryRun.COMMAND to listOf(HistoryRun.after("/p/s.jsonl", "r", 0L), HistoryRun.tail("/p/s.jsonl", "r"), HistoryRun.before("/p/s.jsonl", "r", 30L)),
            HistoryTail.COMMAND to listOf(HistoryTail.args("/p/s.jsonl", 50), HistoryTail.args("/p/s.jsonl", 50, Held("/p/s.jsonl", 3L, 30L, "m"))),
            Turns.COMMAND to listOf(Turns.cover("/p/s.jsonl", 30L), Turns.from("/p/s.jsonl", 30L)),
            TerminalEnd.COMMAND to listOf(TerminalEnd.args("t1")),
            SessionInterrupt.COMMAND to listOf(SessionInterrupt.args("s")),
            KeyAdd.COMMAND to listOf(KeyAdd.args("ssh-ed25519 AAAA k")),
            Login.CANCEL to listOf(Login.cancel("j")),
            Login.FINISH to listOf(Login.finish("j", "code")),
            Login.START to
                listOf(
                    Login.start(Login.account("claude", "a")),
                    Login.start(LoginStartReq.Mcp("claude", "a", "notes", null)),
                    Login.start(LoginStartReq.Mcp("claude", null, "notes", "/w")),
                ),
            NeedsAnswer.COMMAND to
                listOf(
                    NeedsAnswer.args("s", "q", "allow"),
                    NeedsAnswer.args("s", "q", "answer", note = "n", picks = mapOf("0" to NeedsPick(setOf("A"), " t "))),
                ),
            PlanCellView.COMMAND to listOf(PlanCellView.args("/w", "x", "A1")),
            PlanList.COMMAND to listOf(PlanList.args()),
            PlanRead.COMMAND to listOf(PlanRead.args("/w")),
            Profiles.READ to listOf(emptyMap()),
            Profiles.FROM_LINE to listOf(Profiles.fromLineArgs("cct", "claude --resume", "~/w", "cc"), Profiles.fromLineArgs("cct", "claude", null, null)),
            Profiles.WRITE to listOf(Profiles.createArgs("f", FORM), Profiles.createArgs(null, FORM)),
            QuotaRead.COMMAND to listOf(null),
            MachinePrefs.READ to listOf(null),
            MachinePrefs.SET to
                listOf(
                    MachinePrefs.tab(TabKind.CHILD, Tab.SHOW),
                    MachinePrefs.tab(TabKind.BG, Tab.HIDE),
                    MachinePrefs.model("work", "opus"),
                    MachinePrefs.model("work", null),
                ),
            Rotation.FOR_NEW to listOf(Rotation.forNew("s1"), Rotation.forNew(null)),
            Rotation.RELOAD to listOf(Rotation.reload("x")),
            Rotation.SETTINGS to
                listOf(
                    Rotation.settings("x", RotEdit.set("k", null, 1L), "r"),
                    Rotation.settings("x", RotEdit.set("k", 0, null), "r"),
                    Rotation.settings("x", RotEdit.seg("caps", 1, "5h", 0, RotSegment("0:00", "10:00", 50.0)), "r"),
                    Rotation.settings("x", RotEdit.seg("caps", null, "5h", 1, null), "r"),
                    Rotation.settings("x", RotEdit.seg("caps", 0, "5h", null, RotSegment("2000", "23", 0.0)), "r"),
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
            Rotation.TEMP to listOf(Rotation.move("s", "b", 1), Rotation.clearTemp("s"), Rotation.off("s", "a", true)),
            Rotation.USE to listOf(Rotation.use("s", "x"), Rotation.use("s", null)),
            Rotation.USE_NOW to listOf(Rotation.useNow("s", "a")),
            Rotation.VIEW to
                listOf(
                    Rotation.view(RotWhere.Session("s"), 12),
                    Rotation.view(RotWhere.Strategy("x"), 12),
                ),
            SessionArrive.COMMAND to
                listOf(
                    SessionArrive.args(ArriveAsk(ArriveBy.Cwd("/w"), 3L, "w-cc")),
                    SessionArrive.args(ArriveAsk(ArriveBy.Sid("s"), 3L, null)),
                ),
            SessionAttachmentCheck.COMMAND to listOf(SessionAttachmentCheck.args("s", listOf(AttachAsk("a.png", 5L)))),
            SessionAttachmentCommit.COMMAND to listOf(SessionAttachmentCommit.args("s", "a.png", Staged("k", 3L, 9L, SHA))),
            SessionAttachmentDrop.COMMAND to listOf(SessionAttachmentDrop.args("s", "i")),
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
            SessionQueueCancel.COMMAND to listOf(SessionQueueCancel.args("s", "m")),
            SessionQueuePut.COMMAND to
                listOf(
                    SessionQueuePut.args("s", "m", "hi", 1L),
                    SessionQueuePut.args("s", "m", "hi", 1L, listOf("i")),
                    SessionQueuePut.args("s", "m", "hi", 1L, rewind = Rewind("u", RewindHow.CODE_AND_CONVERSATION, RewindWhen.NOW)),
                ),
            SessionQueueRead.COMMAND to listOf(SessionQueueRead.args("s")),
            SessionRewindOptions.COMMAND to listOf(SessionRewindOptions.args("s", "u")),
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

    /** 表与 `link/` 里的命令名（取生成的 `Commands.<命令>.NAME` · 写着的）两向相等；写着 `COMMAND` 的那几处必是后端命令表里有的命令。 */
    @Test
    fun theTableCoversEveryCommandTheLinkNames() {
        val typed =
            Regex("""^\s*"([a-z0-9-]+)": \{ req: """, RegexOption.MULTILINE)
                .findAll(File(Fixtures.repoRoot, "src/frontend/ui/generated/TypedCommands.ts").readText())
                .map { it.groupValues[1] }
                .toSet()
        val sources = LINK.listFiles { f -> f.extension == "kt" }!!.map { it.readText() }
        val decl = Regex("""const val (\w+)(?:: String)? = "([a-z0-9-]+)"""")
        val named = sources.flatMap { s -> decl.findAll(s).map { it.groupValues[1] to it.groupValues[2] } }
        val unknown = named.filter { (k, v) -> k == "COMMAND" && v !in typed }.map { it.second }
        assertEquals("这几个 COMMAND 不在后端命令表里", emptyList<String>(), unknown)
        val byObject = Commands.ALL.values.associateBy { it::class.java.simpleName }
        val taken = Regex("""Commands\.(\w+)\.NAME""")
        val generated = sources.flatMap { s -> taken.findAll(s).map { byObject.getValue(it.groupValues[1]).name } }
        val commands = (named.map { it.second }.filter { it in typed } + generated).toSortedSet()
        assertTrue("link/ 里一条命令名都没认出来 —— 本条在空转", commands.size > 20)
        assertEquals(commands, sent.keys.toSortedSet())
    }

    private companion object {
        const val GOLDEN = "mobile-requests.golden.json"
        val SHA = "ab".repeat(32)

        val LINK = File(Fixtures.repoRoot, "src/mobile/core-claude/src/main/kotlin/com/ccmonitor/mobile/core/claude/link")

        /** 别名页表单那一形（「建这条」交的那几格；没交的那台按缺省）。 */
        val FORM =
            ProfileForm(
                name = "cct",
                from = null,
                account = null,
                tmux = ProfileTmuxPick(ProfileTmuxMode.FIXED, "cc"),
                cwdIf = null,
                cwd = "~/w",
                agent = null,
                args = "--resume",
                launcher = null,
                tmuxSize = null,
                detach = false,
                tab = null,
                tabGroup = null,
                say = null,
            )
    }
}
