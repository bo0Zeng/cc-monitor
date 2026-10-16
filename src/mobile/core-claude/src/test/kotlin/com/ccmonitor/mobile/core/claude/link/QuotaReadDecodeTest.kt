package com.ccmonitor.mobile.core.claude.link

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * `quota-read` 回包（跨语言金样 `quota-read.golden.json`，后端真出口写的）每一例手机都解得开、号的条数不丢：
 * 后端多一格必填 · 改一格的型，或手机少认一格，这里就红。
 */
class QuotaReadDecodeTest {
    @Suppress("UNCHECKED_CAST")
    private val cases = Fixtures.json("quota-read.golden.json")["cases"] as List<Map<String, Any?>>

    @Test
    fun everyGoldenReplyDecodesWithAllItsAccounts() {
        assertTrue("金样一例都没有 —— 本条在空转", cases.isNotEmpty())
        for (c in cases) {
            val reply = c["reply"] as Map<*, *>
            val decoded = QuotaRead.of(reply)
            assertNotNull("${c["name"]} 解不开", decoded)
            val q = decoded!!
            assertEquals("${c["name"]} accounts", (reply["accounts"] as List<*>).size, q.accounts.size)
            assertEquals("${c["name"]} unseen", (reply["unseen"] as List<*>).size, q.unseen.size)
            assertEquals("${c["name"]} 每号的行", (reply["accounts"] as List<*>).map { ((it as Map<*, *>)["rows"] as List<*>).size }, q.accounts.map { it.rows.size })
        }
    }
}
