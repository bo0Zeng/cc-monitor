package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PlansTest {
    /** 回包照帧客户端读进来的样子（数是 `Double`，契约要的每一格都在）：一片两格，一条没认可的需手动。 */
    private val read: Any? =
        RawJson.read(
            """
            {"pb":null,"workspace":"/w","repo":null,"auto":false,"domains":[],"rev":"r0","readAt":0,"readAtText":"02:11",
             "stale":null,"needCount":1,"bySession":{},
             "slices":[{"name":"ledger","domain":"x","path":null,"current":true,"docsOnly":false,"error":null,"stale":null,
               "bare":false,"kinds":[],"phases":[],"doneValue":"done","openCells":1,
               "progress":{"done":1,"of":2},"top":["A1","A2"],"labels":{"A1":"A1 t1","A2":"A2 t2"},
               "cells":[
                 {"id":"A1","title":"t1","kind":"k","body":"b","parent":null,"children":[],
                  "edges":{"to":[],"with":[],"after":[],"replaces":[]},"pointedBy":{"to":[],"with":[],"after":[],"replaces":[]},
                  "files":[],"state":[{"value":"done","count":1}],"stateLine":"s","evidencedBy":[],"statusCode":"done",
                  "undecided":false,"look":"done","end":"s","whyShort":null,"whyCode":null,"sets":[],"block":null,
                  "owner":null,"signer":null,"covers":[],"coveredBy":[],"refs":{"title":[],"body":[]}},
                 {"id":"A2","title":"t2","kind":"k","body":"","parent":null,"children":[],
                  "edges":{"to":[],"with":[],"after":[],"replaces":[]},"pointedBy":{"to":[],"with":[],"after":[],"replaces":[]},
                  "files":[],"state":[],"stateLine":null,"evidencedBy":[],"statusCode":"open",
                  "undecided":true,"look":"undecided","end":null,"whyShort":null,"whyCode":null,"sets":[],"block":null,
                  "owner":null,"signer":null,"covers":[],"coveredBy":[],"refs":{"title":[],"body":[]}}],
               "blocks":[{"id":"project","cells":[],"inside":null,"dir":null,"owner":null,"phase":"p","at":null,"listed":true,
                 "handoff":null,"handoffError":null,"takeover":"open","groups":{"groups":[],"text":"","unread":null}}],
               "unbuiltBlocks":[],"unowned":{"paths":null,"why":null},"pendingSets":[],
               "check":{"unreadable":[],"unparsed":[],"red":[],"undecidable":[]},"archived":[],"needCount":1,
               "needs":[{"key":"k","kind":"top","block":"project","cell":null,"sid":null,"acked":false,
                 "title":"top","lines":["l"],"pos":"1/1"}]}]}
            """.trimIndent(),
        )

    @Test
    fun argsUseTheWireFieldNames() {
        assertEquals(mapOf("workspace" to "/w"), PlanRead.args("/w"))
        assertEquals(mapOf("workspace" to "/w", "slice" to "ledger", "id" to "A1"), PlanCellView.args("/w", "ledger", "A1"))
        assertEquals("plan-list", PlanList.COMMAND)
        assertEquals("pb 印的", PlanCellView.of(mapOf("view" to "pb 印的")))
    }

    @Test
    fun outlineComesBackFromTheCoreWithNeedsFirst() {
        val o = Plans.outline(read, "ledger", null)
        assertNotNull(o)
        o!!
        assertEquals("ledger", o.title)
        assertEquals(listOf("top"), o.needs.map { it.kind })
        assertTrue(o.needs[0].title.isNotEmpty() && o.needs[0].pos.isNotEmpty())
        assertEquals(listOf("A1 t1", "A2 t2"), o.rows.map { it.label })
        assertEquals(listOf(PlanLook.DONE, PlanLook.UNDECIDED), o.rows.map { it.look })
        assertNull(o.mine)
        assertFalse(o.meta.isEmpty())
    }

    @Test
    fun cellAndMissingOnes() {
        val c = Plans.cell(read, "ledger", "A1")!!
        assertEquals("A1 t1", c.label)
        assertEquals("b", c.design?.body)
        assertNull(c.inside)
        assertNull(Plans.cell(read, "ledger", "nope"))
        assertNull(Plans.outline(read, "nope", null))
        assertNull(Plans.outline(null, "ledger", null))
    }
}
