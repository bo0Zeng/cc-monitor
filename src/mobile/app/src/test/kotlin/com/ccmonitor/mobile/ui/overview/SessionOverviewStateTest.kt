package com.ccmonitor.mobile.ui.overview

import com.ccmonitor.mobile.core.claude.catalog.SessionLight
import com.ccmonitor.mobile.core.claude.transport.DaemonSessionSource
import com.ccmonitor.mobile.core.remote.ConnectionDeadException
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** `DaemonSessionSource.State` → 三分区 UI 状态的纯映射。 */
class SessionOverviewStateTest {
    /**
     * 第二个参数是 `activity`：总览的灯读活动档（后端的帧上没有 `status`）。
     * 「灯真的读的是这一格、不读 `status`」由 `SessionOverviewActivityLightTest` 从真帧出发守；本文件只管分区。
     */
    private fun session(
        id: String,
        activity: String? = null,
        attachable: Boolean = true,
        name: String? = null,
        cwd: String? = null,
        waitingFor: String? = null,
    ) = DaemonSessionSource.Session(
        sessionId = id,
        cwd = cwd,
        name = name,
        waitingFor = waitingFor,
        attachable = attachable,
        activity = activity,
    )

    private fun state(vararg s: DaemonSessionSource.Session) = DaemonSessionSource.State(sessions = s.associateBy { it.sessionId })

    /**
     * 分区用的是真活动档词表（`working`/`needs_you`/`idle`，后端 `SessionActivity` 的 snake_case），
     * 而不是自己编的 `running`/`active` 之类。活动档里没有 `shell` 那一档（后端把它并进了 `idle`）。
     */
    @Test
    fun partitionsUseTheRealActivityVocabulary() {
        val ui = state(session("a", "needs_you"), session("b", "working"), session("c", "idle")).toUiState()
        assertEquals("在等人的排最前（我们后端独有的事实）", listOf("a"), ui.needsYou.map { it.sessionId })
        assertEquals(listOf("b"), ui.running.map { it.sessionId })
        assertEquals(listOf("c"), ui.others.map { it.sessionId })
    }

    /**
     * 「其余」是减法不是列举。
     *
     * 列举的话（`light == Idle || light == Shell || …`）`SessionLight` 加一个值，
     * 就会有一批会话凭空消失：每个分区都没匹配上，看不到、也没有任何报错。
     */
    @Test
    fun anUnknownActivityStillShowsUpSomewhere() {
        val ui = state(session("x", "某个我们没见过的状态"), session("y", null)).toUiState()
        assertTrue("前提：这两个都不该落进前两区", ui.needsYou.isEmpty() && ui.running.isEmpty())
        assertEquals("一条都不许丢", setOf("x", "y"), ui.others.map { it.sessionId }.toSet())
        assertEquals("未知状态点中性灯，不误报", SessionLight.Unknown, ui.others.first().light)
    }

    /**
     * `attachable == false` ⇒ 不给 attach 类动作，但这一行仍然要显示。
     * 它是一条真实存在的会话，藏起来才是说谎。
     */
    @Test
    fun anUnattachableSessionIsStillListed() {
        val ui = state(session("a", "working", attachable = false)).toUiState()
        assertEquals("不许从列表里消失", listOf("a"), ui.running.map { it.sessionId })
        assertFalse(ui.running.single().attachable)
    }

    /**
     * 被顶替的单列一区，不混进「其余」：`/clear` 只是原地换了个 sid，
     * 混进去会让人以为会话崩了。
     */
    @Test
    fun supersededSessionsGetTheirOwnBucketAndAreNotCountedAsPresent() {
        val ui =
            DaemonSessionSource
                .State(
                    sessions = mapOf("live" to session("live", "working")),
                    superseded = mapOf("old" to session("old", "working", cwd = "/proj")),
                ).toUiState()
        assertEquals(listOf("old"), ui.superseded.map { it.sessionId })
        assertTrue("不许混进活动分区", (ui.needsYou + ui.running + ui.others).none { it.sessionId == "old" })
        // 归档行不许被判成「已停」。喂 `lightForStatus(_, alive=false)` 会恒得 Stopped
        //   ⇒ 屏上写「× 已停」，数据层刚把「不是死了」救回来、像素层又判死一次。
        assertTrue("归档要有自己的标记，不是一个「已停」的灯", ui.superseded.single().archived)
        assertFalse("活动行不带归档标记", ui.running.single().archived)
        assertEquals("要留住 cwd，否则只剩一个光秃秃的 sid", "/proj", ui.superseded.single().cwd)
        assertFalse("还有活会话 ⇒ 整体不算空", ui.isEmpty)
    }

    /**
     * 下线了但说不出为什么要有自己的一区：这是「生产消费方」那一半。
     *
     * 只验 `DaemonSessionSource.State` 多了一格是不够的（「已 build 零引用」：数据层做对了、
     * UI 一个字都没读，功能零可见效果），所以这条从 UI 状态出发断言那一区非空。
     *
     * 三条各守各的：
     * ① 不许并进「其余」（那一区说的是「还在，只是没在跑」）；
     * ② 不许复用 `archived`（那是「被接替」，一件我们知道的事）；
     * ③ 不许点成「已停」（喂 `lightForStatus(_, alive = false)` 会恒得 Stopped）。
     */
    @Test
    fun aRemovalTheServerCannotExplainGetsItsOwnBucket() {
        val ui =
            DaemonSessionSource
                .State(
                    sessions = mapOf("live" to session("live", "working")),
                    removedUnknown = mapOf("gone?" to session("gone?", "working", cwd = "/proj")),
                ).toUiState()
        assertEquals(listOf("gone?"), ui.removedUnknown.map { it.sessionId })
        assertTrue("不许并进活动分区", (ui.needsYou + ui.running + ui.others).none { it.sessionId == "gone?" })
        assertTrue("也不许并进归档区（那是「被接替」，一件我们知道的事）", ui.superseded.isEmpty())
        val row = ui.removedUnknown.single()
        assertFalse("不是 archived —— 它没被接替", row.archived)
        assertTrue("要有自己的标记", row.removalUnknown)
        assertNotEquals("不许被判成「已停」", SessionLight.Stopped, row.light)
        assertEquals("要留住 cwd，否则只剩一个光秃秃的 sid", "/proj", row.cwd)
    }

    /**
     * 只剩「说不出为什么下线」的行时，屏上不许说「这台服务器上还没有对话」。
     *
     * 空态那一支会让整张列表根本不渲染，那几行连同它们那句解释一起被吞掉，「无声消失」换个地方又发生一次。
     */
    @Test
    fun rowsWeCannotExplainStopTheScreenFromClaimingThereAreNoConversations() {
        val ui = DaemonSessionSource.State(removedUnknown = mapOf("x" to session("x"))).toUiState()
        assertTrue("活动三区确实是空的（前提）", ui.isEmpty)
        assertFalse("但屏幕上不是空的 —— 不许走空态文案", ui.isCompletelyEmpty)
        assertTrue("而真的什么都没有时照旧走空态", DaemonSessionSource.State().toUiState().isCompletelyEmpty)
    }

    /** 归档不算「有会话」：只有归档时空态该照常出现。 */
    @Test
    fun archiveAloneDoesNotCountAsHavingSessions() {
        val ui = DaemonSessionSource.State(superseded = mapOf("old" to session("old"))).toUiState()
        assertTrue(ui.isEmpty)
    }

    /**
     * 拥塞/致命都要看得见，但文案归 app，不归 core。
     *
     * core 造的那句 degraded 带方法名（`acknowledgeDegraded`），原样上屏就把内部术语端给了用户。
     * 所以只吃结构化事实（`dropped`），文案自己写。
     */
    @Test
    fun theBannerTextIsBuiltHereFromFactsNotCopiedFromCore() {
        val ui = DaemonSessionSource.State(dropped = 3).toUiState()
        assertTrue("要说清丢了多少：${ui.degraded}", ui.degraded!!.contains("3"))
        assertFalse("不许把内部术语带上屏", ui.degraded!!.contains("acknowledgeDegraded") || ui.degraded!!.contains("D2"))
        assertNull("没丢过就不该有这条横幅", DaemonSessionSource.State().toUiState().degraded)
    }

    // ---- 血统 + 补齐 ----------------------------------------------------

    /**
     * 没有分支时一行不动：`depth` 全 0，顺序原样。
     * 这是常态（多数对话没有分支），UI 不该为「树」预留恒空的缩进槽位。
     */
    @Test
    fun withoutLineageTheOrderAndDepthAreUntouched() {
        val ui = state(session("a", "idle"), session("b", "idle")).toUiState()
        assertEquals(listOf("a", "b"), ui.others.map { it.sessionId })
        assertTrue(ui.others.all { it.depth == 0 })
    }

    /** 有 `forkedFrom` ⇒ 子紧随父、缩进一级（同一分区内）。 */
    @Test
    fun aForkedSessionIsIndentedUnderItsParent() {
        val ui =
            state(session("root", "idle"), session("other", "idle"), session("fork", "idle"))
                .toUiState(parentOf = mapOf("fork" to "root"))
        assertEquals(listOf("root", "fork", "other"), ui.others.map { it.sessionId })
        assertEquals(listOf(0, 1, 0), ui.others.map { it.depth })
    }

    /**
     * 缩进只在父就在同一分区里时才画（不画空的树枝）。
     *
     * 子在「需手动」、父在「正在跑」时，子行缩进了而它上面没有父，缩进指向空气。
     * 分叉的典型动机就是「跑点别的」，父子状态通常不同，这不是边角，是常态。
     */
    @Test
    fun aChildIsOnlyIndentedWhenItsParentIsInTheSameSection() {
        val crossSection =
            state(session("parent", "working"), session("child", "needs_you"))
                .toUiState(parentOf = mapOf("child" to "parent"))
        assertEquals("前提：父子确实被切进了不同分区", listOf("parent"), crossSection.running.map { it.sessionId })
        assertEquals(listOf("child"), crossSection.needsYou.map { it.sessionId })
        assertEquals("跨分区 ⇒ 不许缩进，那会指向空气", 0, crossSection.needsYou.single().depth)

        val sameSection =
            state(session("parent", "idle"), session("child", "idle"))
                .toUiState(parentOf = mapOf("child" to "parent"))
        assertEquals("同分区 ⇒ 照常缩进", listOf(0, 1), sameSection.others.map { it.depth })
    }

    /**
     * `/branch`、`/clear` 是原地换 sid：父被推进归档表、子留在活动表。
     *
     * `toUiState` 若只把 `sessions` 喂进血统排布，`SessionLineage` 会判「父不在场 ⇒ 按根」，
     * 唯一会真的产生血统的那条路径上树恒是平的。
     */
    @Test
    fun aChildWhoseParentWasSupersededStillGetsItsDepth() {
        val ui =
            DaemonSessionSource
                .State(
                    sessions = mapOf("child" to session("child", "working")),
                    superseded = mapOf("parent" to session("parent", "idle")),
                ).toUiState(parentOf = mapOf("child" to "parent"))
        assertEquals("前提：父确实在归档区", listOf("parent"), ui.superseded.map { it.sessionId })
        // 归档区与活动区是两个分区 ⇒ 按上面那条规则不缩进（缩进会指向空气）。
        //   归档行仍参与排布（顺序对），只是不画那根树枝；真正的树状视图在历史页。
        assertEquals("跨分区不缩进", 0, ui.running.single().depth)
    }

    /**
     * 血统数据坏掉（环 / 父不在场）不许让对话从列表里消失：它只是装饰，而列表是事实。
     */
    @Test
    fun brokenLineageNeverLosesARow() {
        val base = state(session("x", "idle"), session("y", "idle"))
        for (parents in listOf(mapOf("x" to "y", "y" to "x"), mapOf("x" to "不存在"), mapOf("x" to "x"))) {
            val ui = base.toUiState(parentOf = parents)
            assertEquals("形状=$parents", setOf("x", "y"), ui.others.map { it.sessionId }.toSet())
        }
    }

    /** `droppedLines` 是结构化事实，VM 靠它决定重连后要抹平多少（core 只出事实）。 */
    @Test
    fun droppedLinesIsCarriedAsANumberNotOnlyAsProse() {
        val ui = DaemonSessionSource.State(dropped = 7).toUiState()
        assertEquals(7, ui.droppedLines)
        assertTrue("文案是由它派生的：${ui.degraded}", ui.degraded!!.contains("7"))
        assertEquals("没丢过就是 0", 0, DaemonSessionSource.State().toUiState().droppedLines)
    }

    /** 兜底档的文案（既不是连接死、也不是对端退出）：用来证明「对端退出」那档真的不同。 */
    private fun other0(): String =
        DaemonSessionSource.State(fatal = "某个我们没分类过的失败").toUiState().fatal!!

    /**
     * `fatal` 分两档，靠 `State.cause` 分。
     *
     * 「流正常结束（对端退出）」不是「连不上」：无条件前缀「连不上 daemon：」的话，
     * 一张完整的列表上方会挂着「连不上」。
     */
    @Test
    fun fatalIsClassifiedByCauseAndNeverLeaksTheRawString() {
        val dead =
            DaemonSessionSource
                .State(fatal = "daemon 探测失败：命令 /opt/remote-daemon --with-bg", cause = ConnectionDeadException("no conn"))
                .toUiState()
        assertTrue("连接死 ⇒ 「还没连上」：${dead.fatal}", dead.fatal!!.contains("还没连上"))

        val ended = DaemonSessionSource.State(fatal = "daemon 流已结束（对端退出）。此后的会话表不再更新。").toUiState()
        assertFalse("对端退出不是「连不上」：${ended.fatal}", ended.fatal!!.contains("连不上"))
        // 光断言「不含连不上」判别力不够：兜底文案也不含它，把这条分支整个 no-op 掉测试照样绿。
        //   所以要断言它确实说出了「已经退出」这件事。
        assertTrue("要如实说是退出了：${ended.fatal}", ended.fatal!!.contains("退出"))
        assertNotEquals("且必须与兜底文案不同", other0(), ended.fatal)

        // 三条都不许把 core 的原串（含远端命令行/绝对路径/术语）透出去：`daemon`/`exec`/`SSH` 都是禁用词
        val other = DaemonSessionSource.State(fatal = "daemon 探测失败：命令 /opt/remote-daemon --with-bg").toUiState()
        for (t in listOf(dead.fatal!!, ended.fatal!!, other.fatal!!)) {
            assertFalse("不许出现 daemon：$t", t.contains("daemon"))
            assertFalse("不许带远端命令串：$t", t.contains("--with-bg"))
        }
    }

    @Test
    fun titleFallsBackToSid8LikeTheExistingHistorySheet() {
        val ui = state(session("abcdefgh12345", name = null), session("z", name = "取个名字")).toUiState()
        assertEquals(setOf("abcdefgh", "取个名字"), ui.others.map { it.title }.toSet())
        assertEquals(
            "空白名字也要退回 sid8",
            "q1234567",
            state(session("q1234567", name = "  "))
                .toUiState()
                .others
                .single()
                .title,
        )
    }
}
