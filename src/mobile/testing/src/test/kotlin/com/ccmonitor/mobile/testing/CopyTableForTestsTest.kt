package com.ccmonitor.mobile.testing

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** [CopyTableForTests] 的自检：读不出表 ⇒ 靠它的词表判据全部空转，比没有更坏。 */
class CopyTableForTestsTest {
    @Test
    fun theWholeTableIsReadNotJustAFewEntries() {
        assertTrue("文案表只读出 ${CopyTableForTests.entries.size} 条：排版变了、读法没跟上", CopyTableForTests.entries.size > 1000)
    }

    @Test
    fun aKeyBecomesItsSentenceAndAnythingElseStaysAsIs() {
        assertEquals("取消", CopyTableForTests.zhOf("kit.dialog.cancel"))
        assertEquals("取消", CopyTableForTests.shown("kit.dialog.cancel"))
        assertNull("不是键的字面量没有对应的句子", CopyTableForTests.zhOf("Log 里的一句"))
        assertEquals("Log 里的一句", CopyTableForTests.shown("Log 里的一句"))
    }

    @Test
    fun jsonEscapesAreUndone() {
        val multi = CopyTableForTests.zhOf("mobTmux.kill.body")
        assertTrue("换行没解开：$multi", multi != null && '\n' in multi && "\\n" !in multi)
    }
}
