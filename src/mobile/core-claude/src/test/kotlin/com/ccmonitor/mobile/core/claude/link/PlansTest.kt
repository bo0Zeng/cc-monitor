package com.ccmonitor.mobile.core.claude.link

import com.ccmonitor.mobile.core.wire.gen.Commands
import com.ccmonitor.mobile.core.wire.gen.PlanCellViewReq
import com.ccmonitor.mobile.core.wire.gen.PlanListReq
import com.ccmonitor.mobile.core.wire.gen.PlanLook
import com.ccmonitor.mobile.core.wire.gen.PlanPbState
import com.ccmonitor.mobile.core.wire.gen.PlanReadReq
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * 计划三屏读的成品：`plan-list` · `plan-read` 的回包照生成类型解（字都是后端写好的），这里只钉解得开、按编号找得到。
 * 夹具是契约的形状、值是中性的字（`后端·…` ⇒ 看它原样到手）。
 */
class PlansTest {
    private fun read() = Commands.PlanRead.reply(Json.value(READ))

    /** 需手动那条的负责人那一行 · 块根那一整句 · 根格是哪一块：都照后端写好的那一格解出来。 */
    @Test
    fun aPlanReadReplyCarriesTheOwnerLineTheBlockRootAndTheRootBlock() {
        val read = read()
        assertNotNull("夹具照契约的形状，解不开就是两端对不上", read)
        val sl = read!!.slice("ledger")!!
        assertEquals(listOf(null, "后端·负责人"), sl.needs.map { it.who })
        val root = sl.cell("A2")!!
        assertEquals("后端·块根 · 后端·负责人 · 审计", root.said.blockRoot)
        assertEquals("B1", root.rootOf)
        assertNull(sl.cell("A1")!!.rootOf)
        assertNull(sl.cell("A1")!!.said.blockRoot)
    }

    @Test
    fun cellsAreFoundByIdAndLabelledAsTheBackendWrote() {
        val sl = read()!!.slice("ledger")!!
        assertEquals("A1.1 标题A1.1", sl.label("A1.1"))
        assertEquals("没这格", sl.label("没这格"))
        assertEquals(listOf("A1", "A2"), sl.cellsOf(sl.top + "没这格").map { it.id })
        assertTrue(sl.hasKids(sl.cell("A1")!!))
        assertFalse(sl.hasKids(sl.cell("A2")!!))
        assertEquals(listOf(PlanLook.DONE, PlanLook.OPEN), sl.cellsOf(sl.top).map { it.look })
        assertNull(read()!!.slice("没这片"))
        assertNull(sl.cell("没这格"))
    }

    @Test
    fun aListReplyAndTheAsks() {
        val list = Commands.PlanList.reply(Json.value(LIST))!!
        assertEquals(PlanPbState.OK, list.pb.state)
        val slice = list.workspaces[0].slices[0]
        assertEquals("后端·1/4", slice.said.root)
        assertNull(Commands.PlanRead.reply(mapOf("workspace" to "/w")))
        assertEquals(emptyMap<String, Any?>(), Commands.PlanList.args(PlanListReq()))
        assertEquals(mapOf("workspace" to "/w"), Commands.PlanRead.args(PlanReadReq("/w")))
        assertEquals(mapOf("workspace" to "/w", "slice" to "ledger", "id" to "A1"), Commands.PlanCellView.args(PlanCellViewReq("/w", "ledger", "A1")))
        assertEquals("pb 印的", Commands.PlanCellView.reply(mapOf("view" to "pb 印的"))?.view)
    }

    private companion object {
        const val EDGES = """{"to":[],"with":[],"after":[],"replaces":[]}"""

        fun cell(
            id: String,
            more: String,
        ): String =
            """{"id":"$id","title":"标题$id","kind":"模块","body":"做成这样","edges":$EDGES,"pointedBy":$EDGES,"state":[],"stateLine":null,
            |"evidencedBy":[],"undecided":false,"quiet":null,"whyCode":null,"whyShort":null,"sets":[],"block":null,"owner":null,"signer":null,
            |"covers":[],"coveredBy":[],"refs":{"title":[],"body":[]},"returned":null,$more}
            """.trimMargin()

        fun block(
            id: String,
            more: String,
        ): String =
            """{"id":"$id","inside":null,"dir":null,"owner":null,"at":null,"listed":true,"handoff":null,"handoffError":null,
            |"takeover":"open","groups":{"groups":[],"text":"","unread":null},$more}
            """.trimMargin()

        val READ =
            """{"pb":null,"workspace":"/w","repo":null,"auto":false,"domains":[],"rev":"r0","readAt":0,"readAtText":"02:11","seen":null,
            |"stale":null,"needCount":1,"bySession":{},"slices":[{"name":"ledger","domain":"软件","path":null,"current":true,"docsOnly":false,
            |"error":null,"stale":null,"bare":false,"kinds":[],"phases":[],"doneValue":"已签收","top":["A1","A2"],"progress":{"done":1,"of":2},
            |"openCells":1,"blocks":[${block("project", """"cells":[],"phase":"开发","said":{"name":"后端·顶层","inside":null}""")},
            |${block("B1", """"cells":["A2"],"phase":"审计","said":{"name":"后端·块名","inside":"后端·里面"}""")}],
            |"unbuiltBlocks":[],"unowned":{"paths":null,"why":null},"pendingSets":[],"check":{"unreadable":[],"unparsed":[],"undecidable":[],"red":[]},
            |"labels":{"A1":"A1 标题A1","A1.1":"A1.1 标题A1.1","A2":"A2 标题A2"},
            |"cells":[${cell("A1", """"parent":null,"children":["A1.1"],"files":[],"statusCode":"done","look":"done","end":"已签收","rootOf":null,"said":{"trail":null,"blockRoot":null}""")},
            |${cell("A1.1", """"parent":"A1","children":[],"files":[],"statusCode":"done","look":"done","end":"已签收","rootOf":null,"said":{"trail":"后端·上面几层","blockRoot":null}""")},
            |${cell("A2", """"parent":null,"children":[],"files":[],"statusCode":"open","look":"open","end":"1/3","rootOf":"B1","said":{"trail":null,"blockRoot":"后端·块根 · 后端·负责人 · 审计"}""")}],
            |"archived":[],"audits":[],"needCount":2,"returned":null,
            |"needs":[{"key":"k1","kind":"red","block":"B1","cell":"A2","sid":null,"red":0,"acked":false,"title":"后端·不通过","lines":["缺文件"],"pos":"后端·1/2","who":null},
            |{"key":"k2","kind":"ended","block":"B1","cell":"A2","sid":"s1","acked":false,"title":"后端·停了","lines":["后端·底下 1/3"],"pos":"后端·2/2","who":"后端·负责人"}],
            |"said":{"strip":null,"bare":null,"readAt":"后端·读到","top":"后端·顶层","root":"后端·1/2"}}]}
            """.trimMargin()

        const val LIST =
            """{"pb":{"state":"ok","said":null},"workspaces":[{"workspace":"/w","repo":null,"auto":false,"rev":"r0","stale":null,"needCount":1,
            "bySession":{},"slices":[{"name":"ledger","domain":"软件","current":false,"progress":{"done":1,"of":4},"needCount":1,"error":null,
            "stale":null,"said":{"strip":null,"bare":null,"readAt":null,"top":null,"root":"后端·1/4"}}]}]}"""
    }
}
