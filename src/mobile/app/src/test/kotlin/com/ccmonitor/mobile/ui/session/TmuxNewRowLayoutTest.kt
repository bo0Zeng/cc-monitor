package com.ccmonitor.mobile.ui.session

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * tmux 会话那一框底下的新建那一行：名字框与「新建 · 后台 agent · 新建并打开」三颗按钮挤在一个 Row 里、框拿 `weight(1f)` ⇒
 * 窄屏上三颗按钮先量、框只剩一个字宽（标签竖排、敲不进字）。要的是：名字框独占一行占满宽，按钮在它下面一排、放不下就换行。
 *
 * 判别力边界：门禁里量不了真宽度（app 没引 Robolectric）；冒烟台架在模拟器上截屏看。这里钉的是那一段的排法。
 */
class TmuxNewRowLayoutTest {
    @Test
    fun `名字框独占一行占满宽，三颗按钮在下面一排、放不下换行`() {
        val src =
            KotlinSourceScanner.codeOnlyKeepingLiterals(
                (File(PATH).takeIf { it.isFile } ?: File("app/$PATH")).also { assertTrue("前提：得找得到 $PATH", it.isFile) }.readText(),
            )
        val at = src.indexOf("private fun TmuxNewSessionRow(")
        assertTrue("前提：得找得到 TmuxNewSessionRow", at >= 0)
        val end = src.indexOf("\n}\n", at).takeIf { it > 0 } ?: src.length
        val body = src.substring(at, end)
        val field = body.indexOf("OutlinedTextField(")
        val fieldArgs = body.substring(field, body.indexOf("SecondaryButton(", field).coerceAtLeast(field))
        assertFalse("名字框还拿 weight(1f)（与按钮同挤一行）：$fieldArgs", fieldArgs.contains(".weight("))
        assertTrue("名字框没占满宽：$fieldArgs", fieldArgs.contains("fillMaxWidth()"))
        val flow = body.indexOf("FlowRow(")
        assertTrue("三颗按钮不在一排会换行的 FlowRow 里", flow > field)
        assertTrue("三颗按钮都得在 FlowRow 里", body.indexOf("SecondaryButton(") > flow)
        assertFalse("名字框还和按钮同在一个 Row 里", Regex("\\bRow\\(").containsMatchIn(body.substring(0, field)))
    }

    private companion object {
        const val PATH = "src/main/kotlin/com/ccmonitor/mobile/ui/session/TmuxManager.kt"
    }
}
