package com.ccmonitor.mobile.ui.common

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/** ［复制详情］放进剪贴板的那一份与画不画，全仓只在 [copyDetailBody] 判一次：详情空或只有空白 ⇒ 不画；有 ⇒ 上屏那一句 ＋ 换行 ＋ 详情。 */
class CopyDetailBodyTest {
    @Test
    fun `详情空 · 只有空白 · 缺 ⇒ 不画；有 ⇒ 那一句接换行接详情`() {
        assertNull("详情空", copyDetailBody("标题\n那一句", ""))
        assertNull("详情只有空白", copyDetailBody("那一句", "  \n"))
        assertNull("详情缺", copyDetailBody("那一句", null))
        assertEquals("命令已发出 · 会话未报到\n已等 43s 未报到\n\$ claude\n  ╭──", copyDetailBody("命令已发出 · 会话未报到\n已等 43s 未报到", "\$ claude\n  ╭──"))
    }

    /** 两颗按钮都不另判：CopyDetailButton 照 [copyDetailBody]，CopyBodyButton 只认 `null` 不画。 */
    @Test
    fun `判空只在 copyDetailBody 一处`() {
        val src =
            File("src/main/kotlin/com/ccmonitor/mobile/ui/common/CopyDetailButton.kt").takeIf { it.isFile }
                ?: File("app/src/main/kotlin/com/ccmonitor/mobile/ui/common/CopyDetailButton.kt")
        val code = KotlinSourceScanner.codeOnlyKeepingLiterals(src.readText())
        assertEquals("Blank 只出现一次（copyDetailBody 里）：$code", 1, Regex("Blank").findAll(code).count())
        val at = code.indexOf("fun copyDetailBody(")
        assertTrue("前提：得找到 copyDetailBody", at >= 0)
        assertTrue("那一判在 copyDetailBody 里", code.indexOf("Blank") > at && code.indexOf("Blank") < code.indexOf("\n}", at))
        val detail = code.substring(code.indexOf("fun CopyDetailButton("), code.indexOf("\n}", code.indexOf("fun CopyDetailButton(")))
        assertTrue("CopyDetailButton 照 copyDetailBody：$detail", "CopyBodyButton(copyDetailBody(text, detail))" in detail)
    }
}
