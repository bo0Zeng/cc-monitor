package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.core.claude.transport.ClaudePaths
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 没选账号就不许起对话，而且要说人话。
 *
 * 没填 `hosts.claudeDir` 时命令是 `… | env claude --resume …`（`env` 后面什么都没有），
 * 跑成远端默认那个账号（没登录），界面回一句 `api_error`，看到它的人不知道要去选账号。
 *
 * 这份判据钉两件事：
 * ① [hasChosenAccount] 分辨得出「没选过」与「选了『听远端自己的』」：
 *    后者在 [explicitClaudeDir] 眼里同样是 null，拿它当判据会把那种用户永远挡在门外；
 * ② 那句话是具体字符串，不是「界面有没有报错」：后者连 `api_error` 都算通过。
 */
class ChatAccountGateTest {
    private val default = ClaudePaths.DEFAULT_CLAUDE_DIR

    /**
     * 本判据的要害：`hasChosenAccount` 与 `explicitClaudeDir` 在这一格上必须不一致。
     *
     * 「显式选了那个默认表达式」时：`explicitClaudeDir` 给 null（对：不覆盖远端），
     * 而 `hasChosenAccount` 必须给 `true`（用户表过态了）。
     * 两者要是恒等，那本字段就没有存在理由，门也就把这类用户堵死了。
     */
    @Test
    fun choosingTheRemoteDefaultIsStillChoosingEvenThoughNothingGetsOverridden() {
        assertNull("前提：选了默认表达式 ⇒ 什么都不传给远端", explicitClaudeDir(default, null))
        assertTrue("但它是一次选择：门必须放行", hasChosenAccount(default, null))
        assertFalse("而从没选过必须被拦住", hasChosenAccount(null, null))
    }

    /** 空白 = 没填。用户把内容删干净和从没填过是同一件事。 */
    @Test
    fun blankCountsAsNeverChosen() {
        assertFalse("null", hasChosenAccount(null, null))
        assertFalse("空串", hasChosenAccount("", ""))
        assertFalse("全空白", hasChosenAccount("   ", "\t"))
    }

    /** 应用级默认同样算选过：漏掉它就会出现「设置里明明填了、还说没选账号」。 */
    @Test
    fun anAppLevelDefaultAlsoCountsAsChoosing() {
        assertTrue("每主机填了", hasChosenAccount("/home/u/.claude-b", null))
        assertTrue("只有应用级默认填了", hasChosenAccount(null, "/home/u/.claude-b"))
        assertTrue("两个都填了", hasChosenAccount("/a", "/b"))
    }

    /**
     * 判据钉具体字符串，两侧都钉。
     *
     * 只钉「有没有显示东西」的话，`api_error` 也算通过，而它正是本条要消灭的那个东西。
     */
    @Test
    fun theMessageNamesTheRealCauseAndIsNotAMachineCode() {
        assertTrue("必须逐字含这句", NO_ACCOUNT_CHOSEN.contains("这台机器还没选账号"))
        assertFalse("绝不许退化成机器码", NO_ACCOUNT_CHOSEN.contains("api_error"))
        assertTrue("还要说得出该干什么（响亮还要能行动）：从抽屉底行进服务器列表", NO_ACCOUNT_CHOSEN.contains("抽屉最底一行"))
    }

    /**
     * 文案受禁用词表约束。
     *
     * 大小写不敏感：`contains("SSH")` 这种写法会让 `ssh` / `Tmux` / `Daemon` 原样溜过去。
     * 与 `NavRoutesTest` 的 `lowercase()` 写法一致，同一个仓不留两套强度。
     *
     * 判别力边界：它只守 [NO_ACCOUNT_CHOSEN] 这一个常量；主机编辑器里的内联字面量由
     * `SettingsGroupWordlistTest` 管。
     */
    @Test
    fun theMessageObeysTheProductWordList() {
        val banned = listOf("连接", "SSH", "tmux", "会话", "exec", "bridge", "daemon", "offset")
        val lower = NO_ACCOUNT_CHOSEN.lowercase()
        banned.forEach {
            assertFalse("禁用词「$it」不许出现在用户看得见的地方：$NO_ACCOUNT_CHOSEN", lower.contains(it.lowercase()))
        }
    }

    // ---- 射程：门真的挡在起管道那条路上，而不是挂在一个没人走的分支上 ----

    private fun appMainSources(): List<File> {
        val dir = File("src/main/kotlin").takeIf { it.isDirectory } ?: File("app/src/main/kotlin")
        return dir.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList()
    }

    /**
     * 剥注释、留字面量，走全仓唯一一份词法扫描器 [KotlinSourceScanner.codeOnlyKeepingLiterals]。
     *
     * 扫的是 `PipeLauncher.start(` / `launch.accountChosen` / `return@LaunchedEffect` 这类代码。
     * 它看不见什么写在 [KotlinSourceScanner] 的头注里。
     */
    private fun codeOnly(text: String): String = KotlinSourceScanner.codeOnlyKeepingLiterals(text)

    /**
     * 起管道只有一个生产调用点，而门在它前面、且门真的会把这一趟停下来。
     *
     * 光有一个纯函数不算数：它可以被接在一条没人走的路上（「已 build 零引用」）。
     * 这条判据量的是位置 + 那个早退。
     *
     * ### 判别力边界
     *
     * | 形态 | 这条判据 |
     * |---|---|
     * | 门被删掉 / 挪到起管道之后 | 红 |
     * | 少了 `return@LaunchedEffect`（门照报人话、管道照起） | 红（下面那条断言就是为它加的） |
     * | 门被包进一个恒假条件（`if (false && …)`） | 抓不到 |
     * | 谓词写反（`if (launch.accountChosen)`） | 抓不到 |
     * | 起管道那句被格式化成多行 `PipeLauncher\n.start(` | 抓不到（字符串匹配吃格式） |
     *
     * 后三条只能靠真机。
     *
     * 射程只到 `app/src/main/kotlin`：`app/src/debug/kotlin` 里的 `ChatReplayActivity` 有第二个起管道点
     * （debug-only 调试入口），它不过这道门。
     */
    @Test
    fun theGateSitsInFrontOfTheOnlyPlaceThatStartsThePipe() {
        val files = appMainSources()
        assertTrue("前提：得真扫到源文件，否则这条测试是空跑", files.size > 20)

        val starterMark = "PipeLauncher" + ".start("
        val starters = files.filter { codeOnly(it.readText()).contains(starterMark) }.map { it.name }
        // 不写成 assertEquals(listOf("ChatRoute.kt"), …)：那样改个文件名就误红，
        //   而误红会训练人绕过判据。这里分成「只许一条」与「那条是谁」两句，各自的失败含义才清楚。
        assertEquals("main 源集里起管道只许有一条路（多一条就是绕过门的第二条路）：$starters", 1, starters.size)
        assertTrue("那唯一一条必须是聊天屏：$starters", "ChatRoute.kt" in starters)

        val code = codeOnly(files.first { it.name == "ChatRoute.kt" }.readText())
        val gateAt = code.indexOf("launch.accountChosen")
        // 起管道那一步被 detekt `LongMethod` 逼进了一个私有帮手，所以这里量的是对它的调用
        // （第一处出现 = 调用点，声明在文件更靠后处）。
        val startAt = code.indexOf("startPipeFor(")
        assertTrue("门必须真的在代码里（不是只写在注释里）", gateAt >= 0)
        assertTrue("起管道那一步必须真的被调用", startAt >= 0)
        assertTrue("门必须排在起管道之前，实得 gate=$gateAt start=$startAt", gateAt in 0 until startAt)

        // 光有「门在前面」不够：少了早退的话，门只是报个信然后照样起。
        val body = code.substring(gateAt, startAt)
        assertTrue("门与起管道之间必须有那句早退，否则门不是门：$body", body.contains("return@LaunchedEffect"))
        assertTrue("而且要真的把那句人话报出去：$body", body.contains("NO_ACCOUNT_CHOSEN"))
    }

    /**
     * 接线那一行也要有判据。
     *
     * 纯函数判据再多，也守不住「有人把 `accountChosen = …` 改成 `explicitClaudeDir(...) != null`」：
     * 那一行住在 Composable 的 `produceState` 里，上面几条全绿，而要治的 bug 原样复活。
     *
     * 判别力边界：它量的是一行字面量，改个写法（换行、加空格）会误红；
     * 换成等价但错误的表达式（例如把两个实参对调）它也抓不到。
     */
    @Test
    fun theFieldIsWiredToTheRawColumnsNotToTheResolvedOverride() {
        val code = codeOnly(appMainSources().first { it.name == "ChatLaunchContext.kt" }.readText())
        // 外面套了一层按 kind 分档（`accountChosenFor`），门本身是 `hasChosenAccount` 的两个实参，
        //   钉的仍是那两个原始字段。
        assertTrue(
            "必须逐字接在原始字段上（改成 explicitClaudeDir(...) != null 时其余判据全绿）",
            code.contains("accountChosen = accountChosenFor(host?.agentKindOrDefault(), host?.claudeDir, appDefault)"),
        )
        // 分档不许把种类漏掉：接成 `accountChosenFor(null, …)` 的话 Codex 又会被叫去填 Claude 账号目录
        assertTrue(
            "分档必须真的读那台服务器的 agent 种类（逐点判定见 AgentArchetypeTest）",
            code.contains("accountChosenFor(host?.agentKindOrDefault()"),
        )
    }

    /**
     * 起管道那六个参数逐字钉住。
     *
     * `PipeLauncher.start(...)` 住在 `startPipeFor` 里；「参数对调 / 漏传一个」在别处没有任何东西会红
     * （`resumeIdFor`/`newSessionIdFor` 本体有判据，但「`ChatRoute` 有没有把它们接到对的形参上」没有）。
     *
     * 判别力边界：吃格式（换行/改空格会误红），且只认字面量，换成等价但错误的表达式抓不到。
     * 根治要把参数装配提成纯函数返回一个数据类。
     */
    @Test
    fun theSixLaunchArgumentsStayWiredWordForWord() {
        val code = codeOnly(appMainSources().first { it.name == "ChatRoute.kt" }.readText())
        listOf(
            "launchCommand = launch.launchCommand",
            "workdir = launch.workdir",
            "permissionMode = permissionMode",
            "resumeSessionId = resumeIdFor(sessionId, isNew)",
            "newSessionId = newSessionIdFor(sessionId, isNew)",
            "claudeDir = launch.claudeDir",
        ).forEach { assertTrue("起管道的参数少了或换了：「$it」", code.contains(it)) }
    }

    /** 前提自检：剥注释这一步真在起作用，否则上面那条可能因为「整份文件被当注释剥光」而恒绿。 */
    @Test
    fun theCommentStrippingActuallyStripsCommentsAndKeepsCode() {
        val sample =
            """
            /** KDoc 里提 launch.accountChosen 是合法的 */
            // 行注释里也提 launch.accountChosen
            if (!launch.accountChosen) return
            """.trimIndent()
        val code = codeOnly(sample)
        assertTrue("代码里的必须留下来", code.contains("if (!launch.accountChosen)"))
        assertFalse("KDoc 里的必须被剥掉", code.contains("KDoc 里提"))
        assertFalse("行注释里的必须被剥掉", code.contains("行注释里也提"))
    }
}
