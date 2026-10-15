package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.core.ui.icon.Ph
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/** 键帽名里当图标用的字符画成 Phosphor：整个是那个字符，或「字符＋空格」开头；别的照字画。 */
class KeyCapTest {
    @Test
    fun glyphNamesBecomeIcons() {
        assertEquals(Ph.ArrowUp to "", keyCapIcon("↑"))
        assertEquals(Ph.ArrowsLeftRight to "", keyCapIcon("⇌"))
        assertEquals(Ph.ArrowElbowDownLeft to "", keyCapIcon("⏎"))
        assertEquals("「字符＋空格」开头", Ph.Plus to "new", keyCapIcon("+ new"))
    }

    @Test
    fun ordinaryNamesStayText() {
        assertNull(keyCapIcon("Esc"))
        assertNull(keyCapIcon("Ctrl+End"))
        assertNull("字符不在头上不算", keyCapIcon(":w⏎"))
        assertNull(keyCapIcon("↑↑"))
    }
}
