package com.ccmonitor.mobile.ssh

import com.ccmonitor.mobile.core.claude.agent.AgentProfile
import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * resume 的 tmux 脚手架全仓只有一份；每一家都走它；随家而变的只问档案。
 *
 * 两份脚手架（一家一份）逐字几乎相同，改一份忘一份就漂开；靠档案里另一格（比如「会话文件怎么摆」）
 * 捎带决定打不打 `@ccm_sid` 的话，第三家若也按 cwd 编码目录存会话，会静默拿到 Claude 的标记。
 *
 * | # | 问什么 | bug 在时 |
 * |---|---|---|
 * | ① | 脚手架独有的那段 cwd 抽取在全部生产源码里恰好写了一次、就在网关里 | 有人再抄一份 ⇒ 红 |
 * | ② | 把随家而变的三段（定位 · 载荷 · 标记）换成占位符之后，各家命令串逐字相同 | 某一家长出自己的脚手架细节 ⇒ 红 |
 * | ③ | 打不打 `@ccm_sid` 只由档案 `hasCcmIdentity` 答，且那一格逐家等于后端的 `has_identity` | 标记不看档案（恒打 / 恒不打）⇒ 红 |
 *
 * Codex 不带 `@ccm_sid`：后端的 Codex 档 `has_identity: false`，后端自己从不给 Codex 打这个标记，
 * 破坏性动作只认 `@ccm_sid` 这条通道。我们替它打 = 伪造后端的事实，kill / 送键的身份门对它当场放行。
 *
 * ### 判别力边界
 *
 * | 绕过形态 | 本文件 |
 * |---|---|
 * | 在网关外再抄一份脚手架（含那段 cwd 抽取） | ① 红，点名文件与次数 |
 * | 抄一份但把 cwd 抽取改写成别的样子（`jq`、`awk`…） | ① 抓不到（防手滑不防改写）；`TmuxSessionNameSourceTest` ③a 数 `new-session` 闭集（4 支）兜一层 |
 * | 档案里某一格恒定、脚手架照读 | 本文件不答：`AgentProfileTableTest` 钉整张表 |
 * | 命令串在远端真能跑 | 单测答不了（本仓没有 tmux e2e，没在真机上验） |
 */
class ResumeScaffoldTest {
    private val sid = "0f9d3c2a-1111-4222-8333-444455556666"
    private val agentDir = "/d"

    /** ①：脚手架只写了一份。 */
    @Test
    fun theResumeScaffoldIsWrittenExactlyOnceInProduction() {
        val root = repoRoot()
        val sources = productionSources(root)
        assertTrue("前提：得真扫到源文件，实得 ${sources.size} 个", sources.size > 150)
        val hits = sortedMapOf<String, Int>()
        for (f in sources) {
            val n = countOccurrences(KotlinSourceScanner.codeOnlyKeepingLiterals(f.readText()), SCAFFOLD_FINGERPRINT)
            if (n > 0) hits[f.relativeTo(root).invariantSeparatorsPath] = n
        }
        assertEquals(
            "resume 脚手架（以它独有的 cwd 抽取 `$SCAFFOLD_FINGERPRINT` 为指纹）必须全仓只写一次、就在网关里。" +
                "多出来的那一处是不是又给某一家抄了一份？随家而变的东西请进档案（定位 → `SessionLocator`，" +
                "载荷 → `AgentInvocation`，标记 → `AgentProfile.hasCcmIdentity`）。",
            mapOf(GATEWAY to 1),
            hits.toMap(),
        )
    }

    /** ②：每一家都走同一份：换掉随家而变的三段之后逐字相同。 */
    @Test
    fun everyAgentResumesThroughTheSameScaffold() {
        val skeletons = AgentProfile.ALL.associate { p -> p.kind.name to skeletonOf(p) }
        assertTrue("前提：至少要有两家可比，实得 ${skeletons.keys}", skeletons.size >= 2)
        for ((kind, sk) in skeletons) {
            for (slot in listOf("<LOCATE>", "<PAYLOAD>", "<NAME>")) {
                assertTrue("前提：$kind 的串里 `$slot` 那一段得真被认出来、换掉了 —— 否则下面那句比较守了个寂寞：\n$sk", sk.contains(slot))
            }
        }
        assertEquals(
            "换掉定位 / 载荷 / 会话名 / 身份标记之后，各家 resume 串必须逐字相同（= 只有一份脚手架）：\n" +
                skeletons.entries.joinToString("\n") { "${it.key}: ${it.value}" },
            1,
            skeletons.values.toSet().size,
        )
    }

    /** ③：打不打 `@ccm_sid` 只由档案那一格答，且那一格与后端一致。 */
    @Test
    fun theCcmSidTagFollowsTheProfileCellAndTheCellMatchesTheBackend() {
        for (p in AgentProfile.ALL) {
            val cmd = planFor(p).command
            assertEquals(
                "${p.kind}：命令串里有没有 `@ccm_sid` 必须恰好等于档案 `hasCcmIdentity`（=${p.hasCcmIdentity}）：\n$cmd",
                p.hasCcmIdentity,
                cmd.contains(CCM_SID),
            )
        }
        assertEquals(
            "档案 `hasCcmIdentity` 逐家必须等于后端 `LaunchFace.has_identity`（claude 有 · codex 没有）。" +
                "后端改了就先改这张表、再写清依据：别为了让某一家「在电脑菜单里看得见」就替它伪造事实标记。",
            PINNED_IDENTITY_FROM_BACKEND,
            AgentProfile.ALL.associate { it.kind.name to it.hasCcmIdentity },
        )
    }

    /** 指纹与占位符的自检：① 与 ② 的判别力全押在这两个小函数上。 */
    @Test
    fun theFingerprintAndTheSkeletonHelpersSeeWhatTheyClaim() {
        assertEquals("注释里的不算", 0, countOccurrences(KotlinSourceScanner.codeOnlyKeepingLiterals("// $SCAFFOLD_FINGERPRINT\n"), SCAFFOLD_FINGERPRINT))
        assertEquals("字面里的算", 1, countOccurrences(KotlinSourceScanner.codeOnlyKeepingLiterals("val x = \"$SCAFFOLD_FINGERPRINT\"\n"), SCAFFOLD_FINGERPRINT))
        // 骨架里不许残留任何一家的定位深度、载荷关键字或会话名前缀：残留就说明占位没换干净
        for (p in AgentProfile.ALL) {
            val sk = skeletonOf(p)
            for (leak in listOf("-maxdepth", "--resume", "codex resume", "cc-", "cx-", CCM_SID)) {
                assertTrue("${p.kind} 的骨架里残留了 `$leak`：\n$sk", !sk.contains(leak))
            }
        }
    }

    // ---- 工具 ---------------------------------------------------------------

    private fun planFor(p: AgentProfile): ResumePlan =
        TmuxBackend.resume(
            ResumeSpec(
                sessionId = sid,
                launchCandidates = listOf(null),
                claudeDir = agentDir,
                fallbackCwd = "/w",
                alreadyInTmux = false,
                agentKind = p.kind,
            ),
        ) ?: error("前提：${p.kind} 对合法 sid 必须给出计划")

    /** 一家的 resume 串 → 骨架：随家而变的四段换成占位符（身份标记那一段整段拿掉）。 */
    private fun skeletonOf(p: AgentProfile): String {
        val plan = planFor(p)
        val name = plan.sessionName
        val tag = "tmux set-option -t ${shQuote(name)} $CCM_SID ${shQuote(sid)} 2>/dev/null; "
        return plan.command
            .replace(tag, "")
            .replace(p.sessionLocator.findBySessionIdCommand(agentDir, sid), "<LOCATE>")
            .replace(shQuote(p.invocation.resumeInvocation(plan.launchLabel, sid)), "<PAYLOAD>")
            .replace(name, "<NAME>")
    }

    private fun countOccurrences(
        text: String,
        mark: String,
    ): Int = text.windowed(mark.length).count { it == mark }

    private fun repoRoot(): File =
        generateSequence(File(".").absoluteFile.normalize()) { it.parentFile }
            .take(MAX_WALK_UP)
            .firstOrNull { File(it, "settings.gradle.kts").isFile }
            ?: error("找不到仓根（往上 $MAX_WALK_UP 层都没有 settings.gradle.kts）：${File(".").absolutePath}")

    /** 扫描面：`app/src/main/kotlin` + 每个 `core-…` 模块的 `src/main/kotlin`。 */
    private fun productionSources(root: File): List<File> {
        val roots =
            (listOf(File(root, "app")) + (root.listFiles()?.filter { it.isDirectory && it.name.startsWith("core-") } ?: emptyList()))
                .map { File(it, "src/main/kotlin") }
                .filter { it.isDirectory }
        assertTrue("前提：得找到扫描面（app + core-*），实得 ${roots.size} 个源根", roots.size >= 2)
        return roots.flatMap { r -> r.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }
    }

    private companion object {
        const val MAX_WALK_UP = 6
        const val GATEWAY = "app/src/main/kotlin/com/ccmonitor/mobile/ssh/TmuxCommands.kt"
        const val CCM_SID = "@ccm_sid"

        /**
         * 脚手架的指纹：抽权威 cwd 那一段的开头（Kotlin 源码里的字面形态）。它与 agent 无关、只属于 resume 脚手架：
         * 两家的会话文件首个 `"cwd":"…"` 都用它抽（Claude 在记录行里、Codex 在 session_meta 首行）。
         */
        const val SCAFFOLD_FINGERPRINT = "grep -a -m1 -o '\\\"cwd\\\""

        /** 后端各家档案的 `has_identity`。 */
        val PINNED_IDENTITY_FROM_BACKEND = mapOf("ClaudeCode" to true, "Codex" to false)
    }
}
