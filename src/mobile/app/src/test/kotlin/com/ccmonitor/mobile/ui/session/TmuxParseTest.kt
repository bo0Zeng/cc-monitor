package com.ccmonitor.mobile.ui.session

import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** `parseTmuxList` 的解析容错。 */
class TmuxParseTest {
    @Test fun parsesRows() {
        val out = parseTmuxList("main\t2\t1\tclaude\nwork\t1\t0\tbash")
        assertEquals(2, out.size)
        assertEquals(TmuxSession("main", 2, true, "claude"), out[0])
        assertEquals(TmuxSession("work", 1, false, "bash"), out[1])
    }

    @Test fun emptyRawEmptyList() = assertTrue(parseTmuxList("").isEmpty())

    @Test fun skipsShortAndBlankNameLines() {
        // 行字段不足 3 / 会话名空 → 跳过。
        val out = parseTmuxList("onlyname\nname\t1\nok\t1\t1\tsh\n\t3\t0\tx")
        assertEquals(listOf("ok"), out.map { it.name })
    }

    @Test fun windowsNonNumericDefaultsZero() =
        assertEquals(0, parseTmuxList("s\tNaN\t0\tsh").single().windows)

    @Test fun attachedOnlyWhenColumnIsOne() {
        assertTrue(parseTmuxList("s\t1\t1\tsh").single().attached)
        assertTrue(!parseTmuxList("s\t1\t0\tsh").single().attached)
    }

    @Test fun missingCommandFieldBecomesEmpty() =
        assertEquals("", parseTmuxList("s\t1\t0").single().command)

    @Test fun sessionNameWithSpacesKept() =
        assertEquals("my proj", parseTmuxList("my proj\t1\t0\tsh").single().name)
}
