package com.ccmonitor.mobile.core.claude.bridge

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 命令目录的整理。全部喂真 golden（`bridge/vectors/` 下的 `.frames.ndjson`），不用手编数据：
 * 整理逻辑存在的理由就是真数据的性质（`skills ⊆ cmds`、`tools` 不可调用）。
 *
 * 断言写成关系而不是硬编码计数，重录 golden 不会误红；
 * 另用一条独立的测试守住「素材本身还符合前提」。
 */
class CommandCatalogTest {
    private fun vectorsDir(): File = File("../bridge/vectors").takeIf { it.isDirectory } ?: File("bridge/vectors")

    private fun allCases(): List<String> =
        vectorsDir()
            .listFiles { f -> f.name.endsWith(".frames.ndjson") }
            .orEmpty()
            .map { it.name.removeSuffix(".frames.ndjson") }
            .sorted()

    private fun initOf(case: String): BridgeFrame.Init =
        ReplayVectors
            .parse(File(vectorsDir(), "$case.frames.ndjson").readLines().asSequence())
            .map { it.frame }
            .filterIsInstance<BridgeFrame.Init>()
            .first()

    /**
     * 分区逻辑的前提：真数据里 `skills ⊆ cmds`。
     *
     * skill 一组、cmds 一组的话，skill 那些条会各显示两遍。
     * 这条守的是素材，坏了要去看 golden 而不是看代码。
     */
    @Test
    fun everyGoldenCaseStillHasSkillsAsASubsetOfCommands() {
        val cases = allCases()
        assertTrue("前提：真的读到了 golden", cases.isNotEmpty())
        for (case in cases) {
            val init = initOf(case)
            assertTrue(
                "$case：skills 必须是 cmds 的子集，否则分区要重新设计（多出来的：${init.skills - init.cmds.toSet()}）",
                init.cmds.toSet().containsAll(init.skills),
            )
        }
    }

    /** 分区互斥、无遗漏，用关系表达。 */
    @Test
    fun skillsAndCommandsArePartitionedNotOverlapping() {
        for (case in allCases()) {
            val init = initOf(case)
            val catalog = CommandCatalog.from(init)
            val skillNames = catalog.skills.map { it.name }.toSet()
            val commandNames = catalog.commands.map { it.name }.toSet()
            assertTrue(
                "$case：两个分区不许有交集（${skillNames intersect commandNames}）",
                (skillNames intersect commandNames).isEmpty(),
            )
            assertEquals("$case：合起来还是完整的 cmds，一条都没丢", init.cmds.toSet(), skillNames + commandNames)
            assertEquals("$case：命令分区 = cmds − skills", init.cmds.size - init.skills.size, catalog.commands.size)
        }
    }

    /**
     * 判据是「插进去有没有用」，不是「属于哪一类」。
     *
     * 可点：斜杠命令（含 skill）· agent（`@agent-<名>`，CLI 认的 mention 形式）。
     * 只读：tools（模型能力）· mcp（服务器名不是可调用的东西）· plugins。
     */
    @Test
    fun clickabilityFollowsWhetherInsertingItActuallyDoesAnything() {
        val catalog = CommandCatalog.from(initOf("long-reply"))
        assertTrue(
            "前提：这个 case 三类都有",
            catalog.tools.isNotEmpty() && catalog.mcp.isNotEmpty() && catalog.agents.isNotEmpty(),
        )

        assertTrue("工具只读", catalog.tools.all { it.insertable == null })
        assertTrue("MCP 只读：服务器名插进草稿不是可调用的东西", catalog.mcp.all { it.insertable == null })

        assertTrue("命令/skill 可点", (catalog.commands + catalog.skills).all { it.insertable != null })
        assertEquals("斜杠命令 + 一个空格（用户还要接参数）", "/clear ", catalog.commands.first { it.name == "clear" }.insertable)
        // agent 的唯一生效形式是 `@agent-<名>`，且必须在行首或空白后（CLI 的 mention 正则）
        assertEquals("@agent-Explore ", catalog.agents.first { it.name == "Explore" }.insertable)
    }

    /** MCP 的 status 仍要带给 UI 显示：`needs-auth` 要让人看到，只是不当可点判据。 */
    @Test
    fun mcpKeepsItsStatusAsAVisibleNoteEvenThoughItIsReadOnly() {
        val init = initOf("long-reply")
        assertTrue("前提：真数据里确实有连不上的", init.mcp.any { it.status != "connected" })
        val mcp = CommandCatalog.from(init).mcp
        assertTrue("每条都要带 status", mcp.all { it.note != null })
        assertNotNull(mcp.first { it.note == "needs-auth" })
    }

    /**
     * `plugins` 是第六件。所有 golden 里它都是空的，这条守的是「读没读这个字段」。
     */
    @Test
    fun theSixthCatalogFieldIsActuallyRead() {
        val init = initOf("long-reply")
        assertTrue("前提：录到的 golden 里 plugins 恒空", init.plugins.isEmpty())
        // 素材没有内容可测 ⇒ 用一份带 plugin 的 init 证明读了这个字段
        val withPlugin = CommandCatalog.from(init.copy(plugins = listOf("some-plugin")))
        assertEquals(listOf("some-plugin"), withPlugin.plugins.map { it.name })
        assertTrue("插件只读", withPlugin.plugins.all { it.insertable == null })
        assertTrue("也要进 `all`，否则 isEmpty 会说谎", withPlugin.all.any { it.name == "some-plugin" })
    }

    /** 空态不是假想的边角：`demo-debug` 真的没有 skill。 */
    @Test
    fun theEmptyPartitionsInRealDataAreHandled() {
        val init = initOf("demo-debug")
        assertTrue("前提：这个 case 真的没有 skill", init.skills.isEmpty())
        val catalog = CommandCatalog.from(init)
        assertTrue(catalog.skills.isEmpty())
        assertEquals("没有 skill 可减 ⇒ 命令分区就是全部 cmds", init.cmds.size, catalog.commands.size)
        assertFalse("但整体不空 —— 它有命令和工具", catalog.isEmpty)
    }

    /** 还没收到 `init`（重放的是没有 init 的片段）⇒ 空 catalog，不是崩，也不该弹空壳面板。 */
    @Test
    fun aMissingInitFrameYieldsAnEmptyCatalogRatherThanACrash() {
        val catalog = CommandCatalog.from(null)
        assertTrue(catalog.isEmpty)
        assertTrue(catalog.all.isEmpty())
    }
}
