package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class PlansTest {
    /** 回包照帧客户端读进来的样子（数是 `Double`）：一片两格，一条没认可的需手动。 */
    private val read: Any? =
        RawJson.read(
            """
            {"workspace":"/w","readAtText":"02:11","stale":null,"bySession":{},
             "slices":[{"name":"ledger","domain":"x","error":null,"stale":null,"docsOnly":false,
               "progress":{"done":1,"of":2},"top":["A1","A2"],"labels":{"A1":"A1 t1","A2":"A2 t2"},
               "cells":[
                 {"id":"A1","title":"t1","kind":"k","body":"b","parent":null,"children":[],"statusCode":"done","undecided":false,
                  "stateLine":"s","whyShort":null,"whyCode":null,"files":[]},
                 {"id":"A2","title":"t2","kind":"k","body":"","parent":null,"children":[],"statusCode":"open","undecided":true,
                  "stateLine":null,"whyShort":null,"whyCode":null,"files":[]}],
               "blocks":[{"id":"project","cells":[],"phase":"p","owner":null,"inside":null}],
               "check":{"red":[]},
               "needs":[{"key":"k","kind":"top","block":"project","cell":null,"sid":null,"acked":false}]}]}
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
