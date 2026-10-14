package com.ccmonitor.mobile.ui.chat

import com.ccmonitor.mobile.testing.KotlinSourceScanner
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

/**
 * 聊天屏的两样排版判定（模拟器冒烟走主路时看到的两处）：
 * - 键盘弹起：聊天屏自己不让开输入法 ⇒ 系统把整窗往上推，顶栏和消息被推出屏外（消息整列看不见）。
 *   要的是：顶栏留着，消息区缩到输入行上方。
 * - 打开一条长会话：先落在尾段的顶上、当场把更早的整份载进来，人看到的是中间某处而不是最新那几条。
 *   要的是：先贴到底，人自己往上翻到顶才载更早的。
 *
 * 判别力边界：键盘真弹起时排成什么样、滚到底的那一下，门禁里量不了（app 没引 Robolectric）；
 * 冒烟台架（`tests/mobile-smoke`）在模拟器上截屏看。这里钉的是源码里那一句垫没垫、垫的先后，和「该不该载更早」那一判。
 */
class ChatLayoutTest {
    @Test
    fun `聊天屏让开输入法：在 Scaffold 给的内边距之后先认领它、再垫输入法（手势条不算两遍）`() {
        val code = KotlinSourceScanner.codeOnlyKeepingLiterals(source("src/main/kotlin/com/ccmonitor/mobile/ui/chat/ChatRoute.kt"))
        // Scaffold 内容那一块（新建会话的选项 ＋ 聊天面同在一列里）：垫在包着它们的那一列上。
        val at = code.indexOf("{ inner ->")
        val screen = code.indexOf("ChatScreen(", at)
        assertTrue("前提：ChatRoute 里得找到 Scaffold 内容那一块与其中的 ChatScreen(", at >= 0 && screen > at)
        val call = code.substring(at, screen)
        val pad = call.indexOf(".padding(inner)")
        val consume = call.indexOf(".consumeWindowInsets(inner)")
        val ime = call.indexOf(".imePadding()")
        assertTrue("聊天屏没垫输入法：键盘弹起时系统把整窗推上去，顶栏和消息被推出屏外", ime >= 0)
        assertTrue("先垫 Scaffold 的内边距、再认领它（实得 padding@$pad consume@$consume）", pad in 0 until consume)
        assertTrue("认领在垫输入法之前，否则手势条那一段算两遍（实得 consume@$consume ime@$ime）", consume < ime)
    }

    @Test
    fun `主界面那个 Activity 声明 adjustResize：键盘弹起时系统不再整窗上推，让开输入法全交给各屏自己垫`() {
        val manifest = source("src/main/AndroidManifest.xml")
        val activity = Regex("<activity[^>]*MainActivity[^>]*>").find(manifest)?.value
        assertTrue("前提：清单里得找到 MainActivity 那一段", activity != null)
        assertTrue(
            "没声明 adjustResize ⇒ 系统默认整窗上推（adjust=pan），垫了输入法的屏会让开两遍、没垫的被推出屏外：$activity",
            activity!!.contains("android:windowSoftInputMode=\"adjustResize\""),
        )
    }

    @Test
    fun `打开一条长会话：还贴着底要往下滚的那一刻不载更早的`() {
        assertFalse(loadOlderNow(atTop = true, empty = false, canLoadOlder = true, loadingOlder = false, following = true, moreBelow = true))
    }

    @Test
    fun `人往上翻到顶（不再贴底）⇒ 载更早的；内容不满一屏 ⇒ 也载（先把一屏填满）`() {
        assertTrue(loadOlderNow(atTop = true, empty = false, canLoadOlder = true, loadingOlder = false, following = false, moreBelow = true))
        assertTrue(loadOlderNow(atTop = true, empty = false, canLoadOlder = true, loadingOlder = false, following = true, moreBelow = false))
        assertTrue("尾段里一条进界面的都没有", loadOlderNow(atTop = false, empty = true, canLoadOlder = true, loadingOlder = false, following = true, moreBelow = false))
    }

    @Test
    fun `没有更早的 · 正在载 · 没到顶：都不载`() {
        assertFalse(loadOlderNow(atTop = true, empty = false, canLoadOlder = false, loadingOlder = false, following = false, moreBelow = true))
        assertFalse(loadOlderNow(atTop = true, empty = false, canLoadOlder = true, loadingOlder = true, following = false, moreBelow = true))
        assertFalse(loadOlderNow(atTop = false, empty = false, canLoadOlder = true, loadingOlder = false, following = false, moreBelow = true))
    }

    private fun source(relative: String): String =
        (File(relative).takeIf { it.isFile } ?: File("app/$relative")).let {
            assertTrue("前提：得找得到 $relative（实际找的是 ${it.absolutePath}）", it.isFile)
            it.readText()
        }
}
